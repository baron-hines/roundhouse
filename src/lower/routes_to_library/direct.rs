//! `direct :name do |…| route_for :target, … end` → a real
//! `RouteHelpers.<name>_path` function.
//!
//! `direct` is a custom URL helper, not a route: it adds nothing to the
//! dispatch table, it names a builder whose body is arbitrary Ruby.
//! Ingest used to drop it with a ledger line, so campfire's layout
//! called `fresh_account_logo_path` and got a NameError on every page.
//!
//! ## The calling convention
//!
//! Rails invokes the block with the helper's arguments PLUS a trailing
//! options hash, always. `fresh_account_logo_path` (no arguments) calls
//! `do |options|` with `({})`; `fresh_user_avatar_path(user)` calls
//! `do |user, options|` with `(user, {})`. So the LAST block parameter
//! is the options hash, and giving it a `{}` default is what makes the
//! no-argument call sites work.
//!
//! ## `route_for`
//!
//! `route_for :target, arg…, k: v` resolves to the target's own path
//! helper with the positional args, plus a query string built from the
//! keywords. Three rules, all MEASURED against Rails 8.1 rather than
//! assumed (`direct` + `url_for` + `Hash#to_query` compose in ways that
//! are hard to predict):
//!
//! ```text
//! route_for :user_avatar, "tok", v: "123", size: nil  → /users/tok/avatar?v=123
//! route_for :user_avatar, "tok", v: "123", size: 48   → /users/tok/avatar?size=48&v=123
//! ```
//!
//!   * a nil-valued key is DROPPED, not rendered as `size=`;
//!   * keys are sorted ALPHABETICALLY, not written order — so
//!     `v:` before `size:` in the source still emits `size` first;
//!   * positional args fill the target path's segments, each with its
//!     `to_param`: a record its id. The block is given what its caller
//!     passed (`book_slug_path(@book)` hands it the Book), so this is
//!     the one place the record becomes an id.
//!
//! A body that forwards its options (`route_for :t, book, options`)
//! renders the keys its callers pass: each read off the Hash by name
//! and rendered as above, `anchor:` as the fragment after the query.
//! And a name written as an interpolation (`route_for
//! "book_#{leaf.kind}", …`, which Rails sends as `public_send`) becomes
//! a `case` over every named route it can spell with that many
//! segments; one that spells none is reported by
//! `analyze::diagnostics`.
//!
//! The keys are static, so the sort happens at COMPILE time and the
//! emitted body only has to handle the nil-dropping — which it does with
//! a `Vec<String>` of rendered `k=v` pieces, joined with `&`. A concrete
//! `Array[String]` rather than passing the option hash to some runtime
//! query-builder: that would be a type-erased bag, and every target
//! already handles array push/join.

use crate::App;
use crate::dialect::{DirectHelper, LibraryFunction, Param};
use crate::expr::{Expr, ExprNode, InterpPart, Literal};
use crate::ident::{Symbol, VarId};
use crate::span::Span;

use super::super::routes::FlatRoute;

/// One `RouteHelpers.<name>_path` per `direct` declaration.
/// `helper_params`: each generated route helper's parameter types, by
/// name, so a `route_for` argument can meet its target's signature.
pub fn lower_direct_helpers(
    module_path: &[Symbol],
    app: &App,
    flat: &[FlatRoute],
    helper_params: &crate::det::HashMap<String, Vec<crate::ty::Ty>>,
) -> Vec<LibraryFunction> {
    let option_keys = direct_option_keys(app);
    let none = std::collections::BTreeSet::new();
    // Each model, and whether its `to_param` is a String (its own
    // override, or a key that is not an integer) rather than its id.
    let models: crate::det::HashMap<Symbol, bool> = app
        .models
        .iter()
        .map(|m| (m.name.0.clone(), super::model_renders_own_param(m, app)))
        .collect();
    app.routes
        .direct_helpers
        .iter()
        .map(|h| build_direct_helper(module_path, h, flat, helper_params, option_keys.get(&h.name).unwrap_or(&none), &models))
        .collect()
}

/// The option keys each `direct` helper's callers pass, by helper stem:
/// `article_slug_path(article, anchor: "top")` gives `article_slug`
/// the key `anchor`. Rails hands the block those options as its last
/// parameter, and a body that forwards it (`route_for :t, a, options`)
/// renders them on the target's path, which takes each key it can be
/// given by name ([`options_query`]).
fn direct_option_keys(app: &App) -> crate::det::HashMap<Symbol, std::collections::BTreeSet<OptionKey>> {
    fn walk(
        e: &Expr,
        stems: &[Symbol],
        out: &mut crate::det::HashMap<Symbol, std::collections::BTreeSet<OptionKey>>,
    ) {
        if let ExprNode::Send { recv, method, args, .. } = &*e.node {
            let bare_or_helpers = match recv {
                None => true,
                Some(r) => {
                    matches!(&*r.node, ExprNode::Const { path } if path.len() == 1 && path[0].as_str() == "RouteHelpers")
                        || crate::lower::controller_to_library::rewrites::is_rails_url_helpers_chain(r)
                }
            };
            let m = method.as_str();
            let stem = m.strip_suffix("_path").or_else(|| m.strip_suffix("_url")).unwrap_or("");
            if bare_or_helpers && let Some(s) = stems.iter().find(|s| s.as_str() == stem)
                && let Some(ExprNode::Hash { entries, .. }) = args.last().map(|a| &*a.node)
            {
                let keys = out.entry(s.clone()).or_default();
                for (k, _) in entries {
                    match &*k.node {
                        ExprNode::Lit { value: Literal::Sym { value } } => {
                            keys.insert(OptionKey { name: value.as_str().to_string(), symbol: true });
                        }
                        ExprNode::Lit { value: Literal::Str { value } } => {
                            keys.insert(OptionKey { name: value.clone(), symbol: false });
                        }
                        _ => {}
                    }
                }
            }
        }
        e.node.for_each_child(&mut |c| walk(c, stems, out));
    }
    let stems: Vec<Symbol> = app.routes.direct_helpers.iter().map(|h| h.name.clone()).collect();
    let mut out = crate::det::HashMap::default();
    if stems.is_empty() {
        return out;
    }
    crate::lower::for_each_emit_body_ref(app, &mut |e| walk(e, &stems, &mut out));
    out
}

fn build_direct_helper(
    module_path: &[Symbol],
    helper: &DirectHelper,
    flat: &[FlatRoute],
    helper_params: &crate::det::HashMap<String, Vec<crate::ty::Ty>>,
    option_keys: &std::collections::BTreeSet<OptionKey>,
    models: &crate::det::HashMap<Symbol, bool>,
) -> LibraryFunction {
    // The trailing options hash takes a `{}` default — see the header.
    let last = helper.params.len().saturating_sub(1);
    let params: Vec<Param> = helper
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i == last {
                Param::with_default(p.clone(), empty_hash())
            } else {
                Param::positional(p.clone())
            }
        })
        .collect();
    let mut body = helper.body.clone();
    // The temporaries `rewrite_route_for` introduces take names the
    // helper doesn't already use: its block parameters and its locals.
    let mut taken: crate::det::HashSet<Symbol> = helper.params.iter().cloned().collect();
    crate::lower::model_to_library::markers::collect_local_names(&body, &mut taken);
    let fresh = FreshLocals { next: std::cell::Cell::new(0), taken: std::cell::RefCell::new(taken) };
    let options = helper.params.last().map(|name| (name, option_keys));
    rewrite_route_for(&mut body, flat, helper_params, &fresh, options, models);
    LibraryFunction {
        module_path: module_path.to_vec(),
        name: Symbol::from(format!("{}_path", helper.name.as_str())),
        params,
        unsupported_formals: None,
        has_anonymous_block: false,
        body,
        signature: None,
        effects: Default::default(),
        is_async: false,
    }
}

/// An option key a caller writes, in the form it writes it: the options
/// Hash is read back with the same key (`options[:page]`,
/// `options["page"]`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OptionKey {
    name: String,
    symbol: bool,
}

fn empty_hash() -> Expr {
    Expr::new(Span::synthetic(), ExprNode::Hash { entries: vec![], kwargs: false })
}

/// How many positional segments a route's helper takes: those outside
/// its optional groups at least, all of them at most
/// (`/archive(/:period)` takes zero or one).
fn segment_counts(route: &FlatRoute) -> std::ops::RangeInclusive<usize> {
    route.required_params.min(route.path_params.len())..=route.path_params.len()
}

/// The routes a `route_for` call can name, as `as:` names: its first
/// argument's own, or for an interpolated name (`"book_#{leaf.kind}"`,
/// which Rails sends as `public_send("#{name}_url")`) every named route
/// the interpolation can spell that takes `positional` segments. None
/// when the name is not a literal or an interpolation, or names no
/// route of this app.
pub(crate) fn route_for_targets(name: &Expr, positional: usize, flat: &[FlatRoute]) -> Option<Vec<String>> {
    let named = |n: &str| flat.iter().any(|r| r.named && r.as_name == n);
    match &*name.node {
        ExprNode::Lit { value: Literal::Sym { value } } => named(value.as_str()).then(|| vec![value.as_str().to_string()]),
        ExprNode::Lit { value: Literal::Str { value } } => named(value).then(|| vec![value.clone()]),
        ExprNode::StringInterp { parts } => {
            let mut out: Vec<String> = flat
                .iter()
                .filter(|r| r.named && segment_counts(r).contains(&positional) && spells(parts, &r.as_name))
                .map(|r| r.as_name.clone())
                .collect();
            out.sort();
            out.dedup();
            (!out.is_empty()).then_some(out)
        }
        _ => None,
    }
}

/// Can the interpolation's text, with any string in each `#{…}`, be
/// `name`?
pub(super) fn spells(parts: &[InterpPart], name: &str) -> bool {
    fn go(parts: &[InterpPart], rest: &str) -> bool {
        match parts.split_first() {
            None => rest.is_empty(),
            Some((InterpPart::Text { value }, tail)) => rest.strip_prefix(value.as_str()).is_some_and(|r| go(tail, r)),
            Some((InterpPart::Expr { .. }, tail)) => (0..=rest.len()).filter(|&i| rest.is_char_boundary(i)).any(|i| go(tail, &rest[i..])),
        }
    }
    go(parts, name)
}

/// The `route_for` calls in the app's `direct` helpers that name no
/// route [`route_for_targets`] can find: each stays a call to a method
/// no target defines.
pub(crate) fn unresolved_route_for_calls(app: &App) -> Vec<Span> {
    fn walk(e: &Expr, options: Option<&Symbol>, flat: &[FlatRoute], out: &mut Vec<Span>) {
        if let ExprNode::Send { recv: None, method, args, block: None, .. } = &*e.node
            && method.as_str() == "route_for"
            && let Some(name) = args.first()
        {
            let (positional, _, _) = route_for_arguments(&args[1..], options);
            if route_for_targets(name, positional.len(), flat).is_none() {
                out.push(e.span);
            }
        }
        e.node.for_each_child(&mut |c| walk(c, options, flat, out));
    }
    let mut out = Vec::new();
    if app.routes.direct_helpers.is_empty() {
        return out;
    }
    let flat = crate::lower::routes::flatten_routes_for_helpers(app);
    for helper in &app.routes.direct_helpers {
        walk(&helper.body, helper.params.last(), &flat, &mut out);
    }
    out
}

/// The `route_for` call's arguments split into its positional segment
/// arguments, the keyword hash it writes, and whether it ends with the
/// helper's own options parameter (`route_for :t, a, options`).
pub(super) fn route_for_arguments<'a>(rest: &'a [Expr], options: Option<&Symbol>) -> (&'a [Expr], Vec<(Expr, Expr)>, bool) {
    match rest.last().map(|last| &*last.node) {
        Some(ExprNode::Hash { entries, .. }) => (&rest[..rest.len() - 1], entries.clone(), false),
        Some(ExprNode::Var { name, .. }) if Some(name) == options => (&rest[..rest.len() - 1], vec![], true),
        _ => (rest, vec![], false),
    }
}

/// `route_for :target, arg…, k: v` → `<target>_path(arg…)` + query.
fn rewrite_route_for(
    expr: &mut Expr,
    flat: &[FlatRoute],
    helper_params: &crate::det::HashMap<String, Vec<crate::ty::Ty>>,
    fresh: &FreshLocals,
    options: Option<(&Symbol, &std::collections::BTreeSet<OptionKey>)>,
    models: &crate::det::HashMap<Symbol, bool>,
) {
    expr.node
        .for_each_child_mut(&mut |c| rewrite_route_for(c, flat, helper_params, fresh, options, models));

    let ExprNode::Send { recv: None, method, args, block: None, .. } = &*expr.node else {
        return;
    };
    if method.as_str() != "route_for" || args.is_empty() {
        return;
    }
    // Split the rest into positional path arguments and the trailing
    // keyword hash, which is the query.
    let (positional, mut query, forwards_options) = route_for_arguments(&args[1..], options.map(|(name, _)| name));
    // A `route_for` naming a route that does not exist would emit a call
    // to a helper nobody defines. Leave it alone so the failure names the
    // missing route rather than appearing as a mystery NameError
    // (`analyze::diagnostics` reports it).
    let Some(targets) = route_for_targets(&args[0], positional.len(), flat) else {
        return;
    };
    let span = expr.span;
    // The options the helper's callers pass, forwarded: each key read
    // off the options Hash by name, the fragment last.
    let mut anchor = None;
    if forwards_options && let Some((name, keys)) = options {
        let (pairs, fragment) = options_query(name, keys, span);
        query.extend(pairs);
        anchor = fragment;
    }
    let calls: Vec<(String, Expr)> = targets
        .iter()
        .map(|target| {
            let helper = format!("{target}_path");
            let call = target_call(&helper, positional, helper_params, fresh, models, span);
            let mut out = with_query(call, &query, span);
            if let Some(fragment) = &anchor {
                out = concat(out, fragment.clone(), span);
            }
            (target.clone(), out)
        })
        .collect();
    *expr = match &*args[0].node {
        ExprNode::StringInterp { .. } => dispatch_by_name(args[0].clone(), calls, fresh, span),
        _ => calls.into_iter().next().map(|(_, call)| call).expect("one target"),
    };
}

/// `public_send("#{name}_url", …)` over the routes the name can spell:
///
/// ```ruby
/// route_for_arg_0 = "book_#{leaf.kind}"
/// case route_for_arg_0
/// when "book_page" then RouteHelpers.book_page_path(…)
/// else raise NoMethodError, "undefined method '#{route_for_arg_0}_url'"
/// end
/// ```
fn dispatch_by_name(name: Expr, calls: Vec<(String, Expr)>, fresh: &FreshLocals, span: Span) -> Expr {
    let tmp = fresh.name();
    let mut arms: Vec<crate::expr::Arm> = calls
        .into_iter()
        .map(|(target, body)| crate::expr::Arm {
            pattern: crate::expr::Pattern::Lit { value: Literal::Str { value: target } },
            guard: None,
            body,
        })
        .collect();
    let message = Expr::new(
        span,
        ExprNode::StringInterp {
            parts: vec![
                InterpPart::Text { value: "undefined method '".to_string() },
                InterpPart::Expr { expr: var(&tmp, span) },
                InterpPart::Text { value: "_url'".to_string() },
            ],
        },
    );
    let error = Expr::new(
        span,
        ExprNode::Send {
            recv: Some(Expr::new(span, ExprNode::Const { path: vec![Symbol::from("NoMethodError")] })),
            method: Symbol::from("new"),
            args: vec![message],
            block: None,
            parenthesized: true,
        },
    );
    arms.push(crate::expr::Arm {
        pattern: crate::expr::Pattern::Wildcard,
        guard: None,
        body: Expr::new(span, ExprNode::Raise { value: error }),
    });
    Expr::new(
        span,
        ExprNode::Seq {
            exprs: vec![
                assign(&tmp, name, span),
                Expr::new(span, ExprNode::Case { scrutinee: var(&tmp, span), arms }),
            ],
        },
    )
}

/// The query pairs and fragment a forwarded options Hash renders: each
/// key a caller passes, read by name (`options[:q]`), with `anchor:` the
/// fragment and the keys that describe only a host dropped, as a
/// `_path` drops them (`route_url_options::HOST_ONLY_OPTIONS`).
/// `format:` never reaches the Hash: `route_format_suffix` renders it at
/// the call site.
fn options_query(
    options: &Symbol,
    keys: &std::collections::BTreeSet<OptionKey>,
    span: Span,
) -> (Vec<(Expr, Expr)>, Option<Expr>) {
    let literal = |key: &OptionKey| {
        let value = if key.symbol {
            Literal::Sym { value: Symbol::from(key.name.as_str()) }
        } else {
            Literal::Str { value: key.name.clone() }
        };
        Expr::new(span, ExprNode::Lit { value })
    };
    let read = |key: &OptionKey| {
        Expr::new(
            span,
            ExprNode::Send {
                recv: Some(var(options, span)),
                method: Symbol::from("[]"),
                args: vec![literal(key)],
                block: None,
                parenthesized: false,
            },
        )
    };
    let mut pairs = Vec::new();
    let mut anchor = None;
    for key in keys {
        // `url_for`'s reserved options are Symbols; a String key is a
        // query key whatever its name.
        let reserved = |name: &str| key.symbol && key.name == name;
        if reserved("anchor") {
            anchor = Some(fragment(read(key), span));
        } else if !reserved("format")
            && !(key.symbol && crate::lower::route_url_options::HOST_ONLY_OPTIONS.contains(&key.name.as_str()))
        {
            pairs.push((literal(key), read(key)));
        }
    }
    (pairs, anchor)
}

/// `"#" + url_encode(anchor.to_s)`, or `""` for a nil anchor: the
/// fragment after the query string, as the generated helpers render
/// `anchor:` (`routes_to_library::append_anchor`).
fn fragment(anchor: Expr, span: Span) -> Expr {
    let encoded = view_helpers_call("url_encode", vec![to_s(anchor.clone(), span)], span);
    let mut is_nil = send0(anchor, "nil?", span);
    is_nil.ty = Some(crate::ty::Ty::Bool);
    let mut out = Expr::new(
        span,
        ExprNode::If {
            cond: is_nil,
            then_branch: lit_str(String::new(), span),
            else_branch: concat(lit_str("#".to_string(), span), encoded, span),
        },
    );
    out.ty = Some(crate::ty::Ty::Str);
    out
}

/// `RouteHelpers.<helper>(…)` with each `route_for` argument made what
/// the target's segment takes: a record its `to_param` (its id, unless
/// the model renders one of its own), an Integer into a String segment
/// its `to_s`.
fn target_call(
    helper: &str,
    positional: &[Expr],
    helper_params: &crate::det::HashMap<String, Vec<crate::ty::Ty>>,
    fresh: &FreshLocals,
    models: &crate::det::HashMap<Symbol, bool>,
    span: Span,
) -> Expr {
    // An Integer into a segment the helper types `String` (a call site
    // elsewhere fills it with a String) is its `to_s`, as Rails writes
    // it with `to_param` and as the other call sites' rewrite does. An
    // Integer segment stays as it is.
    let declared = helper_params.get(helper);
    let positional: Vec<Expr> = positional
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            // An optional segment (`/archive(/:period)`) is declared
            // `String | nil`: a String slot too.
            let slot = declared.and_then(|tys| tys.get(i));
            let wants_str = matches!(slot.map(crate::ty::Ty::peel_nilable), Some(crate::ty::Ty::Str));
            if let Some(string_param) = record_param(arg.ty.as_ref(), models) {
                return project_record(arg.clone(), string_param, wants_str, fresh, span);
            }
            let is_int = matches!(arg.ty, Some(crate::ty::Ty::Int))
                || matches!(&*arg.node, ExprNode::Lit { value: Literal::Int { .. } });
            // An Integer that can be nil, into an optional segment, keeps
            // its nil (Rails drops the segment) and converts otherwise. A
            // required segment is left as it was: Rails raises on nil there.
            let optional_slot = matches!(slot, Some(crate::ty::Ty::Union { variants }) if variants.contains(&crate::ty::Ty::Nil));
            let nilable_int = matches!(&arg.ty, Some(crate::ty::Ty::Union { variants })
                if variants.contains(&crate::ty::Ty::Int)
                    && variants.iter().all(|v| matches!(v, crate::ty::Ty::Int | crate::ty::Ty::Nil)));
            if wants_str && is_int {
                to_s(arg.clone(), arg.span)
            } else if wants_str && optional_slot && nilable_int {
                to_s_keeping_nil(arg.clone(), fresh)
            } else {
                arg.clone()
            }
        })
        .collect();
    route_helpers_call(helper, positional, span)
}

/// For a model instance (or one that may be nil), whether its
/// `to_param` is a String of its own; None for anything else.
fn record_param(ty: Option<&crate::ty::Ty>, models: &crate::det::HashMap<Symbol, bool>) -> Option<bool> {
    use crate::ty::Ty;
    let model = |t: &Ty| match t {
        Ty::Class { id, .. } => models.get(&id.0).copied(),
        _ => None,
    };
    match ty {
        Some(Ty::Union { variants }) => {
            let models: Vec<bool> = variants.iter().filter_map(model).collect();
            let rest_nil = variants.iter().all(|v| model(v).is_some() || matches!(v, Ty::Nil));
            (!models.is_empty() && rest_nil).then(|| models.contains(&true))
        }
        Some(t) => model(t),
        None => None,
    }
}

/// A record's `to_param` for a segment: its own String one, else its id
/// (as text into a String slot). A record that may be nil is bound once
/// and keeps its nil, which leaves an optional segment out and fails a
/// required one, as Rails does.
fn project_record(record: Expr, string_param: bool, wants_str: bool, fresh: &FreshLocals, span: Span) -> Expr {
    use crate::ty::Ty;
    let project = |e: Expr| {
        if string_param {
            to_param(e, span)
        } else if wants_str {
            to_s(id_of(e, span), span)
        } else {
            id_of(e, span)
        }
    };
    let record_ty = record.ty.clone().unwrap_or(Ty::Untyped);
    if !matches!(&record_ty, Ty::Union { variants } if variants.contains(&Ty::Nil)) {
        return project(record);
    }
    let name = fresh.name();
    let typed = |node: ExprNode, ty: Ty| Expr { ty: Some(ty), ..Expr::new(span, node) };
    let read = |ty: Ty| typed(ExprNode::Var { id: VarId(0), name: name.clone() }, ty);
    let projected = project(read(record_ty.clone().strip_nil()));
    let result_ty = Ty::Union { variants: vec![projected.ty.clone().unwrap_or(Ty::Untyped), Ty::Nil].into() };
    let mut is_nil = send0(read(record_ty.clone()), "nil?", span);
    is_nil.ty = Some(Ty::Bool);
    let check = typed(
        ExprNode::If { cond: is_nil, then_branch: typed(ExprNode::Lit { value: Literal::Nil }, Ty::Nil), else_branch: projected },
        result_ty.clone(),
    );
    typed(ExprNode::Let { id: VarId(0), name, value: record, body: check }, result_ty)
}

fn id_of(record: Expr, span: Span) -> Expr {
    let mut out = send0(record, "id", span);
    out.ty = Some(crate::ty::Ty::Int);
    out
}

fn to_param(record: Expr, span: Span) -> Expr {
    let mut out = send0(record, "to_param", span);
    out.ty = Some(crate::ty::Ty::Str);
    out
}

/// `RouteHelpers.<helper>(args…)`.
fn route_helpers_call(helper: &str, args: Vec<Expr>, span: Span) -> Expr {
    let recv = Expr::new(
        span,
        ExprNode::Const { path: vec![Symbol::from("RouteHelpers")] },
    );
    Expr::new(
        span,
        ExprNode::Send {
            recv: Some(recv),
            method: Symbol::from(helper),
            args,
            block: None,
            parenthesized: true,
        },
    )
}

/// `base` with the query string appended, as a statement sequence in
/// value position.
///
/// Emits, for keys sorted at COMPILE time:
///
/// ```ruby
/// __q = []
/// __q0 = <size expr>
/// __q.push("size=" + ActionView::ViewHelpers.url_encode(__q0.to_s)) unless __q0.nil?
/// __q1 = <v expr>
/// __q.push("v=" + ActionView::ViewHelpers.url_encode(__q1.to_s)) unless __q1.nil?
/// __qbase = RouteHelpers.account_logo_path
/// __q.empty? ? __qbase : __qbase + "?" + __q.join("&")
/// ```
///
/// Each value lands in a temporary first so the nil guard and the
/// rendered piece share ONE evaluation — campfire's
/// `Current.account&.updated_at&.to_fs(:number)` is a chain the
/// safe-navigation desugar already expands, and testing it twice would
/// run it twice. The base is a temporary for the same reason: it is
/// named in both arms of the final conditional.
///
/// Returns `base` untouched when there are no query keys.
fn with_query(base: Expr, query: &[(Expr, Expr)], span: Span) -> Expr {
    let mut pairs: Vec<(String, Expr)> = query
        .iter()
        .filter_map(|(k, v)| match &*k.node {
            ExprNode::Lit { value: Literal::Sym { value } } => {
                Some((value.as_str().to_string(), v.clone()))
            }
            ExprNode::Lit { value: Literal::Str { value } } => Some((value.clone(), v.clone())),
            _ => None,
        })
        .collect();
    if pairs.is_empty() {
        return base;
    }
    // Rails' `Hash#to_query` sorts; written order is not emitted order.
    pairs.sort_by(|a, b| a.0.cmp(&b.0));

    let q = Symbol::from("__q");
    let q_var = || var(&q, span);
    let mut stmts: Vec<Expr> = vec![assign(
        &q,
        Expr::new(span, ExprNode::Array { elements: vec![], style: Default::default() }),
        span,
    )];
    for (i, (key, value)) in pairs.iter().enumerate() {
        let tmp = Symbol::from(format!("__q{i}"));
        stmts.push(assign(&tmp, value.clone(), span));
        let piece = concat(
            lit_str(format!("{key}="), span),
            view_helpers_call("url_encode", vec![to_s(var(&tmp, span), span)], span),
            span,
        );
        let push = Expr::new(
            span,
            ExprNode::Send {
                recv: Some(q_var()),
                method: Symbol::from("push"),
                args: vec![piece],
                block: None,
                parenthesized: true,
            },
        );
        // `unless <value>.nil?` — Rails drops a nil-valued key entirely.
        stmts.push(Expr::new(
            span,
            ExprNode::If {
                cond: send0(var(&tmp, span), "nil?", span),
                then_branch: lit_str(String::new(), span),
                else_branch: push,
            },
        ));
    }
    let base_name = Symbol::from("__qbase");
    stmts.push(assign(&base_name, base, span));
    let joined = concat(
        concat(var(&base_name, span), lit_str("?".to_string(), span), span),
        Expr::new(
            span,
            ExprNode::Send {
                recv: Some(q_var()),
                method: Symbol::from("join"),
                args: vec![lit_str("&".to_string(), span)],
                block: None,
                parenthesized: true,
            },
        ),
        span,
    );
    stmts.push(Expr::new(
        span,
        ExprNode::If {
            cond: send0(q_var(), "empty?", span),
            then_branch: var(&base_name, span),
            else_branch: joined,
        },
    ));
    Expr::new(span, ExprNode::Seq { exprs: stmts })
}

fn var(name: &Symbol, span: Span) -> Expr {
    Expr::new(span, ExprNode::Var { id: VarId(0), name: name.clone() })
}

fn assign(name: &Symbol, value: Expr, span: Span) -> Expr {
    Expr::new(
        span,
        ExprNode::Assign {
            target: crate::expr::LValue::Var { name: name.clone(), id: VarId(0) },
            value,
        },
    )
}

fn lit_str(s: String, span: Span) -> Expr {
    let mut e = Expr::new(span, ExprNode::Lit { value: Literal::Str { value: s } });
    e.ty = Some(crate::ty::Ty::Str);
    e
}

fn send0(recv: Expr, method: &str, span: Span) -> Expr {
    Expr::new(
        span,
        ExprNode::Send {
            recv: Some(recv),
            method: Symbol::from(method),
            args: vec![],
            block: None,
            parenthesized: true,
        },
    )
}

/// Names for the temporaries one direct helper's rewrite introduces:
/// `route_for_arg_<n>`, skipping any name the helper already uses.
struct FreshLocals {
    next: std::cell::Cell<usize>,
    taken: std::cell::RefCell<crate::det::HashSet<Symbol>>,
}

impl FreshLocals {
    fn name(&self) -> Symbol {
        loop {
            let n = self.next.get();
            self.next.set(n + 1);
            let candidate = Symbol::from(format!("route_for_arg_{n}"));
            if self.taken.borrow_mut().insert(candidate.clone()) {
                return candidate;
            }
        }
    }
}

/// `value`'s `to_s`, or nil when it is nil: bound once to a fresh local,
/// then `nil? ? nil : to_s`.
fn to_s_keeping_nil(value: Expr, fresh: &FreshLocals) -> Expr {
    use crate::ty::Ty;
    let span = value.span;
    let name = fresh.name();
    let value_ty = value.ty.clone().unwrap_or(Ty::Untyped);
    let typed = |node: ExprNode, ty: Ty| Expr { ty: Some(ty), ..Expr::new(span, node) };
    let read = || typed(ExprNode::Var { id: VarId(0), name: name.clone() }, value_ty.clone());
    let opt_str = Ty::Union { variants: vec![Ty::Str, Ty::Nil].into() };
    let mut is_nil = send0(read(), "nil?", span);
    is_nil.ty = Some(Ty::Bool);
    let check = typed(
        ExprNode::If {
            cond: is_nil,
            then_branch: typed(ExprNode::Lit { value: Literal::Nil }, Ty::Nil),
            else_branch: to_s(read(), span),
        },
        opt_str.clone(),
    );
    typed(ExprNode::Let { id: VarId(0), name, value, body: check }, opt_str)
}

fn to_s(e: Expr, span: Span) -> Expr {
    let mut out = send0(e, "to_s", span);
    out.ty = Some(crate::ty::Ty::Str);
    out
}

fn view_helpers_call(method: &str, args: Vec<Expr>, span: Span) -> Expr {
    let recv = Expr::new(
        span,
        ExprNode::Const {
            path: vec![Symbol::from("ActionView"), Symbol::from("ViewHelpers")],
        },
    );
    let mut e = Expr::new(
        span,
        ExprNode::Send {
            recv: Some(recv),
            method: Symbol::from(method),
            args,
            block: None,
            parenthesized: true,
        },
    );
    e.ty = Some(crate::ty::Ty::Str);
    e
}

/// `<a> + <b>`, stamped String — `+` rather than `<<`, per the
/// frozen-literal lesson in `lower::literal_append`.
fn concat(a: Expr, b: Expr, span: Span) -> Expr {
    let mut e = Expr::new(
        span,
        ExprNode::Send {
            recv: Some(a),
            method: Symbol::from("+"),
            args: vec![b],
            block: None,
            parenthesized: false,
        },
    );
    e.ty = Some(crate::ty::Ty::Str);
    e
}

/// Interpolation helper kept for symmetry with the other builders; the
/// query path uses `+` concatenation so the pieces stay String-typed.
#[allow(dead_code)]
fn interp(parts: Vec<InterpPart>, span: Span) -> Expr {
    let mut e = Expr::new(span, ExprNode::StringInterp { parts });
    e.ty = Some(crate::ty::Ty::Str);
    e
}
