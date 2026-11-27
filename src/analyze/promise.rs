//! `ActiveRecord::Promise[T]`: what an `async_*` query answers (Rails'
//! promise.rb). Its `value` is the query's result and `then` yields that
//! result and answers a Promise of the block's value, so the result's
//! type rides on the Promise as its one argument. RBS cannot declare a
//! class's type parameter here (a type variable reads as `untyped`), so
//! the runtime's `Promise` holds an untyped value and the call sites are
//! typed by this module: `relation.async_count` in
//! `BodyTyper::async_count_ty`, `value` / `then` in `dispatch_on` and
//! `block_params_for`.

use crate::ident::{ClassId, Symbol};
use crate::ty::Ty;

pub const CLASS: &str = "ActiveRecord::Promise";

pub fn of(value: Ty) -> Ty {
    Ty::Class { id: ClassId(Symbol::from(CLASS)), args: vec![value].into() }
}

/// The value type of a Promise this module typed. `None` for anything
/// else, an argument-less Promise included: that one came from a site
/// that does not know its value, and keeps the catalog's reading.
pub fn value_of(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Class { id, args } if id.0.as_str() == CLASS => args.first(),
        _ => None,
    }
}
