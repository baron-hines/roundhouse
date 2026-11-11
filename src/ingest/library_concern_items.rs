//! A concern's `included do … end` runs on every class that includes it,
//! not only models: mission_control-jobs' `ActiveJob::Executing` gives
//! `ActiveJob::Base` its `attr_accessor :status, …` and its
//! `thread_cattr_accessor :current_queue_adapter` that way. Models take
//! the block through `splice_concerns_into_models`; a library class takes
//! the accessors here, before `thread_mattr` lowers the thread-local ones,
//! and the modules the block includes, which then carry their class-method
//! module and their own `included` block to it.

use crate::app::App;
use crate::dialect::{MethodReceiver, ModelBodyItem};
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
        let lc = &mut app.library_classes[i];
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
