//! `includes(:tags)` with a condition or an order on the association's
//! own table is Rails' eager load: one LEFT OUTER JOIN, the parents
//! filtered and ordered through it, and each parent's `tags` holding only
//! the joined rows, in that order (lobsters' filters page reads
//! `Category.includes(:tags).where(tags: { active: true }).order(...)`).
//!
//! The chain is rewritten on the ruby family:
//!
//!   Category.includes(tags: ActiveRecord::EagerFilter.new([{ tags: { active: true } }], [], [{ tags: { tag: :asc } }], nil))
//!     .left_outer_joins(:tags).distinct.where(…).order(…)
//!
//! The join and `distinct` filter the parents. The EagerFilter rides the
//! includes spec into the model's preload walker, whose `_eager_dispatch`
//! loads the association with the same conditions and order and installs
//! it on each parent.

use crate::dialect::{Association, Model};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol};

/// A model's direct has_many associations an eager load can fill: no
/// `through:`, no polymorphic owner, the owner's own `id`, and a scope
/// that only orders (Rails leaves an association scope's order out of a
/// JOIN-based eager load, so it cannot change which rows are kept).
pub(crate) fn eager_loadable(model: &Model) -> Vec<(Symbol, ClassId, Symbol)> {
    model
        .associations()
        .filter_map(|assoc| match assoc {
            Association::HasMany { name, target, foreign_key, through: None, as_interface: None, primary_key: None, scope, .. }
                if scope.as_ref().is_none_or(scope_only_orders) =>
            {
                Some((name.clone(), target.clone(), foreign_key.clone()))
            }
            _ => None,
        })
        .collect()
}

fn scope_only_orders(scope: &Expr) -> bool {
    let body = match &*scope.node {
        ExprNode::Lambda { body, .. } => body,
        _ => scope,
    };
    matches!(&*body.node, ExprNode::Send { recv: None, method, .. } if method.as_str() == "order")
}

pub fn lower_for_target(app: &crate::App, target: crate::build_target::BuildTarget) -> Option<crate::App> {
    use crate::build_target::BuildTarget;
    if !matches!(target, BuildTarget::Spinel | BuildTarget::Ruby | BuildTarget::Jruby) {
        return None;
    }
    // Not a copy of the whole app for nothing: most chains have no eager load.
    let mut any = false;
    crate::lower::for_each_emit_body_ref(app, &mut |e| any = any || needs_rewrite(e, &app.models, None));
    let own_side = |m: &Model| {
        let own = Some(&m.name);
        m.body.iter().any(|item| match item {
            crate::dialect::ModelBodyItem::Method { method, .. } if method.receiver == crate::dialect::MethodReceiver::Class => {
                needs_rewrite(&method.body, &app.models, own)
            }
            crate::dialect::ModelBodyItem::Scope { scope, .. } => needs_rewrite(&scope.body, &app.models, own),
            _ => false,
        })
    };
    if !any && !app.models.iter().any(own_side) {
        return None;
    }
    let mut lowered = app.clone();
    let models = app.models.clone();
    // A model's class-side bodies root their chains at the model itself (`includes(:comments)…` in `def self.x`).
    for model in &mut lowered.models {
        let own = Some(model.name.clone());
        for item in &mut model.body {
            match item {
                crate::dialect::ModelBodyItem::Method { method, .. }
                    if method.receiver == crate::dialect::MethodReceiver::Class =>
                {
                    rewrite(&mut method.body, &models, own.as_ref());
                }
                crate::dialect::ModelBodyItem::Scope { scope, .. } => rewrite(&mut scope.body, &models, own.as_ref()),
                _ => {}
            }
        }
    }
    crate::lower::for_each_emit_body(&mut lowered, &mut |e| rewrite(e, &models, None));
    Some(lowered)
}

fn needs_rewrite(e: &Expr, models: &[Model], own: Option<&ClassId>) -> bool {
    if chain_of(e, models, own).is_some_and(|chain| !eager_of(e.span, &chain, models).is_empty()) {
        return true;
    }
    let mut found = false;
    e.node.for_each_child(&mut |c| found = found || needs_rewrite(c, models, own));
    found
}

fn rewrite(e: &mut Expr, models: &[Model], own: Option<&ClassId>) {
    if let Some(chain) = chain_of(e, models, own) {
        apply(e, &chain, models);
        // Not into the chain's own receivers again: they are this chain.
        if let ExprNode::Send { args, block, .. } = &mut *e.node {
            for arg in args {
                rewrite(arg, models, own);
            }
            if let Some(b) = block {
                rewrite(b, models, own);
            }
        }
        return;
    }
    e.node.for_each_child_mut(&mut |c| rewrite(c, models, own));
}

struct Chain {
    model: ClassId,
    /// Association names `includes`/`eager_load` give as plain Symbols.
    included: Vec<Symbol>,
    /// Table keys of `where` hashes, and the key/value pairs themselves.
    conditions: Vec<(Symbol, Expr)>,
    /// The same of `where.not` hashes.
    negations: Vec<(Symbol, Expr)>,
    /// Table-keyed `order` hash entries.
    orders: Vec<(Symbol, Expr)>,
    /// A `reorder` was seen: the calls before it (later in the walk) no longer order.
    orders_sealed: bool,
    referenced: Vec<Symbol>,
}

/// The relation chain `e` tops, rooted at a model constant, or at the
/// model itself inside its own class-side body (`own`).
fn chain_of(e: &Expr, models: &[Model], own: Option<&ClassId>) -> Option<Chain> {
    let mut chain = Chain { model: ClassId(Symbol::from("")), included: Vec::new(), conditions: Vec::new(), negations: Vec::new(), orders: Vec::new(), orders_sealed: false, referenced: Vec::new() };
    let mut node = e;
    let mut any = false;
    loop {
        match &*node.node {
            ExprNode::Send { recv, method, args, block: None, .. } => {
                if !step(method.as_str(), args, &mut chain, &mut any) {
                    return None;
                }
                match recv {
                    Some(recv) if !matches!(&*recv.node, ExprNode::SelfRef) => node = recv,
                    _ => {
                        let own = own?;
                        if !any {
                            return None;
                        }
                        chain.model = own.clone();
                        return Some(chain);
                    }
                }
            }
            ExprNode::Const { path } => {
                let name = path.iter().map(|p| p.as_str()).collect::<Vec<_>>().join("::");
                if !any || !models.iter().any(|m| m.name.0.as_str() == name) {
                    return None;
                }
                chain.model = ClassId(Symbol::from(name));
                return Some(chain);
            }
            _ => return None,
        }
    }
}

/// Record one relation step; false for a call that is not a query step.
fn step(method: &str, args: &[Expr], chain: &mut Chain, any: &mut bool) -> bool {
    let table_keyed = |arg: &Expr, out: &mut Vec<(Symbol, Expr)>| {
        if let ExprNode::Hash { entries, .. } = &*arg.node {
            for (k, v) in entries {
                if let (ExprNode::Lit { value: Literal::Sym { value } }, ExprNode::Hash { .. }) = (&*k.node, &*v.node) {
                    out.push((value.clone(), v.clone()));
                }
            }
        }
    };
    match method {
        "includes" | "eager_load" => {
            let mut names: Vec<Symbol> = Vec::new();
            for arg in args {
                match &*arg.node {
                    ExprNode::Lit { value: Literal::Sym { value } } => names.push(value.clone()),
                    // `comments: :user`: the association is loaded too, its own preloads riding along.
                    ExprNode::Hash { entries, .. } => {
                        for (k, _) in entries {
                            if let ExprNode::Lit { value: Literal::Sym { value } } = &*k.node {
                                names.push(value.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            for name in names {
                if method == "eager_load" {
                    chain.referenced.push(name.clone());
                }
                chain.included.push(name);
            }
            *any = true;
        }
        "where" => {
            for arg in args {
                table_keyed(arg, &mut chain.conditions);
            }
        }
        "order" | "reorder" => {
            if chain.orders_sealed {
                return true;
            }
            let mut terms: Vec<(Symbol, Expr)> = Vec::new();
            for arg in args {
                match &*arg.node {
                    ExprNode::Array { elements, .. } => {
                        for element in elements {
                            table_keyed(element, &mut terms);
                        }
                    }
                    _ => table_keyed(arg, &mut terms),
                }
            }
            // The walk runs from the last call back: an earlier call's terms come first.
            chain.orders.splice(0..0, terms);
            chain.orders_sealed = method == "reorder";
        }
        "references" => {
            for arg in args {
                if let ExprNode::Lit { value: Literal::Sym { value } } = &*arg.node {
                    chain.referenced.push(value.clone());
                }
            }
        }
        "not" => {
            for arg in args {
                table_keyed(arg, &mut chain.negations);
            }
        }
        // Not `unscope`: it can drop a condition this chain would have carried to the association.
        "all" | "distinct" | "limit" | "offset" | "joins" | "left_outer_joins" | "left_joins" | "select" => {}
        _ => return false,
    }
    true
}

fn apply(e: &mut Expr, chain: &Chain, models: &[Model]) {
    let eager = eager_of(e.span, chain, models);
    if eager.is_empty() {
        return;
    }
    rewrite_includes(e, &eager);
}

/// The `name: EagerFilter` pairs a chain's includes become; empty when none is an eager load.
fn eager_of(span: crate::span::Span, chain: &Chain, models: &[Model]) -> Vec<(Symbol, Expr)> {
    let Some(model) = models.iter().find(|m| m.name == chain.model) else { return Vec::new() };
    let loadable = eager_loadable(model);
    let mut eager: Vec<(Symbol, Expr)> = Vec::new();
    for name in &chain.included {
        if eager.iter().any(|(n, _)| n == name) {
            continue;
        }
        let Some((_, target, _)) = loadable.iter().find(|(n, _, _)| n == name) else { continue };
        let Some(table) = models.iter().find(|m| &m.name == target).map(|m| m.table.0.clone()) else { continue };
        // Rails keys a `where`/`order` on the table; the association's name reads the same table here.
        let mentions = |key: &Symbol| key == &table || key == name;
        // One Hash per call, applied in turn: merged into one, a column two calls name would keep only the last.
        let on_table = |entries: &[(Symbol, Expr)]| -> Vec<Expr> {
            entries
                .iter()
                .filter(|(k, _)| mentions(k))
                .map(|(_, v)| hash(span, vec![(sym(span, &table), v.clone())]))
                .collect()
        };
        let conditions = on_table(&chain.conditions);
        let negations = on_table(&chain.negations);
        let orders = on_table(&chain.orders);
        let referenced = chain.referenced.iter().any(&mentions);
        if conditions.is_empty() && negations.is_empty() && orders.is_empty() && !referenced {
            continue;
        }
        let filter = Expr::new(
            span,
            ExprNode::Send {
                recv: Some(Expr::new(span, ExprNode::Const { path: vec![Symbol::from("ActiveRecord"), Symbol::from("EagerFilter")] })),
                method: Symbol::from("new"),
                args: [conditions, negations, orders]
                    .into_iter()
                    .map(|elements| Expr::new(span, ExprNode::Array { elements, style: Default::default() }))
                    .chain(std::iter::once(Expr::new(span, ExprNode::Lit { value: Literal::Nil })))
                    .collect(),
                block: None,
                parenthesized: true,
            },
        );
        eager.push((name.clone(), filter));
    }
    eager
}

/// Swap each eager-loaded Symbol of the chain's `includes` for its
/// `name: EagerFilter` pair, and join the association there.
fn rewrite_includes(e: &mut Expr, eager: &[(Symbol, Expr)]) {
    let ExprNode::Send { recv, method, args, .. } = &mut *e.node else { return };
    if !matches!(method.as_str(), "includes" | "eager_load") {
        if let Some(recv) = recv {
            rewrite_includes(recv, eager);
        }
        return;
    }
    let span = e.span;
    // The preloads a Hash names under an eager-loaded association (`comments: :user`) run on the rows it loads.
    let mut nested: Vec<(Symbol, Expr)> = Vec::new();
    for arg in args.iter_mut() {
        if let ExprNode::Hash { entries, .. } = &mut *arg.node {
            entries.retain(|(k, v)| match &*k.node {
                ExprNode::Lit { value: Literal::Sym { value } } if eager.iter().any(|(n, _)| n == value) => {
                    nested.push((value.clone(), v.clone()));
                    false
                }
                _ => true,
            });
        }
    }
    let mut present: Vec<Symbol> = nested.iter().map(|(n, _)| n.clone()).collect();
    let mut kept: Vec<Expr> = Vec::new();
    for arg in args.drain(..) {
        let consumed = match &*arg.node {
            ExprNode::Lit { value: Literal::Sym { value } } if eager.iter().any(|(n, _)| n == value) => {
                present.push(value.clone());
                true
            }
            ExprNode::Hash { entries, .. } => entries.is_empty(),
            _ => false,
        };
        if !consumed {
            kept.push(arg);
        }
    }
    let mut pairs: Vec<(Expr, Expr)> = Vec::new();
    let mut joined: Vec<Symbol> = Vec::new();
    for (name, filter) in eager.iter().filter(|(n, _)| present.contains(n)) {
        let mut filter = filter.clone();
        let inner: Vec<Expr> = nested.iter().filter(|(n, _)| n == name).map(|(_, v)| v.clone()).collect();
        if !inner.is_empty() {
            if let ExprNode::Send { args, .. } = &mut *filter.node {
                args[3] = if inner.len() == 1 {
                    inner.into_iter().next().unwrap()
                } else {
                    Expr::new(span, ExprNode::Array { elements: inner, style: Default::default() })
                };
            }
        }
        pairs.push((sym(span, name), filter));
        joined.push(name.clone());
    }
    if !pairs.is_empty() {
        kept.push(Expr::new(span, ExprNode::Hash { entries: pairs, kwargs: true }));
    }
    *args = kept;
    *method = Symbol::from("includes");
    let mut out = e.clone();
    for name in joined {
        out = send(span, out, "left_outer_joins", vec![sym(span, &name)]);
    }
    *e = send(span, out, "distinct", vec![]);
}

fn send(span: crate::span::Span, recv: Expr, method: &str, args: Vec<Expr>) -> Expr {
    Expr::new(span, ExprNode::Send { recv: Some(recv), method: Symbol::from(method), args, block: None, parenthesized: true })
}

fn sym(span: crate::span::Span, name: &Symbol) -> Expr {
    Expr::new(span, ExprNode::Lit { value: Literal::Sym { value: name.clone() } })
}

fn hash(span: crate::span::Span, entries: Vec<(Expr, Expr)>) -> Expr {
    Expr::new(span, ExprNode::Hash { entries, kwargs: false })
}
