//! A view's `render partial: <computed>, locals: …` whose name an app
//! helper writes (maybe's `render partial:
//! permitted_import_configuration_path(@import), locals: { import: @import }`
//! over `"import/configurations/#{import.type.underscore}"`) becomes a
//! `case` over the partials that name can reach:
//!
//! ```erb
//! <% case permitted_import_configuration_path(@import) %>
//! <% when "import/configurations/account_import" %><%= render partial: "import/configurations/account_import", locals: … %>
//! …
//! <% else %><% raise ActionView::MissingTemplate, … %>
//! <% end %>
//! ```
//!
//! Each arm is then an ordinary named render, so the partial's ivars,
//! locals and lowering follow it as they follow any other. The candidates
//! are the partials whose name every value the helper returns can spell: a
//! String literal, or an interpolation whose literal head names a
//! directory (`"dir/#{…}"`, with a literal tail when it has one). A name
//! the helper computes any other way is left as written, and
//! `analyze::diagnostics` reports it. A name outside the pool raises
//! `ActionView::MissingTemplate`, as Rails does for a partial that is not
//! there.

use crate::App;
use crate::expr::{Arm, Expr, ExprNode, InterpPart, LValue, Literal, Pattern};
use crate::ident::Symbol;

pub(super) fn dispatch(app: &mut App) {
    let partials: Vec<String> = {
        let mut names: Vec<String> = app
            .views
            .iter()
            .filter_map(|v| {
                let (dir, base) = v.name.as_str().rsplit_once('/')?;
                base.strip_prefix('_').map(|b| format!("{dir}/{b}"))
            })
            .collect();
        names.sort();
        names.dedup();
        names
    };
    let helpers: crate::det::HashMap<Symbol, Expr> = app
        .library_classes
        .iter()
        .filter(|lc| lc.is_module && (app.helper_modules.contains(&lc.name) || lc.name.0.as_str().ends_with("Helper")))
        .flat_map(|lc| lc.methods.iter())
        .filter(|m| m.receiver == crate::dialect::MethodReceiver::Instance)
        .map(|m| (m.name.clone(), m.body.clone()))
        .collect();
    for view in &mut app.views {
        let dir = view.name.as_str().rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        rewrite(&mut view.body, &dir, &partials, &helpers);
    }
}

fn rewrite(e: &mut Expr, dir: &str, partials: &[String], helpers: &crate::det::HashMap<Symbol, Expr>) {
    if let ExprNode::Seq { exprs } = &mut *e.node {
        let mut out = Vec::with_capacity(exprs.len());
        for mut stmt in std::mem::take(exprs) {
            match dispatched(&stmt, dir, partials, helpers) {
                Some(stmts) => out.extend(stmts),
                None => {
                    rewrite(&mut stmt, dir, partials, helpers);
                    out.push(stmt);
                }
            }
        }
        *exprs = out;
        return;
    }
    e.node.for_each_child_mut(&mut |c| rewrite(c, dir, partials, helpers));
}

/// `_buf = _buf + (render(partial: <computed>, …)).to_s` as the name bound once, then the `case` over its pool.
fn dispatched(e: &Expr, dir: &str, partials: &[String], helpers: &crate::det::HashMap<Symbol, Expr>) -> Option<Vec<Expr>> {
    let ExprNode::Assign { target: LValue::Var { name, .. }, value } = &*e.node else { return None };
    if name.as_str() != "_buf" {
        return None;
    }
    let ExprNode::Send { method, args, .. } = &*value.node else { return None };
    if method.as_str() != "+" {
        return None;
    }
    let ExprNode::Send { recv: Some(rendered), method: to_s, .. } = &*args.first()?.node else { return None };
    if to_s.as_str() != "to_s" {
        return None;
    }
    let computed = computed_partial(rendered)?;
    let pool = pool(computed, dir, partials, helpers)?;
    let span = e.span;
    // Not the computed call again in the error: a helper with an effect would run twice.
    let local = Symbol::from(format!("__partial_name_{}", span.start));
    let read = Expr::new(span, ExprNode::Var { id: crate::ident::VarId(0), name: local.clone() });
    let bind = Expr::new(span, ExprNode::Assign { target: LValue::Var { id: crate::ident::VarId(0), name: local }, value: computed.clone() });
    let mut arms: Vec<Arm> = pool
        .iter()
        .map(|(spelled, resolved)| {
            let mut body = e.clone();
            name_partial(&mut body, resolved);
            Arm { pattern: Pattern::Lit { value: Literal::Str { value: spelled.clone() } }, guard: None, body }
        })
        .collect();
    let mut error = Expr::new(span, ExprNode::Const { path: vec![Symbol::from("ActionView"), Symbol::from("MissingTemplate")] });
    error.decisions |= crate::expr::GENERATED_CONST_REF;
    let message = Expr::new(span, ExprNode::StringInterp { parts: vec![
        InterpPart::Text { value: "Missing partial ".into() },
        InterpPart::Expr { expr: read.clone() },
    ] });
    let raised = Expr::new(span, ExprNode::Send { recv: None, method: Symbol::from("raise"), args: vec![error, message], block: None, parenthesized: false });
    arms.push(Arm { pattern: Pattern::Wildcard, guard: None, body: raised });
    Some(vec![bind, Expr::new(span, ExprNode::Case { scrutinee: read, arms })])
}

/// The `partial:` value of a block-less `render(partial: …)` whose name is neither a literal nor a variable (the forms
/// the view lowering already reads).
fn computed_partial(rendered: &Expr) -> Option<&Expr> {
    let ExprNode::Send { recv: None, method, args, block: None, .. } = &*rendered.node else { return None };
    if method.as_str() != "render" || args.len() != 1 {
        return None;
    }
    let ExprNode::Hash { entries, .. } = &*args[0].node else { return None };
    let (_, value) = entries.iter().find(|(k, _)| is_sym(k, "partial"))?;
    if !entries.iter().all(|(k, _)| is_sym(k, "partial") || is_sym(k, "locals")) {
        return None;
    }
    (!matches!(&*value.node, ExprNode::Lit { .. } | ExprNode::Var { .. } | ExprNode::Ivar { .. })).then_some(value)
}

fn is_sym(k: &Expr, name: &str) -> bool {
    matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == name)
}

fn name_partial(e: &mut Expr, name: &str) {
    if let ExprNode::Send { recv: None, method, args, block: None, .. } = &mut *e.node
        && method.as_str() == "render"
        && let Some(ExprNode::Hash { entries, .. }) = args.first_mut().map(|a| &mut *a.node)
    {
        for (k, v) in entries.iter_mut() {
            if is_sym(k, "partial") {
                *v = Expr::new(v.span, ExprNode::Lit { value: Literal::Str { value: name.to_string() } });
                v.ty = Some(crate::ty::Ty::Str);
            }
        }
        return;
    }
    e.node.for_each_child_mut(&mut |c| name_partial(c, name));
}

/// The partials `computed` can name, each as the helper spells it and as the render resolves it, or `None` when
/// some value it may take is not a spelling read here.
fn pool(computed: &Expr, dir: &str, partials: &[String], helpers: &crate::det::HashMap<Symbol, Expr>) -> Option<Vec<(String, String)>> {
    let values = match &*computed.node {
        ExprNode::StringInterp { .. } => vec![computed],
        ExprNode::Send { recv: None, method, block: None, .. } => {
            let body = helpers.get(method)?;
            let mut out = Vec::new();
            early_returns(body, &mut out)?;
            returned(body, &mut out)?;
            out
        }
        _ => return None,
    };
    let mut pool: Vec<(String, String)> = Vec::new();
    for value in values {
        let names: Vec<(String, String)> = match &*value.node {
            ExprNode::Lit { value: Literal::Str { value } } => {
                let resolved = if value.contains('/') { value.clone() } else { format!("{dir}/{value}") };
                partials.iter().filter(|p| **p == resolved).map(|p| (value.clone(), p.clone())).collect()
            }
            ExprNode::StringInterp { parts } => {
                let InterpPart::Text { value: head } = parts.first()? else { return None };
                if !head.contains('/') {
                    return None;
                }
                let tail = match parts.last() {
                    Some(InterpPart::Text { value }) if parts.len() > 1 => value.as_str(),
                    _ => "",
                };
                partials
                    .iter()
                    .filter(|p| p.len() >= head.len() + tail.len() && p.starts_with(head.as_str()) && p.ends_with(tail))
                    .map(|p| (p.clone(), p.clone()))
                    .collect()
            }
            _ => return None,
        };
        for n in names {
            if !pool.contains(&n) {
                pool.push(n);
            }
        }
    }
    (!pool.is_empty()).then_some(pool)
}

/// The values of `return`s before the body's last statement; `None` past a `return` whose value is not read here.
fn early_returns<'a>(body: &'a Expr, out: &mut Vec<&'a Expr>) -> Option<()> {
    let mut ok = true;
    fn visit<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>, ok: &mut bool) {
        match &*e.node {
            ExprNode::Return { value } => {
                if returned(value, out).is_none() {
                    *ok = false;
                }
            }
            // A block's `return` leaves the helper, a lambda's only the lambda, and the IR spells both alike: neither is read.
            ExprNode::Lambda { body, .. } => {
                if contains_return(body) {
                    *ok = false;
                }
            }
            _ => e.node.for_each_child(&mut |c| visit(c, out, ok)),
        }
    }
    visit(body, out, &mut ok);
    ok.then_some(())
}

fn contains_return(e: &Expr) -> bool {
    let mut found = matches!(&*e.node, ExprNode::Return { .. });
    e.node.for_each_child(&mut |c| found = found || contains_return(c));
    found
}

/// Every value `body` may answer, leaving out what raises; `None` past a shape whose value is not read here.
fn returned<'a>(body: &'a Expr, out: &mut Vec<&'a Expr>) -> Option<()> {
    match &*body.node {
        ExprNode::Seq { exprs } => returned(exprs.last()?, out),
        ExprNode::If { then_branch, else_branch, .. } => {
            returned(then_branch, out)?;
            returned(else_branch, out)
        }
        ExprNode::Case { arms, .. } => arms.iter().try_for_each(|a| returned(&a.body, out)),
        ExprNode::Return { value } => returned(value, out),
        ExprNode::Send { recv: None, method, .. } if method.as_str() == "raise" => Some(()),
        ExprNode::Lit { value: Literal::Str { .. } } | ExprNode::StringInterp { .. } => {
            out.push(body);
            Some(())
        }
        _ => None,
    }
}
