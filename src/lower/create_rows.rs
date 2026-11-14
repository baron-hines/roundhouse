//! `create` / `create!` of an Array of attribute Hashes → one create per
//! Hash, answering the Array of records:
//!
//!   Model.create!(rows)          ->  rows.map { |__attrs| Model.create!(__attrs) }
//!   relation.create(rows)        ->  rows.map { |__attrs| relation.create(__attrs) }
//!
//! That is Rails' own definition (Persistence::ClassMethods#create! and
//! Relation#create!: `attributes.collect { |attr| create!(attr, &block) }`),
//! so each row is created under the same scope a single Hash would be. A
//! class method that builds its attribute Hashes with `map` hands them to
//! `create!` whole; inside a relation-taking class method the per-row
//! `create!(__attrs)` then takes the relation's scope attributes like any
//! other (`scope_chain::merge_scope_attributes`).
//!
//! Written here rather than as an Array branch in the runtime's `create`
//! because the strict targets type that parameter as the attributes Hash.
//! The analyzer types the call as `Array[Model]`
//! (`analyze::creates_rows`); this pass only fires where it did,
//! so an app's own `def self.create!` keeps its call. The block form is
//! `create_block`'s and stays put.

use crate::app::App;
use crate::expr::{Expr, ExprNode};
use crate::ident::{Symbol, VarId};
use crate::lower::typing::{fn_sig, with_ty};
use crate::ty::Ty;

pub fn apply_create_rows_lowering(app: &mut App) {
    let defines: Vec<_> = app
        .models
        .iter()
        .flat_map(|m| ["create", "create!"].map(|name| (m.name.clone(), name)))
        .filter(|(model, name)| crate::analyze::source_class_method(app, model, name))
        .collect();
    let mut walk = |body: &mut Expr| rewrite(body, &defines);
    super::for_each_hook_body(app, &mut walk);
    for view in &mut app.views {
        walk(&mut view.body);
    }
}

fn rewrite(expr: &mut Expr, defines: &[(crate::ident::ClassId, &str)]) {
    expr.node.for_each_child_mut(&mut |c| rewrite(c, defines));
    let ExprNode::Send { recv, method, args, block: None, .. } = &*expr.node else { return };
    if !crate::analyze::creates_rows(method, args) {
        return;
    }
    let Some(Ty::Array { elem }) = &expr.ty else { return };
    let Ty::Class { id: model, .. } = &**elem else { return };
    // An app's own class-side `create` answers what it answers; a
    // relation's `create` is Relation#create whatever the model defines.
    let on_relation = matches!(recv.as_ref().and_then(|r| r.ty.as_ref()), Some(Ty::Relation { .. }));
    if !on_relation && defines.iter().any(|(m, name)| m == model && *name == method.as_str()) {
        return;
    }
    let Some(Ty::Array { elem: row_ty }) = args[0].ty.clone() else { return };
    let record_ty = (**elem).clone();
    let span = expr.span;
    let attrs = Symbol::from("__attrs");
    let row = with_ty(
        Expr::new(span, ExprNode::Var { id: VarId(0), name: attrs.clone() }),
        (*row_ty).clone(),
    );
    let create_one = with_ty(
        Expr::new(
            span,
            ExprNode::Send {
                recv: recv.clone(),
                method: method.clone(),
                args: vec![row],
                block: None,
                parenthesized: true,
            },
        ),
        record_ty.clone(),
    );
    let block = with_ty(
        Expr::new(
            span,
            ExprNode::Lambda {
                params: vec![attrs.clone()],
                rest_param: None,
                extra_params: Vec::new(),
                block_param: None,
                body: create_one,
                // Braces: the call is often an argument (`check(made.map { … })`).
                block_style: crate::expr::BlockStyle::Brace,
            },
        ),
        fn_sig(vec![(attrs, (*row_ty).clone())], record_ty),
    );
    let rows = args[0].clone();
    let ty = expr.ty.clone();
    *expr = Expr::new(
        span,
        ExprNode::Send {
            recv: Some(rows),
            method: Symbol::from("map"),
            args: vec![],
            block: Some(block),
            parenthesized: false,
        },
    );
    expr.ty = ty;
}
