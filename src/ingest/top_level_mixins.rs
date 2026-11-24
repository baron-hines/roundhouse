//! A file's top-level `X.prepend(M)` / `X.include(M)` (redis's
//! `RedisClient::Config.prepend(Redis::LibIdentity)`, an app's
//! `Foo.prepend(Bar)` beside `module Bar`): Ruby runs Module#prepend as the
//! file loads. It is read as an opening of `X` whose body runs
//! `prepend M`, as `class X; prepend M; end` would, so every tree carries
//! it with the class; a file's top level runs on the ruby family alone, and
//! an app file's was dropped. The file does not say whether `X` is a class
//! or a module, so the opening waits for `X`'s own declaration
//! ([`settle`]); one the program does not declare is reported, not dropped.

use ruby_prism::Node;

use crate::App;
use crate::dialect::LibraryClass;
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol};
use crate::span::Span;

use super::util::{constant_id_str, constant_path_of, node_span};

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
        out.push(LibraryClass {
            name: ClassId(Symbol::from(target.join("::"))),
            is_module: false,
            parent: None,
            parent_span: Span::synthetic(),
            includes,
            include_spans,
            methods: Vec::new(),
            nullable_columns: Vec::new(),
            origin: None,
            constants: Vec::new(),
            conditional_definitions: Vec::new(),
            unknown_calls: vec![body],
            class_ivar_initializers: Vec::new(),
        });
    }
    out
}

fn is_opening(lc: &LibraryClass) -> bool {
    lc.methods.is_empty() && !lc.unknown_calls.is_empty() && lc.unknown_calls.iter().all(|c| c.decisions & crate::expr::TOP_LEVEL_MIXIN != 0)
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
    let classes = std::mem::take(&mut app.library_classes);
    for mut lc in classes {
        if !is_opening(&lc) {
            keep.push(lc);
            continue;
        }
        let name = lc.name.0.as_str().to_string();
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
    for (span, why) in refused {
        super::prism::push(crate::diagnostic::Diagnostic::unsupported(span, None, "top-level mixin", why));
    }
}
