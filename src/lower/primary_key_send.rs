//! `record.send(record.class.primary_key)` (orm_adapter's `conditions_to_fields`) where every model's key is `id`:
//! `record.id`, the same method the send names, an override of `id` included. Only where the analyzer typed the
//! receiver a record: any other object's `class.primary_key` is its own method, of any answer.

use crate::app::App;
use crate::expr::{Expr, ExprNode};
use crate::ident::Symbol;
use crate::ty::Ty;

pub fn apply_primary_key_send(app: &mut App) {
    if !every_primary_key_is_id(app) {
        return;
    }
    let records: crate::det::HashSet<String> = app
        .models
        .iter()
        .map(|m| m.name.0.as_str().to_string())
        .chain(["ActiveRecord::Base".to_string(), "ApplicationRecord".to_string()])
        .collect();
    crate::lower::for_each_emit_body(app, &mut |body| rewrite(body, &records));
}

// Not `record[key]`: that reads the column past a reader the model overrides.
fn rewrite(e: &mut Expr, records: &crate::det::HashSet<String>) {
    e.node.for_each_child_mut(&mut |c| rewrite(c, records));
    let ExprNode::Send { recv: Some(recv), method, args, block: None, .. } = &mut *e.node else { return };
    if !matches!(method.as_str(), "send" | "public_send" | "__send__")
        || !matches!(&*recv.node, ExprNode::Var { .. } | ExprNode::Ivar { .. } | ExprNode::SelfRef)
        || !matches!(recv.ty.as_ref().map(Ty::peel_nilable), Some(Ty::Class { id, .. }) if records.contains(id.0.as_str()))
    {
        return;
    }
    let [name] = args.as_slice() else { return };
    // Only the record's own class's key: another class's (`KeySource.primary_key`) names any column.
    let ExprNode::Send { recv: Some(class), method: key, args: key_args, block: None, .. } = &*name.node else { return };
    if key.as_str() != "primary_key"
        || !key_args.is_empty()
        || !matches!(&*class.node, ExprNode::Send { recv: Some(of), method, args, block: None, .. }
            if method.as_str() == "class" && args.is_empty() && same_read(of, recv))
    {
        return;
    }
    *method = Symbol::from("id");
    args.clear();
}

fn same_read(a: &Expr, b: &Expr) -> bool {
    match (&*a.node, &*b.node) {
        (ExprNode::Var { name: x, .. }, ExprNode::Var { name: y, .. }) | (ExprNode::Ivar { name: x }, ExprNode::Ivar { name: y }) => x == y,
        (ExprNode::SelfRef, ExprNode::SelfRef) => true,
        _ => false,
    }
}

fn every_primary_key_is_id(app: &App) -> bool {
    // Nor where a class of the app answers `primary_key` itself: the send then names whatever that method answers.
    let own_primary_key = app.library_classes.iter().flat_map(|c| c.methods.iter()).any(|m| m.name.as_str() == "primary_key")
        || app.models.iter().any(|m| m.methods().any(|d| d.name.as_str() == "primary_key"));
    !own_primary_key
        && app.models.iter().all(|m| m.primary_key.as_ref().is_none_or(|k| k.as_str() == "id"))
        && app.schema.tables.values().all(|t| {
            t.primary_key_columns.is_empty() && t.columns.iter().filter(|c| c.primary_key).all(|c| c.name.as_str() == "id")
        })
}
