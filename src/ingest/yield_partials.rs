//! A block render, `render "accounts/form", account: a do |form| … end`
//! (or `render layout: "x", locals: {…} do … end`), becomes a render of a
//! copy of the partial made for that call, its `<%= yield form %>`
//! replaced by the block's body with `form` bound to what the yield passes:
//!
//! ```erb
//! <%# accounts/_form__yield0, from credit_cards/_form's call %>
//! … <% __yield0_form = form %><b><%= __yield0_form.x %></b> …
//! ```
//!
//! Rails runs the block at the `yield` (`_layout_for` captures it there),
//! with the yield's arguments, once per yield and inside whatever loop or
//! block the yield sits in; the copy does the same, since the body now sits
//! there. Each name the body binds or reads is renamed apart from the
//! partial's (`__yield<k>_<name>`), and each local it reads from the
//! caller (`account`, the caller partial's own locals) is passed in under
//! that name, as a strict local when the partial declares them.
//!
//! Left as written, and reported by `analyze::diagnostics` as before: a
//! partial whose yield is not an output (`<% yield %>`, `x = yield`), a
//! block that leaves early (`next`, `break`, `return`) or yields itself,
//! one binding names this pass does not rename (a `rescue => e`, a pattern),
//! and a block with more parameters than plain ones.

use crate::App;
use crate::dialect::View;
use crate::expr::{Expr, ExprNode, LValue, Literal};
use crate::ident::{Symbol, VarId};

pub(super) fn specialize(app: &mut App) {
    let mut copies: Vec<View> = Vec::new();
    let mut next = 0usize;
    let snapshot: Vec<View> = app.views.clone();
    let mut spans = Vec::new();
    for view in &mut app.views {
        let dir = view.name.as_str().rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        let format = view.format.clone();
        rewrite(&mut view.body, &dir, &format, &snapshot, &mut copies, &mut next, &mut spans);
    }
    app.views.extend(copies);
    app.yield_specialized = spans;
}

fn rewrite(e: &mut Expr, dir: &str, format: &Symbol, views: &[View], copies: &mut Vec<View>, next: &mut usize, spans: &mut Vec<crate::span::Span>) {
    if let ExprNode::Seq { exprs } = &mut *e.node {
        for stmt in exprs.iter_mut() {
            if let Some((new, span)) = specialized(stmt, dir, format, views, copies, next) {
                *stmt = new;
                spans.push(span);
            } else {
                rewrite(stmt, dir, format, views, copies, next, spans);
            }
        }
        return;
    }
    e.node.for_each_child_mut(&mut |c| rewrite(c, dir, format, views, copies, next, spans));
}

/// `_buf = _buf + (<call>).to_s`'s `<call>`.
fn appended(e: &Expr) -> Option<&Expr> {
    let ExprNode::Assign { target: LValue::Var { name, .. }, value } = &*e.node else { return None };
    if name.as_str() != "_buf" {
        return None;
    }
    let ExprNode::Send { method, args, .. } = &*value.node else { return None };
    if method.as_str() != "+" {
        return None;
    }
    let ExprNode::Send { recv: Some(inner), method: to_s, args: none, .. } = &*args.first()?.node else { return None };
    (to_s.as_str() == "to_s" && none.is_empty()).then_some(inner)
}

fn sym_key<'a>(entries: &'a [(Expr, Expr)], key: &str) -> Option<&'a Expr> {
    entries
        .iter()
        .find(|(k, _)| matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == key))
        .map(|(_, v)| v)
}

fn str_lit(e: &Expr) -> Option<&str> {
    match &*e.node {
        ExprNode::Lit { value: Literal::Str { value } } => Some(value.as_str()),
        _ => None,
    }
}

fn specialized(stmt: &Expr, dir: &str, format: &Symbol, views: &[View], copies: &mut Vec<View>, next: &mut usize) -> Option<(Expr, crate::span::Span)> {
    let call = appended(stmt)?;
    let ExprNode::Send { recv: None, method, args, block: Some(block), .. } = &*call.node else { return None };
    if method.as_str() != "render" {
        return None;
    }
    // `render "p"[, locals]` or `render layout: "p"[, locals: {…}]`.
    let (name, locals): (&str, Vec<(Expr, Expr)>) = match args.as_slice() {
        [first] | [first, _] if str_lit(first).is_some() => {
            let locals = match args.get(1).map(|a| &*a.node) {
                None => Vec::new(),
                Some(ExprNode::Hash { entries, .. }) => entries.clone(),
                Some(_) => return None,
            };
            (str_lit(first)?, locals)
        }
        [opts] => {
            let ExprNode::Hash { entries, .. } = &*opts.node else { return None };
            if !entries.iter().all(|(k, _)| matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if matches!(value.as_str(), "layout" | "locals"))) {
                return None;
            }
            let layout = str_lit(sym_key(entries, "layout")?)?;
            let locals = match sym_key(entries, "locals").map(|l| &*l.node) {
                None => Vec::new(),
                Some(ExprNode::Hash { entries, .. }) => entries.clone(),
                Some(_) => return None,
            };
            (layout, locals)
        }
        _ => return None,
    };
    let ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, body, .. } = &*block.node else { return None };
    if !extra_params.is_empty() || leaves_or_binds(body) {
        return None;
    }
    let (pdir, base) = name.rsplit_once('/').map(|(d, b)| (d.to_string(), b.to_string())).unwrap_or((dir.to_string(), name.to_string()));
    let full = format!("{pdir}/_{base}");
    let partial = views.iter().find(|v| v.name.as_str() == full && v.format == *format)?;
    let mut yields = Vec::new();
    if !only_output_yields(&partial.body, &mut yields) || yields.is_empty() {
        return None;
    }
    // Ruby splats one Array yielded to a block of several parameters, which a binding per parameter does not.
    if params.len() > 1 && yields.iter().any(|n| *n == 1) {
        return None;
    }
    let k = *next;
    *next += 1;
    let renamed = |n: &str| Symbol::from(format!("__yield{k}_{n}"));
    let mut bound: crate::det::HashSet<String> = params.iter().map(|p| p.as_str().to_string()).collect();
    binders(body, &mut bound);
    let mut free: Vec<String> = Vec::new();
    reads(body, &mut |n| {
        if n != "_buf" && !bound.contains(n) && !free.iter().any(|f| f == n) {
            free.push(n.to_string());
        }
    });
    let mut inlined = body.clone();
    rename(&mut inlined, &renamed);
    let params: Vec<Symbol> = params.iter().map(|p| renamed(p.as_str())).collect();
    let mut copy = partial.clone();
    copy.name = Symbol::from(format!("{pdir}/_{base}__yield{k}"));
    replace_yields(&mut copy.body, &params, &inlined);
    if let Some(strict) = &mut copy.strict_locals {
        for n in &free {
            strict.push(crate::dialect::Param { name: renamed(n), default: None, keyword: true, rest: false, forwarding: false, from_keyword: false, from_kwrest: false });
        }
    }
    copies.push(copy);
    let span = call.span;
    let mut entries = locals;
    for n in &free {
        let key = Expr::new(span, ExprNode::Lit { value: Literal::Sym { value: renamed(n) } });
        let value = Expr::new(span, ExprNode::Var { id: VarId(0), name: Symbol::from(n.as_str()) });
        entries.push((key, value));
    }
    let target = Expr::new(span, ExprNode::Lit { value: Literal::Str { value: format!("{pdir}/{base}__yield{k}") } });
    let hash = Expr::new(span, ExprNode::Hash { entries, kwargs: true });
    let render = Expr::new(span, ExprNode::Send { recv: None, method: Symbol::from("render"), args: vec![target, hash], block: None, parenthesized: true });
    let mut out = stmt.clone();
    if let ExprNode::Assign { value, .. } = &mut *out.node
        && let ExprNode::Send { args, .. } = &mut *value.node
        && let Some(first) = args.first_mut()
        && let ExprNode::Send { recv: Some(inner), .. } = &mut *first.node
    {
        *inner = render;
    }
    Some((out, span))
}

/// Whether the block leaves early, yields itself, or binds a name `rename` does not reach.
fn leaves_or_binds(e: &Expr) -> bool {
    let mut found = match &*e.node {
        ExprNode::Next { .. } | ExprNode::Break { .. } | ExprNode::Return { .. } | ExprNode::Redo | ExprNode::Retry | ExprNode::Yield { .. } => true,
        ExprNode::BeginRescue { rescues, .. } => rescues.iter().any(|r| r.binding.is_some()),
        ExprNode::Case { arms, .. } => arms.iter().any(|a| !matches!(a.pattern, crate::expr::Pattern::Lit { .. } | crate::expr::Pattern::Wildcard | crate::expr::Pattern::Expr { .. })),
        ExprNode::Lambda { rest_param, extra_params, block_param, .. } => rest_param.is_some() || !extra_params.is_empty() || block_param.is_some(),
        _ => false,
    };
    e.node.for_each_child(&mut |c| found = found || leaves_or_binds(c));
    found
}

/// The partial's yields, each an output (`<%= yield … %>`), by argument count; false past any other yield.
fn only_output_yields(e: &Expr, counts: &mut Vec<usize>) -> bool {
    // A block whose body is the one statement holds it bare, not in a `Seq`.
    if let Some(ExprNode::Yield { args }) = appended(e).map(|c| &*c.node) {
        counts.push(args.len());
        return args.iter().all(|a| only_output_yields(a, &mut Vec::new()));
    }
    if let ExprNode::Seq { exprs } = &*e.node {
        for stmt in exprs {
            if let Some(ExprNode::Yield { args }) = appended(stmt).map(|c| &*c.node) {
                if args.iter().any(|a| !only_output_yields(a, &mut Vec::new())) {
                    return false;
                }
                counts.push(args.len());
                continue;
            }
            if !only_output_yields(stmt, counts) {
                return false;
            }
        }
        return true;
    }
    if matches!(&*e.node, ExprNode::Yield { .. }) {
        return false;
    }
    let mut ok = true;
    e.node.for_each_child(&mut |c| ok = ok && only_output_yields(c, counts));
    ok
}

fn replace_yields(e: &mut Expr, params: &[Symbol], body: &Expr) {
    if matches!(appended(e).map(|c| &*c.node), Some(ExprNode::Yield { .. })) {
        let mut seq = Expr::new(e.span, ExprNode::Seq { exprs: vec![e.clone()] });
        replace_yields(&mut seq, params, body);
        *e = seq;
        return;
    }
    if let ExprNode::Seq { exprs } = &mut *e.node {
        let mut out = Vec::with_capacity(exprs.len());
        for mut stmt in std::mem::take(exprs) {
            let yielded = match appended(&stmt).map(|c| &*c.node) {
                Some(ExprNode::Yield { args }) => Some(args.clone()),
                _ => None,
            };
            let Some(args) = yielded else {
                replace_yields(&mut stmt, params, body);
                out.push(stmt);
                continue;
            };
            let span = stmt.span;
            for (i, p) in params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or_else(|| Expr::new(span, ExprNode::Lit { value: Literal::Nil }));
                out.push(Expr::new(span, ExprNode::Assign { target: LValue::Var { id: VarId(0), name: p.clone() }, value }));
            }
            match &*body.node {
                ExprNode::Seq { exprs } => out.extend(exprs.iter().cloned()),
                _ => out.push(body.clone()),
            }
        }
        *exprs = out;
        return;
    }
    e.node.for_each_child_mut(&mut |c| replace_yields(c, params, body));
}

/// The names the block binds: its assignments and its nested blocks' parameters.
fn binders(e: &Expr, out: &mut crate::det::HashSet<String>) {
    match &*e.node {
        ExprNode::Assign { target: LValue::Var { name, .. }, .. } | ExprNode::OpAssign { target: LValue::Var { name, .. }, .. } => {
            out.insert(name.as_str().to_string());
        }
        ExprNode::MultiAssign { targets, .. } => {
            for t in targets {
                if let LValue::Var { name, .. } = t {
                    out.insert(name.as_str().to_string());
                }
            }
        }
        ExprNode::Lambda { params, .. } => out.extend(params.iter().map(|p| p.as_str().to_string())),
        _ => {}
    }
    e.node.for_each_child(&mut |c| binders(c, out));
}

fn reads(e: &Expr, f: &mut dyn FnMut(&str)) {
    if let ExprNode::Var { name, .. } = &*e.node {
        f(name.as_str());
    }
    e.node.for_each_child(&mut |c| reads(c, f));
}

fn rename(e: &mut Expr, to: &dyn Fn(&str) -> Symbol) {
    match &mut *e.node {
        ExprNode::Var { name, .. } if name.as_str() != "_buf" => *name = to(name.as_str()),
        ExprNode::Assign { target: LValue::Var { name, .. }, .. } | ExprNode::OpAssign { target: LValue::Var { name, .. }, .. }
            if name.as_str() != "_buf" =>
        {
            *name = to(name.as_str())
        }
        ExprNode::MultiAssign { targets, .. } => {
            for t in targets {
                if let LValue::Var { name, .. } = t {
                    *name = to(name.as_str());
                }
            }
        }
        ExprNode::Lambda { params, .. } => {
            for p in params.iter_mut() {
                *p = to(p.as_str());
            }
        }
        _ => {}
    }
    e.node.for_each_child_mut(&mut |c| rename(c, to));
}
