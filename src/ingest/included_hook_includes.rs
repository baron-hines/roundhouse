//! A plain module whose `self.included(base)` hook mixes another module
//! into every class that includes it:
//!
//! ```ruby
//! module Shippable
//!   def self.included(base)
//!     base.class_eval do
//!       include Weighed
//!     end
//!   end
//! end
//! ```
//!
//! The hook itself survives emission and runs at load time, so a class
//! that includes `Shippable` still gets `Weighed`'s instance methods. What
//! it cannot supply is the half `ActiveSupport::Concern` provides: the
//! emitted modules carry no Concern, so
//! `Weighed.included(Shippable)` never runs its `included do` block
//! (a `scope`, a callback, an accessor) and `Weighed::ClassMethods` is
//! never extended into the includer.
//!
//! So the include is written into each includer as if the source had
//! written it there, right after the `include` that reaches the hook: a
//! model's body takes an `include` statement, which the concern splice
//! carries the block's items through; a library class takes it among its
//! includes, which `library_concern_items` reads the same way.
//!
//! Only those two shapes are read, as the LEADING top-level statements of
//! the hook whose receiver is its own parameter: the scan stops at the
//! first statement of any other shape (a condition, a guard, another
//! receiver, `send(:include, …)`), which is left as written, and the
//! emitted program behaves as it did before.
//!
//! A reopened module's later `self.included` REPLACES the earlier one, as
//! Ruby redefines the method: only the last definition is read.

use crate::det::{HashMap, HashSet};

use crate::App;
use crate::dialect::{MethodReceiver, ModelBodyItem};
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol};

pub(super) fn carry(app: &mut App) {
    let mut hops: HashMap<ClassId, Vec<ClassId>> = HashMap::default();
    for lc in app.library_classes.iter().filter(|lc| lc.is_module) {
        // The LAST definition of `self.included` is the live hook: a
        // reopened module's later opening REPLACES the earlier hook
        // rather than adding to it. A later hook that includes nothing
        // clears the earlier expansion, and one this scan cannot read
        // (no single `base` parameter) is still a replacement — expanding
        // the stale earlier hook would mix a module in where Rails does
        // not.
        let Some(method) = lc
            .methods
            .iter()
            .filter(|m| m.receiver == MethodReceiver::Class && m.name.as_str() == "included")
            .next_back()
        else {
            continue;
        };
        let mut included: Vec<ClassId> = Vec::new();
        if let [base] = method.params.as_slice() {
            let mut written = Vec::new();
            hook_includes(&method.body, &base.name, &mut written);
            for path in written {
                if let Some(module) = resolve(&lc.name, &path, app) {
                    included.push(module);
                }
            }
        }
        hops.insert(lc.name.clone(), included);
    }
    if hops.is_empty() {
        return;
    }
    for model in &mut app.models {
        let mut known: HashSet<ClassId> = crate::analyze::model_includes(model).into_iter().collect();
        let mut i = 0;
        while i < model.body.len() {
            let Some((mixin, span)) = include_statement(&model.body[i]) else {
                i += 1;
                continue;
            };
            let mut queue = hops.get(&mixin).cloned().unwrap_or_default();
            let mut added: Vec<ModelBodyItem> = Vec::new();
            let mut next = 0;
            while next < queue.len() {
                let id = queue[next].clone();
                next += 1;
                if !known.insert(id.clone()) {
                    continue;
                }
                // `Shippable`'s own hook may include a module with a hook.
                queue.extend(hops.get(&id).cloned().unwrap_or_default());
                added.push(ModelBodyItem::Unknown {
                    expr: include_expr(&id, span),
                    leading_comments: Vec::new(),
                    leading_blank_line: false,
                });
            }
            let count = added.len();
            if count > 0 {
                model.body.splice(i + 1..i + 1, added);
                i += count;
            }
            i += 1;
        }
    }
    // A library class keeps its includes as ids, and the concern items a
    // module's block carries are spliced from them.
    let mut additions: Vec<(usize, ClassId)> = Vec::new();
    for (index, lc) in app.library_classes.iter().enumerate() {
        // Not a module's: a concern included by a concern runs the hook on
        // that one's includers. Not a synthesized class's either: it has no
        // source include to reach a hook.
        if lc.is_module || lc.origin.is_some() {
            continue;
        }
        let mut known: HashSet<ClassId> = lc.includes.iter().cloned().collect();
        let mut queue: Vec<ClassId> = lc.includes.iter().filter_map(|id| hops.get(id)).flatten().cloned().collect();
        let mut next = 0;
        while next < queue.len() {
            let id = queue[next].clone();
            next += 1;
            if !known.insert(id.clone()) {
                continue;
            }
            queue.extend(hops.get(&id).cloned().unwrap_or_default());
            additions.push((index, id));
        }
    }
    for (index, module) in additions {
        let lc = &mut app.library_classes[index];
        // The include's span is the statement that reached the hook; the
        // include is not written in this file, so a synthetic one keeps it
        // out of the source-ordered body.
        lc.includes.push(module);
        lc.include_spans.push(crate::span::Span::synthetic());
    }
}

/// The `include X` a model body item is, with its span.
fn include_statement(item: &ModelBodyItem) -> Option<(ClassId, crate::span::Span)> {
    let ModelBodyItem::Unknown { expr, .. } = item else { return None };
    let ExprNode::Send { recv: None, method, args, block: None, .. } = &*expr.node else { return None };
    if method.as_str() != "include" {
        return None;
    }
    let [arg] = args.as_slice() else { return None };
    let ExprNode::Const { path } = &*arg.node else { return None };
    Some((crate::analyze::mixin_path_id(path), expr.span))
}

fn include_expr(module: &ClassId, span: crate::span::Span) -> Expr {
    let path = module.0.as_str().split("::").map(Symbol::from).collect();
    Expr::new(
        span,
        ExprNode::Send {
            recv: None,
            method: Symbol::from("include"),
            args: vec![Expr::new(span, ExprNode::Const { path })],
            block: None,
            parenthesized: false,
        },
    )
}

/// The module paths a hook's body includes on its parameter `base`, as
/// top-level statements: `base.include X` and `base.class_eval { include
/// X }`. The scan stops at the first top-level statement that is not one
/// of those — a `return unless base < ActiveRecord::Base` guard, a
/// condition around the include — because what follows it may not run on
/// this includer, and a module expanded into every includer that the guard
/// excluded is a method where Rails has none.
fn hook_includes(body: &Expr, base: &Symbol, out: &mut Vec<String>) {
    for statement in statements(body) {
        let ExprNode::Send { recv: Some(recv), method, args, block, .. } = &*statement.node else { break };
        if !matches!(&*recv.node, ExprNode::Var { name, .. } if name == base) {
            break;
        }
        match (method.as_str(), block) {
            ("class_eval" | "module_eval" | "class_exec" | "module_exec", Some(block)) => {
                let body = match &*block.node {
                    ExprNode::Lambda { body, .. } => body,
                    _ => block,
                };
                for statement in statements(body) {
                    if let ExprNode::Send { recv: None, method, args, block: None, .. } = &*statement.node
                        && method.as_str() == "include"
                    {
                        out.extend(args.iter().filter_map(written));
                    }
                }
            }
            ("include", None) => out.extend(args.iter().filter_map(written)),
            _ => break,
        }
    }
}

/// A body's statements: a sequence's, or the one expression.
fn statements(body: &Expr) -> &[Expr] {
    match &*body.node {
        ExprNode::Seq { exprs } => exprs,
        _ => std::slice::from_ref(body),
    }
}

fn written(expr: &Expr) -> Option<String> {
    let ExprNode::Const { path } = &*expr.node else { return None };
    Some(path.iter().map(Symbol::as_str).collect::<Vec<_>>().join("::"))
}

/// `written` as Ruby resolves it from `owner`'s namespace outwards, among
/// the app's own modules and classes.
fn resolve(owner: &ClassId, written: &str, app: &App) -> Option<ClassId> {
    let (written, scope): (&str, Vec<&str>) = match written.strip_prefix("::") {
        Some(top) => (top, Vec::new()),
        None => (written, owner.0.as_str().split("::").collect()),
    };
    (0..=scope.len()).rev().find_map(|n| {
        let name = if n == 0 { written.to_string() } else { format!("{}::{written}", scope[..n].join("::")) };
        app.library_classes
            .iter()
            .find(|c| c.origin.is_none() && c.name.0.as_str() == name)
            .map(|c| c.name.clone())
    })
}
