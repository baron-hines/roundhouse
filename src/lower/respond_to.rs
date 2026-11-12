//! `obj.respond_to?(:name)` answered at compile time, where `obj`'s class
//! is known and its method surface is complete.
//!
//! A strict target has no runtime method table to ask, so the question
//! can only be answered from the class registry, walked in Ruby's lookup
//! order (the class, its modules last-included first, then the parent
//! chain). The nearest definition answers: `true` when it is public,
//! `false` when it is private or protected, as `respond_to?` does by
//! default. A miss is `false` only when the walk saw the whole surface:
//! every class on it is one the app declares (so analysis lists its
//! source methods, signed or not, with their visibility), none is open,
//! none has an include analysis could not resolve, none answers
//! dynamically (`method_missing` / `respond_to_missing?`), and the name
//! is not a column attribute. A class that defines `respond_to?` itself
//! answers for itself, so nothing is folded. Anything short of that
//! leaves the call as written, for the target to answer or refuse.
//!
//! Run after analysis ([`apply`], a post-analyze lowering) against the
//! analyzer's own registry, which holds each app class's includes, its
//! source methods with their visibility, and whether the app declares
//! it. The registries the emitters rebuild for test lowering hold
//! signatures only, so they cannot decide a miss. Reached through
//! `assert_respond_to` / `refute_respond_to`, the slice of Minitest a
//! model test uses to say a model does NOT have an attribute
//! (`refute_respond_to(manifest, :shop_id)`).

use crate::analyze::ClassInfo;
use crate::det::HashMap;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol};
use crate::ty::Ty;

/// Answer every `assert_respond_to` / `refute_respond_to` /
/// `assert_not_respond_to` and `respond_to?(:sym)` in the test tree that
/// the analyzer's registry decides, for a target with no runtime method
/// table. An assertion becomes the `assert` / `refute` of its answer and
/// a bare `respond_to?` the Bool. What is not decided stays as written:
/// the assertion inliner turns it into a `respond_to?` guard.
///
/// The Ruby family keeps the call: its runtime answers `respond_to?`
/// itself. Spinel is folded too, since a decided answer is the answer.
pub fn apply(
    app: &mut crate::app::App,
    registry: &HashMap<ClassId, ClassInfo>,
    target: crate::build_target::BuildTarget,
) {
    use crate::build_target::BuildTarget;
    if matches!(target, BuildTarget::Blog | BuildTarget::Ruby | BuildTarget::Jruby | BuildTarget::Roda | BuildTarget::Futamura) {
        return;
    }
    super::for_each_test_body(app, &mut |body| fold(body, registry));
}

fn fold(e: &mut Expr, registry: &HashMap<ClassId, ClassInfo>) {
    e.node.for_each_child_mut(&mut |child| fold(child, registry));
    if let Some((assertion, answer)) = assertion(e, registry) {
        let span = e.span;
        let value = Expr { ty: Some(Ty::Bool), ..Expr::new(span, ExprNode::Lit { value: Literal::Bool { value: answer } }) };
        *e.node = ExprNode::Send {
            recv: None,
            method: Symbol::from(assertion),
            args: vec![value],
            block: None,
            parenthesized: true,
        };
    } else if let Some(answer) = question(e, registry) {
        *e.node = ExprNode::Lit { value: Literal::Bool { value: answer } };
        e.ty = Some(Ty::Bool);
    }
}

/// `assert_respond_to obj, :name` → (`"assert"`, answer); the refute
/// forms → (`"refute"`, answer). The message argument, if any, is
/// dropped, as the inliner drops every assertion's message.
fn assertion(e: &Expr, registry: &HashMap<ClassId, ClassInfo>) -> Option<(&'static str, bool)> {
    let ExprNode::Send { recv: None, method, args, block: None, .. } = &*e.node else { return None };
    let kind = match method.as_str() {
        "assert_respond_to" => "assert",
        "refute_respond_to" | "assert_not_respond_to" => "refute",
        _ => return None,
    };
    let [obj, name, rest @ ..] = args.as_slice() else { return None };
    if rest.len() > 1 {
        return None;
    }
    Some((kind, ask(obj, name, registry)?))
}

/// `obj.respond_to?(:name)`, one argument.
fn question(e: &Expr, registry: &HashMap<ClassId, ClassInfo>) -> Option<bool> {
    let ExprNode::Send { recv: Some(recv), method, args, block: None, .. } = &*e.node else { return None };
    if method.as_str() != "respond_to?" {
        return None;
    }
    let [name] = args.as_slice() else { return None };
    ask(recv, name, registry)
}

fn ask(obj: &Expr, name: &Expr, registry: &HashMap<ClassId, ClassInfo>) -> Option<bool> {
    let ExprNode::Lit { value: Literal::Sym { value: name } } = &*name.node else { return None };
    // Only a receiver whose evaluation has no effect: folding drops it.
    if !matches!(&*obj.node, ExprNode::Var { .. } | ExprNode::Ivar { .. } | ExprNode::SelfRef) {
        return None;
    }
    let Some(Ty::Class { id, .. }) = obj.ty.as_ref() else { return None };
    responds(registry, id, name)
}

/// `Some(true)` when the nearest definition of `name` on the class's
/// ancestor chain is public, `Some(false)` when it is private/protected
/// or when the walk saw the class's whole surface without finding it,
/// `None` otherwise.
///
/// The surface is trusted only for classes the app declares (models,
/// library classes and concerns, whose source methods analysis lists
/// signed or not, with their visibility). A test class, a class declared
/// inside a test file, and a framework stub have no such listing (their
/// registry entries hold signatures only, without ancestry), so a walk
/// that reaches one answers nothing. Nor does one that reaches a class
/// defining `respond_to?` itself: its answer is its own.
pub(crate) fn responds(registry: &HashMap<ClassId, ClassInfo>, class: &ClassId, name: &Symbol) -> Option<bool> {
    let mut walk = Walk::default();
    walk.visit(registry, class);
    let own_answer = Symbol::from("respond_to?");
    if walk.order.iter().flatten().any(|cid| defines(&registry[cid], &own_answer)) {
        return None;
    }
    for step in &walk.order {
        // A class the walk cannot see into comes before any later hit,
        // and may define the name itself.
        let Some(cid) = step else { return None };
        let cls = &registry[cid];
        if defines(cls, name) {
            return Some(!cls.nonpublic_instance_methods.contains(name));
        }
        let column = name.as_str().trim_end_matches(['=', '?']);
        if cls.attributes.fields.contains_key(&Symbol::from(column)) {
            return Some(true);
        }
    }
    if !walk.complete {
        return None;
    }
    // Every catalogued AR instance method, typed or not: dispatch seeds
    // only the typed ones onto the model, but the rest are Rails' too.
    if walk.record
        && crate::catalog::AR_CATALOG
            .iter()
            .any(|e| e.receiver == crate::catalog::ReceiverContext::Instance && e.name == name.as_str())
    {
        return Some(true);
    }
    // Object's own public surface (`to_s`, `nil?`, …) is every class's;
    // the registry models it in dispatch rather than as a class.
    if crate::lower::core_method_names::OBJECT_METHODS.contains(&name.as_str()) {
        return Some(true);
    }
    // ActiveSupport's Object extensions (`blank?`, `try`, `to_param`)
    // are every Rails object's too, but the only list of them is nil's,
    // which also has nil's own (`to_a`, `&`): not answered either way.
    if crate::ingest::delegate::declarations::NIL_PUBLIC_METHODS.contains(&name.as_str()) {
        return None;
    }
    Some(false)
}

fn defines(cls: &ClassInfo, name: &Symbol) -> bool {
    cls.instance_methods.contains_key(name) || cls.source_instance_methods.contains(name)
}

/// An ancestor chain in Ruby's lookup order (the class, its modules
/// last-included first, then its parent's chain): each trusted class,
/// `None` where an untrusted one stands; and whether the chain held
/// only trusted classes and the AR root.
#[derive(Default)]
struct Walk {
    order: Vec<Option<ClassId>>,
    seen: std::collections::BTreeSet<ClassId>,
    complete: bool,
    record: bool,
    started: bool,
}

impl Walk {
    fn visit(&mut self, registry: &HashMap<ClassId, ClassInfo>, cid: &ClassId) {
        if !self.started {
            self.started = true;
            self.complete = true;
        }
        if !self.seen.insert(cid.clone()) {
            return;
        }
        let Some(cls) = registry.get(cid) else {
            // A model's framework root, which the registry does not hold
            // as a class: its surface is the AR catalog's, as dispatch has it.
            if cid.0.as_str() == "ActiveRecord::Base" {
                self.record = true;
            } else {
                self.complete = false;
                self.order.push(None);
            }
            return;
        };
        if !cls.app_declared {
            self.complete = false;
            self.order.push(None);
            return;
        }
        let dynamic = ["method_missing", "respond_to_missing?"].map(Symbol::from);
        if cls.open || cls.unresolved_include || dynamic.iter().any(|m| defines(cls, m)) {
            self.complete = false;
        }
        self.order.push(Some(cid.clone()));
        for module in cls.includes.iter().rev() {
            self.visit(registry, module);
        }
        if let Some(parent) = &cls.parent {
            self.visit(registry, parent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> ClassId {
        ClassId(Symbol::from(name))
    }

    /// `Snapshot < ApplicationRecord < ActiveRecord::Base`, with a
    /// `checksum` column and `Snapshot#digest` public, `#salt` private and
    /// `#rehash` defined in source without a signature.
    fn registry() -> HashMap<ClassId, ClassInfo> {
        let mut registry: HashMap<ClassId, ClassInfo> = HashMap::default();
        let mut base = ClassInfo::default();
        base.app_declared = true;
        base.parent = Some(id("ActiveRecord::Base"));
        registry.insert(id("ApplicationRecord"), base);
        let mut model = ClassInfo::default();
        model.app_declared = true;
        model.parent = Some(id("ApplicationRecord"));
        let mut fields = indexmap::IndexMap::new();
        fields.insert(Symbol::from("checksum"), Ty::Str);
        model.attributes = crate::ty::Row { fields: fields.into(), rest: None };
        model.instance_methods.insert(Symbol::from("digest"), Ty::Str);
        model.instance_methods.insert(Symbol::from("salt"), Ty::Str);
        model.source_instance_methods.extend(["digest", "salt", "rehash"].map(Symbol::from));
        model.nonpublic_instance_methods.insert(Symbol::from("salt"));
        registry.insert(id("Snapshot"), model);
        registry
    }

    fn ask(registry: &HashMap<ClassId, ClassInfo>, class: &str, name: &str) -> Option<bool> {
        responds(registry, &id(class), &Symbol::from(name))
    }

    #[test]
    fn the_nearest_definition_answers_by_its_visibility() {
        let registry = registry();
        assert_eq!(ask(&registry, "Snapshot", "digest"), Some(true));
        assert_eq!(ask(&registry, "Snapshot", "salt"), Some(false));
        assert_eq!(ask(&registry, "Snapshot", "rehash"), Some(true), "a source method without a signature");
        assert_eq!(ask(&registry, "Snapshot", "checksum"), Some(true));
        assert_eq!(ask(&registry, "Snapshot", "checksum="), Some(true));
        assert_eq!(ask(&registry, "Snapshot", "save"), Some(true), "the AR catalog");
        assert_eq!(ask(&registry, "Snapshot", "shop_id"), Some(false));
    }

    #[test]
    fn a_class_answering_respond_to_itself_is_not_folded() {
        let mut registry = registry();
        registry.get_mut(&id("ApplicationRecord")).unwrap().source_instance_methods.insert(Symbol::from("respond_to?"));
        assert_eq!(ask(&registry, "Snapshot", "digest"), None);
        assert_eq!(ask(&registry, "Snapshot", "shop_id"), None);
    }

    #[test]
    fn a_class_the_app_does_not_declare_is_not_folded() {
        // A test class's registry entry: signatures only, no ancestry.
        let mut registry = registry();
        let mut test_class = ClassInfo::default();
        test_class.instance_methods.insert(Symbol::from("helper"), Ty::Nil);
        registry.insert(id("SnapshotTest"), test_class);
        assert_eq!(ask(&registry, "SnapshotTest", "helper"), None);
        assert_eq!(ask(&registry, "SnapshotTest", "shared_helper"), None);
    }

    #[test]
    fn an_unseen_module_before_a_hit_is_not_folded() {
        // The model includes a module the registry does not hold; it
        // comes before ApplicationRecord's `touch_up` in lookup order.
        let mut registry = registry();
        registry.get_mut(&id("ApplicationRecord")).unwrap().source_instance_methods.insert(Symbol::from("touch_up"));
        registry.get_mut(&id("Snapshot")).unwrap().includes.push(id("Gem::Unseen"));
        assert_eq!(ask(&registry, "Snapshot", "digest"), Some(true), "the class's own method still answers");
        assert_eq!(ask(&registry, "Snapshot", "touch_up"), None);
        assert_eq!(ask(&registry, "Snapshot", "shop_id"), None);
    }

    #[test]
    fn a_dynamic_or_open_class_answers_only_hits() {
        let mut registry = registry();
        registry.get_mut(&id("Snapshot")).unwrap().source_instance_methods.insert(Symbol::from("method_missing"));
        assert_eq!(ask(&registry, "Snapshot", "digest"), Some(true));
        assert_eq!(ask(&registry, "Snapshot", "shop_id"), None);
        let mut registry = self::registry();
        registry.get_mut(&id("ApplicationRecord")).unwrap().open = true;
        assert_eq!(ask(&registry, "Snapshot", "shop_id"), None);
    }
}
