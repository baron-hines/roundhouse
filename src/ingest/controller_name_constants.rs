//! `controller_name.classify.constantize` in a controller's body (maybe's `AccountableResource#accountable_type`,
//! spliced into each of nine controllers): Rails names the controller's model, so the copy in a controller whose
//! name classifies to a class the app defines reads that constant.

use crate::App;
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol};

pub(super) fn fold(app: &mut App) {
    let defined: crate::det::HashSet<String> = app
        .models
        .iter()
        .map(|m| m.name.0.as_str().to_string())
        .chain(app.library_classes.iter().map(|c| c.name.0.as_str().to_string()))
        .collect();
    let parents: crate::det::HashSet<ClassId> = app.controllers.iter().filter_map(|c| c.parent.clone()).collect();
    for controller in &mut app.controllers {
        // Not a controller others inherit from: the body runs as each subclass, whose names differ.
        if parents.contains(&controller.name) {
            continue;
        }
        let own_name = controller.body.iter().any(|item| matches!(item,
            crate::dialect::ControllerBodyItem::Action { action, .. } if action.name.as_str() == "controller_name"));
        if own_name {
            continue;
        }
        let model = crate::naming::classify_path(&crate::analyze::controller_name_of(&controller.name));
        if !defined.contains(&model) {
            continue;
        }
        for item in &mut controller.body {
            if let crate::dialect::ControllerBodyItem::Action { action, .. } = item {
                rewrite(&mut action.body, &model);
            }
        }
    }
}

fn rewrite(e: &mut Expr, model: &str) {
    if let ExprNode::Send { recv: Some(classified), method, args, block: None, .. } = &*e.node
        && method.as_str() == "constantize"
        && args.is_empty()
        && let ExprNode::Send { recv: Some(name), method: classify, args: classify_args, block: None, .. } = &*classified.node
        && classify.as_str() == "classify"
        && classify_args.is_empty()
        && matches!(&*name.node, ExprNode::Send { recv: None, method, args, block: None, .. }
            if method.as_str() == "controller_name" && args.is_empty())
    {
        // Not the call's span: the source index resolves a constant at its position, and the concern's file names none.
        let mut read = Expr::new(crate::span::Span::synthetic(), ExprNode::Const { path: model.split("::").map(Symbol::from).collect() });
        read.decisions |= crate::expr::GENERATED_CONST_REF;
        *e = read;
        return;
    }
    // Not into a block another `self` runs (`other.instance_exec { controller_name … }`): it names that one's model.
    if matches!(&*e.node, ExprNode::Send { method, block: Some(_), .. }
        if matches!(method.as_str(), "instance_eval" | "instance_exec" | "class_eval" | "class_exec" | "module_eval" | "module_exec"))
    {
        return;
    }
    e.node.for_each_child_mut(&mut |c| rewrite(c, model));
}
