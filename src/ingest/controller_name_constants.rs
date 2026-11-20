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
    // Either side: Rails' instance `controller_name` reads the class's, so a `def self.controller_name` changes it too.
    let defines_name = |c: &crate::dialect::Controller| c.body.iter().any(|item| match item {
        crate::dialect::ControllerBodyItem::Action { action, .. } => action.name.as_str() == "controller_name",
        crate::dialect::ControllerBodyItem::ClassMethod { method, .. } => method.name.as_str() == "controller_name",
        _ => false,
    });
    let by_name: crate::det::HashMap<ClassId, (Option<ClassId>, bool)> =
        app.controllers.iter().map(|c| (c.name.clone(), (c.parent.clone(), defines_name(c)))).collect();
    // Not a controller that defines `controller_name`, or inherits one: the call reads that, not the name.
    let renamed = |name: &ClassId| {
        let mut cur = Some(name.clone());
        let mut seen = crate::det::HashSet::default();
        while let Some(c) = cur {
            if !seen.insert(c.clone()) {
                break;
            }
            let Some((parent, defines)) = by_name.get(&c) else { break };
            if *defines {
                return true;
            }
            cur = parent.clone();
        }
        false
    };
    let renamed: crate::det::HashSet<ClassId> = app.controllers.iter().map(|c| c.name.clone()).filter(|n| renamed(n)).collect();
    for controller in &mut app.controllers {
        // Not a controller others inherit from: the body runs as each subclass, whose names differ.
        if parents.contains(&controller.name) || renamed.contains(&controller.name) {
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
        // Not relative: Rails' `constantize` reads from the top level, and the controller's namespace may hold the name too.
        let path = std::iter::once(Symbol::from("")).chain(model.split("::").map(Symbol::from)).collect();
        let mut read = Expr::new(crate::span::Span::synthetic(), ExprNode::Const { path });
        read.decisions |= crate::expr::GENERATED_CONST_REF;
        *e = read;
        return;
    }
    // Not into a lambda literal: whatever later runs it (`instance_exec(&callback)`) gives it its `self`.
    if matches!(&*e.node, ExprNode::Lambda { .. }) {
        return;
    }
    if let ExprNode::Send { block: Some(block), method, .. } = &mut *e.node
        && !matches!(method.as_str(), "instance_eval" | "instance_exec" | "class_eval" | "class_exec" | "module_eval" | "module_exec")
        && let ExprNode::Lambda { body, .. } = &mut *block.node
    {
        rewrite(body, model);
    }
    // Not into a block another `self` runs (`other.instance_exec { controller_name … }`): it names that one's model.
    if let ExprNode::Send { recv, method, args, block: Some(_), .. } = &mut *e.node
        && matches!(method.as_str(), "instance_eval" | "instance_exec" | "class_eval" | "class_exec" | "module_eval" | "module_exec")
    {
        recv.iter_mut().chain(args.iter_mut()).for_each(|c| rewrite(c, model));
        return;
    }
    e.node.for_each_child_mut(&mut |c| rewrite(c, model));
}
