//! `Current.set(request: r) { … }` → save, assign, run, restore.
//!
//! `ActiveSupport::CurrentAttributes.set` is a scoped write: it assigns
//! the named attributes, yields, and puts the previous values back —
//! including when the block raises, which is what keeps one request's
//! `Current.user` out of the next. Rails implements it by walking the
//! attribute names at run time; there is no list to walk here, because
//! `ingest::current_attributes` flattens a `Current` into ordinary
//! per-attribute accessors on a thread-local instance.
//!
//! The call site names the attributes, so the triple is spelled out at
//! lower time:
//!
//! ```ruby
//! Current.set(request: r) { body }
//! # →
//! __current_set_arg_request = r
//! __current_set_was_request = Current.request
//! begin
//!   Current.request = __current_set_arg_request
//!   body
//! ensure
//!   Current.request = __current_set_was_request
//! end
//! ```
//!
//! That is `Object#with`'s order: the arguments first (a literal needs
//! no local), then each attribute saved just before it is assigned. With
//! a second attribute, its save and assignment follow the first's inside
//! the `begin`, a count records how many saves were made, and the
//! `ensure` restores those, in order. A block with parameters, or a
//! `next` or `break` of its own, runs as
//! `Current.instance.then { |context| … }`, which yields the instance as
//! `set` does; any other block runs as its body in place. A `set` nested
//! in the block or an argument names its locals with its own level.
//!
//! ENSURE, NOT A TRAILING ASSIGNMENT, and it is the half that matters:
//! `Current.set` exists to be exception-safe. campfire's
//! `actiontext_opengraph_embeds_test` asserts inside the block, so the
//! first failing assertion would otherwise leave `Current.request`
//! pointing at a test host for every test after it — a leak that shows
//! up as an unrelated file failing, which is the worst kind.
//!
//! NO RUNTIME METHOD, deliberately. A generated `Current.set` would
//! need one signature covering every subset of attributes a caller
//! might name, and optional keywords are their own hazard on the strict
//! targets (docs/pipeline/runtime.md). The shape above uses only the
//! accessors the flattening already emits, so it compiles wherever they
//! do.
//!
//! WHAT IT DECLINES: a `set` whose argument is not a literal keyword
//! hash, or whose receiver is not one of the app's own
//! `CurrentAttributes` classes. Both are reported and left as written —
//! the tree has no `set` to call, so the site fails loudly with the
//! reason rather than running the block with the wrong scope.

use crate::app::App;
use crate::diagnostic::{Diagnostic, Severity};
use crate::expr::{Expr, ExprNode, LValue, Literal};
use crate::ident::{Symbol, VarId};
use crate::span::Span;
use std::collections::BTreeSet;

pub fn apply_current_set_lowering(app: &mut App) -> Vec<Diagnostic> {
    let classes: BTreeSet<String> = app
        .current_attribute_classes
        .iter()
        .map(|c| c.0.as_str().to_string())
        .collect();
    if classes.is_empty() {
        return Vec::new();
    }
    let mut diags = Vec::new();
    super::for_each_hook_body(app, &mut |e| rewrite(e, &classes, &mut diags));
    for view in &mut app.views {
        rewrite(&mut view.body, &classes, &mut diags);
    }
    for tm in &mut app.test_modules {
        if let Some(setup) = &mut tm.setup {
            rewrite(setup, &classes, &mut diags);
        }
        for t in &mut tm.tests {
            rewrite(&mut t.body, &classes, &mut diags);
        }
        for m in &mut tm.helpers {
            rewrite(&mut m.body, &classes, &mut diags);
        }
    }
    diags
}

fn rewrite(e: &mut Expr, classes: &BTreeSet<String>, diags: &mut Vec<Diagnostic>) {
    e.node.for_each_child_mut(&mut |c| rewrite(c, classes, diags));
    let span = e.span;
    let ExprNode::Send { recv: Some(recv), method, args, block, .. } = &*e.node else { return };
    if method.as_str() != "set" {
        return;
    }
    let ExprNode::Const { path } = &*recv.node else { return };
    let class = path.iter().map(|p| p.as_str()).collect::<Vec<_>>().join("::");
    if !classes.contains(&class) {
        return;
    }
    let Some(body) = block else {
        report(span, &class, "it takes no block, so there is no scope to restore after", diags);
        return;
    };
    let Some(pairs) = keyword_pairs(args) else {
        report(span, &class, "its argument is not a literal keyword hash", diags);
        return;
    };
    *e = expand(span, &class, &pairs, body);
}

/// `[(attribute, value)]` from the single trailing keyword hash.
fn keyword_pairs(args: &[Expr]) -> Option<Vec<(String, Expr)>> {
    let [opts] = args else { return None };
    let ExprNode::Hash { entries, .. } = &*opts.node else { return None };
    if entries.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (k, v) in entries {
        let name = match &*k.node {
            ExprNode::Lit { value: Literal::Sym { value } } => value.as_str().to_string(),
            ExprNode::Lit { value: Literal::Str { value } } => value.clone(),
            _ => return None,
        };
        out.push((name, v.clone()));
    }
    Some(out)
}

fn expand(span: Span, class: &str, pairs: &[(String, Expr)], body: &Expr) -> Expr {
    let mut stmts: Vec<Expr> = Vec::new();
    // (a `set` inside this block or its arguments saves into locals of its own level: one name for both would have the inner save overwrite the outer's)
    let depth = pairs.iter().map(|(_, v)| nesting(v)).fold(nesting(body), usize::max);
    let level = if depth == 0 { String::new() } else { depth.to_string() };
    let local = |kind: &str, name: &str| Symbol::from(format!("__current_set{level}_{kind}_{name}"));
    // The arguments are evaluated in order before anything is saved, as a
    // call's arguments are: one that raises leaves every attribute as it
    // was, and a later one cannot change an earlier one's value.
    let mut values: Vec<(String, Expr)> = Vec::new();
    for (name, value) in pairs {
        if matches!(&*value.node, ExprNode::Lit { .. }) {
            values.push((name.clone(), value.clone()));
        } else {
            let arg = local("arg", name);
            stmts.push(assign_local(span, arg.clone(), value.clone()));
            values.push((name.clone(), local_read(span, arg)));
        }
    }
    // Then Rails' `Object#with`: per attribute in order, save it and
    // assign it, so each save reads after the writes before it; on the
    // way out, restore the attributes saved so far, in order. The count
    // of saves made is what lets one `ensure` skip those not reached.
    let count = Symbol::from(format!("__current_set{level}_saved"));
    let mut setup: Vec<Expr> = Vec::new();
    let mut restore: Vec<Expr> = Vec::new();
    for (i, (name, value)) in values.iter().enumerate() {
        let saved = local("was", name);
        let save = assign_local(span, saved.clone(), class_read(span, class, name));
        let back = class_write(span, class, name, local_read(span, saved.clone()));
        if i == 0 {
            stmts.push(save);
            restore.push(back);
        } else {
            stmts.push(assign_local(span, saved, nil(span)));
            setup.push(save);
            setup.push(assign_local(span, count.clone(), int(span, i as i64 + 1)));
            restore.push(Expr::new(
                span,
                ExprNode::If {
                    cond: Expr::new(
                        span,
                        ExprNode::Send {
                            recv: Some(local_read(span, count.clone())),
                            method: Symbol::from(">="),
                            args: vec![int(span, i as i64 + 1)],
                            block: None,
                            parenthesized: false,
                        },
                    ),
                    then_branch: back,
                    else_branch: nil(span),
                },
            ));
        }
        setup.push(class_write(span, class, name, value.clone()));
    }
    if values.len() > 1 {
        stmts.push(assign_local(span, count, int(span, 1)));
    }
    setup.push(run_block(span, class, body));
    stmts.push(Expr::new(
        span,
        ExprNode::BeginRescue {
            body: seq(span, setup),
            rescues: Vec::new(),
            else_branch: None,
            ensure: Some(Box::new(seq(span, restore))),
            implicit: false,
        },
    ));
    seq(span, stmts)
}

fn nil(span: Span) -> Expr {
    Expr::new(span, ExprNode::Lit { value: Literal::Nil })
}

fn int(span: Span, value: i64) -> Expr {
    Expr::new(span, ExprNode::Lit { value: Literal::Int { value } })
}

/// How many `set` expansions deep `e` already is: one more than the
/// deepest level whose locals it assigns, 0 for none.
fn nesting(e: &Expr) -> usize {
    let mut deepest = 0;
    let mut visit = |c: &Expr| deepest = deepest.max(nesting(c));
    e.node.for_each_child(&mut visit);
    if let ExprNode::Assign { target: LValue::Var { name, .. }, .. } = &*e.node
        && let Some(rest) = name.as_str().strip_prefix("__current_set")
    {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        deepest = deepest.max(digits.parse::<usize>().unwrap_or(0) + 1);
    }
    deepest
}

/// The block's run inside the save/restore. A block with no parameters
/// and no `next` or `break` of its own runs as its body in place; any
/// other is handed to `Current.instance.then`, which yields the instance
/// as Rails' `set` does and keeps the block's own `next`, `break` and
/// parameters.
fn run_block(span: Span, class: &str, block: &Expr) -> Expr {
    let ExprNode::Lambda { params, rest_param, extra_params, block_param, body, .. } = &*block.node else {
        return block.clone();
    };
    if params.is_empty() && rest_param.is_none() && extra_params.is_empty() && block_param.is_none() && !jumps_out(body) {
        return body.clone();
    }
    let instance = Expr::new(
        span,
        ExprNode::Send {
            recv: Some(const_path(span, class)),
            method: Symbol::from("instance"),
            args: vec![],
            block: None,
            parenthesized: false,
        },
    );
    Expr::new(
        span,
        ExprNode::Send {
            recv: Some(instance),
            method: Symbol::from("then"),
            args: vec![],
            block: Some(block.clone()),
            parenthesized: false,
        },
    )
}

/// Does `e` hold a `next` or `break` that leaves the block itself (one
/// in a nested block or loop leaves that instead)?
fn jumps_out(e: &Expr) -> bool {
    match &*e.node {
        ExprNode::Next { .. } | ExprNode::Break { .. } => true,
        ExprNode::Lambda { .. } | ExprNode::While { .. } => false,
        _ => {
            let mut found = false;
            e.node.for_each_child(&mut |c| found |= jumps_out(c));
            found
        }
    }
}

fn seq(span: Span, exprs: Vec<Expr>) -> Expr {
    if exprs.len() == 1 {
        return exprs.into_iter().next().expect("checked");
    }
    Expr::new(span, ExprNode::Seq { exprs })
}

fn class_read(span: Span, class: &str, name: &str) -> Expr {
    Expr::new(
        span,
        ExprNode::Send {
            recv: Some(const_path(span, class)),
            method: Symbol::from(name),
            args: vec![],
            block: None,
            parenthesized: false,
        },
    )
}

fn class_write(span: Span, class: &str, name: &str, value: Expr) -> Expr {
    Expr::new(
        span,
        ExprNode::Send {
            recv: Some(const_path(span, class)),
            method: Symbol::from(format!("{name}=")),
            args: vec![value],
            block: None,
            parenthesized: false,
        },
    )
}

fn assign_local(span: Span, name: Symbol, value: Expr) -> Expr {
    Expr::new(
        span,
        ExprNode::Assign {
            target: LValue::Var { id: VarId(0), name },
            value,
        },
    )
}

fn local_read(span: Span, name: Symbol) -> Expr {
    Expr::new(span, ExprNode::Var { id: VarId(0), name })
}

fn const_path(span: Span, class: &str) -> Expr {
    Expr::new(span, ExprNode::Const { path: class.split("::").map(Symbol::from).collect() })
}

fn report(span: Span, class: &str, why: &str, diags: &mut Vec<Diagnostic>) {
    let mut d = Diagnostic::unsupported(
        span,
        None,
        "CurrentAttributes#set",
        format!(
            "`{class}.set` is served only as a block form over a literal keyword hash, which is what lets the save/restore be spelled at compile time: {why}"
        ),
    );
    d.severity = Severity::Warning;
    diags.push(d);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ident::ClassId;

    fn app_with(body: &str) -> App {
        let src = format!("class T < ActiveSupport::TestCase\n  test \"t\" do\n    {body}\n  end\nend\n");
        let mut app = App::new();
        app.current_attribute_classes.push(ClassId(Symbol::from("Current")));
        app.test_modules
            .extend(crate::ingest::test::ingest_test_files(src.as_bytes(), "t.rb").expect("ingest"));
        app
    }

    fn lowered(app: &App) -> String {
        crate::emit::ruby::emit_expr(&app.test_modules[0].tests[0].body)
    }

    #[test]
    fn one_attribute_saves_assigns_and_restores_in_an_ensure() {
        let mut app = app_with("Current.set(request: \"r\") do\n      work\n    end");
        let diags = apply_current_set_lowering(&mut app);
        assert!(diags.is_empty(), "{diags:?}");
        let out = lowered(&app);
        assert!(out.contains("__current_set_was_request = Current.request"), "{out}");
        assert!(out.contains("Current.request = \"r\""), "{out}");
        assert!(out.contains("ensure"), "{out}");
        assert!(out.contains("Current.request = __current_set_was_request"), "{out}");
    }

    /// Each attribute is saved just before it is assigned, as
    /// `Object#with` does: the second save reads after the first write.
    #[test]
    fn each_attribute_is_saved_just_before_it_is_assigned() {
        let mut app = app_with("Current.set(request: \"r\", user: \"u\") do\n      work\n    end");
        let diags = apply_current_set_lowering(&mut app);
        assert!(diags.is_empty(), "{diags:?}");
        let out = lowered(&app);
        let at = |s: &str| out.find(s).unwrap_or_else(|| panic!("no `{s}` in:\n{out}"));
        let order = [
            at("__current_set_was_request = Current.request"),
            at("Current.request = \"r\""),
            at("__current_set_was_user = Current.user"),
            at("Current.user = \"u\""),
        ];
        assert!(order.windows(2).all(|w| w[0] < w[1]), "out of Object#with's order:\n{out}");
    }

    /// The attributes saved are restored in the order they were set,
    /// each only once its save was made.
    #[test]
    fn restores_run_in_order_and_only_for_the_saves_made() {
        let mut app = app_with("Current.set(request: \"r\", user: \"u\") { work }");
        assert!(apply_current_set_lowering(&mut app).is_empty());
        let out = lowered(&app);
        let ensure = &out[out.find("ensure").expect(&out)..];
        let request = ensure.find("Current.request = __current_set_was_request").expect(&out);
        let user = ensure.find("Current.user = __current_set_was_user").expect(&out);
        assert!(request < user, "{out}");
        assert!(ensure.contains("__current_set_saved >= 2"), "{out}");
    }

    /// A `set` in another's argument names its locals apart from the
    /// outer one's.
    #[test]
    fn a_set_in_an_argument_keeps_its_own_locals() {
        let mut app = app_with("Current.set(user: a, request: Current.set(user: b) { \"c\" }) { work }");
        assert!(apply_current_set_lowering(&mut app).is_empty());
        let out = lowered(&app);
        assert!(out.contains("__current_set1_arg_user = a") && out.contains("__current_set_arg_user = b"), "{out}");
    }

    /// The block runs: its body sits in the `begin`, not a lambda left
    /// unrun there.
    #[test]
    fn a_plain_block_runs_as_its_body() {
        let mut app = app_with("x = Current.set(request: r) { work }");
        assert!(apply_current_set_lowering(&mut app).is_empty());
        let out = lowered(&app);
        assert!(out.contains("work") && !out.contains("->") && !out.contains("then"), "{out}");
    }

    /// A block parameter, and a `next` or `break` of the block's own,
    /// keep their meaning through `Current.instance.then`.
    #[test]
    fn a_block_with_a_parameter_or_a_jump_runs_through_then() {
        for body in ["Current.set(request: r) { |c| c.request }", "Current.set(request: r) { next 1 }", "Current.set(request: r) { break 1 }"] {
            let mut app = app_with(body);
            assert!(apply_current_set_lowering(&mut app).is_empty());
            let out = lowered(&app);
            assert!(out.contains("Current.instance.then"), "{body}:\n{out}");
        }
    }

    /// A `next` in a nested block or a `break` in a loop leaves that, not
    /// the `set` block, which runs as its body.
    #[test]
    fn a_jump_of_a_nested_block_or_loop_stays_in_place() {
        let mut app = app_with("Current.set(request: r) { [1].each { |n| next n }; while true; break; end }");
        assert!(apply_current_set_lowering(&mut app).is_empty());
        assert!(!lowered(&app).contains("Current.instance.then"), "{}", lowered(&app));
    }

    /// An argument that is a call is evaluated before the first save.
    #[test]
    fn a_call_argument_is_evaluated_before_anything_is_saved() {
        let mut app = app_with("Current.set(request: build) { work }");
        assert!(apply_current_set_lowering(&mut app).is_empty());
        let out = lowered(&app);
        let value = out.find("__current_set_arg_request = build").expect(&out);
        let save = out.find("__current_set_was_request = Current.request").expect(&out);
        assert!(value < save, "{out}");
    }

    #[test]
    fn a_non_keyword_argument_is_reported_and_left() {
        let mut app = app_with("Current.set(attrs) do\n      work\n    end");
        let diags = apply_current_set_lowering(&mut app);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(lowered(&app).contains("Current.set(attrs)"), "{}", lowered(&app));
    }

    /// `set` on anything that is not one of the app's own
    /// CurrentAttributes classes is somebody else's method.
    #[test]
    fn another_receivers_set_is_untouched() {
        let mut app = app_with("Cache.set(key: k) do\n      work\n    end");
        let diags = apply_current_set_lowering(&mut app);
        assert!(diags.is_empty(), "{diags:?}");
        assert!(lowered(&app).contains("Cache.set"), "{}", lowered(&app));
    }
}
