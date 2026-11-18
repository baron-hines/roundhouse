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
    // A name the request decides (`render partial: params[:panel]`, `render params[:panel].to_s`, a `partial:`
    // variable, an interpolation with next to no text) may be any partial's.
    fn reads_params(e: &Expr) -> bool {
        let mut found = matches!(&*e.node, ExprNode::Send { recv: None, method, args, .. } if method.as_str() == "params" && args.is_empty());
        e.node.for_each_child(&mut |c| found = found || reads_params(c));
        found
    }
    // The variables a statement assigns a request-read value (`@panel = params[:panel].to_s`): an ivar from any body
    // (a controller's reaches its views), a local from the body that reads it.
    fn params_fed(e: &Expr, ivars: &mut HashSet<String>, locals: &mut HashSet<String>) {
        if let ExprNode::Assign { target, value } = &*e.node
            && reads_params(value)
        {
            match target {
                crate::expr::LValue::Ivar { name } => {
                    ivars.insert(name.as_str().to_string());
                }
                crate::expr::LValue::Var { name, .. } => {
                    locals.insert(name.as_str().to_string());
                }
                _ => {}
            }
        }
        e.node.for_each_child(&mut |c| params_fed(c, ivars, locals));
    }
    let mut fed_ivars: HashSet<String> = HashSet::default();
    let mut ignored: HashSet<String> = HashSet::default();
    crate::lower::for_each_hook_body_ref(app, &mut |b| params_fed(b, &mut fed_ivars, &mut ignored));
    for v in &app.views {
        params_fed(&v.body, &mut fed_ivars, &mut ignored);
    }
    let open_in = |v: &Expr, fed_locals: &HashSet<String>| -> bool {
        match &*v.node {
            ExprNode::Lit { .. } => false,
            // A collection or a pool a controller assigns (`partial: @tags`), unless a request value was assigned it.
            ExprNode::Ivar { name } => fed_ivars.contains(name.as_str()),
            ExprNode::Var { name, .. } => fed_locals.contains(name.as_str()),
            ExprNode::StringInterp { parts } => {
                parts.iter().map(|p| match p { crate::expr::InterpPart::Text { value } => value.trim_matches('/').len(), _ => 0 }).sum::<usize>() < 2
            }
            ExprNode::Send { method, .. } if method.as_str() == "to_partial_path" => false,
            _ => true,
        }
    };
    let body_computed = |b: &Expr| -> bool {
        let mut locals: HashSet<String> = HashSet::default();
        let mut ivars: HashSet<String> = HashSet::default();
        params_fed(b, &mut ivars, &mut locals);
        computed_partial(b, &|v: &Expr| open_in(v, &locals))
    };
    fn computed_partial(e: &Expr, open_name: &dyn Fn(&Expr) -> bool) -> bool {
        let mut found = false;
        if let ExprNode::Send { method, args, .. } = &*e.node
            && matches!(method.as_str(), "render" | "render_to_string")
            && let Some(first) = args.first()
        {
            found = match &*first.node {
                ExprNode::Hash { entries, .. } => entries.iter().any(|(k, v)| {
                    matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "partial") && open_name(v)
                }),
                ExprNode::StringInterp { .. } => open_name(first),
                // Not an object the render asks to render itself (`render DS::Tabs.new(active_tab: params[:tab])`).
                ExprNode::Send { method, .. } if method.as_str() == "new" => false,
                ExprNode::Var { name, .. } | ExprNode::Ivar { name } => open_name(first) && !name.as_str().is_empty(),
                _ => reads_params(first),
            };
        }
        e.node.for_each_child(&mut |c| found = found || computed_partial(c, open_name));
        found
    }
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
        if dir.starts_with("layouts") || gem_dir(dir) {
            continue;
        }
        let partial = format!("{dir}/{leaf}");
        let named = names.contains(leaf)
            || view_names.contains(&(dir.to_string(), leaf.to_string()))
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
