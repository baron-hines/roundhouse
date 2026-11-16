use ruby_prism::{CallNode, Node};

use crate::dialect::{LibraryClass, LibraryClassOrigin};
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol};

use super::library_class::ingest_library_method_with_keywords;
use super::util::{constant_id_str, constant_path_of, flatten_statements, node_span};
use super::visibility::{self, Visibility};
use super::{IngestError, IngestResult, ingest_expr};

/// Recognize a constant-assigned `Data.define` block containing definitions or
/// visibility markers. This identifies a lifting candidate, not a validated
/// built-in factory; block-shape and constant-identity checks happen separately.
pub(super) fn declaration<'pr>(node: &Node<'pr>) -> Option<CallNode<'pr>> {
    let write = node.as_constant_write_node()?;
    let call = write.value().as_call_node()?;
    if constant_id_str(&call.name()) != "define"
        || constant_path_of(&call.receiver()?)?.join("::") != "Data"
        || call.block().is_none()
    {
        return None;
    }
    let body = call.block()?.as_block_node()?.body()?;
    flatten_statements(body)
        .iter()
        .any(|statement| {
            visibility::definition(statement).is_some()
                || statement
                    .as_call_node()
                    .is_some_and(|call| visibility::marker(&call))
        })
        .then_some(call)
}

/// Ingest the factory receiver and arguments without its lifted block. Retain
/// the call's source span so emission can reunite it with the collected methods
/// at the original constant declaration rather than hoisting a separate class.
pub(super) fn head(call: &CallNode<'_>, file: &str) -> IngestResult<Expr> {
    let recv = call
        .receiver()
        .map(|node| ingest_expr(&node, file))
        .transpose()?;
    let args = call
        .arguments()
        .map(|args| {
            args.arguments()
                .iter()
                .map(|node| ingest_expr(&node, file))
                .collect()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(Expr::new(
        node_span(&call.as_node(), file),
        ExprNode::Send {
            recv,
            method: Symbol::from("define"),
            args,
            block: None,
            parenthesized: true,
        },
    ))
}

/// `do |klass| … end` whose one required parameter no statement of the
/// block reads. Ruby passes the new class to a `Data.define`/`Struct.new`
/// block; a parameter the block never reads changes nothing.
pub(super) fn unread_class_parameter(block: &ruby_prism::BlockNode<'_>) -> bool {
    let Some(params) = block.parameters().and_then(|p| p.as_block_parameters_node()) else { return false };
    if params.locals().iter().next().is_some() {
        return false;
    }
    let Some(list) = params.parameters() else { return false };
    let required: Vec<_> = list.requireds().iter().collect();
    let only_required = list.optionals().iter().next().is_none()
        && list.rest().is_none()
        && list.posts().iter().next().is_none()
        && list.keywords().iter().next().is_none()
        && list.keyword_rest().is_none()
        && list.block().is_none();
    let [param] = required.as_slice() else { return false };
    let Some(param) = param.as_required_parameter_node() else { return false };
    if !only_required {
        return false;
    }
    let name = constant_id_str(&param.name()).to_string();
    struct Reads<'a>(&'a str, bool);
    impl<'pr> ruby_prism::Visit<'pr> for Reads<'_> {
        fn visit_local_variable_read_node(&mut self, node: &ruby_prism::LocalVariableReadNode<'pr>) {
            self.1 |= constant_id_str(&node.name()) == self.0;
        }
    }
    let mut reads = Reads(&name, false);
    if let Some(body) = block.body() {
        ruby_prism::Visit::visit(&mut reads, &body);
    }
    !reads.1
}

/// Lift direct custom factory constants from an owner's body into library classes.
/// Preserve instance and `self` methods, keyword formals, and instance visibility;
/// reject block parameters and other statements instead of discarding them.
pub(super) fn collect(
    body: Option<Node<'_>>,
    owner: &ClassId,
    file: &str,
) -> IngestResult<Vec<LibraryClass>> {
    let mut factories = Vec::new();
    let Some(body) = body else {
        return Ok(factories);
    };
    let statements = flatten_statements(body);
    let data_names = statements.iter().filter_map(|stmt| stmt.as_constant_write_node()).filter(|write| write.value().as_call_node().is_some_and(|call| constant_id_str(&call.name()) == "define" && call.receiver().and_then(|recv| constant_path_of(&recv)).is_some_and(|path| path == ["Data"]))).map(|write| constant_id_str(&write.name()).to_string()).collect::<Vec<_>>();
    for statement in statements {
        if let Some(class) = statement.as_class_node()
            && class.constant_path().as_constant_read_node().is_some_and(|name| data_names.iter().any(|data| data == constant_id_str(&name.name())))
            && class.body().is_some_and(|body| flatten_statements(body).iter().any(|stmt| stmt.as_call_node().is_some_and(|call| constant_id_str(&call.name()) == "delegate_missing_to"))) {
            return Err(IngestError::Unsupported { file: file.into(), message: "delegate_missing_to in a Data reopening is not modeled".into() });
        }
        let Some(call) = declaration(&statement) else {
            continue;
        };
        let unsupported = || {
            IngestError::Unsupported {
            file: file.into(),
            message: "Data.define blocks support only methods, delegates and static visibility declarations".into(),
        }
        };
        let block = call
            .block()
            .and_then(|node| node.as_block_node())
            .ok_or_else(unsupported)?;
        if block.parameters().is_some() && !unread_class_parameter(&block) {
            return Err(unsupported());
        }
        let Some(body) = block.body() else {
            return Err(unsupported());
        };
        let name = statement.as_constant_write_node().unwrap().name();
        let id = ClassId(Symbol::from(format!(
            "{}::{}",
            owner.0.as_str(),
            constant_id_str(&name)
        )));
        visibility::validate_data(block.body(), file, &[], &id)?;
        let visibility = Visibility::resolve(Some(&body), file, None)?;
        let mut methods = Vec::new();
        let mut unknown_calls = Vec::new();
        for statement in flatten_statements(body) {
            if let Some(def) = visibility::definition(&statement) {
                if def
                    .receiver()
                    .is_some_and(|receiver| receiver.as_self_node().is_none())
                {
                    return Err(unsupported());
                }
                let mut method = ingest_library_method_with_keywords(&def, &id, file, true)?;
                // Lexical defaults affect instance methods only, but explicit
                // visibility wrappers such as `private_class_method def self.helper`
                // also apply to singleton methods.
                visibility.apply(&statement, &mut method);
                methods.push(method);
            } else if statement.as_call_node().is_some_and(|call| {
                call.receiver().is_none() && call.block().is_none()
                    && constant_id_str(&call.name()) == "delegate"
            }) {
                let delegate_call = statement.as_call_node().unwrap();
                let args = delegate_call.arguments().map(|args| args.arguments().iter().collect::<Vec<_>>()).unwrap_or_default();
                if args.iter().any(|arg| arg.as_hash_node().is_some()) {
                    return Err(IngestError::Unsupported { file: file.into(), message: "Data delegate requires keyword options, not a positional Hash".into() });
                }
                let target = args.iter().filter_map(|arg| arg.as_keyword_hash_node()).flat_map(|hash| hash.elements().iter().collect::<Vec<_>>())
                    .filter_map(|arg| arg.as_assoc_node()).find(|arg| super::util::symbol_or_string_value(&arg.key()).as_deref() == Some("to"))
                    .and_then(|arg| super::util::symbol_or_string_value(&arg.value()));
                let members = call.arguments().map(|args| args.arguments().iter().filter_map(|arg| super::util::symbol_or_string_value(&arg)).collect::<Vec<_>>()).unwrap_or_default();
                if !target.as_ref().is_some_and(|target| members.contains(target)) {
                    return Err(IngestError::Unsupported { file: file.into(), message: "Data delegate constant target or non-member target has unverified lexical lookup".into() });
                }
                // The shared delegate pass validates options and expands these forwarders.
                unknown_calls.push(ingest_expr(&statement, file)?);
            } else if let Some(target) = statement.as_call_node().and_then(|call| delegate_missing_target(&call)) {
                // Rails' `delegate_missing_to :target` (active_support/core_ext/module/delegation.rb): the two
                // methods it defines. The target read through `self`, not bare: a target named `block` or `args`
                // would read the method's own parameter.
                let source = format!(
                    "class {id}\n  def respond_to_missing?(name, include_private = false)\n    self.{target}.respond_to?(name, include_private) || super\n  end\n\n  def method_missing(method, *args, &block)\n    if self.{target}.respond_to?(method)\n      self.{target}.public_send(method, *args, &block)\n    else\n      super\n    end\n  end\nend\n",
                    id = id.0.as_str(),
                );
                let classes = super::library_class::ingest_library_classes(source.as_bytes(), file)?;
                methods.extend(classes.into_iter().flat_map(|c| c.methods));
            } else if !statement.as_call_node().is_some_and(|call| {
                visibility::marker(&call)
                    && call.receiver().is_none()
                    && call.block().is_none()
                    && matches!(
                        constant_id_str(&call.name()),
                        "public" | "protected" | "private"
                    )
            }) {
                return Err(unsupported());
            }
        }
        let missing_delegate = combined_delegation(block.body());
        if !unknown_calls.is_empty() && (missing_delegate || methods.iter().any(|method| method.receiver == crate::dialect::MethodReceiver::Class && method.name.as_str() == "delegate")) {
            return Err(IngestError::Unsupported { file: file.into(), message: if missing_delegate { "Data.define blocks combining delegate and delegate_missing_to are not modeled" } else { "app-defined delegate on a Data class is not modeled" }.into() });
        }
        factories.push(LibraryClass {
            name: id,
            is_module: false,
            parent: Some(ClassId(Symbol::from("Data"))),
            parent_span: node_span(&call.receiver().unwrap(), file),
            includes: Vec::new(),
            include_spans: Vec::new(),
            methods,
            class_ivar_initializers: Vec::new(),
            nullable_columns: Vec::new(),
            origin: Some(LibraryClassOrigin::DataFactory {
                declaration_span: node_span(&call.as_node(), file),
            }),
            constants: Vec::new(),
            conditional_definitions: Vec::new(),
            unknown_calls,
        });
    }
    Ok(factories)
}

/// `delegate_missing_to :account`: a literal Symbol target and nothing else (no `allow_nil:`).
fn delegate_missing_target(call: &ruby_prism::CallNode<'_>) -> Option<String> {
    if call.receiver().is_some() || call.block().is_some() || constant_id_str(&call.name()) != "delegate_missing_to" {
        return None;
    }
    let args: Vec<_> = call.arguments()?.arguments().iter().collect();
    let [arg] = args.as_slice() else { return None };
    let name = String::from_utf8_lossy(arg.as_symbol_node()?.unescaped()).into_owned();
    (name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.is_empty()).then_some(name)
}

pub(super) fn combined_delegation(body: Option<Node<'_>>) -> bool {
    let statements = body.map(flatten_statements).unwrap_or_default(); ["delegate", "delegate_missing_to"].iter().all(|name| statements.iter().any(|stmt| stmt.as_call_node().is_some_and(|call| constant_id_str(&call.name()) == *name)))
}
