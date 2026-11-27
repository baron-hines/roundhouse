//! `simple_format(text)` in a template: Rails' default (`sanitize: true`) runs `sanitize(text)` and then formats what
//! it answers exactly as the `sanitize: false` form does (actionview 8.1.4 `TextHelper#simple_format`). The call
//! becomes `simple_format(sanitize(text), html_options, sanitize: false, …)`, the form `lower::view` lowers, with the
//! sanitizer the app's own `sanitize` calls get. A call passing `sanitize_options:`, splatting its arguments, or naming
//! a non-literal `sanitize:` stays as it is and is reported.

use crate::app::App;
use crate::expr::{Expr, ExprNode, Literal};
use crate::Symbol;

pub(super) fn expand(app: &mut App) {
    // Not over the app's own helpers of either name: the call is then theirs, not Rails'.
    if ["simple_format", "sanitize"].iter().any(|n| {
        let name = Symbol::from(*n);
        app.helper_method_index.contains_key(&name) || app.view_visible_controller_methods.contains(&name)
    }) {
        return;
    }
    for view in &mut app.views {
        rewrite(&mut view.body);
    }
}

fn rewrite(e: &mut Expr) {
    e.node.for_each_child_mut(&mut |c| rewrite(c));
    let ExprNode::Send { recv: None, method, args, block: None, .. } = &mut *e.node else { return };
    if method.as_str() != "simple_format" || args.is_empty() || args.len() > 3 {
        return;
    }
    let span = e.span;
    let sym = |name: &str| Expr::new(span, ExprNode::Lit { value: Literal::Sym { value: Symbol::from(name) } });
    let mut options: Vec<(Expr, Expr)> = match args.get(2).map(|a| &*a.node) {
        None => Vec::new(),
        Some(ExprNode::Hash { entries, .. }) => entries.clone(),
        Some(_) => return,
    };
    for (k, v) in &options {
        let ExprNode::Lit { value: Literal::Sym { value } } = &*k.node else { return };
        match value.as_str() {
            "wrapper_tag" => {}
            "sanitize" if matches!(&*v.node, ExprNode::Lit { value: Literal::Bool { value: true } }) => {}
            _ => return,
        }
    }
    // Not where the html options could run code: Rails evaluates them before it sanitizes the text, and the rewrite
    // sanitizes first.
    if args.get(1).is_some_and(|a| !inert(a)) || matches!(&*args[0].node, ExprNode::Splat { .. }) {
        return;
    }
    options.retain(|(k, _)| !matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "sanitize"));
    options.push((sym("sanitize"), Expr::new(span, ExprNode::Lit { value: Literal::Bool { value: false } })));
    let text = args[0].clone();
    let sanitized = Expr::new(
        text.span,
        ExprNode::Send { recv: None, method: Symbol::from("sanitize"), args: vec![text], block: None, parenthesized: true },
    );
    let html = args.get(1).cloned().unwrap_or_else(|| Expr::new(span, ExprNode::Hash { entries: Vec::new(), kwargs: false }));
    *args = vec![sanitized, html, Expr::new(span, ExprNode::Hash { entries: options, kwargs: true })];
}

/// A literal, a local or instance variable read, or a Hash or Array of those: nothing it evaluates can change the text.
fn inert(e: &Expr) -> bool {
    match &*e.node {
        ExprNode::Lit { .. } | ExprNode::Var { .. } | ExprNode::Ivar { .. } => true,
        ExprNode::Hash { entries, .. } => entries.iter().all(|(k, v)| inert(k) && inert(v)),
        ExprNode::Array { elements, .. } => elements.iter().all(inert),
        _ => false,
    }
}
