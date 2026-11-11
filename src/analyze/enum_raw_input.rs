//! The enum columns an app reads `<column>_before_type_cast` on. Rails
//! answers that with what was assigned until the record saves or loads,
//! and the stored value after (activerecord 8.1.4). The ruby family and
//! Spinel keep the assigned value for these columns only; the other
//! targets refuse the reader at the project boundary.

use crate::ident::ClassId;
use crate::span::Span;
use crate::ty::Ty;
use crate::{App, Expr, ExprNode, Symbol};
use std::collections::BTreeSet;

/// One call of a generated `<column>_before_type_cast`.
pub(crate) struct GeneratedRead {
    pub(crate) span: Span,
    pub(crate) method: Symbol,
    pub(crate) column: Symbol,
}

/// Enum columns whose generated reader the app calls.
pub(crate) fn raw_input_columns(app: &App) -> BTreeSet<Symbol> {
    generated_reads(app).into_iter().map(|read| read.column).collect()
}

/// Whether this call reaches a generated reader. A receiver or enclosing
/// model that's known answers for itself. Otherwise (label rewriting runs
/// before receivers are typed, and a concern's bare call has no one model)
/// it's generated when any model declaring the column doesn't define it.
fn reaches_generated(app: &App, owner: Option<&ClassId>, recv: Option<&Expr>, method: &str, column: &Symbol) -> bool {
    let receiver = match recv.map(|r| (&*r.node, r.ty.as_ref())) {
        None | Some((ExprNode::SelfRef, _)) => owner,
        Some((_, Some(Ty::Class { id, .. }))) => Some(id),
        Some(_) => None,
    };
    let generated_on = |model: &crate::dialect::Model| {
        model.enums.contains_key(column) && !crate::analyze::source_instance_method(app, &model.name, method)
    };
    match receiver.and_then(|id| app.models.iter().find(|m| &m.name == id)) {
        Some(model) => generated_on(model),
        None => app.models.iter().any(generated_on),
    }
}

/// Every call of a generated reader, in any body the app's emit carries.
pub(crate) fn generated_reads(app: &App) -> Vec<GeneratedRead> {
    reads_in(app, false)
}

/// The same for the bodies Roda converts: models, controllers and views.
/// Its emit carries no test, fixture, seed, route helper or library class.
pub(crate) fn roda_generated_reads(app: &App) -> Vec<GeneratedRead> {
    reads_in(app, true)
}

fn reads_in(app: &App, roda: bool) -> Vec<GeneratedRead> {
    if app.models.iter().all(|model| model.enums.is_empty()) {
        return Vec::new();
    }
    fn walk(expr: &Expr, owner: Option<&ClassId>, app: &App, out: &mut Vec<GeneratedRead>) {
        if let ExprNode::Send { recv, method, .. } = &*expr.node
            && let Some(column) = method.as_str().strip_suffix("_before_type_cast")
        {
            let column = Symbol::from(column);
            if reaches_generated(app, owner, recv.as_ref(), method.as_str(), &column)
                && !out.iter().any(|read| !expr.span.is_synthetic() && read.span == expr.span)
            {
                out.push(GeneratedRead { span: expr.span, method: method.clone(), column });
            }
        }
        expr.node.for_each_child(&mut |child| walk(child, owner, app, out));
    }
    let mut out = Vec::new();
    let not_emitted_by_roda = |owner: Option<&ClassId>| {
        owner.is_some_and(|id| {
            app.library_classes.iter().chain(app.rails_application.iter()).any(|class| &class.name == id)
        })
    };
    crate::lower::for_each_owned_hook_body_ref(app, &mut |owner, body| {
        if !(roda && not_emitted_by_roda(owner)) {
            walk(body, owner, app, &mut out);
        }
    });
    for model in &app.models {
        for method in model.methods() {
            walk(&method.body, Some(&model.name), app, &mut out);
        }
    }
    for view in &app.views {
        walk(&view.body, None, app, &mut out);
    }
    if roda {
        return out;
    }
    for class in &app.library_classes {
        for method in &class.methods {
            walk(&method.body, Some(&class.name), app, &mut out);
        }
    }
    for module in &app.test_modules {
        if let Some(setup) = &module.setup {
            walk(setup, None, app, &mut out);
        }
        for body in module.tests.iter().map(|t| &t.body).chain(module.helpers.iter().map(|h| &h.body)) {
            walk(body, None, app, &mut out);
        }
        for class in &module.inner_classes {
            for method in &class.methods {
                walk(&method.body, Some(&class.name), app, &mut out);
            }
        }
    }
    for fixture in &app.fixtures {
        for expr in &fixture.preamble {
            walk(expr, None, app, &mut out);
        }
    }
    for helper in &app.routes.direct_helpers {
        walk(&helper.body, None, app, &mut out);
    }
    if let Some(seeds) = &app.seeds {
        walk(seeds, None, app, &mut out);
    }
    out
}
