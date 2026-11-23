//! Analyze-time diagnostic detection.
//!
//! Runs after type inference populates `.ty` on an expression's
//! children; inspects the node shape + child types and, if the
//! body-typer recognizes a user error (an operator Ruby would raise
//! on — Incompatible `+`, cross-type comparison, …), annotates
//! `expr.diagnostic`. Downstream consumers:
//!
//! - Emitters check `.diagnostic` at the top of their expression
//!   walker; if set, they emit a target-side raise-equivalent
//!   instead of the normal rendering.
//! - `analyze::diagnose` collects annotations across the tree for
//!   CLI reporting.
//!
//! This module is the single site where analyze-time diagnostics
//! originate. New kinds (embedded conditionals, Hash `+`, etc.) hook
//! in here.

use crate::expr::{Expr, ExprNode};
use crate::ty::Ty;

pub(super) fn detect_diagnostic(expr: &mut Expr) {
    // Operator ownership/operand types can become known in a later fixpoint
    // round. Recompute this classifier's annotation instead of retaining a
    // refusal from an earlier incomplete registry.
    if matches!(&expr.diagnostic(), Some(crate::diagnostic::DiagnosticKind::IncompatibleBinop { .. })) {
        expr.set_diagnostic(None);
    }
    if let ExprNode::Send { recv: Some(r), method, args, .. } = &*expr.node {
        if args.len() != 1 {
            return;
        }
        let rhs = &args[0];
        let incompatible = rhs.decisions & crate::expr::COERCED_OPERAND == 0 && binop_incompatible(method.as_str(), r, rhs);
        let lhs_ty = r.ty.clone().unwrap_or(Ty::Nil);
        let rhs_ty = rhs.ty.clone().unwrap_or(Ty::Nil);
        if incompatible {
            use crate::diagnostic::DiagnosticKind;
            expr.set_diagnostic(Some(DiagnosticKind::IncompatibleBinop {
                op: method.clone(),
                lhs_ty,
                rhs_ty,
            }));
        }
    }
}

fn binop_incompatible(op: &str, r: &Expr, rhs: &Expr) -> bool {
    match op {
        "+" => {
            use crate::emit::shared::add::{AddCase, classify_add};
            matches!(classify_add(r, rhs), AddCase::Incompatible)
        }
        "-" => {
            use crate::emit::shared::sub::{SubCase, classify_sub};
            matches!(classify_sub(r, rhs), SubCase::Incompatible)
        }
        "*" => {
            use crate::emit::shared::mul::{MulCase, classify_mul};
            matches!(classify_mul(r, rhs), MulCase::Incompatible)
        }
        "/" | "**" => {
            use crate::emit::shared::div_pow::{DivPowCase, classify_div_pow};
            matches!(classify_div_pow(r, rhs), DivPowCase::Incompatible)
        }
        "%" => {
            use crate::emit::shared::modulo::{ModuloCase, classify_modulo};
            matches!(classify_modulo(r, rhs), ModuloCase::Incompatible)
        }
        "<" | "<=" | ">" | ">=" => {
            use crate::emit::shared::cmp::{CmpCase, classify_cmp};
            matches!(classify_cmp(r, rhs), CmpCase::Incompatible)
        }
        _ => false,
    }
}

/// Whether some present arm of each operand takes the operator: the ruby
/// family then runs it as Ruby does, raising only for the arms that miss.
pub(crate) fn some_operands_take_it(op: &str, lhs: &Ty, rhs: &Ty) -> bool {
    fn arms(t: &Ty) -> Vec<Ty> {
        match t {
            Ty::Union { variants } => variants.iter().flat_map(arms).collect(),
            Ty::Nil => Vec::new(),
            other => vec![other.clone()],
        }
    }
    let operand = |ty: Ty| Expr {
        ty: Some(ty),
        ..Expr::new(crate::span::Span::synthetic(), ExprNode::Var { id: crate::ident::VarId(0), name: crate::ident::Symbol::from("operand") })
    };
    arms(lhs).into_iter().any(|l| arms(rhs).into_iter().any(|r| !binop_incompatible(op, &operand(l.clone()), &operand(r))))
}
