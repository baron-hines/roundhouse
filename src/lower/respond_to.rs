//! `obj.respond_to?(:name)` answered at compile time, where `obj`'s class
//! is known and its method surface is complete.
//!
//! A strict target has no runtime method table to ask, so the question
//! can only be answered from the class registry: the same walk analyze's
//! dispatch performs (own instance methods, then mixed-in modules, then
//! the parent chain; `send_dispatch::method_return_via_registry`). A hit
//! anywhere on that walk is `true`. A miss is `false` only when the walk
//! saw the whole surface: every class on it is registered, none is open
//! (members the analysis could not see), none answers dynamically
//! (`method_missing` / `respond_to_missing?`), and the name is not one
//! of the class's column attributes. Anything short of that leaves the
//! call as written, for the target to answer or refuse.
//!
//! Reached through `assert_respond_to` / `refute_respond_to`, which the
//! assertion inliner rewrites to a `respond_to?` guard; the slice of
//! Minitest a model test uses to say a model does NOT have an attribute
//! (`refute_respond_to(manifest, :shop_id)`).

use crate::analyze::ClassInfo;
use crate::det::HashMap;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol};
use crate::ty::Ty;

/// Fold every answerable `respond_to?(:sym)` under `body`, bottom-up.
/// Returns whether anything changed.
pub fn fold_body(body: &mut Expr, registry: &HashMap<ClassId, ClassInfo>) -> bool {
    let mut changed = false;
    body.node.for_each_child_mut(&mut |child| changed |= fold_body(child, registry));
    if let Some(answer) = answer(body, registry) {
        *body.node = ExprNode::Lit { value: Literal::Bool { value: answer } };
        body.ty = Some(Ty::Bool);
        changed = true;
    }
    changed
}

fn answer(e: &Expr, registry: &HashMap<ClassId, ClassInfo>) -> Option<bool> {
    let ExprNode::Send { recv: Some(recv), method, args, block: None, .. } = &*e.node else { return None };
    if method.as_str() != "respond_to?" {
        return None;
    }
    let [name] = args.as_slice() else { return None };
    let ExprNode::Lit { value: Literal::Sym { value: name } } = &*name.node else { return None };
    // Only a receiver whose evaluation has no effect: folding drops it.
    if !matches!(&*recv.node, ExprNode::Var { .. } | ExprNode::Ivar { .. } | ExprNode::SelfRef) {
        return None;
    }
    let Some(Ty::Class { id, .. }) = recv.ty.as_ref() else { return None };
    responds(registry, id, name)
}

/// `Some(true)` when the walk finds `name`, `Some(false)` when it saw
/// the class's whole surface without finding it, `None` otherwise.
pub(crate) fn responds(registry: &HashMap<ClassId, ClassInfo>, class: &ClassId, name: &Symbol) -> Option<bool> {
    let dynamic = [Symbol::from("method_missing"), Symbol::from("respond_to_missing?")];
    let mut complete = true;
    let mut record = false;
    let mut seen: std::collections::BTreeSet<ClassId> = Default::default();
    let mut stack = vec![class.clone()];
    while let Some(cid) = stack.pop() {
        if !seen.insert(cid.clone()) {
            continue;
        }
        let Some(cls) = registry.get(&cid) else {
            // A model's framework root, which the registry does not hold
            // as a class: its surface is the AR catalog's, as dispatch has it.
            if cid.0.as_str() == "ActiveRecord::Base" {
                record = true;
            } else {
                complete = false;
            }
            continue;
        };
        if cls.instance_methods.contains_key(name) {
            return Some(true);
        }
        let column = name.as_str().trim_end_matches(['=', '?']);
        if cls.attributes.fields.contains_key(&Symbol::from(column)) {
            return Some(true);
        }
        if cls.open || dynamic.iter().any(|m| cls.instance_methods.contains_key(m)) {
            complete = false;
        }
        stack.extend(cls.includes.iter().cloned());
        if let Some(parent) = &cls.parent {
            stack.push(parent.clone());
        }
    }
    // Every catalogued AR instance method, typed or not: dispatch seeds
    // only the typed ones onto the model, but the rest are Rails' too.
    if record
        && crate::catalog::AR_CATALOG
            .iter()
            .any(|e| e.receiver == crate::catalog::ReceiverContext::Instance && e.name == name.as_str())
    {
        return Some(true);
    }
    // Object's own surface (`to_s`, `nil?`, …) is every class's; the
    // registry models it in dispatch rather than as a class.
    if crate::lower::core_method_names::OBJECT_METHODS.contains(&name.as_str()) {
        return Some(true);
    }
    // ActiveSupport's Object extensions (`blank?`, `try`, `to_param`)
    // are every Rails object's too, but the only list of them is nil's,
    // which also has nil's own (`to_a`, `&`): not answered either way.
    if crate::ingest::delegate::declarations::NIL_PUBLIC_METHODS.contains(&name.as_str()) {
        return None;
    }
    complete.then_some(false)
}
