//! An association that loads records seats its owner on each of them
//! through the association's inverse, as Rails' `set_inverse_instance`
//! does, so `fleet.ships.first.fleet` is `fleet` itself, read without a
//! query, rather than a second copy of its row.
//!
//! Rails seats the owner when a has_many or has_one reader loads its
//! target (`find_target`), when a preload installs one (`Preloader`'s
//! `set_inverse`), and when a belongs_to reader finds its record (a
//! has_one inverse). The inverse is the automatic one Rails finds
//! (`scope_chain::automatic_inverse`, `belongs_to_inverse`).
//!
//! Ruby family only, and an emit pass rather than a shared lowering:
//! the seated record holds its owner by reference, which the strict
//! targets' records do not model.

use crate::App;
use crate::dialect::{Association, LibraryClass, MethodDef, MethodReceiver};
use crate::expr::{Expr, ExprNode, LValue};
use crate::ident::{ClassId, Symbol, VarId};
use crate::lower::scope_chain::{automatic_inverse, belongs_to_inverse};

/// How many records the seated method hands over.
#[derive(Clone, Copy)]
enum Arity {
    /// An Array of records (a has_many reader or preload setter).
    Many,
    /// One record or nil (has_one, belongs_to).
    One,
}

pub(crate) fn apply(lcs: &mut [LibraryClass], app: &App) {
    let inverses = inverses(app);
    for lc in lcs.iter_mut() {
        let class = lc.name.clone();
        let Some(model) = app.models.iter().find(|m| m.name == class) else { continue };
        let own = |name: &Symbol| model.methods().any(|m| &m.name == name && m.receiver == MethodReceiver::Instance);
        for inv in inverses.iter().filter(|i| i.owner == class) {
            let seat = seat_method(&inv.inverse);
            if !own(&inv.name) {
                if let Some(reader) = instance_method(lc, &inv.name) {
                    seat_tail(&mut reader.body, &seat, inv.arity);
                }
            }
            let preload = Symbol::from(format!("_preload_{}", inv.name.as_str()));
            if !own(&preload) {
                if let Some(setter) = instance_method(lc, &preload) {
                    seat_param(setter, &seat, inv.arity);
                }
            }
        }
        // The seat each inverse this model is the target of calls.
        let mut seated: Vec<&Symbol> = Vec::new();
        for inv in inverses.iter().filter(|i| i.target == class) {
            if seated.contains(&&inv.inverse) {
                continue;
            }
            seated.push(&inv.inverse);
            let name = inv.inverse.as_str();
            push_parsed(
                lc,
                &format!("def _inverse_{name}(owner)\n  @{name}_cache = owner\n  @{name}_loaded = true\n  nil\nend\n"),
            );
        }
    }
}

/// One association with an automatic inverse: `owner.<name>` loads
/// `target` records, each seated through `target#<inverse>`.
struct Inverse {
    owner: ClassId,
    name: Symbol,
    target: ClassId,
    inverse: Symbol,
    arity: Arity,
}

/// Every association of the app's models that has an automatic inverse,
/// read off the declarations alone, so a model's seats do not depend on
/// which other models are emitted with it.
fn inverses(app: &App) -> Vec<Inverse> {
    let mut out = Vec::new();
    for model in &app.models {
        // The last declaration of a name is the one Rails keeps.
        let mut names: Vec<&Symbol> = Vec::new();
        for assoc in model.associations().collect::<Vec<_>>().into_iter().rev() {
            if names.contains(&assoc.name()) {
                continue;
            }
            names.push(assoc.name());
            let (target, inverse, arity) = match assoc {
                Association::HasMany { target, .. } => (target, automatic_inverse(&app.models, model, assoc), Arity::Many),
                Association::HasOne { target, .. } => (target, automatic_inverse(&app.models, model, assoc), Arity::One),
                Association::BelongsTo { target, .. } => (target, belongs_to_inverse(&app.models, model, assoc), Arity::One),
                Association::HasAndBelongsToMany { .. } => continue,
            };
            let Some(inverse) = inverse else { continue };
            out.push(Inverse {
                owner: model.name.clone(),
                name: assoc.name().clone(),
                target: target.clone(),
                inverse,
                arity,
            });
        }
    }
    out
}

fn seat_method(inverse: &Symbol) -> Symbol {
    Symbol::from(format!("_inverse_{}", inverse.as_str()))
}

fn instance_method<'a>(lc: &'a mut LibraryClass, name: &Symbol) -> Option<&'a mut MethodDef> {
    lc.methods.iter_mut().find(|m| &m.name == name && m.receiver == MethodReceiver::Instance)
}

fn push_parsed(lc: &mut LibraryClass, src: &str) {
    let methods = crate::runtime_src::parse_methods(src)
        .unwrap_or_else(|e| panic!("inverse_seating: generated source failed to parse: {e}\n{src}"));
    for mut m in methods {
        if lc.methods.iter().any(|existing| existing.name == m.name && existing.receiver == m.receiver) {
            continue;
        }
        m.enclosing_class = Some(lc.name.0.clone());
        lc.methods.push(m);
    }
}

/// The statement seating `self` on `value` (a list or one record or nil).
fn seat_stmt(value: Expr, seat: &Symbol, arity: Arity) -> Expr {
    let src = match arity {
        Arity::Many => format!("def __seat(v)\n  v.each {{ |r| r.{seat}(self) }}\nend\n"),
        Arity::One => format!("def __seat(v)\n  v.{seat}(self) unless v.nil?\nend\n"),
    };
    let mut body = crate::runtime_src::parse_methods(&src).expect("seat statement parses").remove(0).body;
    replace_var(&mut body, "v", &value);
    body
}

fn replace_var(expr: &mut Expr, name: &str, with: &Expr) {
    if matches!(&*expr.node, ExprNode::Var { name: n, .. } if n.as_str() == name) {
        *expr = with.clone();
        return;
    }
    expr.node.for_each_child_mut(&mut |c| replace_var(c, name, with));
}

/// `..., value` -> `..., __rh_seated = value, <seat>, __rh_seated`, into
/// the last statement of nested sequences, so every path that reaches the
/// end of the reader seats what it answers. An early `return` (the cache
/// guard, an unsaved owner) answers records that are already seated.
fn seat_tail(body: &mut Expr, seat: &Symbol, arity: Arity) {
    if let ExprNode::Seq { exprs } = &mut *body.node {
        if let Some(last) = exprs.last_mut() {
            seat_tail(last, seat, arity);
        }
        return;
    }
    let span = body.span;
    let var = Symbol::from("__rh_seated");
    let read = Expr::new(span, ExprNode::Var { id: VarId(0), name: var.clone() });
    let value = std::mem::replace(body, read.clone());
    *body = Expr::new(
        span,
        ExprNode::Seq {
            exprs: vec![
                Expr::new(span, ExprNode::Assign { target: LValue::Var { id: VarId(0), name: var }, value }),
                seat_stmt(read.clone(), seat, arity),
                read,
            ],
        },
    );
}

/// `def _preload_<name>(list) … end` gains the seat of its parameter.
fn seat_param(setter: &mut MethodDef, seat: &Symbol, arity: Arity) {
    let Some(param) = setter.params.first() else { return };
    let read = Expr::new(setter.body.span, ExprNode::Var { id: VarId(0), name: param.name.clone() });
    let stmt = seat_stmt(read, seat, arity);
    let span = setter.body.span;
    let old = std::mem::replace(&mut setter.body, Expr::new(span, ExprNode::Seq { exprs: vec![] }));
    let mut exprs = match *old.node {
        ExprNode::Seq { exprs } => exprs,
        _ => vec![old],
    };
    // The setter answers nil, as it did.
    let nil = |e: &Expr| matches!(&*e.node, ExprNode::Lit { value: crate::expr::Literal::Nil });
    if exprs.last().is_some_and(nil) {
        exprs.pop();
    }
    exprs.push(stmt);
    exprs.push(Expr::new(span, ExprNode::Lit { value: crate::expr::Literal::Nil }));
    setter.body = Expr::new(span, ExprNode::Seq { exprs });
}
