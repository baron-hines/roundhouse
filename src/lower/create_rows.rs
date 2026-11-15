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
//! so an app's own `def self.create!` keeps its call, and so does an app
//! object's (a factory module) whose `create!` answers an Array: only a
//! relation's or a model class's call is Rails'. A block goes to
//! each row's `create!`, as Rails passes `&block` on; `create_block`,
//! which runs next, then inlines it per row.

use crate::app::App;
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol, VarId};
use crate::lower::typing::{fn_sig, with_ty};
use crate::ty::Ty;

pub fn apply_create_rows_lowering(app: &mut App) {
    let defines: Vec<_> = app
        .models
        .iter()
        .flat_map(|m| ["create", "create!"].map(|name| (m.name.clone(), name)))
        .filter(|(model, name)| crate::analyze::source_class_method(app, model, name))
        .collect();
    let models: Vec<ClassId> = app.models.iter().map(|m| m.name.clone()).collect();
    super::for_each_owned_hook_body(app, &mut |owner, body| {
        let mut bound = 0;
        rewrite(body, owner, &models, &defines, &mut bound);
    });
    for view in &mut app.views {
        let mut bound = 0;
        rewrite(&mut view.body, None, &models, &defines, &mut bound);
    }
}

fn rewrite(
    expr: &mut Expr,
    owner: Option<&ClassId>,
    models: &[ClassId],
    defines: &[(ClassId, &str)],
    bound: &mut u32,
) {
    expr.node.for_each_child_mut(&mut |c| rewrite(c, owner, models, defines, bound));
    let ExprNode::Send { recv, method, args, block, .. } = &*expr.node else { return };
    if !crate::analyze::creates_rows(method, args) {
        return;
    }
    let Some(Ty::Array { elem }) = &expr.ty else { return };
    let Ty::Class { id: model, .. } = &**elem else { return };
    // A relation's `create` is Relation#create whatever the model
    // defines; a model class's is Rails' unless the app defines its own.
    let on_relation = matches!(recv.as_ref().and_then(|r| r.ty.as_ref()), Some(Ty::Relation { .. }));
    let on_model = match recv.as_ref().map(|r| (&*r.node, r.ty.as_ref())) {
        // The implicit (or `self`) receiver of one of the model's class methods.
        None | Some((ExprNode::SelfRef, _)) => owner == Some(model),
        Some((ExprNode::Const { .. }, Some(Ty::Class { id, .. }))) => id == model,
        _ => false,
    };
    if !on_relation && !(on_model && models.contains(model)) {
        return;
    }
    if !on_relation && defines.iter().any(|(m, name)| m == model && *name == method.as_str()) {
        return;
    }
    let Some(Ty::Array { elem: row_ty }) = args[0].ty.clone() else { return };
    let record_ty = (**elem).clone();
    let span = expr.span;
    // Rails reads the receiver once, before the rows, and for an empty
    // Array too, so it is bound first and the binding read per row. A
    // receiver that is more than a read (a `where` chain whose arguments
    // call out) has to be bound for that to hold. So does a local: the
    // rows argument can assign it (`scope.create!([(scope = other; { … })])`),
    // and Ruby has already read the receiver by then. A constant or `self`
    // needs none — the rows can point neither somewhere else.
    // Each binding takes its own name: a nested array-create inside these
    // rows (`where(…).create!([{ …, other.create!([…]) }])`) binds one too,
    // and a shared name left the outer rows created on the inner receiver.
    let (bind_recv, recv) = match recv {
        Some(r) if !matches!(&*r.node, ExprNode::Const { .. } | ExprNode::SelfRef) => {
            let name = match *bound {
                0 => Symbol::from("__creator"),
                n => Symbol::from(format!("__creator{n}").as_str()),
            };
            *bound += 1;
            let recv_ty = r.ty.clone().unwrap_or(Ty::Untyped);
            let assign = with_ty(
                Expr::new(
                    span,
                    ExprNode::Assign {
                        target: crate::expr::LValue::Var { id: VarId(0), name: name.clone() },
                        value: r.clone(),
                    },
                ),
                recv_ty.clone(),
            );
            let read = with_ty(Expr::new(span, ExprNode::Var { id: VarId(0), name }), recv_ty);
            (Some(assign), Some(read))
        }
        other => (None, other.clone()),
    };
    let attrs = Symbol::from("__attrs");
    let row = with_ty(
        Expr::new(span, ExprNode::Var { id: VarId(0), name: attrs.clone() }),
        (*row_ty).clone(),
    );
    let create_one = with_ty(
        Expr::new(
            span,
            ExprNode::Send {
                recv,
                method: method.clone(),
                args: vec![row],
                block: block.clone(),
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
    let map = Expr::new(
        span,
        ExprNode::Send {
            recv: Some(rows),
            method: Symbol::from("map"),
            args: vec![],
            block: Some(block),
            parenthesized: false,
        },
    );
    *expr = match bind_recv {
        Some(assign) => Expr::new(span, ExprNode::Seq { exprs: vec![assign, with_ty(map, ty.clone().unwrap_or(Ty::Untyped))] }),
        None => map,
    };
    expr.ty = ty;
}
