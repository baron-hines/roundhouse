//! A call no method of its receiver's class answers, on a class whose chain defines `method_missing`
//! (Hashie's `Mash`: `options.form`, `info.last_name?`), written as the `method_missing(:name, ...)` call Ruby makes
//! for it. Spinel dispatches no `method_missing` (every call is resolved as it builds) but calls one written out.
//! The Ruby family's trees only: there the rewrite is the call Ruby runs.

use crate::app::App;
use crate::det::HashMap;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol};
use crate::ty::Ty;

pub fn apply_method_missing_calls(app: &mut App, registry: &HashMap<ClassId, crate::analyze::ClassInfo>, target: crate::build_target::BuildTarget) {
    if !target.has_runtime_relation() {
        return;
    }
    let missing = Symbol::from("method_missing");
    let answers_missing = |id: &ClassId| {
        crate::analyze::diagnostics::method_registered_on(registry, Some(&Ty::Class { id: id.clone(), args: vec![].into() }), &missing)
    };
    // The core container each class descends from, by its written parents: what its instances answer from there.
    let parents: HashMap<String, Option<String>> = app
        .library_classes
        .iter()
        .filter(|c| !c.is_module)
        .map(|c| (c.name.0.as_str().to_string(), c.parent.as_ref().map(|p| p.0.as_str().trim_start_matches("::").to_string())))
        .collect();
    let core_of = |id: &ClassId| -> &'static [&'static str] {
        let mut current = id.0.as_str().to_string();
        for _ in 0..32 {
            let Some(Some(parent)) = parents.get(&current) else { break };
            let scope: Vec<&str> = current.split("::").collect();
            let resolved = (0..scope.len())
                .rev()
                .map(|depth| if depth == 0 { parent.clone() } else { format!("{}::{parent}", scope[..depth].join("::")) })
                .find(|candidate| *candidate != current && parents.contains_key(candidate));
            match resolved {
                Some(next) => current = next,
                None => {
                    return match parent.as_str() {
                        "Hash" => super::core_instance_methods::HASH,
                        "Array" => super::core_instance_methods::ARRAY,
                        "String" => super::core_instance_methods::STRING,
                        _ => super::core_instance_methods::OBJECT,
                    };
                }
            }
        }
        super::core_instance_methods::OBJECT
    };
    for lc in &mut app.library_classes {
        let owner = lc.name.clone();
        let own_missing = answers_missing(&owner);
        for m in lc.methods.iter_mut().filter(|m| m.receiver == crate::dialect::MethodReceiver::Instance) {
            // Not inside `method_missing` itself, where an unanswered name is the one it was asked.
            if m.name == missing || m.name.as_str() == "respond_to_missing?" {
                continue;
            }
            let locals: Vec<Symbol> = m.params.iter().map(|p| p.name.clone()).collect();
            rewrite(&mut m.body, &owner, own_missing, &locals, registry, &answers_missing, &core_of);
        }
    }
}

fn rewrite(
    e: &mut Expr,
    owner: &ClassId,
    own_missing: bool,
    locals: &[Symbol],
    registry: &HashMap<ClassId, crate::analyze::ClassInfo>,
    answers_missing: &dyn Fn(&ClassId) -> bool,
    core_of: &dyn Fn(&ClassId) -> &'static [&'static str],
) {
    e.node.for_each_child_mut(&mut |c| rewrite(c, owner, own_missing, locals, registry, answers_missing, core_of));
    if !crate::analyze::diagnostics::is_unknown_ty(e.ty.as_ref()) {
        return;
    }
    let ExprNode::Send { recv, method, args, .. } = &mut *e.node else { return };
    if method.as_str() == "method_missing" || method.as_str().starts_with(|c: char| !c.is_ascii_alphabetic() && c != '_') {
        return;
    }
    let class = match recv.as_ref() {
        Some(r) => match r.ty.as_ref().map(Ty::peel_nilable) {
            Some(Ty::Class { id, .. }) if answers_missing(id) => id.clone(),
            _ => return,
        },
        None if own_missing && args.is_empty() && !locals.contains(method) => owner.clone(),
        None => return,
    };
    // Not a method the core class it descends from answers (`options.merge(...)` on a Hash subclass).
    if core_of(&class).contains(&method.as_str()) {
        return;
    }
    let class_ty = Ty::Class { id: class, args: vec![].into() };
    if crate::analyze::diagnostics::method_registered_on(registry, Some(&class_ty), method) {
        return;
    }
    let name = Expr::new(e.span, ExprNode::Lit { value: Literal::Sym { value: method.clone() } });
    args.insert(0, name);
    *method = Symbol::from("method_missing");
}
