//! `params.expect(:id)` handed to a String column's condition stays the
//! String Rails' `expect` answers. The controller lowering reads a
//! one-symbol `expect` as a record id (`@params.fetch("id", "0").to_s
//! .to_i`), which is right for `find(params.expect(:id))` and wrong for
//! lobsters' `ModMail.find_by(short_id: params.expect(:id))`: the short
//! id became 0 and every mod mail page found nothing.

use crate::app::App;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;
use crate::ty::Ty;

const CONDITION_METHODS: &[&str] = &["find_by", "find_by!", "find_sole_by", "where", "exists?"];

pub fn apply_expect_string_keys(app: &mut App) {
    let mut string_columns: crate::det::HashMap<String, crate::det::HashSet<String>> = Default::default();
    for model in &app.models {
        let Some(table) = app.schema.tables.get(&model.table.0) else { continue };
        let cols = table
            .columns
            .iter()
            .filter(|c| matches!(c.col_type, crate::schema::ColumnType::String { .. } | crate::schema::ColumnType::Text))
            .map(|c| c.name.as_str().to_string())
            .collect();
        string_columns.insert(model.name.0.as_str().to_string(), cols);
    }
    if string_columns.is_empty() {
        return;
    }
    for controller in &mut app.controllers {
        for action in controller.actions_mut() {
            rewrite(&mut action.body, &string_columns);
        }
    }
}

fn rewrite(expr: &mut Expr, string_columns: &crate::det::HashMap<String, crate::det::HashSet<String>>) {
    expr.node.for_each_child_mut(&mut |c| rewrite(c, string_columns));
    let ExprNode::Send { recv: Some(recv), method, args, .. } = &mut *expr.node else { return };
    if !CONDITION_METHODS.contains(&method.as_str()) {
        return;
    }
    let model = match &*recv.node {
        ExprNode::Const { path } => Some(path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("::")),
        _ => recv.ty.as_ref().and_then(|ty| record_model(ty, string_columns)),
    };
    let Some(columns) = model.and_then(|m| string_columns.get(&m)) else { return };
    let [arg] = args.as_mut_slice() else { return };
    let ExprNode::Hash { entries, .. } = &mut *arg.node else { return };
    for (k, v) in entries.iter_mut() {
        let ExprNode::Lit { value: Literal::Sym { value: key } } = &*k.node else { continue };
        if !columns.contains(key.as_str()) {
            continue;
        }
        if let Some(name) = expected_key(v) {
            *v = param_string(&name, v.span);
        }
    }
}

/// The model a receiver's type queries: the model itself, or the element
/// of a relation or association collection over it.
fn record_model(ty: &Ty, models: &crate::det::HashMap<String, crate::det::HashSet<String>>) -> Option<String> {
    match ty.peel_nilable() {
        Ty::Class { id, args } => {
            if models.contains_key(id.0.as_str()) {
                return Some(id.0.as_str().to_string());
            }
            args.iter().find_map(|a| record_model(a, models))
        }
        Ty::Array { elem } => record_model(elem, models),
        Ty::Relation { of } => models.contains_key(of.0.as_str()).then(|| of.0.as_str().to_string()),
        _ => None,
    }
}

/// The key of a bare `params.expect(:key)`.
fn expected_key(v: &Expr) -> Option<Symbol> {
    let ExprNode::Send { recv: Some(recv), method, args, block: None, .. } = &*v.node else { return None };
    if method.as_str() != "expect" {
        return None;
    }
    let ExprNode::Send { recv: None, method: params, args: none, block: None, .. } = &*recv.node else { return None };
    if params.as_str() != "params" || !none.is_empty() {
        return None;
    }
    let [arg] = args.as_slice() else { return None };
    match &*arg.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(value.clone()),
        _ => None,
    }
}

/// `params.fetch("key", "").to_s`, which the controller lowering reads off `@params`.
fn param_string(key: &Symbol, span: crate::span::Span) -> Expr {
    let str_lit = |s: &str| {
        let mut e = Expr::new(span, ExprNode::Lit { value: Literal::Str { value: s.to_string() } });
        e.ty = Some(Ty::Str);
        e
    };
    let fetched = Expr::new(
        span,
        ExprNode::Send {
            // Not the `@params` ivar itself: the controller lowering turns a bare `params` into it, and an ivar written here is one no analysis typed.
            recv: Some(Expr::new(
                span,
                ExprNode::Send { recv: None, method: Symbol::from("params"), args: vec![], block: None, parenthesized: false },
            )),
            method: Symbol::from("fetch"),
            args: vec![str_lit(key.as_str()), str_lit("")],
            block: None,
            parenthesized: true,
        },
    );
    let mut out = Expr::new(
        span,
        ExprNode::Send { recv: Some(fetched), method: Symbol::from("to_s"), args: vec![], block: None, parenthesized: false },
    );
    out.ty = Some(Ty::Str);
    out
}
