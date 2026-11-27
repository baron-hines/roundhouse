//! `X = Class.new(Parent)` as the class a `class X < Parent` would make.
//!
//! At file level Ruby sets the constant on Object; in a class or module body
//! the constant is under the owner. A parameterless block of class-body
//! statements (`def`, `attr_*`, visibility, `include`/`extend` of a constant,
//! alias) is the class body — Ruby runs it with the new class as `self`. A
//! constant, class variable or nested `class`/`module` in the block belongs
//! to the enclosing scope, so those blocks stay constants.

use crate::dialect::LibraryClass;
use crate::ident::{ClassId, Symbol};
use crate::library_class_order;

use super::library_class::{DeclBody, DeclBodyMode, walk_decl_body};
use super::util::{constant_id_str, constant_path_of, flatten_statements, node_span, symbol_value};

/// Classes a body defines through `X = Class.new(…)`, with the constant names
/// they replace. `owner` is `None` at file level (top-level class on Object).
pub(super) fn constant_classes(
    owner: Option<&ClassId>,
    body: Option<ruby_prism::Node<'_>>,
    file: &str,
) -> Vec<(Symbol, LibraryClass)> {
    let mut out: Vec<(Symbol, LibraryClass)> = Vec::new();
    let Some(body) = body else { return out };
    let qualified = |name: &str| match owner {
        Some(owner) => format!("{}::{name}", owner.0.as_str()),
        None => name.to_string(),
    };
    for stmt in flatten_statements(body) {
        let Some(ClassNewFactory { name, parent: parent_node, body: block_body }) = factory(&stmt) else {
            continue;
        };
        let Some(written) = constant_path_of(&parent_node) else { continue };
        let rooted = parent_node
            .as_constant_path_node()
            .is_some_and(|p| super::util::constant_path_is_rooted(&p));
        let written: Vec<String> = written.into_iter().filter(|s| !s.is_empty()).collect();
        let parent = match written.as_slice() {
            [one] if !rooted && out.iter().any(|(n, _)| n.as_str() == one) => qualified(one),
            // Kept `::`-rooted: emitted inside the owner, a bare name would resolve to the sibling.
            _ if rooted && owner.is_some() => format!("::{}", written.join("::")),
            _ => written.join("::"),
        };
        let id = ClassId(Symbol::from(qualified(name.as_str())));
        // A block the class-body walk refuses stays a constant, as before.
        let Ok(DeclBody {
            includes,
            include_spans,
            methods,
            constants,
            unknown_calls,
            class_initializers,
            class_attributes: _, conditional_definitions,
        }) = walk_decl_body(block_body, &id, file, DeclBodyMode::Instance)
        else {
            continue;
        };
        out.push((
            name,
            LibraryClass {
                name: id,
                is_module: false,
                parent: Some(ClassId(Symbol::from(parent))),
                parent_span: node_span(&parent_node, file),
                includes,
                include_spans,
                methods,
                nullable_columns: Vec::new(),
                origin: None,
                constants,
                conditional_definitions,
                unknown_calls,
                class_ivar_initializers: class_initializers,
            },
        ));
    }
    out
}

/// True when the program body has a `Class.new` factory this module admits.
pub(super) fn file_has_factory(root: &ruby_prism::Node<'_>) -> bool {
    root.as_program_node().is_some_and(|program| {
        flatten_statements(program.statements().as_node())
            .iter()
            .any(|stmt| factory(stmt).is_some())
    })
}

/// Combine file-level factories with the file's other classes so a same-file
/// superclass is defined before any subclass that names it. Factories whose
/// parents are outside the file still precede classes that depend on them.
pub(super) fn order_with_file_classes(
    factories: Vec<LibraryClass>,
    rest: Vec<LibraryClass>,
) -> Vec<LibraryClass> {
    if factories.is_empty() {
        return rest;
    }
    let mut pending = factories;
    pending.extend(rest);
    library_class_order::order_by_deps(
        pending,
        |class| class.name.0.as_str(),
        |class| {
            class
                .parent
                .iter()
                .chain(class.includes.iter())
                .map(|dep| dep.0.as_str().to_string())
                .collect()
        },
    )
}

/// `NAME = Class.new(Parent)` with one constant argument, and no block or a
/// parameterless block of class-body statements: the class a
/// `class NAME < Parent` with that body makes. An empty block stays a
/// constant for the checker's `Class.new` pass and its mutation guards.
struct ClassNewFactory<'pr> {
    name: Symbol,
    parent: ruby_prism::Node<'pr>,
    body: Option<ruby_prism::Node<'pr>>,
}

fn factory<'pr>(stmt: &ruby_prism::Node<'pr>) -> Option<ClassNewFactory<'pr>> {
    let cw = stmt.as_constant_write_node()?;
    let call = cw.value().as_call_node()?;
    if constant_id_str(&call.name()) != "new"
        || constant_path_of(&call.receiver()?)?
            .into_iter()
            .filter(|s| !s.is_empty())
            .ne(["Class".to_string()])
    {
        return None;
    }
    let mut args = call.arguments()?.arguments().iter();
    let (Some(parent), None) = (args.next(), args.next()) else {
        return None;
    };
    constant_path_of(&parent)?;
    let body = match call.block() {
        None => None,
        Some(block) => {
            let block = block.as_block_node()?;
            let body = block.body()?;
            let statements = body.as_statements_node()?.body();
            if block.parameters().is_some()
                || statements.iter().next().is_none()
                || !statements.iter().all(|stmt| class_body_statement(&stmt))
            {
                return None;
            }
            Some(body)
        }
    };
    Some(ClassNewFactory {
        name: Symbol::from(constant_id_str(&cw.name())),
        parent,
        body,
    })
}

/// A statement that does in a `Class.new` block what it does in a `class`
/// body. Ruby runs the block with the new class as `self`, so a `def`, an
/// `attr_*` accessor, a visibility marker, an `include` or `extend` of a
/// constant, or an alias acts on that class. A constant, a class variable
/// or a `class`/`module` written in the block belongs to the enclosing
/// scope instead, so a block with one of those, or with any other
/// statement, does not make the class a `class` body makes.
fn class_body_statement(stmt: &ruby_prism::Node<'_>) -> bool {
    if stmt.as_def_node().is_some() || stmt.as_alias_method_node().is_some() {
        return true;
    }
    let Some(call) = stmt.as_call_node() else {
        return false;
    };
    if call.receiver().is_some() || call.block().is_some() {
        return false;
    }
    let args: Vec<_> = call
        .arguments()
        .map(|args| args.arguments().iter().collect())
        .unwrap_or_default();
    let symbols = |args: &[ruby_prism::Node<'_>]| args.iter().all(|arg| symbol_value(arg).is_some());
    match constant_id_str(&call.name()) {
        "attr_reader" | "attr_writer" | "attr_accessor" => !args.is_empty() && symbols(&args),
        "private" | "protected" | "public" => {
            symbols(&args) || matches!(args.as_slice(), [def] if def.as_def_node().is_some())
        }
        "include" | "extend" => !args.is_empty() && args.iter().all(|arg| constant_path_of(arg).is_some()),
        "alias_method" => args.len() == 2 && symbols(&args),
        _ => false,
    }
}
