//! A template no request can reach has its errors reported as warnings:
//! its ivars have no action to come from, so every read reported "has
//! no known type" against a page Rails never renders (lobsters'
//! `mod/notes/index`, whose route redirects away and whose controller
//! has only `create`). It is still typed and emitted, so a judgement
//! made wrongly here changes what is reported, not what runs.
//!
//! Unreachable means all of: its directory's controller exists and
//! descends from Rails' own base through the app's controllers; neither
//! that controller, its ancestors, the modules they include, nor any
//! controller inheriting from it (which renders an inherited template)
//! defines the action; no route names it; and no `render` in those
//! controllers or anywhere else names the template. A controller among
//! them that renders a computed name, or includes a module the app does
//! not define, keeps every template of the directory, since what it
//! renders cannot be read here.

use crate::det::{HashMap, HashSet};

use crate::app::App;
use crate::dialect::{Controller, ControllerBodyItem};
use crate::expr::{Expr, ExprNode, Literal};

/// The controllers a source gem's files declare that no route dispatches to, neither them nor a
/// controller that inherits from them (devise's `PasswordsController` where no model is `:recoverable`):
/// Rails loads them, and never runs an action of theirs.
pub(crate) fn unrouted_gem_controllers(app: &App) -> HashSet<String> {
    let routed: HashSet<String> = crate::lower::routes::flatten_routes_for_helpers(app)
        .into_iter()
        .map(|r| r.controller.0.as_str().to_string())
        .collect();
    let by_name: HashMap<&str, &Controller> = app.controllers.iter().map(|c| (c.name.0.as_str(), c)).collect();
    let gem_source = |c: &Controller| {
        app.source_path(c.parent_span.file)
            .is_some_and(|p| p.contains("/.roundhouse/source_gems/") || p.starts_with(".roundhouse/source_gems/"))
    };
    let descends = |child: &Controller, ancestor: &str| {
        let mut current = child;
        for _ in 0..32 {
            if current.name.0.as_str() == ancestor {
                return true;
            }
            let Some(next) = current.parent.as_ref().and_then(|p| by_name.get(p.0.as_str().trim_start_matches("::"))) else { return false };
            current = next;
        }
        false
    };
    app.controllers
        .iter()
        .filter(|c| gem_source(c))
        .filter(|c| !app.controllers.iter().any(|d| routed.contains(d.name.0.as_str()) && descends(d, c.name.0.as_str())))
        .map(|c| c.name.0.as_str().to_string())
        .collect()
}

pub(super) fn mark(app: &mut App) {
    if app.controllers.is_empty() || app.views.is_empty() {
        return;
    }
    let unrouted = unrouted_gem_controllers(app);
    let by_name: HashMap<&str, &Controller> = app.controllers.iter().map(|c| (c.name.0.as_str(), c)).collect();
    let modules: HashMap<&str, HashSet<&str>> = app
        .library_classes
        .iter()
        .map(|lc| (lc.name.0.as_str(), lc.methods.iter().map(|m| m.name.as_str()).collect()))
        .collect();
    let routes: HashSet<(String, String)> = crate::lower::routes::flatten_routes_for_helpers(app)
        .into_iter()
        .map(|r| (r.controller.0.as_str().to_string(), r.action.as_str().to_string()))
        .collect();
    let named_anywhere = render_literals_outside_controllers(app, &unrouted);

    let mut unreachable: Vec<usize> = Vec::new();
    for (i, view) in app.views.iter().enumerate() {
        let (dir, base) = crate::lower::view_to_library::split_view_name(view.name.as_str());
        if dir.is_empty() || dir.starts_with("layouts") || base.starts_with('_') {
            continue;
        }
        let class = format!("{}Controller", crate::naming::camelize_path(dir));
        let Some(controller) = by_name.get(class.as_str()) else { continue };
        if !rooted_in_rails(controller, &by_name) {
            continue;
        }
        let family = family(controller, &by_name);
        // Not by an ancestor's render either, for a gem controller no route dispatches to: Rails looks a template up
        // under the running controller's own prefixes, and no running controller has this directory among them.
        let unrouted_dir = unrouted.contains(controller.name.0.as_str());
        let reachable = (!unrouted_dir && family.iter().any(|c| {
            // Not an action of a gem's controller no route dispatches to: Rails never runs it.
            (defines(c, base, &modules) && !unrouted.contains(c.name.0.as_str()))
                || routes.contains(&(c.name.0.as_str().to_string(), base.to_string()))
                || (renders(c, base, view.name.as_str()) && !unrouted.contains(c.name.0.as_str()))
        })) || named_anywhere.iter().any(|lit| names_template(lit, view.name.as_str()));
        if !reachable {
            unreachable.push(i);
        }
    }
    unreachable.extend(unreachable_partials(app));
    for i in unreachable {
        let view = &app.views[i];
        let mut d = crate::diagnostic::Diagnostic::unsupported(
            view.body.span,
            None,
            "template",
            format!(
                "`{}` is rendered by no action, route or render call, so Rails never reaches it; its errors are reported as warnings",
                view.name.as_str()
            ),
        );
        d.severity = crate::diagnostic::Severity::Warning;
        super::prism::push(d);
        app.unreachable_views.insert(view.name.clone());
    }
}

/// The partials nothing renders (maybe's `accounts/show/_activity`, which a component replaced): no String literal
/// anywhere in the app names one (a Symbol is no partial's name: `render :x` is a template) (its base name, its path, or a path ending in it), no interpolation's text
/// spells part of it, it is not a model's `to_partial_path`, and its directory is not a locked gem's (a gem's own
/// code renders `kaminari/_page` by name).
fn unreachable_partials(app: &App) -> Vec<usize> {
    fn collect(e: &Expr, names: &mut HashSet<String>, fragments: &mut Vec<String>) {
        match &*e.node {
            ExprNode::Lit { value: Literal::Str { value } } => {
                names.insert(value.trim_start_matches('/').to_string());
            }
            ExprNode::StringInterp { parts } => {
                for part in parts {
                    if let crate::expr::InterpPart::Text { value } = part {
                        fragments.push(value.clone());
                    }
                }
            }
            _ => {}
        }
        e.node.for_each_child(&mut |c| collect(c, names, fragments));
    }
    // A name the request decides (`render partial: params[:panel]`, `render session[:panel].to_s`, a `partial:`
    // local not every assignment of which is a literal, an interpolation with no fragment to match) may be any
    // partial's.
    // A request value carried as a name: `params[:panel]`, `session[:p].to_s`, a name a request value was assigned,
    // an interpolation or a branch of one. Not a record a request selects (`Article.find(params[:id])`), which is an
    // object the render asks for its own partial.
    fn reads_request(e: &Expr, fed: &HashSet<String>) -> bool {
        match &*e.node {
            ExprNode::Send { recv: None, method, args, .. } if matches!(method.as_str(), "sprintf" | "format") => args.iter().any(|a| reads_request(a, fed)),
            ExprNode::Send { recv: None, method, args, .. } => args.is_empty() && matches!(method.as_str(), "params" | "session" | "cookies" | "request"),
            // A name built from one (`"articles/" + params[:panel]`), apart from a record a request selects.
            ExprNode::Send { recv: Some(r), method, args, .. } if method.as_str() == "+" => {
                reads_request(r, fed) || args.iter().any(|a| reads_request(a, fed))
            }
            // Any method on one (`gsub`, `cookies.signed[...]`); a record a request selects has it as an argument instead.
            ExprNode::Send { recv: Some(r), .. } => reads_request(r, fed),
            ExprNode::OpAssign { value, .. } => reads_request(value, fed),
            ExprNode::Var { name, .. } => fed.contains(name.as_str()),
            ExprNode::Ivar { name } => fed.contains(&format!("@{name}")),
            ExprNode::StringInterp { parts } => parts.iter().any(|p| matches!(p, crate::expr::InterpPart::Expr { expr } if reads_request(expr, fed))),
            ExprNode::If { then_branch, else_branch, .. } => reads_request(then_branch, fed) || reads_request(else_branch, fed),
            ExprNode::BoolOp { left, right, .. } => reads_request(left, fed) || reads_request(right, fed),
            _ => false,
        }
    }
    // Every local and ivar assignment of a body, `||=` too, by name (an ivar's with its `@`, apart from a local's).
    fn assignments<'a>(e: &'a Expr, out: &mut Vec<(String, bool, &'a Expr)>) {
        // A `+=`'s value is not the name it leaves, so the write itself is what the name was assigned.
        let written = match &*e.node {
            ExprNode::Assign { target, value } => Some((target, value)),
            ExprNode::OpAssign { target, .. } => Some((target, e)),
            _ => None,
        };
        if let Some((target, value)) = written {
            match target {
                crate::expr::LValue::Var { name, .. } => out.push((name.as_str().to_string(), false, value)),
                crate::expr::LValue::Ivar { name } => out.push((format!("@{name}"), true, value)),
                _ => {}
            }
        }
        e.node.for_each_child(&mut |c| assignments(c, out));
    }
    // The names a request value reaches, through aliases too (`source = params[:p]; panel = source`).
    fn request_fed(assigns: &[(String, bool, &Expr)], fed: &mut HashSet<String>) {
        loop {
            let before = fed.len();
            for (name, _, value) in assigns {
                if !fed.contains(name) && reads_request(value, fed) {
                    fed.insert(name.clone());
                }
            }
            if fed.len() == before {
                break;
            }
        }
    }
    // The ivars a request value reaches, from any body (a controller's reaches its views), to a fixed point.
    let mut fed_ivars: HashSet<String> = HashSet::default();
    loop {
        let before = fed_ivars.len();
        let mut grow = |b: &Expr| {
            let mut here = Vec::new();
            assignments(b, &mut here);
            // Through the body's locals too (`candidate = params[:p]; @panel = candidate`), exporting only its ivars.
            let mut fed = fed_ivars.clone();
            request_fed(&here, &mut fed);
            fed_ivars.extend(fed.into_iter().filter(|n| n.starts_with('@')));
        };
        crate::lower::for_each_hook_body_ref(app, &mut |b| grow(b));
        for v in &app.views {
            grow(&v.body);
        }
        if fed_ivars.len() == before {
            break;
        }
    }
    // A block's parameters, which shadow a local of the name: the assignments by name are not that binding's.
    fn block_params(e: &Expr, out: &mut HashSet<String>) {
        if let ExprNode::Lambda { params, rest_param, extra_params, block_param, .. } = &*e.node {
            out.extend(params.iter().chain(rest_param).chain(block_param).map(|p| p.as_str().to_string()));
            out.extend(extra_params.iter().map(|p| p.name.as_str().to_string()));
        }
        e.node.for_each_child(&mut |c| block_params(c, out));
    }
    // A local a String mutator changes in place (`panel.concat("ost")`): its literal assignments no longer name it.
    fn mutated_locals(e: &Expr, out: &mut HashSet<String>) {
        const MUTATORS: &[&str] = &["concat", "<<", "replace", "insert", "prepend", "sub!", "gsub!", "tr!", "squeeze!", "delete!", "slice!", "chomp!", "chop!", "strip!", "lstrip!", "rstrip!", "downcase!", "upcase!", "capitalize!", "swapcase!", "succ!", "next!", "reverse!", "[]=", "clear", "force_encoding", "unicode_normalize!", "squish!"];
        if let ExprNode::Send { recv: Some(r), method, .. } = &*e.node
            && let ExprNode::Var { name, .. } = &*r.node
            && MUTATORS.contains(&method.as_str())
        {
            out.insert(name.as_str().to_string());
        }
        e.node.for_each_child(&mut |c| mutated_locals(c, out));
    }
    let open_in = |v: &Expr, locals: &[(String, bool, &Expr)], shadowed: &HashSet<String>, mutated: &HashSet<String>, fed: &HashSet<String>| -> bool {
        match &*v.node {
            // Not `partial: true` (the action's own name, read per controller) or any other non-String literal.
            ExprNode::Lit { value: Literal::Str { .. } } => false,
            ExprNode::Lit { .. } => true,
            // A collection or a pool a controller assigns (`partial: @tags`), unless a request value reaches it.
            ExprNode::Ivar { name } => fed.contains(&format!("@{name}")),
            // A local is a known name only where each assignment of it is a literal.
            ExprNode::Var { name, .. } if shadowed.contains(name.as_str()) || mutated.contains(name.as_str()) => true,
            ExprNode::Var { name, .. } => {
                let mine: Vec<_> = locals.iter().filter(|(n, ivar, _)| !*ivar && n == name.as_str()).collect();
                mine.is_empty() || mine.iter().any(|(_, _, value)| !matches!(&*value.node, ExprNode::Lit { value: Literal::Str { .. } }))
            }
            ExprNode::StringInterp { parts } => !parts
                .iter()
                .any(|p| matches!(p, crate::expr::InterpPart::Text { value } if value.trim_matches('/').len() >= 2)),
            ExprNode::Send { method, .. } if method.as_str() == "to_partial_path" => false,
            _ => true,
        }
    };
    // A partial render's `layout:` is a partial too; not an action's or `render html:`'s layout, nor `layout: false`.
    fn options_open(entries: &[(Expr, Expr)], open: &dyn Fn(&Expr) -> bool) -> bool {
        let key = |k: &Expr, name: &str| matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == name);
        let partial = entries.iter().any(|(k, _)| key(k, "partial"));
        entries.iter().any(|(k, v)| {
            (key(k, "partial") && open(v))
                || (partial && key(k, "layout") && !matches!(&*v.node, ExprNode::Lit { value: Literal::Bool { .. } | Literal::Nil }) && open(v))
                || (partial && key(k, "spacer_template") && open(v))
        })
    }
    fn computed_partial(
        e: &Expr,
        open: &dyn Fn(&Expr) -> bool,
        request: &dyn Fn(&Expr) -> bool,
        locals: &[(String, bool, &Expr)],
    ) -> bool {
        let mut found = false;
        if let ExprNode::Send { method, args, .. } = &*e.node
            && matches!(method.as_str(), "render" | "render_to_string")
            && let Some(first) = args.first()
        {
            found = match &*first.node {
                ExprNode::Hash { entries, .. } => options_open(entries, open),
                // Options passed through a local (`opts = { partial: … }; render opts`).
                // Or a name built in place (`panel += "ost"`); not a local holding a record, which renders itself.
                ExprNode::Var { name, .. } => {
                    request(first)
                        || locals.iter().any(|(n, ivar, h)| {
                            !*ivar
                                && n == name.as_str()
                                && (matches!(&*h.node, ExprNode::Hash { entries, .. } if options_open(entries, open))
                                    || matches!(&*h.node, ExprNode::OpAssign { op: crate::expr::OpAssignOp::Add, value, .. }
                                        if matches!(&*value.node, ExprNode::Lit { value: Literal::Str { .. } } | ExprNode::StringInterp { .. })))
                        })
                }
                ExprNode::StringInterp { .. } => open(first),
                // Not an object the render asks to render itself (`render DS::Tabs.new(active_tab: params[:tab])`).
                ExprNode::Send { method, .. } if method.as_str() == "new" => false,
                // An object render (`render post.comments`) unless a request value reaches the argument.
                _ => request(first),
            };
        }
        e.node.for_each_child(&mut |c| found = found || computed_partial(c, open, request, locals));
        found
    }
    let body_computed = |b: &Expr| -> bool {
        let mut here = Vec::new();
        assignments(b, &mut here);
        let mut fed = fed_ivars.clone();
        request_fed(&here, &mut fed);
        let mut shadowed = HashSet::default();
        block_params(b, &mut shadowed);
        let mut mutated = HashSet::default();
        mutated_locals(b, &mut mutated);
        // An alias shares the String (`alias_name = panel; alias_name.concat(…)`): a mutation of either is of both.
        loop {
            let before = mutated.len();
            for (name, ivar, value) in &here {
                if let ExprNode::Var { name: other, .. } = &*value.node
                    && !*ivar
                    && (mutated.contains(name) || mutated.contains(other.as_str()))
                {
                    mutated.insert(name.clone());
                    mutated.insert(other.as_str().to_string());
                }
            }
            if mutated.len() == before {
                break;
            }
        }
        computed_partial(b, &|v: &Expr| open_in(v, &here, &shadowed, &mutated, &fed), &|v: &Expr| reads_request(v, &fed), &here)
    };
    let mut computed = false;
    crate::lower::for_each_hook_body_ref(app, &mut |b| computed = computed || body_computed(b));
    if computed || app.views.iter().any(|v| body_computed(&v.body)) {
        return Vec::new();
    }
    let mut names: HashSet<String> = HashSet::default();
    let mut fragments: Vec<String> = Vec::new();
    crate::lower::for_each_hook_body_ref(app, &mut |b| collect(b, &mut names, &mut fragments));
    // A view's bare name (`render "activity"`) is relative to its own directory: elsewhere `tab: "activity"` names
    // no partial of `accounts/show`.
    let mut view_names: HashSet<(String, String)> = HashSet::default();
    for v in &app.views {
        let dir = v.name.as_str().rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        let mut own: HashSet<String> = HashSet::default();
        collect(&v.body, &mut own, &mut fragments);
        for n in own {
            if n.contains('/') {
                names.insert(n);
            } else {
                view_names.insert((dir.clone(), n));
            }
        }
    }
    // A bare render target resolves through the rendering controller's lookup prefixes, not the view's own
    // directory (`shared/_card`'s `render "ghost"` may be `articles/_ghost`), so its leaf names any directory's.
    // A local read there names what its literal assignments hold (`panel = "ghost"; render partial: panel`).
    fn bare_render_targets(e: &Expr, locals: &[(String, bool, &Expr)], out: &mut HashSet<String>) {
        if let ExprNode::Send { method, args, .. } = &*e.node
            && matches!(method.as_str(), "render" | "render_to_string")
            && let Some(first) = args.first()
        {
            let mut take = |v: &Expr| {
                let values: Vec<&Expr> = match &*v.node {
                    ExprNode::Var { name, .. } => locals.iter().filter(|(n, ivar, _)| !*ivar && n == name.as_str()).map(|(_, _, value)| *value).collect(),
                    _ => vec![v],
                };
                for v in values {
                    if let ExprNode::Lit { value: Literal::Str { value } } = &*v.node
                        && !value.contains('/')
                    {
                        out.insert(value.clone());
                    }
                }
            };
            match &*first.node {
                ExprNode::Hash { entries, .. } => {
                    for (k, v) in entries {
                        if matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if matches!(value.as_str(), "partial" | "layout" | "spacer_template")) {
                            take(v);
                        }
                    }
                }
                _ => take(first),
            }
        }
        e.node.for_each_child(&mut |c| bare_render_targets(c, locals, out));
    }
    let mut bare_targets: HashSet<String> = HashSet::default();
    let mut bare_in = |b: &Expr, out: &mut HashSet<String>| {
        let mut here = Vec::new();
        assignments(b, &mut here);
        bare_render_targets(b, &here, out);
    };
    for v in &app.views {
        bare_in(&v.body, &mut bare_targets);
    }
    crate::lower::for_each_hook_body_ref(app, &mut |b| bare_in(b, &mut bare_targets));
    // Not models alone: an STI subclass ingested as a library class (`AssistantMessage < Message`), or any class
    // answering `to_partial_path`, is rendered by `render @messages` the same way.
    let model_partials: HashSet<String> = app
        .models
        .iter()
        .map(|m| &m.name)
        .chain(app.library_classes.iter().map(|c| &c.name))
        .map(|name| {
            let path = crate::naming::snake_case(&name.0.as_str().replace("::", "/"));
            let (dir, leaf) = path.rsplit_once('/').map(|(d, l)| (format!("{d}/"), l.to_string())).unwrap_or((String::new(), path.clone()));
            format!("{dir}{}/{leaf}", crate::naming::pluralize_snake(&leaf))
        })
        .collect();
    let gem_dir = |dir: &str| {
        let head = dir.split('/').next().unwrap_or(dir);
        app.gem_lock.as_ref().is_some_and(|lock| lock.has(head))
    };
    let mut out = Vec::new();
    for (i, view) in app.views.iter().enumerate() {
        let Some((dir, base)) = view.name.as_str().rsplit_once('/') else { continue };
        let Some(leaf) = base.strip_prefix('_') else { continue };
        // Not a source gem's view by its file either: `source_gem_shake` drops a marked one from the output.
        let gem_file = app.source_path(view.body.span.file).is_some_and(|p| p.contains("/.roundhouse/source_gems/") || p.starts_with(".roundhouse/source_gems/"));
        if dir.starts_with("layouts") || gem_dir(dir) || gem_file {
            continue;
        }
        let partial = format!("{dir}/{leaf}");
        let named = names.contains(leaf)
            || view_names.contains(&(dir.to_string(), leaf.to_string()))
            || bare_targets.contains(leaf)
            || names.contains(&partial)
            || names.iter().any(|n| n.ends_with(&format!("/{leaf}")))
            // `"ghost_#{kind}"` may name `_ghost_card`: any piece of an interpolation's text the name carries (one too
            // short to tell is an open name above).
            || fragments.iter().any(|f| {
                let piece = f.trim_matches('/');
                f.contains(leaf) || (piece.len() >= 2 && partial.contains(piece))
            })
            || model_partials.contains(&partial);
        if !named {
            out.push(i);
        }
    }
    out
}

/// Whether the ancestry ends at Rails' own base: a gem's controller
/// (Doorkeeper's `AuthorizationsController`) brings actions not read here.
fn rooted_in_rails(controller: &Controller, by_name: &HashMap<&str, &Controller>) -> bool {
    let mut current = controller;
    for _ in 0..32 {
        let Some(parent) = current.parent.as_ref() else { return false };
        let name = parent.0.as_str().trim_start_matches("::");
        // Not Rails' bases alone: `Rails::ApplicationController` is Rails' own subclass of `ActionController::Base` (view_component's preview controller's).
        if matches!(name, "ActionController::Base" | "ActionController::API" | "Rails::ApplicationController") {
            return true;
        }
        let Some(next) = by_name.get(name) else { return false };
        current = next;
    }
    false
}

/// The controller, its ancestors and every controller inheriting from it.
fn family<'a>(controller: &'a Controller, by_name: &HashMap<&str, &'a Controller>) -> Vec<&'a Controller> {
    let mut out = vec![controller];
    let mut current = controller;
    while let Some(parent) = current.parent.as_ref().and_then(|p| by_name.get(p.0.as_str())) {
        if out.iter().any(|c| c.name == parent.name) {
            break;
        }
        out.push(parent);
        current = parent;
    }
    let root = controller.name.0.as_str();
    for c in by_name.values() {
        let mut cur = *c;
        for _ in 0..32 {
            let Some(parent) = cur.parent.as_ref() else { break };
            if parent.0.as_str() == root {
                if !out.iter().any(|o| o.name == c.name) {
                    out.push(c);
                }
                break;
            }
            let Some(next) = by_name.get(parent.0.as_str()) else { break };
            cur = next;
        }
    }
    out
}

/// Whether `c` or a module it includes defines `name`; a module the app
/// does not define might, so it counts as defining it.
fn defines(c: &Controller, name: &str, modules: &HashMap<&str, HashSet<&str>>) -> bool {
    c.body.iter().any(|item| match item {
        ControllerBodyItem::Action { action, .. } => action.name.as_str() == name,
        ControllerBodyItem::Unknown { expr, .. } => included(expr).iter().any(|m| modules.get(m.as_str()).is_none_or(|defs| defs.contains(name))),
        _ => false,
    })
}

fn included(expr: &Expr) -> Vec<String> {
    let ExprNode::Send { recv: None, method, args, .. } = &*expr.node else { return Vec::new() };
    if method.as_str() != "include" {
        return Vec::new();
    }
    args.iter()
        .filter_map(|a| match &*a.node {
            ExprNode::Const { path } => Some(path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::")),
            _ => None,
        })
        .collect()
}

/// Whether a render in `c` may reach the template: one naming it, or one
/// whose target is computed.
fn renders(c: &Controller, action: &str, template: &str) -> bool {
    let mut found = false;
    for item in &c.body {
        let body = match item {
            ControllerBodyItem::Action { action, .. } => &action.body,
            ControllerBodyItem::Unknown { expr, .. } => expr,
            _ => continue,
        };
        visit_renders(body, &mut |lits, dynamic| {
            found |= dynamic || lits.iter().any(|l| l == action || names_template(l, template));
        });
    }
    found
}

fn names_template(lit: &str, template: &str) -> bool {
    lit.trim_start_matches('/') == template
}

fn render_literals_outside_controllers(app: &App, unrouted: &HashSet<String>) -> Vec<String> {
    // Not a concern's method spliced into a gem controller no route dispatches to (view_component's
    // `PreviewActions#index`): it runs only as that controller's action, which never runs.
    // Nor one any class that runs also takes: a routed controller's copy, a model's, or the module's own includer.
    let spans_of = |live: bool| -> HashSet<crate::span::Span> {
        app.controllers
            .iter()
            .filter(|c| unrouted.contains(c.name.0.as_str()) != live)
            .flat_map(|c| c.actions().map(|a| a.body.span))
            .filter(|span| !span.is_synthetic())
            .collect()
    };
    let unrouted_actions = spans_of(false);
    let live_actions = spans_of(true);
    let model_spans: HashSet<crate::span::Span> = app.models.iter().flat_map(|m| m.methods().map(|m| m.body.span)).collect();
    let included = |lc: &crate::dialect::LibraryClass| {
        app.library_classes.iter().any(|c| c.includes.iter().any(|i| i.0.as_str().trim_start_matches("::") == lc.name.0.as_str()))
    };
    let mut out = Vec::new();
    let mut collect = |e: &Expr| visit_renders(e, &mut |lits, _| out.extend(lits.iter().cloned()));
    for lc in &app.library_classes {
        let module_included = included(lc);
        let runs_nowhere = |span: &crate::span::Span| {
            unrouted_actions.contains(span) && !live_actions.contains(span) && !model_spans.contains(span) && !module_included
        };
        for m in lc.methods.iter().filter(|m| !runs_nowhere(&m.body.span)) {
            collect(&m.body);
        }
    }
    // A controller that runs may render another directory's template (`render "vendor/pages/detail"`).
    for c in app.controllers.iter().filter(|c| !unrouted.contains(c.name.0.as_str())) {
        for a in c.actions() {
            collect(&a.body);
        }
    }
    for v in &app.views {
        collect(&v.body);
    }
    out
}

/// Each render-like call under `e`: the String/Symbol literals among its
/// arguments, and whether its target is computed.
fn visit_renders(e: &Expr, f: &mut dyn FnMut(&[String], bool)) {
    e.node.for_each_child(&mut |c| visit_renders(c, f));
    let ExprNode::Send { method, args, .. } = &*e.node else { return };
    if !matches!(method.as_str(), "render" | "render_to_string" | "respond_with") {
        return;
    }
    let mut lits = Vec::new();
    let mut dynamic = method.as_str() == "respond_with";
    for (i, a) in args.iter().enumerate() {
        match &*a.node {
            ExprNode::Lit { value: Literal::Str { value } } => lits.push(value.clone()),
            ExprNode::Lit { value: Literal::Sym { value } } => lits.push(value.as_str().to_string()),
            ExprNode::Hash { entries, .. } => {
                for (k, v) in entries {
                    let key = match &*k.node {
                        ExprNode::Lit { value: Literal::Sym { value } } => value.as_str().to_string(),
                        _ => continue,
                    };
                    if !matches!(key.as_str(), "action" | "template" | "file") {
                        continue;
                    }
                    match &*v.node {
                        ExprNode::Lit { value: Literal::Str { value } } => lits.push(value.clone()),
                        ExprNode::Lit { value: Literal::Sym { value } } => lits.push(value.as_str().to_string()),
                        _ => dynamic = true,
                    }
                }
            }
            _ if i == 0 => dynamic = true,
            _ => {}
        }
    }
    f(&lits, dynamic);
}
