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
    let Some((call, _)) = machine_call(model) else { return Vec::new() };
    // A method the model writes itself is its own, whatever the block would define.
    let own: Vec<&Symbol> = model.methods().filter(|m| m.receiver == crate::dialect::MethodReceiver::Instance).map(|m| &m.name).collect();
    machine_names(call).unwrap_or_default().into_iter().map(|n| Symbol::from(n.as_str())).filter(|n| !own.contains(&n))
        .map(|n| (n, Ty::Bool)).collect()
}

/// The scope aasm creates per state (`Account.active`, `family.accounts.active`), each the model's relation: the
/// gem calls `scope` with it as the class loads, which the ruby family's runtime defines on the class and its
/// relation (`active_record_runtime_macros`). Not with `create_scopes: false`, nor over a class method or scope the
/// model has of the name, which aasm leaves alone.
pub fn scope_names(model: &Model) -> Vec<Symbol> {
    let Some((call, scopes)) = machine_call(model) else { return Vec::new() };
    if !scopes || machine_names(call).is_none() {
        return Vec::new();
    }
    let own: Vec<&Symbol> = model.methods().filter(|m| m.receiver == crate::dialect::MethodReceiver::Class).map(|m| &m.name)
        .chain(model.scopes().map(|s| &s.name)).collect();
    states(call).into_iter().map(|n| Symbol::from(n.as_str())).filter(|n| !own.contains(&n)).collect()
}

/// The model's one `aasm` call, and whether it creates scopes.
fn machine_call(model: &Model) -> Option<(&Expr, bool)> {
    if !super::model_includes(model).iter().any(|id| id.0.as_str() == "AASM") {
        return None;
    }
    let calls: Vec<&Expr> = model.body.iter().filter_map(|item| match item {
        ModelBodyItem::Unknown { expr, .. } if matches!(&*expr.node, ExprNode::Send { recv, method, .. }
            if method.as_str() == "aasm" && recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef))) => Some(expr),
        _ => None,
    }).collect();
    // A second `aasm` call reopens the machine with what the first configured (a namespace, say): not read alone.
    let [call] = calls.as_slice() else { return None };
    let ExprNode::Send { args, .. } = &*call.node else { return None };
    let scopes = !args.iter().any(|a| matches!(&*a.node, ExprNode::Hash { entries, .. } if entries.iter().any(|(k, v)|
        matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "create_scopes")
            && !matches!(&*v.node, ExprNode::Lit { value: Literal::Bool { value: true } }))));
    Some((*call, scopes))
}

/// The states the block declares, in order.
fn states(expr: &Expr) -> Vec<String> {
    let ExprNode::Send { block: Some(block), .. } = &*expr.node else { return Vec::new() };
    let ExprNode::Lambda { body, .. } = &*block.node else { return Vec::new() };
    let statements: Vec<&Expr> = match &*body.node {
        ExprNode::Seq { exprs } => exprs.iter().collect(),
        _ => vec![body],
    };
    statements.into_iter().filter_map(|s| match &*s.node {
        ExprNode::Send { recv: None, method, args, block: None, .. } if method.as_str() == "state" => Some(symbols(args)),
        _ => None,
    }).flatten().collect()
}

fn machine_names(expr: &Expr) -> Option<Vec<String>> {
    let ExprNode::Send { method, args, block: Some(block), .. } = &*expr.node else { return None };
    if method.as_str() != "aasm" {
        return None;
    }
    // A named machine (`aasm(:review)`) or a namespace renames what it defines; only the default one is read.
    if args.iter().any(|a| !matches!(&*a.node, ExprNode::Hash { .. })) {
        return None;
    }
    if let Some(ExprNode::Hash { entries, .. }) = args.first().map(|a| &*a.node)
        // `with_klass:` builds the machine with another DSL, whose methods are not these.
        && entries.iter().any(|(k, _)| matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if matches!(value.as_str(), "namespace" | "with_klass")))
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
            // A machine-wide callback (`after_all_transitions :handle_transition`) names no method.
            "after_all_transitions" | "before_all_transactions" | "after_all_transactions" | "before_all_events"
            | "after_all_events" | "ensure_on_all_events" | "error_on_all_events" | "after_all_commits"
                if block.is_none() && symbols(args).len() == args.len() => {}
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
