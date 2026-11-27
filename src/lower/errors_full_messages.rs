//! `record.errors.full_messages` → `record.errors`.
//!
//! The shared framework runtime's `errors` is already the array of
//! full-message strings — `validates` bakes humanized text at lower
//! time ("Short can't be blank") and hand-written `errors.add` calls
//! ground into the same shape (see [`super::errors_add`], which owns
//! that half and documents the accumulator invariant). Rails' extra
//! `.full_messages` hop is therefore an identity on our runtime, and
//! leaving it in place is not harmless: `Array[String]` has no
//! `full_messages`, so every strict target gets a hard unresolved-method
//! error at a site whose meaning we already know.
//!
//! Scope is `for_each_hook_body`, view bodies, and TEST bodies — the
//! construct has real presence in all three (lobsters reads it from
//! controllers, from models, and from an `application_helper`
//! error-list builder that views call). That matches
//! [`super::exclude_predicate`], the other total rewrite with a view
//! presence.
//!
//! Tests were the last to join, and asking for them by name is the
//! convention [`super::for_each_test_body`] documents. The site is
//! campfire's `assert_equal [ "Title can't be blank", … ],
//! opengraph.errors.full_messages` — an assertion reading the
//! accumulator directly, which is exactly the shape this pass folds
//! everywhere else. Left out, it read as a missing method on `Array`
//! and the identity we already knew went unapplied purely because of
//! which walk the pass had asked for.
//!
//! The Roda ingest used to fold the same send in its view bodies at
//! ingest time. Folded before analysis, `errors.each` there typed its
//! block parameter as an `ActiveModel::Error`, so
//! [`super::errors_index`] then iterated the details, not the messages;
//! the Roda views now reach this pass like any other.

use crate::app::App;
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol};
use crate::det::HashSet;

pub fn apply_errors_full_messages_lowering(app: &mut App) {
    let fold = Fold::of(app);
    super::for_each_owned_hook_body(app, &mut |owner, body| rewrite(body, &(owner, &fold)));
    super::for_each_test_body(app, &mut |body| rewrite(body, &(None, &fold)));
    for view in &mut app.views {
        rewrite(&mut view.body, &(None, &fold));
    }
}

/// Where the fold applies when the app owns the name `full_messages`.
pub(crate) struct Fold {
    /// The app defines its own `full_messages`: its `errors.full_messages`
    /// may be a call on the app's own collection, so only a model's own
    /// accumulator folds.
    stands_down: bool,
    /// The tableless models (no superclass) whose `errors` is the plain
    /// accumulator: none defines `errors`. An ActiveRecord model's is
    /// extended at run time to answer `full_messages` itself.
    models: HashSet<ClassId>,
}

impl Fold {
    pub(crate) fn of(app: &App) -> Self {
        let defines_errors = |m: &crate::dialect::Model| {
            m.body.iter().any(|item| {
                matches!(item, crate::dialect::ModelBodyItem::Method { method, .. }
                    if method.name.as_str() == "errors")
            })
        };
        Fold {
            stands_down: app_defines_full_messages(app),
            models: app
                .models
                .iter()
                .filter(|m| m.parent.is_none() && !defines_errors(m))
                .map(|m| m.name.clone())
                .collect(),
        }
    }

    /// Whether the `errors` read is a model's accumulator: the owner's
    /// own, or a receiver typed as such a model.
    fn accumulator(&self, errors: &Expr, owner: Option<&ClassId>) -> bool {
        let ExprNode::Send { recv, .. } = &*errors.node else { return false };
        match recv {
            None => owner.is_some_and(|o| self.models.contains(o)),
            Some(r) if matches!(&*r.node, ExprNode::SelfRef) => owner.is_some_and(|o| self.models.contains(o)),
            Some(r) => matches!(&r.ty, Some(crate::ty::Ty::Class { id, .. }) if self.models.contains(id)),
        }
    }
}

/// The class whose body is being rewritten, and where the fold applies.
pub(crate) type Site<'a> = (Option<&'a ClassId>, &'a Fold);

/// Does any app class define its own `full_messages`? Then the name
/// doesn't mean Rails' and the pass folds only what is certainly the
/// framework's accumulator (see [`Fold`]), rather than folding away a
/// real call. Mirrors `exclude_predicate`'s guard.
pub(crate) fn app_defines_full_messages(app: &App) -> bool {
    let is_fm = |n: &Symbol| n.as_str() == "full_messages";
    app.models.iter().any(|m| {
        m.body.iter().any(|item| {
            matches!(item, crate::dialect::ModelBodyItem::Method { method, .. }
                if is_fm(&method.name))
        })
    }) || app
        .library_classes
        .iter()
        .any(|lc| lc.methods.iter().any(|m| is_fm(&m.name)))
}

fn rewrite(expr: &mut Expr, site: &Site<'_>) {
    expr.node.for_each_child_mut(&mut |c| rewrite(c, site));
    rewrite_node(expr, site);
}

pub(crate) fn rewrite_node(expr: &mut Expr, &(owner, fold): &Site<'_>) {
    // `<recv>.errors.full_messages` — zero-arg, block-free on both hops.
    // The receiver being an `errors` reader is what makes this our
    // accumulator rather than some other object's method of the same
    // name; the receiver of `errors` itself is unconstrained, so both
    // `errors.full_messages` (a model validating itself) and
    // `record.errors.full_messages` (a controller reading another
    // object's) fold, and the surviving expression keeps that receiver.
    let replacement = match &*expr.node {
        ExprNode::Send { recv: Some(r), method, args, block: None, .. }
            if method.as_str() == "full_messages"
                && args.is_empty()
                && matches!(
                    &*r.node,
                    ExprNode::Send { method: em, args: ea, block: None, .. }
                        if em.as_str() == "errors" && ea.is_empty()
                )
                && (!fold.stands_down || fold.accumulator(r, owner)) =>
        {
            Some((*r.node).clone())
        }
        _ => None,
    };
    if let Some(node) = replacement {
        expr.node = Box::new(node);
        return;
    }
    fold_safe_nav(expr, owner, fold);
}

/// `x.errors&.full_messages`, which `&.` binds as
/// `(__safe_nav1 = x.errors) && __safe_nav1.full_messages`, and
/// `x&.errors&.full_messages`, where the bound value is itself
/// `x && x.errors`: the hop is the same identity, and it is nil-safe on
/// its own (nil `errors` answers nil either way), so the binding goes
/// with it. `e.record&.errors&.full_messages&.first` reads the messages
/// whatever `e.record`'s type, as the `.` spelling does.
fn fold_safe_nav(expr: &mut Expr, owner: Option<&ClassId>, fold: &Fold) {
    use crate::expr::{BoolOpKind, LValue};
    let is_errors = |e: &Expr| {
        matches!(&*e.node, ExprNode::Send { method, args, block: None, .. }
            if method.as_str() == "errors" && args.is_empty())
    };
    let ExprNode::BoolOp { op: BoolOpKind::And, left, right, .. } = &*expr.node else { return };
    let ExprNode::Assign { target: LValue::Var { name: temp, .. }, value } = &*left.node else { return };
    let ExprNode::Send { recv: Some(read), method, args, block: None, .. } = &*right.node else { return };
    if method.as_str() != "full_messages"
        || !args.is_empty()
        || !matches!(&*read.node, ExprNode::Var { name, .. } if name == temp)
    {
        return;
    }
    let errors = match &*value.node {
        _ if is_errors(value) => value,
        ExprNode::BoolOp { op: BoolOpKind::And, right: inner, .. } if is_errors(inner) => inner,
        _ => return,
    };
    if !fold.stands_down || fold.accumulator(errors, owner) {
        let value = value.clone();
        *expr = value;
    }
}
