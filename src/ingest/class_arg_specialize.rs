//! A controller method a class is handed to as an argument, copied per
//! class: `render_serialized(@post, PostSerializer)` calls
//! `render_serialized__PostSerializer(@post)`, whose body reads
//! `::PostSerializer` where it read the parameter.
//!
//! The analysis types a constant as an instance of its class, so a
//! parameter that several call sites hand different classes types as their
//! union of instances, and `serializer.new(obj, opts)` answers nothing it
//! can follow. A copy per class gives each call its own class. A parameter
//! qualifies only when the body uses it as the receiver of `new`, the
//! value of a Hash entry or an index write, or an argument handed on, so
//! the copy reads the class where the original read the parameter; a site
//! passing anything but a constant of an app class keeps the original.

use crate::det::{HashMap, HashSet};
use crate::dialect::{Action, ControllerBodyItem};
use crate::expr::{Expr, ExprNode, LValue};
use crate::ident::Symbol;

pub(super) fn specialize(app: &mut crate::App) {
    let classes: HashSet<String> = app
        .library_classes
        .iter()
        .filter(|c| !c.is_module)
        .map(|c| c.name.0.as_str().to_string())
        .chain(app.models.iter().map(|m| m.name.0.as_str().to_string()))
        .collect();
    let mut made: HashSet<(String, String)> = HashSet::default();
    // (method, slot it removes, class) → the copy's name, and back: `A::B` and `A_B` spell one suffix, so the name is allocated, not derived.
    let mut names: HashMap<(String, Option<usize>, String), String> = HashMap::default();
    let mut class_of_copy: HashMap<String, String> = HashMap::default();
    let taken: HashSet<String> = app.controllers.iter().flat_map(|c| c.actions().map(|a| a.name.as_str().to_string())).collect();
    let mut originals: HashSet<(usize, String)> = HashSet::default();
    // A copy's body hands the class on (`serialize_data(obj, PostSerializer, opts)`), which the next round copies.
    for _ in 0..8 {
        let candidates = candidates(app);
        if candidates.is_empty() && made.is_empty() {
            return;
        }
        let parents: HashMap<String, String> = app
            .controllers
            .iter()
            .filter_map(|c| Some((c.name.0.as_str().to_string(), c.parent.as_ref()?.0.as_str().trim_start_matches("::").to_string())))
            .collect();
        let index_of: HashMap<String, usize> =
            app.controllers.iter().enumerate().map(|(i, c)| (c.name.0.as_str().to_string(), i)).collect();
        // The controller whose method `name` a call in `owner` reaches.
        let resolve = |owner: &str, name: &str| -> Option<usize> {
            let mut current = owner.to_string();
            for _ in 0..16 {
                let &i = index_of.get(&current)?;
                if app.controllers[i].actions().any(|a| a.name.as_str() == name) {
                    return Some(i);
                }
                current = parents.get(&current)?.clone();
            }
            None
        };
        // A call the class's subclasses override is not renamed: on one of them, `build(S)` reaches their `build`.
        let overridden = |owner: &str, name: &str| -> bool {
            app.controllers.iter().any(|c| {
                let mut current = c.name.0.as_str().to_string();
                for _ in 0..16 {
                    let Some(parent) = parents.get(&current) else { return false };
                    if parent == owner {
                        return c.actions().any(|a| a.name.as_str() == name);
                    }
                    current = parent.clone();
                }
                false
            })
        };
        // (controller, method, the parameter the class replaces or none, class): a site passing the class, or one
        // handing on what a copy for the class answers (`render_json_dump(serialize_data__PostSerializer(obj))`),
        // whose callee then sees that copy's type alone.
        let handed = |class_of_copy: &HashMap<String, String>, args: &[Expr]| -> Option<String> {
            let mut found: Option<String> = None;
            for arg in args {
                let ExprNode::Send { recv, method, .. } = &*arg.node else { continue };
                if !recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef)) {
                    continue;
                }
                let Some(class) = class_of_copy.get(method.as_str()) else { continue };
                if found.as_ref().is_some_and(|f| f != class) {
                    return None;
                }
                found = Some(class.clone());
            }
            found
        };
        let mut wanted: Vec<(usize, String, Option<usize>, String)> = Vec::new();
        let mut rewrites: Vec<(usize, usize, HashSet<(String, usize, Option<usize>, String)>)> = Vec::new();
        for (ci, controller) in app.controllers.iter().enumerate() {
            let owner = controller.name.0.as_str();
            for (ai, item) in controller.body.iter().enumerate() {
                let ControllerBodyItem::Action { action, .. } = item else { continue };
                let mut sites = Vec::new();
                find_sites(&action.body, &mut |method, args| {
                    if method.contains("__") || overridden(owner, method) {
                        return;
                    }
                    let Some(target) = resolve(owner, method) else { return };
                    for (k, arg) in args.iter().enumerate() {
                        // Not past a splat: which formal a later argument binds is known only when it runs.
                        if matches!(&*arg.node, ExprNode::Splat { .. } | ExprNode::ForwardArgs) {
                            break;
                        }
                        if !candidates.contains(&(target, method.to_string(), k)) {
                            continue;
                        }
                        let ExprNode::Const { path } = &*arg.node else { continue };
                        let Some(class) = resolve_const(path, owner, &classes) else { continue };
                        sites.push((target, method.to_string(), Some(k), class));
                        return;
                    }
                    if let Some(class) = handed(&class_of_copy, args) {
                        sites.push((target, method.to_string(), None, class));
                    }
                });
                if !sites.is_empty() {
                    let exact = sites.iter().map(|(t, m, k, c)| (m.clone(), *t, *k, c.clone())).collect();
                    wanted.extend(sites);
                    rewrites.push((ci, ai, exact));
                }
            }
        }
        if rewrites.is_empty() {
            break;
        }
        for (_, method, k, class) in &wanted {
            let key = (method.clone(), *k, class.clone());
            if names.contains_key(&key) {
                continue;
            }
            let base = format!("{method}__{}", class.replace("::", "_"));
            let mut name = base.clone();
            let mut n = 2;
            while taken.contains(&name) || class_of_copy.contains_key(&name) {
                name = format!("{base}_{n}");
                n += 1;
            }
            class_of_copy.insert(name.clone(), class.clone());
            names.insert(key, name);
        }
        // The sites first: a copy inserted into a body moves the items after it.
        for (ci, ai, exact) in rewrites {
            let owner = app.controllers[ci].name.0.as_str().to_string();
            // The controller each renamed method resolves to here: two controllers' `build(S)` are two copies' sites.
            let resolved: HashMap<String, usize> = exact.iter().map(|(m, t, _, _)| (m.clone(), *t)).collect();
            let ControllerBodyItem::Action { action, .. } = &mut app.controllers[ci].body[ai] else { continue };
            rewrite_sites(&mut action.body, &mut |method, args| {
                let &target = resolved.get(method)?;
                for (k, arg) in args.iter().enumerate() {
                    if matches!(&*arg.node, ExprNode::Splat { .. } | ExprNode::ForwardArgs) {
                        break;
                    }
                    let ExprNode::Const { path } = &*arg.node else { continue };
                    let Some(class) = resolve_const(path, &owner, &classes) else { continue };
                    if !exact.contains(&(method.to_string(), target, Some(k), class.clone())) {
                        continue;
                    }
                    let mut args = args.to_vec();
                    args.remove(k);
                    return Some((names[&(method.to_string(), Some(k), class)].clone(), args));
                }
                let class = handed(&class_of_copy, args)?;
                exact.contains(&(method.to_string(), target, None, class.clone())).then(|| (names[&(method.to_string(), None, class)].clone(), args.to_vec()))
            });
        }
        for (target, method, k, class) in &wanted {
            let copy = names[&(method.clone(), *k, class.clone())].clone();
            if !made.insert((app.controllers[*target].name.0.as_str().to_string(), copy.clone())) {
                continue;
            }
            originals.insert((*target, method.clone()));
            let controller = &mut app.controllers[*target];
            let Some(pos) = controller.body.iter().position(|i| matches!(i, ControllerBodyItem::Action { action, .. } if action.name.as_str() == method)) else { continue };
            let ControllerBodyItem::Action { action, leading_comments: _, leading_blank_line } = &controller.body[pos] else { continue };
            let mut action: Action = action.clone();
            if let Some(k) = k {
                let param = action.params.fields.keys().nth(*k).cloned().expect("candidate parameter");
                let fields: indexmap::IndexMap<Symbol, crate::ty::Ty> =
                    action.params.fields.iter().filter(|(n, _)| **n != param).map(|(n, t)| (n.clone(), t.clone())).collect();
                action.params = crate::ty::Row { fields: fields.into(), rest: None };
                substitute(&mut action.body, &param, class);
            }
            action.name = Symbol::from(copy.as_str());
            let item = ControllerBodyItem::Action { action, leading_comments: Vec::new(), leading_blank_line: *leading_blank_line };
            controller.body.insert(pos + 1, item);
        }
    }
    // Until none goes: an original another original calls goes once that one has.
    let mut originals: Vec<(usize, String)> = originals.into_iter().collect();
    loop {
        let before = originals.len();
        originals.retain(|(ci, method)| {
            let mut named = false;
            crate::lower::for_each_emit_body_ref(app, &mut |body| named |= mentions(body, method));
            if named {
                return true;
            }
            app.controllers[*ci].body.retain(|i| !matches!(i, ControllerBodyItem::Action { action, .. } if action.name.as_str() == method.as_str()));
            false
        });
        if originals.len() == before {
            break;
        }
    }
}

/// The class a constant written in `owner` names: its lexical scopes, then the top level.
fn resolve_const(path: &[Symbol], owner: &str, classes: &HashSet<String>) -> Option<String> {
    let rooted = path.first().is_some_and(|s| s.as_str().is_empty());
    let written = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::");
    if !rooted {
        let mut scope: Vec<&str> = owner.split("::").collect();
        scope.pop();
        while !scope.is_empty() {
            let candidate = format!("{}::{written}", scope.join("::"));
            if classes.contains(&candidate) {
                return Some(candidate);
            }
            scope.pop();
        }
    }
    classes.contains(&written).then_some(written)
}

/// Each (controller, method, parameter index) whose parameter the body uses only where a class can stand in.
fn candidates(app: &crate::App) -> HashSet<(usize, String, usize)> {
    let mut out = HashSet::default();
    for (ci, controller) in app.controllers.iter().enumerate() {
        for action in controller.actions() {
            if action.name.as_str().contains("__") {
                continue;
            }
            for (k, param) in action.params.fields.keys().enumerate() {
                let (total, ok) = uses(&action.body, param);
                // Not one a default reads (`opts = { serializer: klass }`): the copy has no formal left for it.
                let defaulted = action.opt_params.iter().map(|(_, d)| d).chain(action.kw_params.iter().filter_map(|(_, d)| d.as_ref()))
                    .any(|d| uses(d, param).0 > 0);
                if total > 0 && total == ok && !defaulted && !assigned(&action.body, param) {
                    out.insert((ci, action.name.as_str().to_string(), k));
                }
            }
        }
    }
    out
}

fn is_var(e: &Expr, name: &Symbol) -> bool {
    matches!(&*e.node, ExprNode::Var { name: n, .. } if n == name)
}

/// The reads of `name` in `e`, and how many sit where a class can stand in.
fn uses(e: &Expr, name: &Symbol) -> (usize, usize) {
    let mut total = usize::from(is_var(e, name));
    let mut ok = 0;
    match &*e.node {
        ExprNode::Send { recv: Some(r), method, .. } if method.as_str() == "new" && is_var(r, name) => ok += 1,
        _ => {}
    }
    match &*e.node {
        ExprNode::Send { recv, args, .. } if recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef)) => {
            ok += args.iter().filter(|a| is_var(a, name)).count();
        }
        ExprNode::Send { method, args, .. } if method.as_str() == "[]=" && args.len() == 2 && is_var(&args[1], name) => ok += 1,
        ExprNode::Hash { entries, .. } => ok += entries.iter().filter(|(_, v)| is_var(v, name)).count(),
        ExprNode::Assign { target: LValue::Index { .. }, value } if is_var(value, name) => ok += 1,
        _ => {}
    }
    e.node.for_each_child(&mut |c| {
        let (t, o) = uses(c, name);
        total += t;
        ok += o;
    });
    (total, ok)
}

fn assigned(e: &Expr, name: &Symbol) -> bool {
    let here = match &*e.node {
        ExprNode::Assign { target: LValue::Var { name: n, .. }, .. } | ExprNode::OpAssign { target: LValue::Var { name: n, .. }, .. } => n == name,
        ExprNode::Lambda { params, .. } => params.contains(name),
        _ => false,
    };
    let mut found = here;
    e.node.for_each_child(&mut |c| found |= assigned(c, name));
    found
}

fn find_sites(e: &Expr, f: &mut dyn FnMut(&str, &[Expr])) {
    if let ExprNode::Send { recv, method, args, .. } = &*e.node
        && recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef))
    {
        f(method.as_str(), args);
    }
    e.node.for_each_child(&mut |c| find_sites(c, f));
}

fn rewrite_sites(e: &mut Expr, f: &mut dyn FnMut(&str, &[Expr]) -> Option<(String, Vec<Expr>)>) {
    e.node.for_each_child_mut(&mut |c| rewrite_sites(c, f));
    if let ExprNode::Send { recv, method, args, .. } = &mut *e.node
        && recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef))
        && let Some((name, new_args)) = f(method.as_str(), args)
    {
        *method = Symbol::from(name.as_str());
        *args = new_args;
    }
}

/// A call of `method`, or its name as a Symbol or String (`send(:m)`, `method(:m)`).
fn mentions(e: &Expr, method: &str) -> bool {
    let mut found = match &*e.node {
        ExprNode::Send { method: m, .. } => m.as_str() == method,
        ExprNode::Lit { value: crate::expr::Literal::Sym { value } } => value.as_str() == method,
        ExprNode::Lit { value: crate::expr::Literal::Str { value } } => value.as_str() == method,
        _ => false,
    };
    e.node.for_each_child(&mut |c| found |= mentions(c, method));
    found
}

fn substitute(e: &mut Expr, param: &Symbol, class: &str) {
    if is_var(e, param) {
        let path = std::iter::once(Symbol::from("")).chain(class.split("::").map(Symbol::from)).collect();
        *e.node = ExprNode::Const { path };
        // Not read back from its span, which holds the parameter's name.
        e.decisions |= crate::expr::GENERATED_CONST_REF;
        return;
    }
    e.node.for_each_child_mut(&mut |c| substitute(c, param, class));
}
