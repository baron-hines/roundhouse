//! A file's top-level `X.prepend(M)` / `X.include(M)` (redis's
//! `RedisClient::Config.prepend(Redis::LibIdentity)`, an app's
//! `Foo.prepend(Bar)` beside `module Bar`): Ruby runs Module#prepend as the
//! file loads. It is read as an opening of `X` whose body runs
//! `prepend M`, as `class X; prepend M; end` would, so every tree carries
//! it with the class; a file's top level runs on the ruby family alone, and
//! an app file's was dropped. The file does not say whether `X` is a class
//! or a module, so the opening waits for `X`'s own declaration
//! ([`settle`]); one the program does not declare is reported, not dropped.
//!
//! A constant written into another class (message_bus's
//! `MessageBus::BACKENDS = {}` at a file's top level, redis-client's
//! `CommandError::ERRORS = { … }` in `class RedisClient`) is the same: an
//! opening of that class holding the constant. Ruby finds the class as it
//! finds any constant, innermost namespace first, so the opening is made for
//! each namespace the write sits in and [`settle`] keeps the first one the
//! program declares.

use ruby_prism::Node;

use crate::App;
use crate::dialect::LibraryClass;
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol};
use crate::span::Span;

use super::util::{constant_id_str, constant_path_of, flatten_statements, node_span};

/// `X.include(M, …)` / `X.prepend(M, …)` of constants, with no block: the verb, `X` and the modules, as written.
fn mixin(stmt: &Node<'_>) -> Option<(String, Vec<String>, Vec<Vec<String>>)> {
    let call = stmt.as_call_node()?;
    let verb = constant_id_str(&call.name()).to_string();
    if !matches!(verb.as_str(), "include" | "prepend") || call.block().is_some() {
        return None;
    }
    let target = call.receiver().and_then(|r| constant_path_of(&r))?;
    let args: Vec<Node<'_>> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
    let modules: Vec<Vec<String>> = args.iter().map(constant_path_of).collect::<Option<_>>()?;
    (!modules.is_empty()).then_some((verb, target, modules))
}

/// Whether a file's top-level statement is one [`openings`] reads.
pub(super) fn is_mixin(stmt: &Node<'_>) -> bool {
    mixin(stmt).is_some()
}

/// The openings `root`'s top-level mixins make, one per statement.
pub(super) fn openings(root: &Node<'_>, file: &str) -> Vec<LibraryClass> {
    let Some(program) = root.as_program_node() else { return Vec::new() };
    let mut out = Vec::new();
    constant_writes(program.statements().body().iter().collect(), &[], file, &mut out);
    for stmt in program.statements().body().iter() {
        let Some((verb, target, modules)) = mixin(&stmt) else { continue };
        let span = node_span(&stmt, file);
        let module_exprs: Vec<Expr> = modules
            .iter()
            .map(|path| {
                let mut e = Expr::new(span, ExprNode::Const { path: std::iter::once(Symbol::from("")).chain(path.iter().map(|s| Symbol::from(s.as_str()))).collect() });
                e.decisions |= crate::expr::GENERATED_CONST_REF;
                e
            })
            .collect();
        let mut body = Expr::new(span, ExprNode::Send { recv: None, method: Symbol::from(verb.as_str()), args: module_exprs, block: None, parenthesized: false });
        body.decisions |= crate::expr::REPLAYED_CLASS_BODY | crate::expr::TOP_LEVEL_MIXIN;
        let (includes, include_spans) = if verb == "include" {
            (modules.iter().map(|m| ClassId(Symbol::from(m.join("::")))).collect(), vec![span; modules.len()])
        } else {
            (Vec::new(), Vec::new())
        };
        out.push(opening(target.join("::"), includes, include_spans, Vec::new(), vec![body]));
    }
    out
}

/// `A::B = value` in `statements` and the class and module bodies in them, under the namespaces `scope` names.
fn constant_writes(statements: Vec<Node<'_>>, scope: &[String], file: &str, out: &mut Vec<LibraryClass>) {
    for stmt in statements {
        let declared = stmt.as_module_node().map(|m| (m.constant_path(), m.body()))
            .or_else(|| stmt.as_class_node().map(|c| (c.constant_path(), c.body())));
        if let Some((path, body)) = declared {
            let (Some(path), Some(body)) = (constant_path_of(&path), body) else { continue };
            let inner: Vec<String> = scope.iter().cloned().chain(path).collect();
            constant_writes(flatten_statements(body), &inner, file, out);
            continue;
        }
        let Some(write) = stmt.as_constant_path_write_node() else { continue };
        let target = write.target().as_node();
        let Some(mut path) = constant_path_of(&target) else { continue };
        let Some(name) = path.pop() else { continue };
        if path.is_empty() {
            continue;
        }
        let Ok(mut value) = super::expr::ingest_expr(&write.value(), file) else { continue };
        // A file's top level reads constants from Object; the opening is written inside the class, which would look
        // there first.
        if scope.is_empty() {
            root_constants(&mut value);
        }
        value.decisions |= crate::expr::TOP_LEVEL_MIXIN;
        let rooted = target.location().as_slice().starts_with(b"::");
        let namespaces = if rooted { 0 } else { scope.len() };
        for depth in (0..=namespaces).rev() {
            let owner: Vec<&str> = scope[..depth].iter().chain(path.iter()).map(String::as_str).collect();
            out.push(opening(owner.join("::"), Vec::new(), Vec::new(), vec![(Symbol::from(name.as_str()), value.clone())], Vec::new()));
        }
    }
}

fn root_constants(expr: &mut Expr) {
    if let ExprNode::Const { path } = &mut *expr.node
        && path.first().is_some_and(|part| !part.as_str().is_empty())
    {
        path.insert(0, Symbol::from(""));
    }
    expr.node.for_each_child_mut(&mut root_constants);
}

fn opening(name: String, includes: Vec<ClassId>, include_spans: Vec<Span>, constants: Vec<(Symbol, Expr)>, unknown_calls: Vec<Expr>) -> LibraryClass {
    LibraryClass {
        name: ClassId(Symbol::from(name)),
        is_module: false,
        parent: None,
        parent_span: Span::synthetic(),
        includes,
        include_spans,
        methods: Vec::new(),
        nullable_columns: Vec::new(),
        origin: None,
        constants,
        conditional_definitions: Vec::new(),
        unknown_calls,
        class_ivar_initializers: Vec::new(),
    }
}

fn is_opening(lc: &LibraryClass) -> bool {
    let marked = |e: &Expr| e.decisions & crate::expr::TOP_LEVEL_MIXIN != 0;
    lc.methods.is_empty()
        && (!lc.unknown_calls.is_empty() || !lc.constants.is_empty())
        && lc.unknown_calls.iter().all(marked)
        && lc.constants.iter().all(|(_, v)| marked(v))
}

/// Each opening takes the kind of `X`'s declaration; one whose `X` or module the program does not declare, or whose
/// `X` is a model, is reported and left out.
pub(super) fn settle(app: &mut App) {
    let mut kinds: crate::det::HashMap<String, bool> = crate::det::HashMap::default();
    for c in app.library_classes.iter().filter(|c| !is_opening(c)) {
        kinds.entry(c.name.0.as_str().to_string()).or_insert(c.is_module);
    }
    let declared = |name: &str| kinds.get(name).copied();
    let mut keep = Vec::with_capacity(app.library_classes.len());
    let mut refused: Vec<(Span, String)> = Vec::new();
    // A constant write's openings, one per namespace it may name, share its value's span: the first declared is it.
    let mut settled_writes: crate::det::HashSet<Span> = crate::det::HashSet::default();
    let mut unsettled_writes: Vec<(Span, String)> = Vec::new();
    let classes = std::mem::take(&mut app.library_classes);
    for mut lc in classes {
        if !is_opening(&lc) {
            keep.push(lc);
            continue;
        }
        let name = lc.name.0.as_str().to_string();
        if let Some((constant, value)) = lc.constants.first() {
            let span = value.span;
            if settled_writes.contains(&span) {
                continue;
            }
            if app.models.iter().any(|m| m.name.0.as_str() == name) {
                settled_writes.insert(span);
                refused.push((span, format!("`{name}` is a model: a constant written into one from outside is not carried")));
                continue;
            }
            match declared(&name) {
                Some(is_module) => {
                    settled_writes.insert(span);
                    lc.is_module = is_module;
                    keep.push(lc);
                }
                None => unsettled_writes.push((span, format!("`{name}` (written `{name}::{constant}`) is no class or module the program declares"))),
            }
            continue;
        }
        let span = lc.unknown_calls[0].span;
        let missing_module = lc.unknown_calls.iter().flat_map(|c| match &*c.node {
            ExprNode::Send { args, .. } => args.clone(),
            _ => Vec::new(),
        }).find_map(|a| match &*a.node {
            ExprNode::Const { path } => {
                let full = path.iter().map(|p| p.as_str()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("::");
                declared(&full).filter(|is_module| *is_module).is_none().then_some(full)
            }
            _ => None,
        });
        if app.models.iter().any(|m| m.name.0.as_str() == name) {
            refused.push((span, format!("`{name}` is a model: a top-level mixin into one is not carried")));
            continue;
        }
        let Some(is_module) = declared(&name) else {
            refused.push((span, format!("`{name}` is no class or module the program declares")));
            continue;
        };
        if let Some(module) = missing_module {
            refused.push((span, format!("`{module}` is no module the program declares")));
            continue;
        }
        lc.is_module = is_module;
        keep.push(lc);
    }
    app.library_classes = keep;
    // The outermost namespace's is the last a write makes, and it names the write as written.
    let mut last: Vec<(Span, String)> = Vec::new();
    for (span, why) in unsettled_writes.into_iter().filter(|(span, _)| !settled_writes.contains(span)) {
        last.retain(|(seen, _)| *seen != span);
        last.push((span, why));
    }
    for (span, why) in refused {
        super::prism::push(crate::diagnostic::Diagnostic::unsupported(span, None, "top-level mixin", why));
    }
    for (span, why) in last {
        super::prism::push(crate::diagnostic::Diagnostic::unsupported(span, None, "constant written into another class", why));
    }
}
