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
    let mut actions = crate::det::HashSet::default();
    for controller in &app.controllers {
        for def in controller.class_methods() {
            by_name.entry(def.name.clone()).or_default().push(def);
        }
        actions.extend(controller.actions().map(|a| a.name.clone()));
    }
    // An action's name taken by an app method makes the name ambiguous, not chained.
    by_name.retain(|name, _| !actions.contains(name));
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
    let mut opaque = Vec::new();
    collect_writes(body, false, &mut writes, &mut opaque);
    // Only a local the body writes unconditionally, before any statement reads it, holds a value wherever it is read.
    let mut top: Vec<&Symbol> = Vec::new();
    if let ExprNode::Seq { exprs } = &*body.node {
        for (i, e) in exprs.iter().enumerate() {
            if let ExprNode::Assign { target: LValue::Var { name, .. }, value } = &*e.node
                && !top.contains(&name)
                && !reads(value, name)
                && !exprs[..i].iter().any(|before| reads(before, name))
            {
                top.push(name);
            }
        }
    }
    let mut aliases = vec![param.clone()];
    loop {
        let before = aliases.len();
        for (name, value) in &writes {
            if !aliases.contains(name) && top.contains(&name) && chain_on(value, &aliases) {
                aliases.push(name.clone());
            }
        }
        if aliases.len() == before {
            break;
        }
    }
    let clean = writes.iter().all(|(name, value)| !aliases.contains(name) || chain_on(value, &aliases))
        && !opaque.iter().any(|name| aliases.contains(name));
    clean.then_some(aliases)
}

/// `opaque` collects the locals written where the order of writes is not the body's: in a block, which may run any
/// number of times, or by a multiple assignment.
fn collect_writes<'a>(expr: &'a Expr, in_block: bool, out: &mut Vec<(Symbol, &'a Expr)>, opaque: &mut Vec<Symbol>) {
    match &*expr.node {
        ExprNode::Assign { target: LValue::Var { name, .. }, value } => {
            if in_block {
                opaque.push(name.clone());
            } else {
                out.push((name.clone(), value));
            }
            collect_writes(value, in_block, out, opaque);
        }
        ExprNode::OpAssign { target: LValue::Var { name, .. }, value, .. } => {
            opaque.push(name.clone());
            collect_writes(value, in_block, out, opaque);
        }
        ExprNode::MultiAssign { targets, value } => {
            opaque.extend(targets.iter().filter_map(|t| match t {
                LValue::Var { name, .. } => Some(name.clone()),
                _ => None,
            }));
            collect_writes(value, in_block, out, opaque);
        }
        ExprNode::Lambda { .. } => expr.node.for_each_child(&mut |c| collect_writes(c, true, out, opaque)),
        _ => expr.node.for_each_child(&mut |c| collect_writes(c, in_block, out, opaque)),
    }
}

fn reads(expr: &Expr, local: &Symbol) -> bool {
    if matches!(&*expr.node, ExprNode::Var { name, .. } if name == local) {
        return true;
    }
    let mut found = false;
    expr.node.for_each_child(&mut |c| found = found || reads(c, local));
    found
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

/// The names a block binds itself: inside it they are the block's, not the method's.
fn block_names(expr: &Expr) -> Vec<Symbol> {
    match &*expr.node {
        ExprNode::Lambda { params, rest_param, extra_params, block_param, .. } => params.iter()
            .chain(rest_param.iter()).chain(block_param.iter()).chain(extra_params.iter().map(|p| &p.name))
            .cloned().collect(),
        _ => Vec::new(),
    }
}

fn calls_builder(expr: &Expr, aliases: &[Symbol]) -> bool {
    let bound = block_names(expr);
    if bound.iter().any(|b| aliases.contains(b)) {
        let outer: Vec<Symbol> = aliases.iter().filter(|a| !bound.contains(a)).cloned().collect();
        let mut found = false;
        expr.node.for_each_child(&mut |c| found |= calls_builder(c, &outer));
        return found;
    }
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
    let mut in_block = false;
    collect_returns(body, false, &mut explicit, &mut in_block);
    !in_block && explicit.iter().all(|v| tail_chains(v, aliases)) && tail_chains(body, aliases)
}

/// `in_block` is set by a `return` inside a block, which may return from the method with a value the block computes.
fn collect_returns<'a>(expr: &'a Expr, nested: bool, out: &mut Vec<&'a Expr>, in_block: &mut bool) {
    match &*expr.node {
        ExprNode::Return { .. } if nested => *in_block = true,
        ExprNode::Return { value } => out.push(value),
        ExprNode::Lambda { .. } => expr.node.for_each_child(&mut |c| collect_returns(c, true, out, in_block)),
        _ => expr.node.for_each_child(&mut |c| collect_returns(c, nested, out, in_block)),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn params_of(src: &str) -> RelationParams {
        let mut app = App::default();
        app.library_classes = crate::ingest::ingest_library_classes(src.as_bytes(), "s.rb").expect("ingest");
        relation_params(&app)
    }

    #[test]
    fn a_filter_that_returns_its_scope_or_a_query_on_it() {
        let got = params_of("class S\n  def self.f(scope, q)\n    return scope if q.blank?\n\n    query = scope\n    query = query.where(a: q) if q\n    query\n  end\nend\n");
        assert!(got.chains(&Symbol::from("f"), 0) && got.returns(&Symbol::from("f"), 0), "{got:?}");
    }

    #[test]
    fn a_local_first_written_in_a_branch_or_a_block_is_no_alias() {
        let branch = params_of("class S\n  def self.f(scope, q)\n    query = scope if q\n    query.where(a: 1)\n  end\nend\n");
        assert!(!branch.chains(&Symbol::from("f"), 0), "{branch:?}");
        let block = params_of("class S\n  def self.f(scope, q)\n    query = scope\n    q.each { |x| query = x }\n    query.where(a: 1)\n  end\nend\n");
        assert!(!block.chains(&Symbol::from("f"), 0), "{block:?}");
        let multi = params_of("class S\n  def self.f(scope, q)\n    query = scope\n    query, other = q\n    query.where(a: 1)\n  end\nend\n");
        assert!(!multi.chains(&Symbol::from("f"), 0), "{multi:?}");
    }

    #[test]
    fn a_local_read_before_its_write_or_compound_assigned_is_no_alias() {
        let early = params_of("class S\n  def self.f(scope, query, early)\n    return query if early\n    query = scope\n    query.where(a: 1)\n  end\nend\n");
        assert!(!early.returns(&Symbol::from("f"), 0), "{early:?}");
        let compound = params_of("class S\n  def self.f(scope)\n    scope.where(a: 1)\n    scope &&= []\n    scope\n  end\nend\n");
        assert!(!compound.chains(&Symbol::from("f"), 0), "{compound:?}");
    }

    #[test]
    fn a_block_parameter_of_the_same_name_is_the_blocks() {
        let got = params_of("class S\n  def self.f(children)\n    [1].each { |children| children.order(:id) }\n    children\n  end\nend\n");
        assert!(!got.chains(&Symbol::from("f"), 0), "{got:?}");
        let outer = params_of("class S\n  def self.f(scope)\n    query = scope\n    [1].each { |scope| query.where(a: 1) }\n    query\n  end\nend\n");
        assert!(outer.chains(&Symbol::from("f"), 0), "{outer:?}");
    }

    #[test]
    fn a_return_from_a_block_is_not_the_scope() {
        let got = params_of("class S\n  def self.f(scope, q)\n    q.each { |x| return x }\n    scope.where(a: 1)\n  end\nend\n");
        assert!(got.chains(&Symbol::from("f"), 0) && !got.returns(&Symbol::from("f"), 0), "{got:?}");
    }
}
