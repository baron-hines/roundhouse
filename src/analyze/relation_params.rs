//! Parameters an app method reads as a relation: `def apply_filter(scope, q)
//! = scope.where(…)`, directly or through a local it reassigns
//! (`query = scope; query = query.where(…) if q; query`).
//!
//! A has_many read handed to such a parameter is the CollectionProxy Rails
//! passes, a relation; the lowered read is the loaded Array, which answers no
//! `where`, so the argument is lowered to the association's relation and
//! typed as one. Where every value the method returns is the parameter or a
//! chain on it, the call answers the relation its argument is
//! (`EntrySearch.apply_search_filter(query, search)` is the caller's query).
//!
//! Keyed by method name and position: every app definition of the name
//! must agree, so a call resolves without its receiver's type.

use crate::app::App;
use crate::dialect::MethodDef;
use crate::expr::{Expr, ExprNode, LValue, Pattern};
use crate::ident::Symbol;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RelationParams {
    /// `(method, positional index)` a relation builder is called on.
    pub chained: crate::det::HashSet<(Symbol, usize)>,
    /// Of those, the ones every return value of the method is a chain on.
    pub returned: crate::det::HashSet<(Symbol, usize)>,
}

impl RelationParams {
    pub fn chains(&self, method: &Symbol, index: usize) -> bool {
        self.chained.contains(&(method.clone(), index))
    }

    pub fn returns(&self, method: &Symbol, index: usize) -> bool {
        self.returned.contains(&(method.clone(), index))
    }
}

pub fn relation_params(app: &App) -> RelationParams {
    let mut by_name: crate::det::HashMap<Symbol, Vec<&MethodDef>> = crate::det::HashMap::default();
    for def in app.models.iter().flat_map(|m| m.methods()).chain(app.library_classes.iter().flat_map(|c| c.methods.iter())) {
        by_name.entry(def.name.clone()).or_default().push(def);
    }
    for controller in &app.controllers {
        for def in controller.class_methods() {
            by_name.entry(def.name.clone()).or_default().push(def);
        }
        for action in controller.actions() {
            // An action's name taken by an app method makes the name ambiguous, not chained.
            if let Some(defs) = by_name.get_mut(&action.name) {
                defs.clear();
            } else {
                by_name.insert(action.name.clone(), Vec::new());
            }
        }
    }
    let mut out = RelationParams::default();
    for (name, defs) in by_name {
        let Some(first) = defs.first() else { continue };
        for (index, param) in first.params.iter().enumerate() {
            if !positional(param) {
                break;
            }
            let mut chained = true;
            let mut returned = true;
            for def in &defs {
                let Some(p) = def.params.get(index).filter(|p| positional(p)) else {
                    chained = false;
                    break;
                };
                let aliases = aliases_of(&def.body, &p.name);
                chained &= aliases.as_ref().is_some_and(|a| calls_builder(&def.body, a));
                returned &= aliases.as_ref().is_some_and(|a| all_returns_chain(&def.body, a));
            }
            if chained {
                out.chained.insert((name.clone(), index));
                if returned {
                    out.returned.insert((name.clone(), index));
                }
            }
        }
    }
    out
}

fn positional(param: &crate::dialect::Param) -> bool {
    !param.keyword && !param.name.as_str().is_empty() && param.default.is_none() && !is_rest(param)
}

fn is_rest(param: &crate::dialect::Param) -> bool {
    param.rest || param.forwarding || param.from_keyword || param.from_kwrest
}

/// The locals holding the parameter or a chain on it, when every write to each is one.
fn aliases_of(body: &Expr, param: &Symbol) -> Option<Vec<Symbol>> {
    let mut writes: Vec<(Symbol, &Expr)> = Vec::new();
    collect_writes(body, &mut writes);
    let mut aliases = vec![param.clone()];
    loop {
        let before = aliases.len();
        for (name, value) in &writes {
            if !aliases.contains(name) && chain_on(value, &aliases) {
                aliases.push(name.clone());
            }
        }
        if aliases.len() == before {
            break;
        }
    }
    let clean = writes.iter().all(|(name, value)| !aliases.contains(name) || chain_on(value, &aliases));
    clean.then_some(aliases)
}

fn collect_writes<'a>(expr: &'a Expr, out: &mut Vec<(Symbol, &'a Expr)>) {
    match &*expr.node {
        ExprNode::Assign { target: LValue::Var { name, .. }, value } => {
            out.push((name.clone(), value));
            collect_writes(value, out);
        }
        // A block or lambda body runs at another time, and may write the local more than once.
        ExprNode::Lambda { .. } => {}
        _ => expr.node.for_each_child(&mut |c| collect_writes(c, out)),
    }
}

fn chain_on(expr: &Expr, aliases: &[Symbol]) -> bool {
    match &*expr.node {
        ExprNode::Var { name, .. } => aliases.contains(name),
        ExprNode::Send { recv: Some(recv), method, block: None, .. } => {
            crate::catalog::is_relation_builder_name(method.as_str()) && chain_on(recv, aliases)
        }
        _ => false,
    }
}

fn calls_builder(expr: &Expr, aliases: &[Symbol]) -> bool {
    if let ExprNode::Send { recv: Some(recv), method, .. } = &*expr.node
        && crate::catalog::is_relation_builder_name(method.as_str())
        && chain_on(recv, aliases)
    {
        return true;
    }
    let mut found = false;
    expr.node.for_each_child(&mut |c| found |= calls_builder(c, aliases));
    found
}

fn all_returns_chain(body: &Expr, aliases: &[Symbol]) -> bool {
    let mut explicit = Vec::new();
    collect_returns(body, &mut explicit);
    explicit.iter().all(|v| tail_chains(v, aliases)) && tail_chains(body, aliases)
}

fn collect_returns<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
    match &*expr.node {
        ExprNode::Return { value } => out.push(value),
        ExprNode::Lambda { .. } => {}
        _ => expr.node.for_each_child(&mut |c| collect_returns(c, out)),
    }
}

fn tail_chains(expr: &Expr, aliases: &[Symbol]) -> bool {
    match &*expr.node {
        ExprNode::Seq { exprs } => exprs.last().is_some_and(|e| tail_chains(e, aliases)),
        ExprNode::If { then_branch, else_branch, .. } => tail_chains(then_branch, aliases) && tail_chains(else_branch, aliases),
        ExprNode::Case { arms, .. } => {
            arms.iter().any(|a| matches!(a.pattern, Pattern::Wildcard)) && arms.iter().all(|a| tail_chains(&a.body, aliases))
        }
        // Its value was checked with the other explicit returns.
        ExprNode::Return { .. } => true,
        ExprNode::Assign { target: LValue::Var { name, .. }, value } => aliases.contains(name) && chain_on(value, aliases),
        _ => chain_on(expr, aliases),
    }
}
