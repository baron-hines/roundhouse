//! `render action: "new", layout: !request.xhr?`: a layout picked on each request by a negation, which answers only
//! `true` (the default layout) or `false` (none). The render becomes an `if` on the negated value with one literal
//! `layout:` per branch, as Rails' `_layout_for_option` reads those two, so every later pass sees literal layouts.
//! Any other computed `layout:` stays as it is and is reported (`controller_layout`).

use crate::app::App;
use crate::dialect::ControllerBodyItem;
use crate::expr::{Expr, ExprNode, Literal};

pub(super) fn expand(app: &mut App) {
    for controller in &mut app.controllers {
        for item in &mut controller.body {
            if let ControllerBodyItem::Action { action, .. } = item {
                rewrite(&mut action.body);
            }
        }
    }
}

fn rewrite(e: &mut Expr) {
    e.node.for_each_child_mut(&mut |c| rewrite(c));
    let Some(negated) = negated_layout(e) else { return };
    let branch = |layout: bool| {
        let mut render = e.clone();
        set_layout(&mut render, layout);
        render
    };
    let (shown, hidden) = (branch(true), branch(false));
    *e = Expr::new(e.span, ExprNode::If { cond: negated, then_branch: hidden, else_branch: shown });
}

/// `e` of a `render …, layout: !e`.
fn negated_layout(e: &Expr) -> Option<Expr> {
    let ExprNode::Send { recv: None, method, args, .. } = &*e.node else { return None };
    if method.as_str() != "render" {
        return None;
    }
    let ExprNode::Hash { entries, .. } = &*args.last()?.node else { return None };
    entries.iter().find_map(|(k, v)| match (&*k.node, &*v.node) {
        (ExprNode::Lit { value: Literal::Sym { value } }, ExprNode::Send { recv: Some(inner), method, args, block: None, .. })
            if value.as_str() == "layout" && method.as_str() == "!" && args.is_empty() =>
        {
            Some(inner.clone())
        }
        _ => None,
    })
}

fn set_layout(render: &mut Expr, layout: bool) {
    let ExprNode::Send { args, .. } = &mut *render.node else { return };
    let Some(last) = args.last_mut() else { return };
    let ExprNode::Hash { entries, .. } = &mut *last.node else { return };
    for (k, v) in entries.iter_mut() {
        if matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "layout") {
            *v = Expr::new(v.span, ExprNode::Lit { value: Literal::Bool { value: layout } });
        }
    }
}
