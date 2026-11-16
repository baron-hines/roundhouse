//! `concerning :Topic do … end` in a model body.
//!
//! Active Support's `Module#concerning` is two steps:
//!
//! ```ruby
//! def concerning(topic, &block)
//!   include const_set(topic, Module.new { extend ::ActiveSupport::Concern; module_eval(&block) })
//! end
//! ```
//!
//! so `concerning :Heating do … end` in a model defines the Concern module
//! `Model::Heating` and includes it at once. Ingest saw only a class-body
//! call it could not unroll: a `scope` inside the block was reported as a
//! runtime-defined method, and `Model::Heating` was a constant nothing
//! defined.
//!
//! Desugared here into exactly those two steps, so the concern machinery
//! that already carries an `included do` block and a module's
//! `class_methods` into its includers does the rest: the block's source
//! text becomes a nested Concern module, and the statement becomes
//! `include Model::Topic` in its place.

use crate::App;
use crate::dialect::ModelBodyItem;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol};
use crate::span::SourceFile;

use super::library_class::ConcernClassMethodSpans;

pub(super) fn expand(
    app: &mut App,
    sources: &[SourceFile],
    enum_constants: &super::model::EnumConstants,
    carriers: &mut Vec<ConcernClassMethodSpans>,
) -> Vec<(ClassId, Vec<super::library_class::ConcernEnumDecl>)> {
    // (model index, item index): rewritten after the walk, since each
    // desugar re-ingests into `app`.
    let mut found: Vec<(usize, usize)> = Vec::new();
    let mut enums: Vec<(ClassId, Vec<super::library_class::ConcernEnumDecl>)> = Vec::new();
    for (model_index, model) in app.models.iter().enumerate() {
        for (item_index, item) in model.body.iter().enumerate() {
            if concerning(&item).is_some() {
                found.push((model_index, item_index));
            }
        }
    }
    for (model_index, item_index) in found {
        let model = app.models[model_index].name.clone();
        let item = &app.models[model_index].body[item_index];
        let Some((topic, body)) = concerning(item) else { continue };
        let module = ClassId(Symbol::from(format!("{}::{}", model.0.as_str(), topic.as_str())));
        let Some(block) = block_body_text(sources, body) else { continue };
        let source = format!(
            "module {}\n  extend ActiveSupport::Concern\n{block}\nend\n",
            module.0.as_str(),
        );
        let file = format!("<concerning {}>", module.0.as_str());
        let classes = match super::library_class::ingest_app_library_classes(source.as_bytes(), &file) {
            Ok(classes) => classes,
            Err(err) => {
                super::survey::record(&err);
                continue;
            }
        };
        app.library_classes.extend(classes);
        // The block's lexical scope is the MODEL's class body, since that
        // is where `concerning` runs; the module's own file has
        // `Model::Topic` as its scope instead, where the model's constants
        // are not — spelled out before anything reads them.
        qualify_model_constants(app, &module);
        let scan = super::library_class::ingest_concern_model_items_with_constants(
            source.as_bytes(),
            &file,
            enum_constants,
        );
        // An `enum` an `included do` block declares reaches the model
        // through the concern fold, exactly as one written in a file's
        // module does.
        enums.extend(super::app::append_concern_items(app, scan, |_| true));
        for (id, name) in super::library_class::ingest_concern_included_redefs(source.as_bytes(), &file) {
            app.concern_included_redefs.entry(id).or_default().push(name);
        }
        let (spans, _shadows) = super::library_class::ingest_concern_class_method_spans(source.as_bytes(), &file);
        carriers.extend(spans);
        // The statement becomes the include, keeping its span so the
        // body keeps its order.
        if let ModelBodyItem::Unknown { expr, .. } = &mut app.models[model_index].body[item_index] {
            let span = expr.span;
            expr.node = Box::new(ExprNode::Send {
                recv: None,
                method: Symbol::from("include"),
                args: vec![Expr::new(
                    span,
                    ExprNode::Const { path: module.0.as_str().split("::").map(Symbol::from).collect() },
                )],
                block: None,
                parenthesized: false,
            });
        }
    }
    enums
}

/// Spell the model's own constants out inside the desugared module.
///
/// A constant the block reads is resolved in the lexical scope the block
/// was written in — the model's class body — and only then in `Object`'s.
/// The generated module is a file of its own, whose compact header
/// (`module Model::Topic`) makes `Model::Topic` its lexical scope: the
/// model's constants are not there, so plain Ruby raises `uninitialized
/// constant` where the concerning block found them. Qualified here, the
/// emitted module reads `Model::CONST` and answers what Rails answers.
fn qualify_model_constants(app: &mut App, module: &ClassId) {
    let mut defined: crate::det::HashSet<String> = crate::det::HashSet::default();
    for class in &app.library_classes {
        defined.insert(class.name.0.as_str().to_string());
        for (name, _) in &class.constants {
            defined.insert(format!("{}::{}", class.name.0.as_str(), name.as_str()));
        }
    }
    for model in &app.models {
        defined.insert(model.name.0.as_str().to_string());
        // A model's class-body constants sit in `Unknown` items; the
        // analyzer already names them.
        for name in crate::analyze::extract_const_assignments(&model.body).keys() {
            defined.insert(format!("{}::{}", model.name.0.as_str(), name.as_str()));
        }
    }
    let concern = module.0.as_str();
    for class in app.library_classes.iter_mut().filter(|class| class.name == *module) {
        let qualify = |expr: &mut Expr| super::app::qualify_carried_constants(expr, concern, &defined);
        for method in &mut class.methods {
            for param in &mut method.params {
                if let Some(default) = &mut param.default {
                    qualify(default);
                }
            }
            qualify(&mut method.body);
        }
        for call in &mut class.unknown_calls {
            qualify(call);
        }
        for value in class.constants.iter_mut().map(|(_, value)| value) {
            qualify(value);
        }
    }
}

/// `concerning :Topic do … end`: the topic and the block's body.
fn concerning<'a>(item: &'a ModelBodyItem) -> Option<(Symbol, &'a Expr)> {
    let ModelBodyItem::Unknown { expr, .. } = item else { return None };
    let ExprNode::Send { recv: None, method, args, block: Some(block), .. } = &*expr.node else { return None };
    if method.as_str() != "concerning" {
        return None;
    }
    let [topic] = args.as_slice() else { return None };
    let topic = match &*topic.node {
        ExprNode::Lit { value: Literal::Sym { value } } => value.clone(),
        ExprNode::Lit { value: Literal::Str { value } } => Symbol::from(value.as_str()),
        _ => return None,
    };
    Some((topic, block))
}

/// The block's own source text, so the desugared module declares exactly
/// what the source wrote. A block whose span is synthetic (a macro
/// expanded into this call) has no source to copy, so `None` leaves the
/// statement as written; a block whose span holds no text is re-emitted
/// from the IR instead.
fn block_body_text(sources: &[SourceFile], block: &Expr) -> Option<String> {
    let (body, span) = match &*block.node {
        ExprNode::Lambda { body, .. } => (body, body.span),
        _ => (block, block.span),
    };
    let file = sources.get(span.file.0.checked_sub(1)? as usize)?;
    match file.text.get(span.start as usize..span.end as usize) {
        Some(text) if !text.trim().is_empty() => Some(text.to_string()),
        _ => Some(crate::emit::ruby::expr::emit_expr(body)),
    }
}
