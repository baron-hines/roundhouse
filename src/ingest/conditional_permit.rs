// Not left to the params lowering as written: it reads a permit list of literal keys only, so one gated key (`(:mod_note if moderator)`) left the whole list a raw Hash call.
use crate::app::App;
use crate::expr::{BoolOpKind, BoolOpSurface, Expr, ExprNode, Literal};
use crate::ident::Symbol;

pub(super) fn apply(app: &mut App) {
    for controller in &mut app.controllers {
        for action in controller.actions_mut() {
            crate::lower::controller_to_library::util::map_expr_mut(&mut action.body, &rewrite);
        }
    }
}

fn gated_key(e: &Expr) -> Option<(Expr, Expr)> {
    let ExprNode::If { cond, then_branch, else_branch } = &*e.node else { return None };
    let is_sym = |x: &Expr| matches!(&*x.node, ExprNode::Lit { value: Literal::Sym { .. } });
    let is_nil = |x: &Expr| matches!(&*x.node, ExprNode::Lit { value: Literal::Nil });
    if is_sym(then_branch) && is_nil(else_branch) {
        return Some((cond.clone(), then_branch.clone()));
    }
    None
}

fn rewrite(e: &Expr) -> Option<Expr> {
    let ExprNode::Send { recv: Some(recv), method, args, block: None, parenthesized } = &*e.node else {
        return None;
    };
    if method.as_str() != "permit" || !args.iter().any(|a| gated_key(a).is_some()) {
        return None;
    }
    let sym_name = |x: &Expr| match &*x.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(value.clone()),
        _ => None,
    };
    let mut keys: Vec<Expr> = Vec::new();
    let mut always: Vec<Symbol> = Vec::new();
    let mut gated: Vec<(Symbol, Expr, Expr)> = Vec::new();
    for a in args {
        match gated_key(a) {
            Some((cond, key)) => {
                let name = sym_name(&key).expect("gated key is a Symbol");
                if let Some(entry) = gated.iter_mut().find(|(n, _, _)| *n == name) {
                    entry.2 = Expr::new(e.span, ExprNode::BoolOp {
                        op: BoolOpKind::Or,
                        surface: BoolOpSurface::Symbol,
                        left: entry.2.clone(),
                        right: cond,
                    });
                } else {
                    keys.push(key.clone());
                    gated.push((name, key, cond));
                }
            }
            None => {
                if let Some(name) = sym_name(a) {
                    always.push(name);
                }
                keys.push(a.clone());
            }
        }
    }
    let permitted = Expr::new(e.span, ExprNode::Send {
        recv: Some(recv.clone()),
        method: method.clone(),
        args: keys,
        block: None,
        parenthesized: *parenthesized,
    });
    // Not one branch per gate: each would hold the whole permit again, doubling the tree per gated key.
    let dropped: Vec<Expr> = gated
        .into_iter()
        .filter(|(name, _, _)| !always.contains(name))
        .map(|(_, key, cond)| {
            let kept = Expr::new(e.span, ExprNode::Lit { value: Literal::Sym { value: Symbol::from("__rh_permitted") } });
            Expr::new(e.span, ExprNode::If { cond, then_branch: kept, else_branch: key })
        })
        .collect();
    if dropped.is_empty() {
        return Some(permitted);
    }
    Some(Expr::new(e.span, ExprNode::Send {
        recv: Some(permitted),
        method: Symbol::from("except"),
        args: dropped,
        block: None,
        parenthesized: true,
    }))
}
