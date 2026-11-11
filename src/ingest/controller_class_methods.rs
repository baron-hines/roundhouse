//! The class methods of a Concern that a controller calls.
//!
//! ```ruby
//! class DeviseController < ApplicationController
//!   include Devise::Controllers::ScopedViews
//!   def _prefixes = self.class.scoped_views? ? scoped : super
//! end
//! ```
//!
//! Rails gets `scoped_views?` from the extend Concern runs, and the
//! emitted modules have no Concern, so the call needs the method on the
//! controller. Models and library classes get every class method
//! (`splice_concern_class_methods_into_includers`). A controller gets the
//! ones some controller calls, and those they call in turn: a copy is typed
//! as controller code, and most class methods here are class-body macros,
//! which `expand_class_body_macros` and `class_configuration` answer.
//!
//! An ivar only the unused writer would set can't be defined, so a
//! `defined?` guard on it becomes its fallback.
//!
//! Runs after those two passes' expansions, so what they already put on
//! the controller stays. The controller's own method wins, then the
//! include Ruby would reach first. A subclass inherits the copy.

use crate::App;
use crate::dialect::{ControllerBodyItem, MethodReceiver};
use crate::det::{HashMap, HashSet};
use crate::expr::{Expr, ExprNode, LValue};
use crate::ident::{ClassId, Symbol};

use super::app::{concern_class_method_catalog, concern_class_method_copy, controller_concern_surfaces};
use super::library_class::ConcernClassMethodSpans;

pub(super) fn splice(app: &mut App, carriers: &[ConcernClassMethodSpans]) {
    let catalog = concern_class_method_catalog(&app.library_classes, carriers);
    let mut wanted = HashSet::default();
    for item in app.controllers.iter().flat_map(|c| &c.body) {
        match item {
            ControllerBodyItem::Action { action, .. } => calls_on_the_class(&action.body, &mut wanted),
            ControllerBodyItem::ClassMethod { method, .. } => calls_on_self(&method.body, &mut wanted),
            _ => {}
        }
    }
    if wanted.is_empty() {
        return;
    }
    loop {
        let before = wanted.len();
        let mut called = HashSet::default();
        for method in catalog.values().flat_map(|(methods, _)| methods).filter(|m| wanted.contains(&m.name)) {
            calls_on_self(&method.body, &mut called);
        }
        wanted.extend(called);
        if wanted.len() == before {
            break;
        }
    }

    let mut written = HashSet::default();
    for method in catalog.values().flat_map(|(methods, _)| methods).filter(|m| wanted.contains(&m.name)) {
        writes_ivars(&method.body, &mut written);
    }
    for item in app.controllers.iter().flat_map(|c| &c.body) {
        match item {
            ControllerBodyItem::ClassMethod { method, .. } => writes_ivars(&method.body, &mut written),
            ControllerBodyItem::ClassIvarInit { expr, .. } | ControllerBodyItem::Unknown { expr, .. } => writes_ivars(expr, &mut written),
            _ => {}
        }
    }

    // The module keeps its own copy, unreachable but still typed.
    for lc in app.library_classes.iter_mut().filter(|lc| catalog.contains_key(&lc.name)) {
        for method in lc.methods.iter_mut().filter(|m| m.receiver == MethodReceiver::Class && wanted.contains(&m.name)) {
            fold_unwritten_ivars(&mut method.body, &written);
        }
    }

    let surfaces = controller_concern_surfaces(app);
    // A class reopened across files is one class: its own methods are all of them.
    let mut own: HashMap<ClassId, HashSet<Symbol>> = HashMap::default();
    for controller in &app.controllers {
        let names = own.entry(controller.name.clone()).or_default();
        names.extend(controller.class_methods().filter(|m| m.receiver == MethodReceiver::Class).map(|m| m.name.clone()));
    }
    let mut class_body: HashMap<ClassId, HashSet<Symbol>> = HashMap::default();
    for controller in &app.controllers {
        let calls = class_body.entry(controller.name.clone()).or_default();
        for item in &controller.body {
            if let ControllerBodyItem::ClassIvarInit { expr, .. } | ControllerBodyItem::Unknown { expr, .. } = item {
                calls_on_self(expr, calls);
            }
        }
    }
    let mut spliced: HashSet<ClassId> = HashSet::default();
    for controller in &mut app.controllers {
        if !spliced.insert(controller.name.clone()) {
            continue;
        }
        let surface = &surfaces.controllers[&controller.name];
        let taken = own.entry(controller.name.clone()).or_default();
        let mut added = Vec::new();
        // Registration order, so the last is what Ruby's ancestors reach first. An ancestor's Concern is the ancestor's copy.
        for concern in surface.includes.iter().rev().filter(|c| !surface.inherited_includes.contains(c)) {
            let Some((methods, consts)) = catalog.get(concern) else { continue };
            // A class-body call that is still here wasn't expanded, so a copied reader would answer without its effect. Left off, the read still fails loudly.
            if methods.iter().any(|m| class_body[&controller.name].contains(&m.name)) {
                continue;
            }
            for method in methods.iter().filter(|m| wanted.contains(&m.name)) {
                if taken.insert(method.name.clone()) {
                    let mut method = concern_class_method_copy(method, concern, consts);
                    fold_unwritten_ivars(&mut method.body, &written);
                    added.push(ControllerBodyItem::ClassMethod {
                        method,
                        configuration_slot: None,
                        configuration_role: None,
                        leading_comments: vec![],
                        leading_blank_line: true,
                    });
                }
            }
        }
        controller.body.splice(0..0, added);
    }
}

/// Class ivars `e` assigns.
fn writes_ivars(e: &Expr, out: &mut HashSet<Symbol>) {
    if let ExprNode::Assign { target: LValue::Ivar { name }, .. } | ExprNode::OpAssign { target: LValue::Ivar { name }, .. } = &*e.node {
        out.insert(name.clone());
    }
    e.node.for_each_child(&mut |c| writes_ivars(c, out));
}

/// `defined?(@x) ? @x : fallback` where nothing can write `@x` is the
/// fallback, and an ivar with no write has no type to compile with
/// (`scoped_views?`, whose writer nothing calls).
fn fold_unwritten_ivars(e: &mut Expr, written: &HashSet<Symbol>) {
    e.node.for_each_child_mut(&mut |c| fold_unwritten_ivars(c, written));
    let ExprNode::If { cond, else_branch, .. } = &*e.node else { return };
    let operand = match &*cond.node {
        ExprNode::Defined { operand } => operand,
        ExprNode::Send { recv: None, method, args, .. } if method.as_str() == "defined?" && args.len() == 1 => &args[0],
        _ => return,
    };
    let ExprNode::Ivar { name } = &*operand.node else { return };
    if !written.contains(name) {
        *e = else_branch.clone();
    }
}

/// `self.class.name`, anywhere in `e`.
fn calls_on_the_class(e: &Expr, out: &mut HashSet<Symbol>) {
    if let ExprNode::Send { recv: Some(recv), method, .. } = &*e.node
        && let ExprNode::Send { recv: Some(inner), method: class, .. } = &*recv.node
        && class.as_str() == "class"
        && matches!(&*inner.node, ExprNode::SelfRef)
    {
        out.insert(method.clone());
    }
    e.node.for_each_child(&mut |c| calls_on_the_class(c, out));
}

/// `name` or `self.name`, anywhere in the body of a class method.
fn calls_on_self(e: &Expr, out: &mut HashSet<Symbol>) {
    if let ExprNode::Send { recv, method, .. } = &*e.node
        && recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef))
    {
        out.insert(method.clone());
    }
    e.node.for_each_child(&mut |c| calls_on_self(c, out));
}
