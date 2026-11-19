//! The pagy gem's `pagy(scope[, limit: n])`, typed at the call.
//!
//! `@pagy, @records = pagy(scope)` answers `[Pagy, <scope>]`: the second
//! element is the scope's own type, which a registry entry cannot say (it
//! maps a name to one type, with no way to name "argument 0's"). So the
//! call is typed here, from the argument, and only for the shape the
//! runtime (runtime/ruby/action_controller/pagy.rb) models:
//!
//! * one ActiveRecord scope, typed as a Relation, or one the analysis
//!   cannot type at all (gradual) -- not a known Array (`Model.to_a`, a
//!   literal), which the gem paginates only through `pagy(:offset, array)`
//!   and which has no `offset` / `limit` for this runtime to call;
//! * optionally one trailing hash holding only `limit:`, a literal
//!   Integer of at least 1 (a computed limit may be 0, which the gem
//!   rejects with `Pagy::OptionError` and the app turns into a 404 in a
//!   `rescue_from` no target replays; a literal cannot reach that), and
//!   `items:`, which pagy 43 does not read at all (it was renamed
//!   `limit:`), so the page size stays 20. The runtime ignores it the way
//!   the gem does, and a warning says so: EngineeredAt's Madmin lists write
//!   `items: 50` and get 20.
//!
//! Anything else -- a paginator symbol (`pagy(:countless, ...)`), another
//! option (`page:`, `count:`, `max_limit:`, `jsonapi:`, ...), a splat --
//! is refused with a diagnostic that names it, and the call is still
//! typed as if it ran so the refusal is the one error.

use super::*;

impl<'a> BodyTyper<'a> {
    /// What `pagy` hands back as the records: the scope's own type when it
    /// is a Relation (never an Array: the runtime calls `offset` / `limit`
    /// on the scope, which an Array lacks), the
    /// unique such type across the closed members of a union, and
    /// `Untyped` when the analysis could not type the scope at all (a base
    /// class it does not model, a method whose other return paths are
    /// open) -- gradual, as everywhere else. `None` for a known type that
    /// is not a scope, or a union of different scopes.
    fn pagy_scope_ty(&self, ty: Option<&Ty>) -> Option<Ty> {
        match ty {
            None | Some(Ty::Untyped) | Some(Ty::Var { .. }) => Some(Ty::Untyped),
            Some(Ty::Relation { of }) => Some(Ty::Relation { of: of.clone() }),
            Some(Ty::Array { .. }) => None,
            Some(Ty::Union { variants }) => {
                let mut closed: Option<Ty> = None;
                for variant in variants {
                    match self.pagy_scope_ty(Some(variant))? {
                        Ty::Untyped => {}
                        known => match &closed {
                            Some(prev) if *prev != known => return None,
                            _ => closed = Some(known),
                        },
                    }
                }
                Some(closed.unwrap_or(Ty::Untyped))
            }
            _ => None,
        }
    }

    /// `@pagy.series_nav(id: "x")` and the like: the object's helpers take
    /// no arguments here, and an argument the runtime would ignore is an
    /// approximation, so it is refused by name instead.
    pub(super) fn pagy_object_refusal(
        &self,
        recv_ty: Option<&Ty>,
        method: &Symbol,
        args: &[Expr],
    ) -> Option<crate::diagnostic::DiagnosticKind> {
        let Some(Ty::Class { id, .. }) = recv_ty else { return None };
        // Cleared by the `[]` read on it when its key is one pagy9.rb's `vars` holds (`pagy9_vars_read`).
        if id.0.as_str() == "Pagy" && method.as_str() == "vars" && self.classes().contains_key(&ClassId(Symbol::from("Pagy::Backend"))) {
            return Some(crate::diagnostic::DiagnosticKind::Unsupported {
                target: None,
                construct: Symbol::from("pagy"),
                detail: "`Pagy#vars` is modeled only as `vars[k]` for `count`, `limit`, `page`, `items`, `outset` or `size`".into(),
            });
        }
        if id.0.as_str() != "Pagy" || args.is_empty() {
            return None;
        }
        let known = self.classes().get(id).is_some_and(|c| c.instance_methods.contains_key(method));
        // An unknown method is already "no known method on Pagy".
        known.then(|| crate::diagnostic::DiagnosticKind::Unsupported {
            target: None,
            construct: Symbol::from("pagy"),
            detail: format!(
                "`Pagy#{}` takes no arguments here (`id:`, `aria_label:`, `item_name:`, a style, ... are not modeled)",
                method.as_str()
            ),
        })
    }

    /// `Some(type)` when this implicit-self `pagy` call is the gem's.
    /// `None` leaves ordinary dispatch to it (not a controller, or the app
    /// does not lock the gem).
    pub(super) fn pagy_call_ty(
        &self,
        method: &Symbol,
        args: &[Expr],
        ctx: &Ctx,
        refusal: &mut Option<crate::diagnostic::DiagnosticKind>,
    ) -> Option<Ty> {
        let pagy_id = ClassId(Symbol::from("Pagy"));
        self.classes().get(&pagy_id)?;
        if self.classes().contains_key(&ClassId(Symbol::from("Pagy::Backend"))) {
            return self.pagy9_call_ty(method, args, ctx, refusal);
        }
        if method.as_str() != "pagy" {
            return None;
        }
        let Some(Ty::Class { id: self_id, .. }) = &ctx.self_ty else { return None };
        // Only inside a controller: some class up the chain carries the
        // registry's `pagy` (registry/controllers). A `pagy` the app
        // defines on the way is a wrapper around the gem's -- ingest
        // (`ingest::pagy`) refuses any that does not hand back what
        // `super` returned -- so the call is typed the same.
        let mut current = Some(self_id);
        let mut in_controller = false;
        for _ in 0..32 {
            let Some(cid) = current else { break };
            let Some(cls) = self.classes().get(cid) else { break };
            if cls.class_methods.contains_key(&Symbol::from("pagy")) {
                in_controller = true;
                break;
            }
            current = cls.parent.as_ref();
        }
        if !in_controller {
            return None;
        }

        let refuse = |detail: String, refusal: &mut Option<crate::diagnostic::DiagnosticKind>| {
            *refusal = Some(crate::diagnostic::DiagnosticKind::Unsupported {
                target: None,
                construct: Symbol::from("pagy"),
                detail,
            });
        };
        let scope_ty = args.first().and_then(|a| a.ty.as_ref());
        let records = self.pagy_scope_ty(scope_ty);
        let result = |records: Ty| Ty::Tuple {
            elems: vec![Ty::Class { id: pagy_id.clone(), args: vec![].into() }, records].into(),
        };
        match args {
            [] => {
                refuse("`pagy` needs a collection".into(), refusal);
                return Some(result(Ty::Untyped));
            }
            [first, ..] if matches!(&*first.node, ExprNode::Lit { value: crate::expr::Literal::Sym { .. } }) => {
                refuse(
                    "an explicit paginator argument (`pagy(:offset, ...)`, `:countless`, `:keyset`, `:calendar`, ...) is not modeled; \
                     call `pagy(scope)` for the default offset paginator"
                        .into(),
                    refusal,
                );
                return Some(result(Ty::Untyped));
            }
            _ => {}
        }
        if records.is_none() {
            refuse(
                format!(
                    "the collection must be an ActiveRecord scope the analysis can type (got {}); \
                     paginating an Array or an untyped value is not modeled",
                    scope_ty.map_or("an untyped value".to_string(), |t| format!("{t:?}")),
                ),
                refusal,
            );
        }
        let mut ignored_items = false;
        match &args[1..] {
            [] => {}
            [opts] => match &*opts.node {
                ExprNode::Hash { entries, .. } => {
                    for (k, v) in entries {
                        let key = match &*k.node {
                            ExprNode::Lit { value: crate::expr::Literal::Sym { value } } => value.as_str().to_string(),
                            _ => {
                                refuse("only literal `limit:` is modeled; this option key is not a symbol".into(), refusal);
                                break;
                            }
                        };
                        if key == "items" {
                            ignored_items = true;
                            continue;
                        }
                        if key != "limit" {
                            refuse(
                                format!("option `{key}:` is not modeled (only a literal `limit:` is)"),
                                refusal,
                            );
                            break;
                        }
                        match &*v.node {
                            ExprNode::Lit { value: crate::expr::Literal::Int { value } } if *value >= 1 => {}
                            _ => {
                                refuse(
                                    "`limit:` must be a literal Integer of at least 1 (the gem raises Pagy::OptionError otherwise)"
                                        .into(),
                                    refusal,
                                );
                                break;
                            }
                        }
                    }
                }
                _ => refuse("the options must be a literal hash of `limit:`".into(), refusal),
            },
            _ => refuse("expected a collection and at most one options hash".into(), refusal),
        }
        if ignored_items && refusal.is_none() {
            *refusal = Some(crate::diagnostic::DiagnosticKind::LowerResidue {
                pass: Symbol::from("pagy"),
                construct: Symbol::from("pagy(..., items:)"),
                reason: Symbol::from(
                    "pagy 43 does not read `items:` (it is `limit:` now), so the page size stays 20",
                ),
            });
        }
        Some(result(records.unwrap_or(Ty::Untyped)))
    }

    fn includes_pagy_backend(&self, ctx: &Ctx) -> bool {
        let Some(Ty::Class { id: self_id, .. }) = &ctx.self_ty else { return false };
        let mut current = Some(self_id);
        for _ in 0..32 {
            let Some(cid) = current else { break };
            let Some(cls) = self.classes().get(cid) else { break };
            if cls.includes.iter().any(|m| m.0.as_str().trim_start_matches("::") == "Pagy::Backend") {
                return true;
            }
            current = cls.parent.as_ref();
        }
        false
    }

    /// Whether a controller up `ctx`'s chain carries the registry's `method`.
    fn in_pagy_controller(&self, method: &Symbol, ctx: &Ctx) -> bool {
        let Some(Ty::Class { id: self_id, .. }) = &ctx.self_ty else { return false };
        let mut current = Some(self_id);
        for _ in 0..32 {
            let Some(cid) = current else { break };
            let Some(cls) = self.classes().get(cid) else { break };
            if cls.class_methods.contains_key(method) {
                return true;
            }
            current = cls.parent.as_ref();
        }
        false
    }

    /// pagy before 43 (runtime/ruby/action_controller/pagy9.rb):
    /// `pagy(scope, **vars)` and the array extra's `pagy_array(array,
    /// **vars)`, with `limit:` and `page:` any value the gem reads with
    /// `to_i` (a String from the request, an Integer, nil), and `items:`
    /// a literal Integer it only keeps in `vars`.
    fn pagy9_call_ty(
        &self,
        method: &Symbol,
        args: &[Expr],
        ctx: &Ctx,
        refusal: &mut Option<crate::diagnostic::DiagnosticKind>,
    ) -> Option<Ty> {
        let array = match method.as_str() {
            "pagy" => false,
            "pagy_array" => true,
            _ => return None,
        };
        let refuse = |detail: String, refusal: &mut Option<crate::diagnostic::DiagnosticKind>| {
            if refusal.is_none() {
                *refusal = Some(crate::diagnostic::DiagnosticKind::Unsupported {
                    target: None,
                    construct: Symbol::from("pagy"),
                    detail,
                });
            }
        };
        if !self.in_pagy_controller(method, ctx) {
            // pagy9.rb's `Pagy::Backend` is an empty marker: outside a controller the include brings no `pagy`.
            if self.includes_pagy_backend(ctx) {
                refuse(format!("`{method}` from `include Pagy::Backend` is modeled on controllers only (it reads the request's params)"), refusal);
                return Some(Ty::Untyped);
            }
            return None;
        }
        let pagy_ty = Ty::Class { id: ClassId(Symbol::from("Pagy")), args: vec![].into() };
        let result = |records: Ty| Ty::Tuple { elems: vec![pagy_ty.clone(), records].into() };
        let Some(collection) = args.first() else {
            refuse(format!("`{method}` needs a collection"), refusal);
            return Some(result(Ty::Untyped));
        };
        let records = if array {
            match collection.ty.as_ref() {
                Some(Ty::Array { elem }) => Some(Ty::Array { elem: elem.clone() }),
                None | Some(Ty::Untyped) | Some(Ty::Var { .. }) => Some(Ty::Untyped),
                _ => None,
            }
        } else {
            self.pagy_scope_ty(collection.ty.as_ref())
        };
        if records.is_none() {
            let wanted = if array { "an Array the analysis can type" } else { "an ActiveRecord scope the analysis can type" };
            refuse(format!("the collection must be {wanted} (got {:?})", collection.ty), refusal);
        }
        match &args[1..] {
            [] => {}
            [opts] => match &*opts.node {
                ExprNode::Hash { entries, .. } => {
                    for (k, v) in entries {
                        let key = match &*k.node {
                            ExprNode::Lit { value: crate::expr::Literal::Sym { value } } => value.as_str().to_string(),
                            _ => {
                                refuse("an option key that is not a Symbol literal is not modeled".into(), refusal);
                                continue;
                            }
                        };
                        match key.as_str() {
                            "limit" | "page" => {
                                if !Self::pagy9_var_ty(v.ty.as_ref()) {
                                    refuse(
                                        format!("`{key}:` must be an Integer, a String or nil, read with `to_i` (got {:?})", v.ty),
                                        refusal,
                                    );
                                }
                            }
                            "items" => {
                                if !matches!(&*v.node, ExprNode::Lit { value: crate::expr::Literal::Int { .. } }) {
                                    refuse("`items:` is modeled only as a literal Integer (it is kept in `vars`, not read)".into(), refusal);
                                }
                            }
                            _ => refuse(format!("option `{key}:` is not modeled (only `limit:`, `page:` and `items:` are)"), refusal),
                        }
                    }
                }
                _ => refuse("the options must be a literal hash".into(), refusal),
            },
            _ => refuse("expected a collection and at most one options hash".into(), refusal),
        }
        Some(result(records.unwrap_or(Ty::Untyped)))
    }

    /// A value the runtime reads as the gem does: `nil`/`""` the default, an Integer or a String by `to_i`, and a
    /// request param of another shape (an Array, a nested hash, a file) raising Pagy::VariableError.
    fn pagy9_var_ty(ty: Option<&Ty>) -> bool {
        match ty {
            None | Some(Ty::Untyped) | Some(Ty::Var { .. }) | Some(Ty::Int) | Some(Ty::Str) | Some(Ty::Nil) | Some(Ty::Array { .. }) => true,
            Some(Ty::Union { variants }) => variants.iter().all(|v| Self::pagy9_var_ty(Some(v))),
            Some(Ty::Class { id, .. }) => matches!(id.0.as_str(), "ActionController::Parameters" | "ActionDispatch::Http::UploadedFile") || Some(&Ty::Class { id: id.clone(), args: vec![].into() }) == Some(&send::param_value_ty()),
            Some(_) => false,
        }
    }

    /// `pagy.vars[k]` for a key pagy9.rb's `vars` holds, the only use of `vars` admitted.
    pub(super) fn pagy9_vars_read(recv: &Expr, method: &Symbol, args: &[Expr]) -> bool {
        let ExprNode::Send { recv: Some(pagy), method: vars, args: none, .. } = &*recv.node else { return false };
        method.as_str() == "[]"
            && vars.as_str() == "vars"
            && none.is_empty()
            && matches!(&pagy.ty, Some(Ty::Class { id, .. }) if id.0.as_str() == "Pagy")
            && matches!(args, [k] if matches!(&*k.node, ExprNode::Lit { value: crate::expr::Literal::Sym { value } }
                if matches!(value.as_str(), "count" | "limit" | "page" | "items" | "outset" | "size")))
    }
}
