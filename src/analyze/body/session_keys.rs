//! A session read answers what the app stores under that key.
//!
//! Rails keeps a session value's type (the cookie store round-trips it as
//! JSON), so `session[:return_to] = request.url` makes
//! `session.delete(:return_to)` a String or nil, and
//! `session[:at] = Time.now.to_i` makes `session[:at]` an Integer or nil.
//! The writes are collected across the app each fixpoint round
//! (`app_session_key_types`); a key the app never writes (one a gem
//! writes, Warden's) keeps the registry's untyped answer.

use crate::det::HashMap;
use crate::expr::{Expr, ExprNode, LValue, Literal};
use crate::ident::Symbol;
use crate::ty::Ty;
use crate::App;

use super::BodyTyper;

const SESSION: &str = "ActionDispatch::Request::Session";

impl BodyTyper<'_> {
    /// `session[:k]` / `session.delete(:k)` for a key the app writes.
    pub(super) fn session_read(&self, recv_ty: Option<&Ty>, method: &Symbol, args: &[Expr]) -> Option<Ty> {
        let keys = self.session_key_types?;
        let Some(Ty::Class { id, .. }) = recv_ty else { return None };
        if id.0.as_str() != SESSION || !matches!(method.as_str(), "[]" | "delete") {
            return None;
        }
        let [key] = args else { return None };
        let stored = keys.get(&literal_key(key)?)?;
        Some(super::union_of(stored.clone(), Ty::Nil))
    }
}

/// A Symbol or String literal key, as Rails' session reads both alike.
fn literal_key(e: &Expr) -> Option<String> {
    match &*e.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(value.as_str().to_string()),
        ExprNode::Lit { value: Literal::Str { value } } => Some(value.clone()),
        _ => None,
    }
}

fn is_session(recv: &Expr) -> bool {
    matches!(&recv.ty, Some(Ty::Class { id, .. }) if id.0.as_str() == SESSION)
        || matches!(&*recv.node, ExprNode::Send { recv: None, method, args, .. } if method.as_str() == "session" && args.is_empty())
}

/// Every literal key the app writes, with the join of the values written under it. Empty (every read
/// untyped) when a write names its key at run time or replaces the session wholesale, since that may
/// write any key. A key a write has not typed yet is left out until a later round types it.
pub(crate) fn app_session_key_types(app: &App) -> HashMap<String, Ty> {
    fn walk(e: &Expr, out: &mut HashMap<String, Option<Ty>>, dynamic: &mut bool) {
        match &*e.node {
            ExprNode::Assign { target: LValue::Index { recv, index }, value }
            | ExprNode::OpAssign { target: LValue::Index { recv, index }, value, .. }
                if is_session(recv) =>
            {
                match literal_key(index) {
                    Some(key) => record(out, key, value),
                    None => *dynamic = true,
                }
            }
            ExprNode::Send { recv: Some(recv), method, args, .. } if is_session(recv) => match method.as_str() {
                "[]=" | "store" => match (args.first().and_then(literal_key), args.get(1)) {
                    (Some(key), Some(value)) => record(out, key, value),
                    _ => *dynamic = true,
                },
                "merge!" | "update" | "replace" => *dynamic = true,
                _ => {}
            },
            _ => {}
        }
        e.node.for_each_child(&mut |c| walk(c, out, dynamic));
    }
    fn record(out: &mut HashMap<String, Option<Ty>>, key: String, value: &Expr) {
        let slot = out.entry(key).or_insert_with(|| Some(Ty::Bottom));
        *slot = match (slot.take(), &value.ty) {
            (Some(acc), Some(ty)) if !matches!(ty, Ty::Var { .. }) => Some(super::union_of(acc, ty.clone())),
            _ => None,
        };
    }
    let mut out: HashMap<String, Option<Ty>> = HashMap::default();
    let mut dynamic = false;
    crate::lower::for_each_hook_body_ref(app, &mut |body| walk(body, &mut out, &mut dynamic));
    for view in &app.views {
        walk(&view.body, &mut out, &mut dynamic);
    }
    for controller in &app.controllers {
        for action in controller.actions() {
            walk(&action.body, &mut out, &mut dynamic);
        }
    }
    for lc in &app.library_classes {
        for m in &lc.methods {
            walk(&m.body, &mut out, &mut dynamic);
        }
    }
    if dynamic {
        return HashMap::default();
    }
    out.into_iter().filter_map(|(k, v)| v.map(|t| (k, t))).collect()
}
