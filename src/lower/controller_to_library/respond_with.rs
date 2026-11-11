//! `respond_with` (the responders gem) lowered to the response it makes.
//!
//! The Responder compiled from its source cannot run: it dispatches on
//! `"to_#{format}"`, asks `respond_to?` and renders from an options Hash
//! built at run time. What it decides for a navigational (html) request is
//! small and fixed, so it is written out where the call is, in terms of the
//! `render` and `redirect_to` every other action uses:
//!
//! - A template for the action: render it, with the Responder's `error_status`
//!   when a non-GET resource has errors.
//! - No template: GET raises `ActionView::MissingTemplate`; a resource with
//!   errors renders what its verb implies (`new` after POST, `edit` after
//!   PATCH/PUT, none after DELETE), or a redirect otherwise (to `location:`,
//!   with the Responder's `redirect_status`).
//!
//! `location:` also reaches `render`: Rails sets the `Location` header before
//! it looks for the template, so the answer carries it even when rendering.
//!
//! Only controllers whose declared formats are html-only are rewritten (a
//! declaration of `:json` leaves `respond_with` standing and Spinel refuses
//! it: the `api_behavior` it would need is not modeled).

use crate::dialect::{Controller, ControllerBodyItem};
use crate::expr::{BoolOpKind, BoolOpSurface, Expr, ExprNode, LValue, Literal};
use crate::ident::{Symbol, VarId};
use crate::span::Span;

/// The app's responders that define an instance method (`to_html`, `navigation_behavior`, …), which this lowering would bypass.
/// A class-level `error_status` or `redirect_status` is read where the response is written, and the gem's own class stops the walk.
pub(super) fn responders_with_behavior(classes: &[crate::dialect::LibraryClass]) -> crate::det::HashSet<String> {
    const BASE: &str = "ActionController::Responder";
    let defines = |c: &crate::dialect::LibraryClass| c.methods.iter().any(|m| m.receiver == crate::dialect::MethodReceiver::Instance);
    classes
        .iter()
        .filter(|c| c.name.0.as_str() != BASE)
        .filter(|c| {
            let mut cur = Some(*c);
            let mut depth = 0;
            while let Some(class) = cur.filter(|class| class.name.0.as_str() != BASE && depth < 16) {
                if defines(class) {
                    return true;
                }
                depth += 1;
                cur = class.parent.as_ref().and_then(|p| classes.iter().find(|k| &k.name == p));
            }
            false
        })
        .map(|c| c.name.0.as_str().to_string())
        .collect()
}

/// What every controller's `respond_with` lowering needs from the whole app.
pub(super) struct Responders {
    /// `responders_with_behavior`.
    pub with_behavior: crate::det::HashSet<String>,
    /// jbuilder is loaded, so `ActionController::API` renders views too (its railtie includes `ActionView::Rendering`).
    pub jbuilder_loaded: bool,
}

/// Is `responder` a constant naming one of `with_behavior`?
pub(super) fn names_responder_with_behavior(responder: &Expr, with_behavior: &crate::det::HashSet<String>) -> bool {
    let ExprNode::Const { path } = &*responder.node else { return false };
    let name = path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("::");
    with_behavior.contains(&name)
}

/// The class-level `respond_to` of `controller`'s chain, root first, as declared.
/// `Some(true)` when it is html alone; `None` for no declaration or one this cannot read.
pub(super) fn html_only_declared(chain: &[&Controller]) -> Option<bool> {
    declared_formats(chain).map(|formats| formats.iter().all(|f| f == "html"))
}

/// `Some(true)` when the chain's class-level `respond_to` is json alone.
pub(super) fn json_only_declared(chain: &[&Controller]) -> Option<bool> {
    declared_formats(chain).map(|formats| formats.iter().all(|f| f == "json"))
}

fn declared_formats(chain: &[&Controller]) -> Option<Vec<String>> {
    let mut formats: Vec<String> = Vec::new();
    let mut declared = false;
    for controller in chain.iter().rev() {
        for item in &controller.body {
            let ControllerBodyItem::Unknown { expr, .. } = item else { continue };
            let declaration = match &*expr.node {
                ExprNode::Send { recv: None, method, args, block: None, .. } if method.as_str() == "respond_to" => Some((args, true)),
                // `respond_to :html if mimes_for_respond_to.empty?` (Devise): only when nothing above declared one.
                ExprNode::If { cond, then_branch, else_branch } if matches!(&*else_branch.node, ExprNode::Seq { exprs } if exprs.is_empty()) || matches!(&*else_branch.node, ExprNode::Lit { value: Literal::Nil }) => {
                    let asks_empty = matches!(&*cond.node, ExprNode::Send { recv: Some(r), method, .. } if method.as_str() == "empty?" && matches!(&*r.node, ExprNode::Send { recv: None, method, .. } if method.as_str() == "mimes_for_respond_to"));
                    match &*then_branch.node {
                        ExprNode::Send { recv: None, method, args, block: None, .. } if method.as_str() == "respond_to" && asks_empty => Some((args, formats.is_empty())),
                        _ => None,
                    }
                }
                _ => None,
            };
            let Some((args, applies)) = declaration else { continue };
            if !applies {
                continue;
            }
            for arg in args.iter() {
                match &*arg.node {
                    ExprNode::Lit { value: Literal::Sym { value } } => {
                        declared = true;
                        formats.push(value.as_str().to_string());
                    }
                    // Keywords (`only:`, `except:`) narrow by action; not read here.
                    ExprNode::Hash { kwargs: true, .. } => return None,
                    _ => return None,
                }
            }
        }
    }
    if !declared {
        return None;
    }
    Some(formats)
}

/// Facts about the enclosing action a `respond_with` needs.
pub(super) struct Facts<'a> {
    /// The action this body belongs to, `None` for a private helper.
    pub action: Option<&'a str>,
    /// A template for `action` exists (in this controller's chain).
    pub has_template: bool,
    /// What `responder` reads in this chain, for its statuses: `self.responder = EXPR`.
    pub responder: Option<&'a Expr>,
    /// The chain declares json alone: the Responder's `to_format` path, not the navigational one.
    pub json: bool,
    /// The controller renders views (`ActionView::Rendering`: `ActionController::Base`, or `API` with jbuilder loaded).
    pub view_rendering: bool,
}

fn sym(s: &str, span: Span) -> Expr {
    Expr::new(span, ExprNode::Lit { value: Literal::Sym { value: Symbol::from(s) } })
}

fn var(name: &str, span: Span) -> Expr {
    Expr::new(span, ExprNode::Var { id: VarId(0), name: Symbol::from(name) })
}

fn call(recv: Option<Expr>, method: &str, args: Vec<Expr>, span: Span) -> Expr {
    Expr::new(span, ExprNode::Send { recv, method: Symbol::from(method), args, block: None, parenthesized: true })
}

fn assign(name: &str, value: Expr, span: Span) -> Expr {
    Expr::new(span, ExprNode::Assign { target: LValue::Var { id: VarId(0), name: Symbol::from(name) }, value })
}

fn kwargs(entries: Vec<(Expr, Expr)>, span: Span) -> Expr {
    Expr::new(span, ExprNode::Hash { entries, kwargs: true })
}

fn cond(cond: Expr, then_branch: Expr, else_branch: Expr, span: Span) -> Expr {
    Expr::new(span, ExprNode::If { cond, then_branch, else_branch })
}

fn and(left: Expr, right: Expr, span: Span) -> Expr {
    Expr::new(span, ExprNode::BoolOp { op: BoolOpKind::And, surface: BoolOpSurface::Symbol, left, right })
}

fn or(left: Expr, right: Expr, span: Span) -> Expr {
    Expr::new(span, ExprNode::BoolOp { op: BoolOpKind::Or, surface: BoolOpSurface::Symbol, left, right })
}

fn request_is(verb: &str, span: Span) -> Expr {
    call(Some(call(None, "request", vec![], span)), &format!("{verb}?"), vec![], span)
}

/// `responder.error_status` / `.redirect_status`: the chain's `self.responder =` if it has one, else Responder's defaults.
fn status(facts: &Facts<'_>, reader: &str, default: &str, span: Span) -> Expr {
    match facts.responder {
        Some(responder) => call(Some(responder.clone()), reader, vec![], span),
        None => sym(default, span),
    }
}

fn is_pure(expr: &Expr) -> bool {
    matches!(&*expr.node, ExprNode::Var { .. } | ExprNode::Ivar { .. } | ExprNode::Lit { .. } | ExprNode::SelfRef | ExprNode::Const { .. })
}

/// The html arm of a `respond_with` block: the body of its one `format.html { … }` or `format.any(…) { … }` naming html,
/// or a call of a forwarded `&block`. `Ok(None)` when that arm has no block of its own. `Err` for anything else: Rails picks
/// an arm per request and runs whatever else the block holds, which a single arm cannot keep.
fn block_html_arm(block: &Expr) -> Result<Option<Expr>, ()> {
    let ExprNode::Lambda { body, params, .. } = &*block.node else { return Ok(None) };
    let Some(format_param) = params.first().map(|p| p.as_str()) else { return Ok(None) };
    let stmt = match &*body.node {
        ExprNode::Seq { exprs } if exprs.len() == 1 => &exprs[0],
        ExprNode::Seq { .. } => return Err(()),
        _ => body,
    };
    let ExprNode::Send { recv: Some(r), method, args, block: arm, .. } = &*stmt.node else { return Err(()) };
    if !matches!(&*r.node, ExprNode::Var { name, .. } if name.as_str() == format_param) {
        return Err(());
    }
    // Devise calls `format.any(*navigational_formats, &block)`; its default list holds html.
    let navigational = |a: &Expr| matches!(&*a.node, ExprNode::Splat { value } if matches!(&*value.node, ExprNode::Send { recv: None, method, args, block: None, .. } if method.as_str() == "navigational_formats" && args.is_empty()));
    let html = |a: &Expr| matches!(&*a.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "html");
    let answers_html = match method.as_str() {
        "html" => args.is_empty(),
        "any" | "all" => args.iter().all(|a| navigational(a) || matches!(&*a.node, ExprNode::Lit { .. })) && args.iter().any(|a| navigational(a) || html(a)),
        _ => false,
    };
    if !answers_html {
        return Err(());
    }
    match arm {
        // `format.html { |variant| … }` is handed a variant collector, which the body alone loses.
        Some(arm) => match &*arm.node {
            ExprNode::Lambda { params, .. } if !params.is_empty() => Err(()),
            ExprNode::Lambda { body, .. } => Ok(Some(body.clone())),
            // `&block`: the block the enclosing method was given is the arm.
            _ => Ok(Some(call(Some(arm.clone()), "call", vec![], arm.span))),
        },
        None => Ok(None),
    }
}

/// `respond_with` rewritten to the navigational response, or `None` for a shape this does not read
/// (it stays as written, and Spinel refuses it there).
fn lower_call(call_expr: &Expr, facts: &Facts<'_>) -> Option<Expr> {
    if facts.json {
        return lower_json_call(call_expr, facts);
    }
    let span = call_expr.span;
    let ExprNode::Send { recv, method, args, block, .. } = &*call_expr.node else { return None };
    if method.as_str() != "respond_with" || !recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef)) {
        return None;
    }
    let mut args: Vec<Expr> = args.clone();
    if let Some(block) = block {
        if let Some(arm) = block_html_arm(block).ok()? {
            // A custom responder need not use the block at all.
            let names_responder = args.iter().any(|a| matches!(&*a.node, ExprNode::Hash { entries, .. } if entries.iter().any(|(k, _)| matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "responder"))));
            if names_responder {
                return None;
            }
            // The block is the html response; the responders' arguments only pass to a block the block ignores.
            let mut stmts: Vec<Expr> = args
                .into_iter()
                .filter(|a| !is_pure(a) && !matches!(&*a.node, ExprNode::Splat { .. } | ExprNode::Hash { .. }))
                .collect();
            stmts.push(arm);
            return Some(Expr::new(span, ExprNode::Seq { exprs: stmts }));
        }
    }
    // The options: a Hash after the resource. A call can build one at run time, as Devise's
    // `serialize_options` does; a variable or ivar there names a further resource, which stays as written.
    let options = match args.last().map(|a| &*a.node) {
        Some(ExprNode::Hash { .. } | ExprNode::Send { .. }) if args.len() > 1 => args.pop(),
        _ => None,
    };
    let mut location = None;
    let mut user_status = None;
    let mut action_opt = None;
    let mut render_opt = None;
    let mut rest: Vec<(Expr, Expr)> = Vec::new();
    let mut prelude: Vec<Expr> = Vec::new();
    match options.as_ref().map(|o| (&*o.node, o)) {
        Some((ExprNode::Hash { entries, .. }, _)) => {
            for (k, v) in entries {
                let ExprNode::Lit { value: Literal::Sym { value } } = &*k.node else { return None };
                match value.as_str() {
                    "location" => {
                        if matches!(&*v.node, ExprNode::Lambda { .. }) {
                            return None;
                        }
                        location = Some(v.clone());
                    }
                    "status" => user_status = Some(v.clone()),
                    "action" => action_opt = Some(v.clone()),
                    "render" => render_opt = Some(v.clone()),
                    // `responder:` substitutes the class this models, `allow_other_host:` reaches a redirect path this does not write.
                    "responder" | "allow_other_host" => return None,
                    // Other keys are `render` options a template render ignores (`methods:`, `only:`, `root:`) or carry over.
                    _ => rest.push((k.clone(), v.clone())),
                }
            }
        }
        // A Hash computed at run time (`respond_with(resource, serialize_options(resource))`) is evaluated as
        // Ruby does. Its keys can't be read here, so one that would change the response raises rather than being dropped.
        Some((_, o)) => {
            prelude.push(assign("__rh_options", o.clone(), span));
            let names_a_response_key = ["location", "status", "action", "render", "responder", "allow_other_host"]
                .into_iter()
                .map(|key| call(Some(var("__rh_options", span)), "key?", vec![sym(key, span)], span))
                .reduce(|left, right| or(left, right, span))
                .expect("the list is not empty");
            let message = Expr::new(span, ExprNode::Lit { value: Literal::Str { value: "respond_with: an options Hash computed at run time cannot set the response".to_string() } });
            let error = call(Some(Expr::new(span, ExprNode::Const { path: vec![Symbol::from("ArgumentError")] })), "new", vec![message], span);
            prelude.push(cond(names_a_response_key, Expr::new(span, ExprNode::Raise { value: error }), Expr::new(span, ExprNode::Seq { exprs: vec![] }), span));
        }
        None => {}
    }
    // Without a block, the default response needs the action's own name.
    let action = facts.action?;
    // One resource: several would need the array written once for both the errors check and the redirect.
    if args.len() != 1 || matches!(&*args[0].node, ExprNode::Splat { .. }) {
        return None;
    }
    let resource_expr = args.last().expect("checked").clone();
    let resource_is_empty_hash = matches!(&*resource_expr.node, ExprNode::Hash { entries, .. } if entries.is_empty());

    // `{}` is no resource, so without `location:` there is nothing to redirect to.
    if resource_is_empty_hash && location.is_none() {
        return None;
    }
    // Arguments in the order Ruby evaluates them: the resource, then the options.
    let mut stmts = Vec::new();
    if !resource_is_empty_hash {
        stmts.push(assign("__rh_resource", resource_expr, span));
    }
    stmts.extend(prelude);
    let location_read = location.map(|l| {
        stmts.push(assign("__rh_location", l, span));
        var("__rh_location", span)
    });
    // The redirect's target: `location:`, or the resource.
    let target = location_read.clone().unwrap_or_else(|| var("__rh_resource", span));
    // `resource.respond_to?(:errors) && !resource.errors.empty?`
    let has_errors = (!resource_is_empty_hash).then(|| {
        let responds = call(Some(var("__rh_resource", span)), "respond_to?", vec![sym("errors", span)], span);
        let empty = call(Some(call(Some(var("__rh_resource", span)), "errors", vec![], span)), "empty?", vec![], span);
        let not_empty = Expr::new(span, ExprNode::Send { recv: Some(empty), method: Symbol::from("!"), args: vec![], block: None, parenthesized: false });
        assign("__rh_has_errors", and(responds, not_empty, span), span)
    });
    let errors_read = has_errors.is_some().then(|| var("__rh_has_errors", span));
    if let Some(a) = has_errors {
        stmts.push(a);
    }

    // `render <template>, status: …, location: …`, the rest of the options carried over.
    let render = |template: &str, status: Option<Expr>| {
        let mut entries: Vec<(Expr, Expr)> = Vec::new();
        if let Some(s) = status {
            entries.push((sym("status", span), s));
        }
        if let Some(l) = &location_read {
            entries.push((sym("location", span), l.clone()));
        }
        entries.extend(rest.iter().cloned());
        let mut args = vec![sym(template, span)];
        if !entries.is_empty() {
            args.push(kwargs(entries, span));
        }
        call(None, "render", args, span)
    };
    let redirect = || {
        let entries = vec![(sym("status", span), status(facts, "redirect_status", "found", span))];
        call(None, "redirect_to", vec![target.clone(), kwargs(entries, span)], span)
    };

    let response = if facts.has_template {
        // `default_render`: a template of this action is rendered, with `error_status` when a non-GET resource has errors.
        let error_status = user_status.clone().unwrap_or_else(|| status(facts, "error_status", "ok", span));
        match &errors_read {
            Some(errors) => {
                let failed = and(Expr::new(span, ExprNode::Send { recv: Some(request_is("get", span)), method: Symbol::from("!"), args: vec![], block: None, parenthesized: false }), errors.clone(), span);
                cond(failed, render(action, Some(error_status)), render(action, user_status), span)
            }
            None => render(action, user_status),
        }
    } else {
        // `navigation_behavior`: GET raises; errors render what the verb implies; otherwise redirect.
        let error_status = status(facts, "error_status", "ok", span);
        let error_branch = match (&action_opt, &render_opt) {
            // `render: { … }`: Responder hands it to `render` whole; the `location:` a template-less `render` already set stays.
            (_, Some(render_options)) => {
                let ExprNode::Hash { entries, .. } = &*render_options.node else { return None };
                let mut entries = entries.clone();
                if let Some(l) = &location_read {
                    entries.push((sym("location", span), l.clone()));
                }
                call(None, "render", vec![kwargs(entries, span)], span)
            }
            (Some(a), None) => {
                let ExprNode::Lit { value: Literal::Sym { value } } = &*a.node else { return None };
                render(value.as_str(), Some(error_status))
            }
            (None, None) => cond(
                request_is("post", span),
                render("new", Some(error_status.clone())),
                cond(
                    or(request_is("patch", span), request_is("put", span), span),
                    render("edit", Some(error_status)),
                    redirect(),
                    span,
                ),
                span,
            ),
        };
        let unless_get = match errors_read {
            Some(errors) => cond(errors, error_branch, redirect(), span),
            None => redirect(),
        };
        let missing = Expr::new(span, ExprNode::Raise { value: Expr::new(span, ExprNode::Const { path: vec![Symbol::from("ActionView"), Symbol::from("MissingTemplate")] }) });
        cond(request_is("get", span), missing, unless_get, span)
    };
    stmts.push(response);
    Some(Expr::new(span, ExprNode::Seq { exprs: stmts }))
}

/// `respond_with` in a controller whose formats are json alone: the Responder's `to_format`.
///
/// - A bare `Accept: */*` with no explicit format: the json path where views render; without them
///   (`ActionController::API`, no jbuilder) a blank body with the call's status, location and options.
/// - Not a json request: the block's html arm for html, else `ActionController::UnknownFormat` (406).
/// - Errors on a non-GET request: `render json: { errors: resource.errors }, status: :unprocessable_entity`,
///   with the errors keyed by attribute (`Base#errors_json`).
/// - A json template for the action: render it (`default_render`).
/// - Otherwise `api_behavior`: GET renders the resource as json, POST renders it with 201 and the
///   location, and any other verb answers 204. A controller that renders views keeps the location
///   there too, because Rails sets it before the template lookup fails.
///
/// A call outside a routed action, in a controller that renders views, stays: `default_render` would look up the
/// calling action's template. Only a literal options Hash is read; `action:`, `render:`, `responder:` and `allow_other_host:` leave
/// the call standing, as do several resources, a computed options Hash and a block with another arm. A POST with
/// no `location:` whose resource is not an instance variable raises on success, as native has no `url_for(record)`.
fn lower_json_call(call_expr: &Expr, facts: &Facts<'_>) -> Option<Expr> {
    let span = call_expr.span;
    let ExprNode::Send { recv, method, args, block, .. } = &*call_expr.node else { return None };
    if method.as_str() != "respond_with" || !recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef)) {
        return None;
    }
    let html_arm = match block {
        Some(block) => Some(block_html_arm(block).ok()??),
        None => None,
    };
    let mut args: Vec<Expr> = args.clone();
    let options = match args.last().map(|a| &*a.node) {
        Some(ExprNode::Hash { .. }) if args.len() > 1 => args.pop(),
        Some(ExprNode::Send { .. }) if args.len() > 1 => return None,
        _ => None,
    };
    let mut location = None;
    let mut user_status = None;
    let mut rest: Vec<(Expr, Expr)> = Vec::new();
    if let Some(options) = &options {
        let ExprNode::Hash { entries, .. } = &*options.node else { return None };
        for (k, v) in entries {
            let ExprNode::Lit { value: Literal::Sym { value } } = &*k.node else { return None };
            match value.as_str() {
                "location" if !matches!(&*v.node, ExprNode::Lambda { .. }) => location = Some(v.clone()),
                "status" => user_status = Some(v.clone()),
                "location" | "action" | "render" | "responder" | "allow_other_host" | "json" => return None,
                _ => rest.push((k.clone(), v.clone())),
            }
        }
    }
    if args.len() != 1 || matches!(&*args[0].node, ExprNode::Splat { .. }) {
        return None;
    }
    let resource_expr = args[0].clone();
    let resource_is_empty_hash = matches!(&*resource_expr.node, ExprNode::Hash { entries, .. } if entries.is_empty());
    // `api_location` without `location:` is the resource's own url.
    let location_fallback = match &*resource_expr.node {
        ExprNode::Ivar { .. } if location.is_none() => Some(resource_expr.clone()),
        _ => None,
    };

    let resource_is_hash = matches!(&*resource_expr.node, ExprNode::Hash { .. });
    let mut stmts = vec![assign("__rh_resource", resource_expr, span)];
    let location_read = location.map(|l| {
        stmts.push(assign("__rh_location", l, span));
        var("__rh_location", span)
    });
    let has_errors = (!resource_is_empty_hash).then(|| {
        let responds = call(Some(var("__rh_resource", span)), "respond_to?", vec![sym("errors", span)], span);
        let empty = call(Some(call(Some(var("__rh_resource", span)), "errors", vec![], span)), "empty?", vec![], span);
        let not_empty = Expr::new(span, ExprNode::Send { recv: Some(empty), method: Symbol::from("!"), args: vec![], block: None, parenthesized: false });
        and(responds, not_empty, span)
    });
    let not_get = Expr::new(span, ExprNode::Send { recv: Some(request_is("get", span)), method: Symbol::from("!"), args: vec![], block: None, parenthesized: false });

    // `render(json: …)` / `render(:action, …)` with the call's options carried over.
    let render_with = |first: Option<Expr>, body: Option<(&str, Expr)>, status: Option<Expr>, location: Option<Expr>| {
        let mut entries: Vec<(Expr, Expr)> = Vec::new();
        if let Some((key, value)) = body {
            entries.push((sym(key, span), value));
        }
        if let Some(s) = status {
            entries.push((sym("status", span), s));
        }
        if let Some(l) = location {
            entries.push((sym("location", span), l));
        }
        entries.extend(rest.iter().cloned());
        let mut args: Vec<Expr> = first.into_iter().collect();
        if !entries.is_empty() {
            args.push(kwargs(entries, span));
        }
        call(None, "render", args, span)
    };
    let resource = || var("__rh_resource", span);
    let get_branch = render_with(None, Some(("json", resource())), user_status.clone(), location_read.clone());
    let post_status = Some(user_status.clone().unwrap_or_else(|| sym("created", span)));
    let post_branch = match location_read.clone().or(location_fallback) {
        Some(post_location) => render_with(None, Some(("json", resource())), post_status, Some(post_location)),
        // `api_location` is `url_for(resource)`, which only an instance variable's name resolves on native
        // (`rewrite_location_in_kwargs`). Any other record raises rather than answer with a wrong Location;
        // its errors branch still answers 422. A Hash has no url.
        None if !resource_is_hash => Expr::new(
            span,
            ExprNode::Raise {
                value: Expr::new(
                    span,
                    ExprNode::Lit {
                        value: Literal::Str { value: "roundhouse: respond_with on a POST needs location: for a resource that is not an instance variable".into() },
                    },
                ),
            },
        ),
        None => return None,
    };
    let mut head_args = vec![sym("no_content", span)];
    if facts.view_rendering && let Some(l) = &location_read {
        head_args.push(kwargs(vec![(sym("location", span), l.clone())], span));
    }
    let other_branch = call(None, "head", head_args, span);
    let api_behavior = cond(request_is("get", span), get_branch, cond(request_is("post", span), post_branch, other_branch, span), span);
    // `default_render` looks up the calling action's template, which a method no route reaches can't name.
    if facts.action.is_none() && facts.view_rendering {
        return None;
    }
    // `default_render`: the action's json template, given the call's options.
    let rendered = match (facts.action, facts.has_template && facts.view_rendering) {
        (Some(action), true) => render_with(Some(sym(action, span)), None, user_status.clone(), location_read.clone()),
        _ => api_behavior,
    };
    let json_path = match has_errors {
        Some(errors) => {
            // `errors_json`, as `errors_index` writes a `render json:` errors read: Rails renders `errors.as_json`,
            // keyed by attribute. That pass runs before this one, so it never sees this Hash.
            let body = Expr::new(span, ExprNode::Hash { entries: vec![(sym("errors", span), call(Some(resource()), "errors_json", vec![], span))], kwargs: false });
            // `display_errors` renders the errors and the status alone, none of the call's options.
            let display_errors = call(
                None,
                "render",
                vec![kwargs(vec![(sym("json", span), body), (sym("status", span), sym("unprocessable_entity", span))], span)],
                span,
            );
            cond(and(not_get, errors, span), display_errors, rendered, span)
        }
        None => rendered,
    };
    let unknown = Expr::new(span, ExprNode::Raise { value: Expr::new(span, ExprNode::Const { path: vec![Symbol::from("ActionController"), Symbol::from("UnknownFormat")] }) });
    let format_is = |f: &str| call(Some(call(Some(call(None, "request", vec![], span)), "format", vec![], span)), &format!("{f}?"), vec![], span);
    let otherwise = match html_arm {
        Some(arm) => cond(format_is("html"), arm, unknown, span),
        None => unknown,
    };
    // A bare `Accept: */*` with no explicit format (`params[:format]`) negotiates to the first declared format, json.
    // Where views render that is the json path. `ActionController::API` without them renders a blank body
    // with the call's status, location and options instead.
    let format_param = call(Some(call(None, "params", vec![], span)), "[]", vec![sym("format", span)], span);
    let any_format = and(call(None, "accepts_any_format", vec![], span), call(Some(call(Some(format_param), "to_s", vec![], span)), "empty?", vec![], span), span);
    let negotiated = if facts.view_rendering {
        cond(or(format_is("json"), any_format, span), json_path, otherwise, span)
    } else {
        let blank = render_with(None, Some(("plain", Expr::new(span, ExprNode::Lit { value: Literal::Str { value: " ".into() } }))), user_status.clone(), location_read.clone());
        cond(format_is("json"), json_path, cond(any_format, blank, otherwise, span), span)
    };
    stmts.push(negotiated);
    Some(Expr::new(span, ExprNode::Seq { exprs: stmts }))
}

/// Every `respond_with` in `body` that lowers, replaced; the rest left standing.
pub(super) fn lower(body: &Expr, facts: &Facts<'_>) -> Expr {
    let mut out = body.clone();
    walk(&mut out, facts);
    out
}

fn walk(e: &mut Expr, facts: &Facts<'_>) {
    if let Some(lowered) = lower_call(e, facts) {
        *e = lowered;
        return;
    }
    e.node.for_each_child_mut(&mut |c| walk(c, facts));
}
