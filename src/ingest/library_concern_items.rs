//! A concern's `included do … end` runs on every class that includes it,
//! not only models: mission_control-jobs' `ActiveJob::Executing` gives
//! `ActiveJob::Base` its `attr_accessor :status, …` and its
//! `thread_cattr_accessor :current_queue_adapter` that way. Models take
//! the block through `splice_concerns_into_models`; a library class takes
//! the accessors here, before `thread_mattr` lowers the thread-local ones,
//! and the modules the block includes, which then carry their class-method
//! module and their own `included` block to it.

use crate::app::App;
use crate::dialect::{LibraryClass, MethodDef, MethodReceiver, ModelBodyItem};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol};
use crate::span::Span;

const THREAD_DECLS: &[&str] = &[
    "thread_mattr_accessor",
    "thread_mattr_reader",
    "thread_mattr_writer",
    "thread_cattr_accessor",
    "thread_cattr_reader",
    "thread_cattr_writer",
];

pub(super) fn splice(app: &mut App) {
    for i in 0..app.library_classes.len() {
        let lc = &app.library_classes[i];
        // Not into a module: a concern included by a concern runs its block on that one's includers.
        if lc.is_module {
            continue;
        }
        // An `include` in the block mixes into the includer, and the module
        // it names runs its own block there in turn, as on a model.
        let mut includes: Vec<ClassId> = lc.includes.clone();
        let mut added: Vec<(ClassId, Span)> = Vec::new();
        let mut statements: Vec<Expr> = Vec::new();
        let mut k = 0;
        while k < includes.len() {
            let module = includes[k].clone();
            k += 1;
            let Some(items) = app.concern_model_items.get(&module) else { continue };
            for item in items {
                let ModelBodyItem::Unknown { expr, .. } = item else { continue };
                let ExprNode::Send { recv: None, method, args, block: None, .. } = &*expr.node else {
                    statements.push(expr.clone());
                    continue;
                };
                if method.as_str() != "include" {
                    statements.push(expr.clone());
                    continue;
                }
                for arg in args {
                    let ExprNode::Const { path } = &*arg.node else { continue };
                    let segs: Vec<&str> = path.iter().map(|p| p.as_str()).collect();
                    if crate::ingest::util::is_view_helper_marker_include(&segs)
                        || crate::ingest::util::is_serialization_marker_include(&segs)
                        || crate::ingest::util::is_concern_marker_include(&segs)
                    {
                        continue;
                    }
                    let id = ClassId(Symbol::from(segs.join("::")));
                    if !includes.contains(&id) {
                        includes.push(id.clone());
                        added.push((id, expr.span));
                    }
                }
            }
        }
        // A def the block makes is the includer's own, ahead of the module.
        // The concern hoist carries it on the module, which the same lookup
        // finds, unless the module's own def of the name (a later opening's)
        // shadows it there: then this class takes the block's def as its own
        // (#2278). One the class writes itself stays; Ruby keeps it when it is
        // written below the `include`.
        // Not a module a superclass has already: Ruby's `include` adds nothing
        // for it, and ActiveSupport runs no hook.
        let inherited = inherited_modules(app, app.library_classes[i].parent.as_ref());
        // The last hook def of each name per module, and whether the module's
        // own def of the name shadows it.
        let mut ran: Vec<(&MethodDef, bool)> = Vec::new();
        let mut seen: Vec<&ClassId> = Vec::new();
        for module in includes.iter().filter(|m| !inherited.contains(*m)) {
            if seen.contains(&module) {
                continue;
            }
            seen.push(module);
            let openings = || app.library_classes.iter().filter(|o| o.is_module && o.name == *module);
            let own: Vec<&Symbol> = openings()
                .flat_map(|o| o.methods.iter().filter(move |m| m.receiver == MethodReceiver::Instance && !in_included_hook(o, m)))
                .map(|m| &m.name)
                .collect();
            // The last opening's def of a name is the one Ruby keeps.
            let hooked: Vec<&MethodDef> = openings()
                .flat_map(|o| o.methods.iter().filter(move |m| m.receiver == MethodReceiver::Instance && in_included_hook(o, m)))
                .collect();
            for (at, def) in hooked.iter().enumerate() {
                if !hooked[at + 1..].iter().any(|later| later.name == def.name) {
                    ran.push((*def, own.contains(&&def.name)));
                }
            }
        }
        let mut hook_defs: Vec<MethodDef> = Vec::new();
        for (def, shadowed) in &ran {
            // Of two hooks that define the name, the one that runs last wins,
            // an order not read here: those stay as the modules have them.
            let alone = ran.iter().filter(|(d, _)| d.name == def.name).count() == 1;
            let written = app.library_classes[i].methods.iter().any(|m| m.receiver == MethodReceiver::Instance && m.name == def.name);
            if *shadowed && alone && !written {
                hook_defs.push((*def).clone());
            }
        }
        let lc = &mut app.library_classes[i];
        lc.methods.extend(hook_defs);
        for (id, span) in added {
            lc.includes.push(id);
            lc.include_spans.push(span);
        }
        for stmt in statements {
            let ExprNode::Send { recv: None, method, args, block: None, .. } = &*stmt.node else { continue };
            let names: Option<Vec<Symbol>> = args
                .iter()
                .map(|a| match &*a.node {
                    ExprNode::Lit { value: Literal::Sym { value } } => Some(value.clone()),
                    _ => None,
                })
                .collect();
            let Some(names) = names.filter(|n| !n.is_empty()) else { continue };
            let kw = method.as_str();
            if THREAD_DECLS.contains(&kw) {
                if !lc.unknown_calls.iter().any(|c| *c == stmt) {
                    lc.unknown_calls.push(stmt.clone());
                }
                continue;
            }
            let (reader, writer) = match kw {
                "attr_accessor" => (true, true),
                "attr_reader" => (true, false),
                "attr_writer" => (false, true),
                _ => continue,
            };
            for name in names {
                let defines = |lc: &crate::dialect::LibraryClass, n: &str| {
                    lc.methods.iter().any(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == n)
                };
                if reader && !defines(lc, name.as_str()) {
                    lc.methods.push(super::library_class::synth_attr_reader(&lc.name, &name, MethodReceiver::Instance));
                }
                if writer && !defines(lc, &format!("{}=", name.as_str())) {
                    lc.methods.push(super::library_class::synth_attr_writer(&lc.name, &name, MethodReceiver::Instance));
                }
            }
        }
    }
}

/// Whether `m` is written in one of `lc`'s `included do` blocks (or `self.included do`): a def the hook makes on the includer.
pub(super) fn in_included_hook(lc: &LibraryClass, m: &MethodDef) -> bool {
    let at = m.name_span;
    !at.is_synthetic()
        && lc.unknown_calls.iter().any(|call| {
            matches!(&*call.node, ExprNode::Send { recv, method, block: Some(_), .. }
                if method.as_str() == "included" && recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef)))
                && call.span.file == at.file
                && call.span.start <= at.start
                && at.end <= call.span.end
        })
}

/// The modules the classes above `parent` and `parent` itself include, with
/// what those modules include in turn: a library class's or a controller's.
fn inherited_modules(app: &App, parent: Option<&ClassId>) -> crate::det::HashSet<ClassId> {
    let mut out: crate::det::HashSet<ClassId> = crate::det::HashSet::default();
    let mut walked: Vec<ClassId> = Vec::new();
    let mut next = parent.cloned();
    while let Some(class) = next.take() {
        if walked.contains(&class) {
            break;
        }
        for lc in app.library_classes.iter().filter(|c| !c.is_module && c.name == class) {
            out.extend(lc.includes.iter().cloned());
            next = next.or_else(|| lc.parent.clone());
        }
        for controller in app.controllers.iter().filter(|c| c.name == class) {
            out.extend(crate::analyze::controller_include_statements(controller).into_iter().flat_map(|(_, group)| group));
            next = next.or_else(|| controller.parent.clone());
        }
        walked.push(class);
    }
    let mut queue: Vec<ClassId> = out.iter().cloned().collect();
    while let Some(module) = queue.pop() {
        for opening in app.library_classes.iter().filter(|o| o.is_module && o.name == module) {
            for included in &opening.includes {
                if out.insert(included.clone()) {
                    queue.push(included.clone());
                }
            }
        }
    }
    out
}
