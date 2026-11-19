//! The pagy gem's `Pagy::Method`, as an app wires it: a concern that
//! includes the module and WRAPS `pagy`.
//!
//! The gem's own half is runtime (runtime/ruby/action_controller/pagy.rb,
//! `pagy` on `ActionController::Base`) plus the analysis
//! (`analyze::body::pagy`). What an app adds on top is ordinary Ruby, and
//! EngineeredAt's is the usual shape for turning the gem's quiet
//! out-of-range page (an empty page, `page > last`) into a 404:
//!
//! ```ruby
//! module Pagination
//!   extend ActiveSupport::Concern
//!
//!   module BoundsCheck
//!     def pagy(collection, **vars)
//!       pagy, records = super
//!       raise ActiveRecord::RecordNotFound if pagy.page > pagy.last
//!       [pagy, records]
//!     end
//!   end
//!
//!   included do
//!     include Pagy::Method
//!     prepend BoundsCheck
//!     rescue_from Pagy::OptionError do
//!       raise ActiveRecord::RecordNotFound
//!     end
//!   end
//! end
//! ```
//!
//! No target replays an `included do` block of a module (the include is
//! kept, the block is inert, and the ledger says so), so without this pass
//! the guard is defined and never in the lookup chain: every page past the
//! end would render empty with a 200, where the app answers 404. This pass
//! reads the block for the two statements that matter:
//!
//! * `prepend M`, where `M` is a module that defines `pagy`: becomes a
//!   `prepend M` in the body of every controller that includes the
//!   concern, which is the form the controller concern splice already
//!   reads (it copies `M#pagy` into the controller, `super` and all, so
//!   `super` reaches `Base#pagy`). A controller that defines `pagy`
//!   itself is refused: the splice would keep its method over the
//!   wrapper, where `prepend` runs the wrapper first.
//! * `include Pagy::Method`: consumed. `pagy` is on every controller of an
//!   app that locks the gem (see the runtime file's header), so the
//!   include has nothing left to do.
//!
//! The rest of the block -- the `rescue_from Pagy::OptionError` -- stays in
//! `unknown_calls` and is reported as dropped, as before. It is
//! unreachable for what is modeled: `Pagy::OptionError` comes from an
//! invalid `limit:`, which the analysis only admits as a literal of at
//! least 1.
//!
//! Gated on the lockfile (`pagy`), like the other gem models.

use crate::dialect::{ControllerBodyItem, LibraryClass, MethodReceiver};
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol};

fn const_path(expr: &Expr) -> Option<Vec<String>> {
    let ExprNode::Const { path } = &*expr.node else { return None };
    Some(path.iter().map(|s| s.as_str().to_string()).filter(|s| !s.is_empty()).collect())
}

/// Statements of the concern's `included do … end` blocks, with the
/// owning `unknown_calls` index and the statement index.
fn included_statements(lc: &LibraryClass) -> Vec<(usize, usize, &Expr)> {
    let mut out = Vec::new();
    for (ci, call) in lc.unknown_calls.iter().enumerate() {
        let ExprNode::Send { recv: None, method, block: Some(block), .. } = &*call.node else { continue };
        if method.as_str() != "included" {
            continue;
        }
        let ExprNode::Lambda { body, .. } = &*block.node else { continue };
        match &*body.node {
            ExprNode::Seq { exprs } => out.extend(exprs.iter().enumerate().map(|(si, e)| (ci, si, e))),
            _ => out.push((ci, 0, body)),
        }
    }
    out
}

/// `pagy, records = super` ... `[pagy, records]`: a `pagy` that hands back
/// the pair the gem's gave it (guards and raises in between are fine).
/// That is the only override the analysis can type like the gem's own
/// (`analyze::body::pagy`): one that returns something else, or never
/// asks the gem, is a different method.
fn passes_through(body: &Expr) -> bool {
    use crate::expr::LValue;
    let stmts: Vec<&Expr> = match &*body.node {
        ExprNode::Seq { exprs } => exprs.iter().collect(),
        _ => vec![body],
    };
    let (Some(first), Some(last)) = (stmts.first(), stmts.last()) else { return false };
    let ExprNode::MultiAssign { targets, value } = &*first.node else { return false };
    if !matches!(&*value.node, ExprNode::Super { .. }) {
        return false;
    }
    let names: Vec<&Symbol> = targets
        .iter()
        .filter_map(|t| match t {
            LValue::Var { name, .. } => Some(name),
            _ => None,
        })
        .collect();
    if names.len() != 2 || targets.len() != 2 {
        return false;
    }
    let ExprNode::Array { elements, .. } = &*last.node else { return false };
    elements.len() == 2
        && elements.iter().zip(&names).all(|(e, n)| matches!(&*e.node, ExprNode::Var { name, .. } if name == *n))
        // Nothing in between may rebind either name, however nested
        // (`pagy = other if cond`, `||=`, a block, a `rescue => pagy`).
        && !stmts[1..stmts.len() - 1].iter().any(|st| writes_any(st, &names))
        // Nor may an early `return` / method-level `next` hand back anything
        // but the pair itself.
        && !stmts.iter().any(|st| leaves_other_than_pair(st, &names, false))
}

/// Does a `case/in` / `=>` / `in` pattern bind one of `names`?
fn binds_any(pattern: &crate::expr::MatchPattern, names: &[&Symbol]) -> bool {
    let mut bound = Vec::new();
    pattern.bound_names(&mut bound);
    bound.iter().any(|b| names.contains(&b))
}

/// Does `e`, or anything under it, `return` (or `next` out of the method
/// body itself, not out of a block) something other than `[a, b]` in the
/// names' order?
fn leaves_other_than_pair(e: &Expr, names: &[&Symbol], in_block: bool) -> bool {
    let is_pair = |v: &Expr| {
        matches!(&*v.node, ExprNode::Array { elements, .. } if elements.len() == 2
            && elements.iter().zip(names).all(|(x, n)| matches!(&*x.node, ExprNode::Var { name, .. } if name == *n)))
    };
    let here = match &*e.node {
        ExprNode::Return { value } => !is_pair(value),
        ExprNode::Next { value } if !in_block => !value.as_ref().is_some_and(is_pair),
        _ => false,
    };
    let in_block = in_block || matches!(&*e.node, ExprNode::Lambda { .. });
    let mut nested = false;
    e.node.for_each_child(&mut |c| nested = nested || leaves_other_than_pair(c, names, in_block));
    here || nested
}

/// Does `e`, or anything under it, write one of `names` as a local?
fn writes_any(e: &Expr, names: &[&Symbol]) -> bool {
    use crate::expr::LValue;
    let hits = |t: &LValue| matches!(t, LValue::Var { name, .. } if names.contains(&name));
    let here = match &*e.node {
        ExprNode::Assign { target, .. } | ExprNode::OpAssign { target, .. } => hits(target),
        ExprNode::MultiAssign { targets, .. } => targets.iter().any(hits),
        ExprNode::BeginRescue { rescues, .. } => rescues.iter().any(|r| r.binding.as_ref().is_some_and(|b| names.contains(&b))),
        ExprNode::CaseMatch { arms, .. } => arms.iter().any(|a| binds_any(&a.pattern, names)),
        ExprNode::MatchPredicate { pattern, .. } | ExprNode::MatchRequired { pattern, .. } => binds_any(pattern, names),
        _ => false,
    };
    let mut nested = false;
    e.node.for_each_child(&mut |c| nested = nested || writes_any(c, names));
    here || nested
}

pub(super) fn lower_pagy(app: &mut crate::App) -> super::IngestResult<()> {
    if !app.gem_lock.as_ref().is_some_and(|lock| lock.has("pagy")) {
        return Ok(());
    }
    let defines_pagy = |app: &crate::App, name: &str| {
        app.library_classes.iter().any(|lc| {
            lc.name.0.as_str() == name
                && lc.methods.iter().any(|m| m.name.as_str() == "pagy" && m.receiver == MethodReceiver::Instance)
        })
    };

    // (module index, [(call index, statement index)] to consume, wrapper, concern)
    let mut found: Vec<(usize, Vec<(usize, usize)>, Vec<String>, ClassId)> = Vec::new();
    for (li, lc) in app.library_classes.iter().enumerate() {
        if !lc.is_module {
            continue;
        }
        let mut consume = Vec::new();
        let mut wrappers = Vec::new();
        for (ci, si, stmt) in included_statements(lc) {
            let ExprNode::Send { recv: None, method, args, block: None, .. } = &*stmt.node else { continue };
            let [arg] = args.as_slice() else { continue };
            let Some(path) = const_path(arg) else { continue };
            match method.as_str() {
                "include" if path == ["Pagy", "Method"] => consume.push((ci, si)),
                "prepend" => {
                    // Lexically nearest first: `BoundsCheck` inside
                    // `Pagination` is `Pagination::BoundsCheck`.
                    let nested = format!("{}::{}", lc.name.0.as_str(), path.join("::"));
                    let plain = path.join("::");
                    let target = [nested, plain].into_iter().find(|n| defines_pagy(app, n));
                    if let Some(target) = target {
                        consume.push((ci, si));
                        wrappers.push(target);
                    }
                }
                _ => {}
            }
        }
        if !consume.is_empty() || !wrappers.is_empty() {
            found.push((li, consume, wrappers, lc.name.clone()));
        }
    }

    // Every `pagy` the app defines for a controller must wrap the gem's.
    let mut included: Vec<ClassId> = Vec::new();
    for controller in &app.controllers {
        for inc in crate::analyze::controller_includes(controller) {
            if !included.contains(&inc) {
                included.push(inc);
            }
        }
    }
    let prepended: Vec<String> = found.iter().flat_map(|(_, _, w, _)| w.iter().cloned()).collect();
    // `pagy_array` too: the analysis types a call to it as the gem's pair whatever the app's own returns.
    let array_extra = locked_before_43(app) && app.pagy.array;
    let gem_method = |name: &str| name == "pagy" || (array_extra && name == "pagy_array");
    let mut refusals: Vec<(String, String, String)> = Vec::new();
    for controller in &app.controllers {
        for action in controller.actions().filter(|a| gem_method(a.name.as_str())) {
            if !passes_through(&action.body) {
                refusals.push((controller.name.0.as_str().to_string(), controller.name.0.as_str().to_string(), action.name.as_str().to_string()));
            }
        }
    }
    for lc in &app.library_classes {
        let defines = lc.methods.iter().filter(|m| gem_method(m.name.as_str()) && m.receiver == MethodReceiver::Instance);
        for method in defines {
            let wrapped = included.contains(&lc.name) || prepended.contains(&lc.name.0.as_str().to_string());
            if wrapped && !passes_through(&method.body) {
                refusals.push((lc.name.0.as_str().to_string(), lc.name.0.as_str().to_string(), method.name.as_str().to_string()));
            }
        }
    }
    for (file, owner, method) in refusals {
        super::survey::continue_or_fail(super::IngestError::Unsupported {
            file,
            message: format!(
                "`{owner}#{method}` overrides the pagy gem's `{method}` without handing back the `[pagy, records]` pair \
                 `super` returned (`pagy, records = super` ... `[pagy, records]`): not modeled"
            ),
        })?;
    }
    for (li, consume, wrappers, concern) in found {
        for wrapper in &wrappers {
            for ci in 0..app.controllers.len() {
                if !crate::analyze::controller_includes(&app.controllers[ci]).contains(&concern) {
                    continue;
                }
                let name = app.controllers[ci].name.0.as_str().to_string();
                // The splice keeps a controller's own definition over the
                // module's, which is the opposite of `prepend`: the
                // wrapper would be silently skipped. Refuse that shape.
                if app.controllers[ci].actions().any(|a| a.name.as_str() == "pagy") {
                    super::survey::continue_or_fail(super::IngestError::Unsupported {
                        file: name.clone(),
                        message: format!(
                            "`{name}` defines `pagy` itself and also gets `prepend {wrapper}` from `{}`: a prepended wrapper would run ahead of it, which is not modeled",
                            concern.0.as_str()
                        ),
                    })?;
                    continue;
                }
                let span = crate::span::Span::synthetic();
                let target = Expr::new(
                    span,
                    ExprNode::Const { path: wrapper.split("::").map(Symbol::from).collect() },
                );
                let prepend = Expr::new(
                    span,
                    ExprNode::Send {
                        recv: None,
                        method: Symbol::from("prepend"),
                        args: vec![target],
                        block: None,
                        parenthesized: false,
                    },
                );
                app.controllers[ci].body.push(ControllerBodyItem::Unknown {
                    expr: prepend,
                    leading_comments: Vec::new(),
                    leading_blank_line: false,
                });
            }
        }
        // Drop what was consumed from the blocks, highest index first;
        // an emptied block goes whole.
        let lc = &mut app.library_classes[li];
        let mut by_call: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
        for (ci, si) in consume {
            by_call.entry(ci).or_default().push(si);
        }
        let mut emptied = Vec::new();
        for (ci, mut stmts) in by_call {
            stmts.sort_unstable();
            let ExprNode::Send { block: Some(block), .. } = &mut *lc.unknown_calls[ci].node else { continue };
            let ExprNode::Lambda { body, .. } = &mut *block.node else { continue };
            let mut exprs: Vec<Expr> = match &*body.node {
                ExprNode::Seq { exprs } => exprs.clone(),
                _ => vec![body.clone()],
            };
            for si in stmts.into_iter().rev() {
                exprs.remove(si);
            }
            if exprs.is_empty() {
                emptied.push(ci);
            } else {
                *body = Expr::new(body.span, ExprNode::Seq { exprs });
            }
        }
        for ci in emptied.into_iter().rev() {
            lc.unknown_calls.remove(ci);
        }
    }
    Ok(())
}

/// Whether the app locks a pagy 9, whose API (`runtime/ruby/action_controller/pagy9.rb`) is another one than 43's.
/// Not every major before 43: 8 and earlier read `items:` as the page size, which pagy9.rb does not.
pub fn locked_before_43(app: &crate::App) -> bool {
    locked_major(app) == Some(9)
}

/// The locked pagy's major version.
pub fn locked_major(app: &crate::App) -> Option<u32> {
    app.gem_lock.as_ref().and_then(|lock| lock.version_of("pagy")).and_then(|v| v.split('.').next()?.parse::<u32>().ok())
}

/// The initializers' pagy extras and `Pagy::DEFAULT[...] =` settings (`:overflow` and `:limit`), in
/// load order: `require "pagy/extras/overflow"` sets the extra's own
/// `:empty_page`, and a later `Pagy::DEFAULT[:overflow] =` replaces it.
pub(super) fn load_settings<V: crate::vfs::Vfs + ?Sized>(
    vfs: &V,
    dir: &std::path::Path,
) -> super::IngestResult<crate::app::PagyConfig> {
    let init_dir = dir.join("config/initializers");
    let mut state = Settings::default();
    if vfs.is_dir(&init_dir) {
        for entry in super::app::read_rb_files(vfs, &init_dir)? {
            if let Ok(bytes) = vfs.read(&entry) {
                read_settings(&bytes, &entry.display().to_string(), &mut state);
            }
        }
    }
    Ok(crate::app::PagyConfig {
        overflow: state.overflow_extra.then(|| state.overflow.unwrap_or_else(|| "empty_page".into())),
        array: state.array,
        limit: state.limit,
        unmodeled: state.unmodeled,
    })
}

#[derive(Default)]
pub(super) struct Settings {
    overflow_extra: bool,
    overflow: Option<String>,
    array: bool,
    limit: Option<i64>,
    unmodeled: Vec<(crate::span::Span, String)>,
    /// Whether any statement was a `Pagy::DEFAULT[...] =` (the initializer ledger's "consumed").
    pub(super) recognized: bool,
}

pub(super) fn read_settings(source: &[u8], file: &str, state: &mut Settings) {
    if !String::from_utf8_lossy(source).contains("agy") {
        return;
    }
    let result = super::prism::parse(source, file);
    let root = result.node();
    let Some(program) = root.as_program_node() else { return };
    let span = |node: &ruby_prism::Node<'_>| {
        let (start, end) = super::prism_api::span_offsets(&node.location());
        crate::span::Span { file: super::sources::file_id(file), start, end }
    };
    for stmt in super::app::initializer_statements(&program) {
        let Some(call) = stmt.as_call_node() else { continue };
        let name = super::util::constant_id_str(&call.name());
        let args: Vec<ruby_prism::Node<'_>> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
        if name == "require" && call.receiver().is_none() {
            let Some(path) = args.first().and_then(|a| a.as_string_node()) else { continue };
            let path = String::from_utf8_lossy(path.unescaped()).into_owned();
            let Some(extra) = path.strip_prefix("pagy/extras/") else { continue };
            match extra {
                // Not on a second `require`: Ruby loads a feature once, so its `DEFAULT[:overflow] = :empty_page` runs once.
                "overflow" if !state.overflow_extra => {
                    state.overflow_extra = true;
                    state.overflow = Some("empty_page".into());
                }
                "overflow" => {}
                "array" => state.array = true,
                _ => state.unmodeled.push((span(&stmt), format!("the `pagy/extras/{extra}` extra"))),
            }
            continue;
        }
        if name != "[]=" {
            continue;
        }
        let on_default = call.receiver().is_some_and(|r| {
            matches!(String::from_utf8_lossy(r.location().as_slice()).trim_start_matches("::"), "Pagy::DEFAULT")
        });
        if !on_default {
            continue;
        }
        state.recognized = true;
        let key = args.first().and_then(|k| k.as_symbol_node()).map(|k| String::from_utf8_lossy(k.unescaped()).into_owned());
        let value = args.get(1).and_then(|v| v.as_symbol_node()).map(|v| String::from_utf8_lossy(v.unescaped()).into_owned());
        let limit = args.get(1).and_then(|v| v.as_integer_node()).and_then(|i| super::util::integer_i64(&i.value())).filter(|n| *n >= 1);
        match (key.as_deref(), value) {
            (Some("overflow"), Some(mode)) if matches!(mode.as_str(), "last_page" | "empty_page" | "exception") => state.overflow = Some(mode),
            (Some("limit"), _) if limit.is_some() => state.limit = limit,
            (Some(key), _) => state.unmodeled.push((span(&stmt), format!("`Pagy::DEFAULT[:{key}] =` with this value"))),
            (None, _) => state.unmodeled.push((span(&stmt), "`Pagy::DEFAULT[...] =` with a key that is not a Symbol".into())),
        }
    }
}
