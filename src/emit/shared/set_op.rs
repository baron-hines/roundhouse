//! Shared classifier for Array `&` and `|`.
//!
//! On an `Array[T]` receiver Ruby's `&` is set intersection and `|` is
//! set union, both order-preserving (receiver order first) and
//! duplicate-free. No target's native `&`/`|` means that: TypeScript's
//! is bitwise (two arrays coerce to `0`), Python's is undefined for
//! lists, and Rust has no operator on `Vec`. Emitters consult
//! [`classify_set_op`] and render a combinator for the array cases;
//! anything else (Integer/Bool bit operations, gradual operands) keeps
//! the native infix.

use crate::expr::Expr;
use crate::ty::Ty;

pub enum SetOpCase<'a> {
    /// `Array[T] & Array[T]` — elements of lhs also in rhs, first
    /// occurrence kept.
    ArrayIntersect { elem: &'a Ty },
    /// `Array[T] | Array[T]` — lhs then rhs, first occurrence kept.
    ArrayUnion { elem: &'a Ty },
    /// Not an array set operation — native infix.
    Other,
}

pub fn classify_set_op<'a>(method: &str, lhs: &'a Expr, rhs: &'a Expr) -> SetOpCase<'a> {
    match (lhs.ty.as_ref(), rhs.ty.as_ref()) {
        (Some(Ty::Array { elem: l }), Some(Ty::Array { elem: r })) if l == r => match method {
            "&" => SetOpCase::ArrayIntersect { elem: l.as_ref() },
            "|" => SetOpCase::ArrayUnion { elem: l.as_ref() },
            _ => SetOpCase::Other,
        },
        _ => SetOpCase::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::ExprNode;
    use crate::ident::{Symbol, VarId};
    use crate::span::Span;

    fn var_typed(name: &str, ty: Ty) -> Expr {
        let mut e = Expr::new(
            Span::synthetic(),
            ExprNode::Var { id: VarId(0), name: Symbol::from(name) },
        );
        e.ty = Some(ty);
        e
    }

    fn int_array(name: &str) -> Expr {
        var_typed(name, Ty::Array { elem: Box::new(Ty::Int) })
    }

    #[test]
    fn array_and_array_is_intersect() {
        let (l, r) = (int_array("a"), int_array("b"));
        assert!(matches!(classify_set_op("&", &l, &r), SetOpCase::ArrayIntersect { elem: Ty::Int }));
    }

    #[test]
    fn array_or_array_is_union() {
        let (l, r) = (int_array("a"), int_array("b"));
        assert!(matches!(classify_set_op("|", &l, &r), SetOpCase::ArrayUnion { elem: Ty::Int }));
    }

    #[test]
    fn int_bitwise_is_other() {
        let (l, r) = (var_typed("a", Ty::Int), var_typed("b", Ty::Int));
        assert!(matches!(classify_set_op("&", &l, &r), SetOpCase::Other));
        assert!(matches!(classify_set_op("|", &l, &r), SetOpCase::Other));
    }
}
