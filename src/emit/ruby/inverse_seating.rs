//! An association that loads records seats its owner on each of them
//! through the association's inverse, as Rails' `set_inverse_instance`
//! does, so `fleet.ships.first.fleet` is `fleet` itself, read without a
//! query, rather than a second copy of its row.
//!
//! Rails seats the owner when a has_many or has_one reader loads its
//! target (`find_target`), when a preload installs one (`Preloader`'s
//! `set_inverse`), when a belongs_to reader finds its record (a has_one
//! inverse), and when a relation built off a has_many loads rows
//! (`AssociationRelation#exec_queries`, which leaves the owner's own
//! target alone). The inverse is the automatic one Rails finds
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
        // The relation hook answers for the has_many this model inherits
        // too: it replaces its parent's, and a subclass's own name comes
        // first, as its declaration replaces the parent's.
        // A name a nearer class declares is that class's, inverse or not
        // (a scoped redeclaration has none).
        let mut from_queries: Vec<&Inverse> = Vec::new();
        let mut declared: Vec<&Symbol> = Vec::new();
        let mut at = Some(model);
        while let Some(m) = at {
            for inv in inverses.iter().filter(|i| i.owner == m.name && i.collection) {
                if !declared.contains(&&inv.name) {
                    from_queries.push(inv);
                }
            }
            declared.extend(m.associations().map(|a| a.name()));
            at = m.parent.as_ref().and_then(|p| app.models.iter().find(|pm| &pm.name == p));
        }
        if !from_queries.is_empty() {
            push_parsed(lc, &set_inverse_from_queries_source(&from_queries));
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
    /// A has_many, whose relations seat their rows too.
    collection: bool,
    /// The key column on `target` that names the owner's `id`.
    foreign_key: Symbol,
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
            let (target, foreign_key, inverse, arity) = match assoc {
                Association::HasMany { target, foreign_key, .. } => {
                    (target, foreign_key, automatic_inverse(&app.models, model, assoc), Arity::Many)
                }
                Association::HasOne { target, foreign_key, .. } => {
                    (target, foreign_key, automatic_inverse(&app.models, model, assoc), Arity::One)
                }
                Association::BelongsTo { target, foreign_key, .. } => {
                    (target, foreign_key, belongs_to_inverse(&app.models, model, assoc), Arity::One)
                }
                Association::HasAndBelongsToMany { .. } => continue,
            };
            let Some(inverse) = inverse else { continue };
            out.push(Inverse {
                owner: model.name.clone(),
                name: assoc.name().clone(),
                target: target.clone(),
                inverse,
                arity,
                collection: matches!(assoc, Association::HasMany { .. }),
                foreign_key: foreign_key.clone(),
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

/// The owner's `_set_inverse_from_queries`, which `Relation#load_records`
/// calls for a relation built off one of its has_many (`_inversed_from`).
/// A row is seated only when its key names this owner, or the owner is
/// unsaved, as Rails' `inversed_from_queries` checks `matches_foreign_key?`:
/// a chain can drop the key's condition (`unscope(where: :fleet_id)`) and
/// load other owners' rows. A row whose `select` left the key out is not
/// seated, and its key is not read (which would raise).
fn set_inverse_from_queries_source(arms: &[&Inverse]) -> String {
    let mut src = String::from("def _set_inverse_from_queries(name, records)\n  case name\n");
    for inv in arms {
        let (name, seat, fk) = (&inv.name, seat_method(&inv.inverse), &inv.foreign_key);
        src.push_str(&format!(
            "  when :{name}\n    records.each {{ |r| r.{seat}(self) if new_record? || (r.has_attribute?(:{fk}) && r.{fk} == id) }}\n"
        ));
    }
    src.push_str("  end\nend\n");
    src
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
