//! The instance methods aasm defines as a model's `aasm do … end` block runs (`state :draft` → `draft?`,
//! `event :publish` → `publish`, `publish!`, `publish_without_validation!`, `may_publish?`), typed for the analysis.
//!
//! aasm defines them with `klass.send(:define_method, …)` on a class it holds in a variable, which the analysis does
//! not follow, so a call to one was a dispatch error though the gem, compiled with the app, defines it as the class
//! loads. The names follow the attribution rules (`attribution::generated_methods::state_machines::aasm`), here
//! read from the ingested call; a block they do not read whole defines nothing here.

use crate::dialect::{Model, ModelBodyItem};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;
use crate::ty::Ty;

/// The model's aasm instance methods and their types: each a Bool, as aasm's predicates, `may_*?` and event
/// methods answer (an event method answers `true`, or `false` when a guard stops it; the bang raises instead).
pub fn instance_methods(model: &Model) -> Vec<(Symbol, Ty)> {
    if !super::model_includes(model).iter().any(|id| id.0.as_str() == "AASM") {
        return Vec::new();
    }
    let calls: Vec<&Expr> = model.body.iter().filter_map(|item| match item {
        ModelBodyItem::Unknown { expr, .. } if matches!(&*expr.node, ExprNode::Send { recv: None, method, .. } if method.as_str() == "aasm") => Some(expr),
        _ => None,
    }).collect();
    // A second `aasm` call reopens the machine with what the first configured (a namespace, say): not read alone.
    let [call] = calls.as_slice() else { return Vec::new() };
    machine_names(call).unwrap_or_default().into_iter().map(|n| (Symbol::from(n.as_str()), Ty::Bool)).collect()
}

fn machine_names(expr: &Expr) -> Option<Vec<String>> {
    let ExprNode::Send { recv: None, method, args, block: Some(block), .. } = &*expr.node else { return None };
    if method.as_str() != "aasm" {
        return None;
    }
    // A named machine (`aasm(:review)`) or a namespace renames what it defines; only the default one is read.
    if args.iter().any(|a| !matches!(&*a.node, ExprNode::Hash { .. })) {
        return None;
    }
    if let Some(ExprNode::Hash { entries, .. }) = args.first().map(|a| &*a.node)
        && entries.iter().any(|(k, _)| matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "namespace"))
    {
        return None;
    }
    let ExprNode::Lambda { body, .. } = &*block.node else { return None };
    let statements: Vec<&Expr> = match &*body.node {
        ExprNode::Seq { exprs } => exprs.iter().collect(),
        _ => vec![body],
    };
    let mut names = Vec::new();
    for statement in statements {
        let ExprNode::Send { recv: None, method, args, block, .. } = &*statement.node else { return None };
        match method.as_str() {
            "state" if block.is_none() => names.extend(symbols(args).into_iter().map(|s| format!("{s}?"))),
            "event" => {
                let event = symbols(args).into_iter().next()?;
                if let Some(block) = block {
                    let ExprNode::Lambda { body, .. } = &*block.node else { return None };
                    let inner: Vec<&Expr> = match &*body.node {
                        ExprNode::Seq { exprs } => exprs.iter().collect(),
                        _ => vec![body],
                    };
                    // Only literal transitions: other statements in the event body may reconfigure the machine.
                    for s in inner {
                        let ExprNode::Send { recv: None, method, block: None, .. } = &*s.node else { return None };
                        if method.as_str() != "transitions" {
                            return None;
                        }
                    }
                }
                names.extend([format!("{event}!"), format!("{event}_without_validation!"), format!("may_{event}?"), event]);
            }
            _ => return None,
        }
    }
    Some(names)
}

fn symbols(args: &[Expr]) -> Vec<String> {
    args.iter()
        .filter_map(|a| match &*a.node {
            ExprNode::Lit { value: Literal::Sym { value } } => Some(value.as_str().to_string()),
            _ => None,
        })
        .collect()
}
