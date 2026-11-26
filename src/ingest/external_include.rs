//! `Other.include(M)` in a class or module body, where both `Other` and
//! `M` are the app's own: activemerchant 1.137.0's billing/compatibility.rb
//! ends `module Billing` with
//!
//! ```ruby
//! Compatibility::Model.include Rails::Model
//! ```
//!
//! That gives `Compatibility::Model#valid?`'s `super` its method
//! (`Rails::Model#valid?`). A class-body call on another constant is
//! captured and then dropped by every emit that does not replay the
//! whole body, so `CreditCard#valid?` raised NoMethodError (no superclass
//! method). The call becomes an `include` of `Other` itself, which has
//! the same ancestors: Ruby 3+ gives a module's existing includers a
//! module it includes later. `Other` and `M` are resolved lexically from
//! the body the call is written in, as Ruby resolves them there.
//! Anything else stays captured.
//!
//! A railtie writes the same call inside a boot block (railties'
//! `initializer`, `config.to_prepare`, activesupport's `on_load`):
//!
//! ```ruby
//! initializer "gem.setup" do
//!   Metadata.include RedactableModel
//! end
//! ```
//!
//! When `Other` is an ActiveSupport::Concern, that call adds `M` to its
//! `@_dependencies` (concern.rb `append_features`), so every class that
//! includes `Other` afterwards gets `M`'s `ClassMethods` and `included`
//! block. Ingest lowers the app's concerns into their includers, so the
//! dependency has to be an include of `Other` for the splices to see it.
//! The block itself is left where it is.

use crate::App;
use crate::dialect::LibraryClass;
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol};

pub(super) fn carry(app: &mut App) {
    // A body statement: `Other`'s include, and the statement goes.
    let mut moves: Vec<(usize, usize, ClassId, ClassId)> = Vec::new();
    // An include in a boot block: `Other`'s include, and the block stays.
    let mut dependencies: Vec<(ClassId, ClassId)> = Vec::new();
    for (i, lc) in app.library_classes.iter().enumerate() {
        for (j, call) in lc.unknown_calls.iter().enumerate() {
            if let Some((target, module)) = include_on(call) {
                let Some(target) = resolve(lc, &target, None, &app.library_classes) else { continue };
                let Some(module) = resolve(lc, &module, Some(true), &app.library_classes) else { continue };
                moves.push((i, j, target.name.clone(), module.name.clone()));
                continue;
            }
            boot_block_statements(call, &mut |stmt| {
                let Some((target, module)) = include_on(stmt) else { return };
                let Some(target) = resolve(lc, &target, Some(true), &app.library_classes) else { return };
                let Some(module) = resolve(lc, &module, Some(true), &app.library_classes) else { return };
                dependencies.push((target.name.clone(), module.name.clone()));
            });
        }
    }
    for (i, j, target, module) in moves.into_iter().rev() {
        if include(app, &target, module) {
            app.library_classes[i].unknown_calls.remove(j);
        }
    }
    for (target, module) in dependencies {
        include(app, &target, module);
    }
}

/// `module` among `target`'s includes; false when `target` is not an app
/// class or module of its own.
fn include(app: &mut App, target: &ClassId, module: ClassId) -> bool {
    let Some(t) = app.library_classes.iter_mut().find(|c| c.origin.is_none() && c.name == *target) else {
        return false;
    };
    if *target != module && !t.includes.contains(&module) {
        // A statement of its own: a span no other include shares.
        if t.include_spans.len() == t.includes.len() {
            t.include_spans.push(crate::span::Span::synthetic());
        }
        t.includes.push(module);
    }
    true
}

/// The statements a boot hook (`initializer "x" do … end`,
/// `config.to_prepare { … }`, `ActiveSupport.on_load(:x) { … }`) runs
/// unconditionally: its block's sequence, and the body of a `begin`,
/// through a hook the block itself registers (`on_load` in an
/// `initializer`).
fn boot_block_statements<'a>(call: &'a Expr, f: &mut dyn FnMut(&'a Expr)) {
    fn statements<'a>(expr: &'a Expr, f: &mut dyn FnMut(&'a Expr)) {
        match &*expr.node {
            ExprNode::Seq { exprs } => exprs.iter().for_each(|e| statements(e, f)),
            ExprNode::BeginRescue { body, .. } => statements(body, f),
            _ => {
                f(expr);
                boot_block_statements(expr, f);
            }
        }
    }
    let ExprNode::Send { method, block: Some(block), .. } = &*call.node else { return };
    if !matches!(
        method.as_str(),
        "initializer" | "to_prepare" | "before_initialize" | "after_initialize" | "before_eager_load" | "on_load"
    ) {
        return;
    }
    if let ExprNode::Lambda { body, .. } = &*block.node {
        statements(body, f);
    }
}

/// `A::B.include(C::D)`: the written receiver and module.
fn include_on(call: &Expr) -> Option<(String, String)> {
    let ExprNode::Send { recv: Some(recv), method, args, block: None, .. } = &*call.node else { return None };
    if method.as_str() != "include" {
        return None;
    }
    let [arg] = args.as_slice() else { return None };
    Some((written(recv)?, written(arg)?))
}

fn written(e: &Expr) -> Option<String> {
    let ExprNode::Const { path } = &*e.node else { return None };
    Some(path.iter().map(Symbol::as_str).collect::<Vec<_>>().join("::"))
}

/// `written` as Ruby resolves it from `lc`'s namespace outwards, among
/// the app's own (not reopened native) classes and modules; `module`
/// restricts the kind.
fn resolve<'a>(lc: &LibraryClass, written: &str, module: Option<bool>, classes: &'a [LibraryClass]) -> Option<&'a LibraryClass> {
    // `::X` is the top level's.
    let (written, scope): (&str, Vec<&str>) = match written.strip_prefix("::") {
        Some(top) => (top, Vec::new()),
        None => (written, lc.name.0.as_str().split("::").collect()),
    };
    (0..=scope.len()).rev().find_map(|n| {
        let name = if n == 0 { written.to_string() } else { format!("{}::{written}", scope[..n].join("::")) };
        classes
            .iter()
            .find(|c| module.is_none_or(|m| c.is_module == m) && c.origin.is_none() && c.name.0.as_str() == name)
    })
}
