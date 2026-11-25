//! `form_with(model: x) do |f| … render "x/fields", form: f … end`: a partial
//! handed the builder alone reads the form's record through it, which the
//! partial lowering (`lower::view_to_library::partial_form_bindings`) binds
//! from a local holding the record. Such a render gains that local,
//! `__<local>_object: f.object`, so the partial is bound as one handed the
//! record is; Rails passes no such local, and the partial reads nothing
//! else of it. A strict-locals partial declares it first, the position its
//! lowering reads the record from.

use crate::App;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;
use crate::span::Span;

pub(super) fn add(app: &mut App) {
    let mut added: Vec<(String, Symbol)> = Vec::new();
    for view in &mut app.views {
        let own_dir = view.name.as_str().rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        let mut found = Vec::new();
        scopes(&mut view.body, &mut found);
        for (path, local) in found {
            let (dir, base) = match path.rsplit_once('/') {
                Some((d, b)) => (d.to_string(), b.to_string()),
                None => (own_dir.clone(), path),
            };
            added.push((format!("{dir}/_{base}"), local));
        }
    }
    for (name, local) in added {
        for view in app.views.iter_mut().filter(|v| v.name.as_str() == name) {
            if let Some(locals) = &mut view.strict_locals
                && !locals.iter().any(|p| p.name == local)
            {
                locals.insert(0, crate::dialect::Param::positional(local.clone()));
            }
        }
    }
}

fn scopes(e: &mut Expr, found: &mut Vec<(String, Symbol)>) {
    if let ExprNode::Send { recv: None, method, args, block: Some(block), .. } = &mut *e.node
        && matches!(method.as_str(), "form_with" | "form_for")
        && let ExprNode::Lambda { params, extra_params, body, .. } = &mut *block.node
        && extra_params.is_empty()
        && let Some(param) = params.first().cloned()
    {
        let model = args.iter().find_map(|a| match &*a.node {
            ExprNode::Hash { entries, .. } => entries.iter().find_map(|(k, v)| {
                // Not the Array itself: `model: [:mod, @story]` names the record `@story` behind a route namespace.
                let record = match &*v.node {
                    ExprNode::Array { elements, .. } => elements.last()?,
                    _ => v,
                };
                matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "model").then(|| simple_ref(record)).flatten()
            }),
            _ => None,
        });
        renders(body, &param, model.as_deref(), found);
    }
    e.node.for_each_child_mut(&mut |c| scopes(c, found));
}

fn simple_ref(e: &Expr) -> Option<String> {
    match &*e.node {
        ExprNode::Ivar { name } => Some(format!("@{}", name.as_str())),
        ExprNode::Var { name, .. } => Some(name.as_str().to_string()),
        ExprNode::Send { recv: None, method, args, block: None, .. } if args.is_empty() => Some(method.as_str().to_string()),
        _ => None,
    }
}

fn renders(e: &mut Expr, param: &Symbol, model: Option<&str>, found: &mut Vec<(String, Symbol)>) {
    // Not inside a block that binds the same name: its `f` is not the builder.
    if let ExprNode::Lambda { params, .. } = &*e.node
        && params.contains(param)
    {
        return;
    }
    e.node.for_each_child_mut(&mut |c| renders(c, param, model, found));
    let ExprNode::Send { recv: None, method, args, .. } = &mut *e.node else { return };
    if method.as_str() != "render" {
        return;
    }
    let is_param = |v: &Expr| matches!(&*v.node, ExprNode::Var { name, .. } if name == param);
    let (path, locals) = match args.as_mut_slice() {
        [first, ..] if matches!(&*first.node, ExprNode::Hash { kwargs: true, .. }) => {
            let ExprNode::Hash { entries, .. } = &mut *first.node else { return };
            let path = entries.iter().find_map(|(k, v)| match (&*k.node, &*v.node) {
                (ExprNode::Lit { value: Literal::Sym { value: key } }, ExprNode::Lit { value: Literal::Str { value } }) if key.as_str() == "partial" => Some(value.clone()),
                _ => None,
            });
            let Some((_, v)) = entries.iter_mut().find(|(k, _)| matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "locals")) else { return };
            let ExprNode::Hash { entries, .. } = &mut *v.node else { return };
            (path, entries)
        }
        [path, locals] => {
            let ExprNode::Lit { value: Literal::Str { value: path } } = &*path.node else { return };
            let path = Some(path.clone());
            let ExprNode::Hash { entries, kwargs: true } = &mut *locals.node else { return };
            (path, entries)
        }
        _ => return,
    };
    let Some(form_local) = locals.iter().find_map(|(k, v)| match &*k.node {
        ExprNode::Lit { value: Literal::Sym { value } } if is_param(v) => Some(value.clone()),
        _ => None,
    }) else {
        return;
    };
    // Not when a local is already the record (the form's `model:` or `f.object`): the binding reads that one.
    let is_record = |v: &Expr| {
        model.is_some_and(|m| simple_ref(v).as_deref() == Some(m))
            || matches!(&*v.node, ExprNode::Send { recv: Some(r), method, .. } if method.as_str() == "object" && is_param(r))
    };
    if locals.iter().any(|(_, v)| is_record(v)) {
        return;
    }
    let span = Span::synthetic();
    let local = Symbol::from(format!("__{}_object", form_local.as_str()));
    if let Some(path) = path {
        found.push((path, local.clone()));
    }
    let key = Expr::new(span, ExprNode::Lit { value: Literal::Sym { value: local } });
    let read = Expr::new(span, ExprNode::Var { id: crate::ident::VarId(0), name: param.clone() });
    let value = Expr::new(span, ExprNode::Send { recv: Some(read), method: Symbol::from("object"), args: Vec::new(), block: None, parenthesized: false });
    locals.push((key, value));
}
