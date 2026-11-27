//! The layout a controller render is wrapped in, picked as Rails 8.1.4's
//! `ActionView::Layouts` picks it (`_write_layout_method`,
//! `_layout_for_option`).
//!
//! Each class's `_layout` runs its `layout` call when the call's
//! `only:` / `except:` admit the action, and the default lookup when they
//! do not: the template `layouts/<controller_path>` if there is one, else
//! the parent class's `_layout`, down to no layout below
//! `ApplicationController`. A class with no call of its own keeps its
//! parent's call, conditions included, and never looks for a template of
//! its own name. A String names the template, a Symbol is a method Rails
//! calls on each render (read here only when its body is a literal),
//! `false` is no layout and `nil` the default lookup. `render layout:`
//! takes a String, `true` (the default layout, which must exist) and
//! `false` / `nil`; Rails raises for a Symbol.
//!
//! Rails picks per request, so a render in a method that runs under
//! several controllers or actions can take several layouts; each is
//! kept with the runs that take it (`render_cases`).
//!
//! Analysis, view reachability, the ruby family's layout wrap and the
//! other targets' refusals all ask this one resolver, so what a page is
//! typed against, emitted in and refused for is the same layout.

use crate::det::{HashMap, HashSet};
use crate::diagnostic::Diagnostic;
use crate::dialect::{Action, Controller, ControllerBodyItem, LayoutCall, LayoutDecl, LayoutValue, View};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;
use crate::span::Span;

/// What a render is wrapped in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Layout {
    /// A layout template, by view name (`layouts/admin`).
    Template(Symbol),
    /// No layout.
    None,
    /// Not decided before the request, or Rails raises deciding it.
    Unsupported { span: Span, detail: String },
}

/// One layout a render takes, and the (controller, action) runs it takes it under.
#[derive(Clone, Debug)]
pub struct Case {
    pub layout: Layout,
    pub runs: Vec<(String, String)>,
}

/// A render whose layout counts: a template render, or one given `layout:`.
pub struct Site<'e> {
    pub class: &'e str,
    pub method: &'e str,
    /// The `layout:` option, when the render passes one.
    pub option: Option<&'e Expr>,
    pub span: Span,
}

struct Class<'a> {
    parent: Option<&'a str>,
    call: Option<&'a LayoutCall>,
    /// Every method of every opening, with whether it is public.
    methods: Vec<(&'a Action, bool)>,
    /// The methods its body runs other than as the routed action: filter
    /// targets, handlers a class-body call names (`rescue_from …, with:`),
    /// and methods its methods call.
    invoked: HashSet<&'a str>,
}

pub struct Layouts<'a> {
    classes: HashMap<&'a str, Class<'a>>,
    children: HashMap<&'a str, Vec<&'a str>>,
    /// The html layout templates. `None` before the templates are read
    /// (view paths), when any lookup may find one.
    templates: Option<HashSet<&'a str>>,
}

const RAILS_BASES: &[&str] = &["ActionController::Base", "ActionController::API", "ActionController::Metal"];

impl<'a> Layouts<'a> {
    pub fn new(controllers: &'a [Controller], views: &'a [View]) -> Self {
        let mut layouts = Self::without_templates(controllers);
        layouts.templates = Some(
            views
                .iter()
                .filter(|v| v.format.as_str() == "html" && v.name.as_str().starts_with("layouts/"))
                .map(|v| v.name.as_str())
                .collect(),
        );
        layouts
    }

    /// The resolver before the templates are read: every layout lookup
    /// may or may not find its template.
    pub fn without_templates(controllers: &'a [Controller]) -> Self {
        let mut classes: HashMap<&'a str, Class<'a>> = HashMap::default();
        for c in controllers {
            let class = classes
                .entry(c.name.0.as_str())
                .or_insert(Class { parent: None, call: None, methods: Vec::new(), invoked: HashSet::default() });
            if class.parent.is_none() {
                class.parent = c.parent.as_ref().map(|p| p.0.as_str().trim_start_matches("::"));
            }
            if let LayoutDecl::Declared(call) = &c.layout {
                class.call = Some(call);
            }
            let mut public = true;
            for item in &c.body {
                match item {
                    ControllerBodyItem::PrivateMarker { .. } => public = false,
                    ControllerBodyItem::Action { action, .. } => {
                        class.methods.push((action, public));
                        calls(&action.body, action.name.as_str(), &mut class.invoked);
                    }
                    ControllerBodyItem::Filter { filter, .. } => {
                        class.invoked.insert(filter.target.as_str());
                    }
                    ControllerBodyItem::Unknown { expr, .. } => handlers(expr, &mut class.invoked),
                    _ => {}
                }
            }
        }
        let mut children: HashMap<&'a str, Vec<&'a str>> = HashMap::default();
        let mut names: Vec<&'a str> = classes.keys().copied().collect();
        names.sort_unstable();
        for name in names {
            if let Some(parent) = classes[name].parent.filter(|p| classes.contains_key(p)) {
                children.entry(parent).or_default().push(name);
            }
        }
        Layouts { classes, children, templates: None }
    }

    pub fn is_controller(&self, class: &str) -> bool {
        self.classes.contains_key(class)
    }

    /// What an action's template renders in when the render names no
    /// layout (Rails' `_default_layout`).
    pub fn for_action(&self, class: &str, action: &str) -> Layout {
        self.layout_method(class, class, action, 0).swap_remove(0)
    }

    /// Every layout the action's template can render in while the
    /// templates are unknown (`without_templates`).
    pub fn possible_for_action(&self, class: &str, action: &str) -> Vec<Layout> {
        self.layout_method(class, class, action, 0)
    }

    /// What `render …, layout: <option>` renders in (Rails'
    /// `_layout_for_option`).
    pub fn for_option(&self, class: &str, action: &str, option: &Expr) -> Layout {
        let span = option.span;
        match &*option.node {
            ExprNode::Lit { value: Literal::Str { value } } => self.named(value, span),
            ExprNode::Lit { value: Literal::Bool { value: false } } | ExprNode::Lit { value: Literal::Nil } => Layout::None,
            ExprNode::Lit { value: Literal::Bool { value: true } } => match self.for_action(class, action) {
                Layout::None => unsupported(
                    span,
                    format!("`render layout: true` in `{class}#{action}`, which has no layout: Rails raises ArgumentError (no default layout)"),
                ),
                layout => layout,
            },
            ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "default" => self.for_action(class, action),
            ExprNode::Lit { value: Literal::Sym { value } } => unsupported(
                span,
                format!("`render layout: :{}`: Rails takes a String, true or false here and raises ArgumentError for a Symbol", value.as_str()),
            ),
            _ => unsupported(span, "`render layout:` given a computed value, which picks the layout on each request".to_string()),
        }
    }

    /// What a render in `method` of `class` renders in, for every
    /// controller and action that can run it: each layout it takes (a
    /// template or none) with the runs that take it, or the first one
    /// that cannot be picked. An action runs in its class and in each
    /// subclass that inherits it or reaches it through `super`; a method
    /// run as a callback, a handler or from another method (a public one
    /// too) also runs under every action of those classes.
    pub fn render_cases(&self, class: &str, method: &str, option: Option<&Expr>) -> Result<Vec<Case>, Layout> {
        let pick = |k: &str, action: &str| match option {
            Some(option) => self.for_option(k, action, option),
            None => self.for_action(k, action),
        };
        let action = self.defines(class, method).is_some_and(|public| public);
        let runners = self.runners(class, method);
        let invoked = !action || self.invoked_in(&runners, class, method);
        let mut cases: Vec<Case> = Vec::new();
        for &k in &runners {
            let mut actions = if action { vec![method] } else { Vec::new() };
            if invoked {
                actions.extend(self.actions(k).into_iter().filter(|a| *a != method));
            }
            for a in actions {
                let layout = pick(k, a);
                if matches!(layout, Layout::Unsupported { .. }) {
                    return Err(layout);
                }
                let run = (k.to_string(), a.to_string());
                match cases.iter_mut().find(|c| c.layout == layout) {
                    Some(case) => case.runs.push(run),
                    None => cases.push(Case { layout, runs: vec![run] }),
                }
            }
        }
        if cases.is_empty() {
            let layout = pick(class, method);
            if matches!(layout, Layout::Unsupported { .. }) {
                return Err(layout);
            }
            cases.push(Case { layout, runs: vec![(class.to_string(), method.to_string())] });
        }
        Ok(cases)
    }

    /// The layout templates the public action `action` of `class` renders
    /// in: its default layout's and those its own renders name.
    pub fn templates_of_action(&self, class: &str, action: &str) -> Vec<Symbol> {
        let mut out = Vec::new();
        let mut add = |layout: Layout| {
            if let Layout::Template(name) = layout
                && !out.contains(&name)
            {
                out.push(name);
            }
        };
        add(self.for_action(class, action));
        if let Some(body) = self.body_of(class, action) {
            template_renders(body, &mut |option, _| {
                if let Some(option) = option {
                    add(self.for_option(class, action, option));
                }
            });
        }
        out
    }

    /// The layout templates any public action of `class` renders in.
    pub fn templates_of_class(&self, class: &str) -> Vec<Symbol> {
        let mut out: Vec<Symbol> = Vec::new();
        for action in self.actions(class) {
            for name in self.templates_of_action(class, action) {
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        out
    }

    /// Whether `action` is a public action of `class`, its own or inherited.
    pub fn is_action(&self, class: &str, action: &str) -> bool {
        self.actions(class).contains(&action)
    }

    /// The render sites of every controller method.
    pub fn sites(&self, controllers: &'a [Controller], views: &[View]) -> Vec<Site<'a>> {
        let pages: HashSet<&str> = views.iter().filter(|v| v.format.as_str() == "html").map(|v| v.name.as_str()).collect();
        let mut out = Vec::new();
        for c in controllers {
            let class = c.name.0.as_str();
            let mut public = true;
            for item in &c.body {
                let action = match item {
                    ControllerBodyItem::PrivateMarker { .. } => {
                        public = false;
                        continue;
                    }
                    ControllerBodyItem::Action { action, .. } => action,
                    _ => continue,
                };
                let method = action.name.as_str();
                let before = out.len();
                template_renders(&action.body, &mut |option, span| out.push(Site { class, method, option, span }));
                let explicit = out.len() > before || renders_anything(&action.body);
                if public && !explicit {
                    let implicit = self
                        .runners(class, method)
                        .iter()
                        .any(|k| pages.contains(format!("{}/{method}", controller_path(k)).as_str()));
                    if implicit {
                        let span = if action.name_span.is_synthetic() { action.body.span } else { action.name_span };
                        out.push(Site { class, method, option: None, span });
                    }
                }
            }
        }
        out
    }

    /// `_layout` of class `k` on an instance of `instance`.
    fn layout_method(&self, k: &str, instance: &str, action: &str, depth: u32) -> Vec<Layout> {
        if depth > 64 {
            return vec![Layout::None];
        }
        // The conditions are a class attribute: the instance's nearest call's.
        let applies = self.call_of(instance).is_none_or(|c| c.applies_to(action));
        if !applies {
            return self.default_lookup(k, instance, action, depth);
        }
        let Some(call) = self.call_of(k) else {
            return self.default_lookup(k, instance, action, depth);
        };
        match &call.value {
            LayoutValue::Name { name } => vec![self.named(name.as_str(), call.span)],
            LayoutValue::None => vec![Layout::None],
            LayoutValue::Default => self.default_lookup(k, instance, action, depth),
            LayoutValue::Computed => vec![unsupported(
                call.span,
                "`layout` given a Proc or a computed value, which picks the layout on each render".to_string(),
            )],
            LayoutValue::Method { name } => match self.method_literal(instance, name.as_str()) {
                Some(Some(Literal::Str { value })) => vec![self.named(value, call.span)],
                Some(Some(Literal::Nil)) => self.default_lookup(k, instance, action, depth),
                Some(Some(Literal::Bool { value: false })) => vec![Layout::None],
                _ => vec![unsupported(
                    call.span,
                    format!(
                        "`layout :{0}` calls `{0}` on each render, and only a controller method whose body is a String literal, `nil` or `false` is read before the request",
                        name.as_str()
                    ),
                )],
            },
        }
    }

    /// `layouts/<controller_path>` if the template exists, else the parent's `_layout`.
    fn default_lookup(&self, k: &str, instance: &str, action: &str, depth: u32) -> Vec<Layout> {
        let implied = normalize(&controller_path(k));
        match self.exists(&implied) {
            Some(true) => vec![Layout::Template(Symbol::from(implied))],
            Some(false) => self.parent_layout(k, instance, action, depth),
            None => {
                let mut out = vec![Layout::Template(Symbol::from(implied))];
                for l in self.parent_layout(k, instance, action, depth) {
                    if !out.contains(&l) {
                        out.push(l);
                    }
                }
                out
            }
        }
    }

    fn parent_layout(&self, k: &str, instance: &str, action: &str, depth: u32) -> Vec<Layout> {
        match self.classes.get(k).and_then(|c| c.parent) {
            Some(parent) if self.classes.contains_key(parent) => self.layout_method(parent, instance, action, depth + 1),
            Some(parent) if RAILS_BASES.contains(&parent) => vec![Layout::None],
            // A parent the app does not define (a gem's controller): its
            // own lookup is not read, and the application layout stands in.
            _ => match self.exists("layouts/application") {
                Some(true) => vec![Layout::Template(Symbol::from("layouts/application"))],
                Some(false) => vec![Layout::None],
                None => vec![Layout::Template(Symbol::from("layouts/application")), Layout::None],
            },
        }
    }

    /// A layout named by the app; Rails raises MissingTemplate rendering in
    /// one that has no template.
    fn named(&self, name: &str, span: Span) -> Layout {
        let name = normalize(name);
        if self.exists(&name) == Some(false) {
            return unsupported(
                span,
                format!("`{name}` has no html template: Rails raises ActionView::MissingTemplate rendering in it"),
            );
        }
        Layout::Template(Symbol::from(name))
    }

    fn exists(&self, name: &str) -> Option<bool> {
        self.templates.as_ref().map(|t| t.contains(name))
    }

    /// The nearest `layout` call up `k`'s ancestry.
    fn call_of(&self, k: &str) -> Option<&'a LayoutCall> {
        let mut current = Some(k);
        for _ in 0..64 {
            let class = self.classes.get(current?)?;
            if class.call.is_some() {
                return class.call;
            }
            current = class.parent;
        }
        None
    }

    /// The literal body of the method `name` as `instance` resolves it:
    /// `None` when no app controller defines it, `Some(None)` when its
    /// body is not a literal.
    fn method_literal(&self, instance: &str, name: &str) -> Option<Option<&'a Literal>> {
        let mut current = Some(instance);
        for _ in 0..64 {
            let class = self.classes.get(current?)?;
            if let Some((action, _)) = class.methods.iter().rev().find(|(a, _)| a.name.as_str() == name) {
                if !action.params.fields.is_empty() {
                    return Some(None);
                }
                return Some(literal_body(&action.body));
            }
            current = class.parent;
        }
        None
    }

    /// The body of `method` as `class` resolves it.
    fn body_of(&self, class: &str, method: &str) -> Option<&'a Expr> {
        let mut current = Some(class);
        for _ in 0..64 {
            let class = self.classes.get(current?)?;
            if let Some((action, _)) = class.methods.iter().rev().find(|(a, _)| a.name.as_str() == method) {
                return Some(&action.body);
            }
            current = class.parent;
        }
        None
    }

    /// Whether `class` itself defines `method`, and if so whether publicly.
    fn defines(&self, class: &str, method: &str) -> Option<bool> {
        self.classes.get(class)?.methods.iter().rev().find(|(a, _)| a.name.as_str() == method).map(|(_, public)| *public)
    }

    /// `class` and the subclasses that run its `method`: those that do not
    /// define their own, and those whose own calls `super`.
    fn runners(&self, class: &str, method: &str) -> Vec<&str> {
        let Some((&class, _)) = self.classes.get_key_value(class) else { return Vec::new() };
        let mut out = vec![class];
        let mut queue = vec![class];
        while let Some(k) = queue.pop() {
            for &child in self.children.get(k).into_iter().flatten() {
                let reaches = match self.body_of_own(child, method) {
                    None => true,
                    Some(body) => calls_super(body),
                };
                if reaches && !out.contains(&child) {
                    out.push(child);
                    queue.push(child);
                }
            }
        }
        out
    }

    /// The body of `class`'s own `method`, if it defines one.
    fn body_of_own(&self, class: &str, method: &str) -> Option<&'a Expr> {
        self.classes.get(class)?.methods.iter().rev().find(|(a, _)| a.name.as_str() == method).map(|(a, _)| &a.body)
    }

    /// Whether `method` runs other than as the routed action in `class`,
    /// its ancestors or the `runners`.
    fn invoked_in(&self, runners: &[&str], class: &str, method: &str) -> bool {
        let mut current = self.classes.get(class).and_then(|c| c.parent);
        let mut ancestors: Vec<&str> = Vec::new();
        for _ in 0..64 {
            let Some(k) = current.filter(|k| self.classes.contains_key(k)) else { break };
            ancestors.push(k);
            current = self.classes[k].parent;
        }
        runners.iter().chain(&ancestors).any(|k| self.classes.get(k).is_some_and(|c| c.invoked.contains(method)))
    }

    /// The public actions of `k`, its own and those it inherits from app controllers.
    fn actions(&self, k: &str) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::default();
        let mut current = Some(k);
        for _ in 0..64 {
            let Some(class) = current.and_then(|c| self.classes.get(c)) else { break };
            for (action, public) in &class.methods {
                if seen.insert(action.name.as_str()) && *public {
                    out.push(action.name.as_str());
                }
            }
            current = class.parent;
        }
        out
    }
}

/// The diagnostics for every render whose layout this build cannot pick
/// as Rails does: one chosen per request by app code, or one Rails raises
/// for. A render whose layout differs between the controllers and actions
/// running it is each target's to report.
pub fn diagnostics(controllers: &[Controller], views: &[View]) -> Vec<Diagnostic> {
    let layouts = Layouts::new(controllers, views);
    let mut seen: HashSet<(Span, String)> = HashSet::default();
    let mut out = Vec::new();
    for site in layouts.sites(controllers, views) {
        if let Err(Layout::Unsupported { span, detail }) = layouts.render_cases(site.class, site.method, site.option) {
            if seen.insert((span, detail.clone())) {
                out.push(Diagnostic::unsupported(span, None, "controller layout", detail));
            }
        }
    }
    out
}

/// `Admin::ReportsController` → `admin/reports`, Rails' `controller_path`.
pub fn controller_path(class: &str) -> String {
    let stem = class.strip_suffix("Controller").unwrap_or(class);
    stem.split("::").map(crate::naming::snake_case).collect::<Vec<_>>().join("/")
}

/// Rails' `_normalize_layout`: a name not already under `layouts` gets the prefix.
fn normalize(name: &str) -> String {
    let under_layouts = name.match_indices("layouts").any(|(i, _)| {
        name[..i].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    });
    if under_layouts { name.to_string() } else { format!("layouts/{name}") }
}

fn unsupported(span: Span, detail: String) -> Layout {
    Layout::Unsupported { span, detail }
}

/// The methods `body` calls on self, other than `caller` itself.
fn calls<'e>(body: &'e Expr, caller: &str, out: &mut HashSet<&'e str>) {
    if let ExprNode::Send { recv, method, .. } = &*body.node
        && recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef))
        && method.as_str() != caller
    {
        out.insert(method.as_str());
    }
    body.node.for_each_child(&mut |c| calls(c, caller, out));
}

/// The handlers `expr` names by `with:` (`rescue_from Error, with: :handler`).
fn handlers<'e>(expr: &'e Expr, out: &mut HashSet<&'e str>) {
    if let ExprNode::Hash { entries, .. } = &*expr.node {
        for (k, v) in entries {
            if let (ExprNode::Lit { value: Literal::Sym { value: key } }, ExprNode::Lit { value: Literal::Sym { value } }) = (&*k.node, &*v.node)
                && key.as_str() == "with"
            {
                out.insert(value.as_str());
            }
        }
    }
    expr.node.for_each_child(&mut |c| handlers(c, out));
}

fn calls_super(body: &Expr) -> bool {
    let mut found = matches!(&*body.node, ExprNode::Super { .. });
    body.node.for_each_child(&mut |c| found = found || calls_super(c));
    found
}

fn literal_body(body: &Expr) -> Option<&Literal> {
    match &*body.node {
        ExprNode::Lit { value } => Some(value),
        ExprNode::Seq { exprs } if exprs.len() == 1 => literal_body(&exprs[0]),
        _ => None,
    }
}

/// Template renders in `body`, with their `layout:` option. Renders that
/// take no layout (`json:`, `partial:`, `plain:` and the like with no
/// `layout:`) are left out.
fn template_renders<'e>(body: &'e Expr, found: &mut dyn FnMut(Option<&'e Expr>, Span)) {
    body.node.for_each_child(&mut |c| template_renders(c, found));
    let ExprNode::Send { recv, method, args, .. } = &*body.node else { return };
    if method.as_str() != "render" || !recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef)) {
        return;
    }
    // `ingest::inertia`'s own page render names `application` for the
    // gem's layout setting, which is not modeled; it is not the app's render.
    if args.first().is_some_and(|a| matches!(&*a.node, ExprNode::Send { method, .. } if method.as_str() == "inertia_root_html")) {
        return;
    }
    let mut option = None;
    let mut keys: Vec<&str> = Vec::new();
    for arg in args {
        if let ExprNode::Hash { entries, .. } = &*arg.node {
            for (k, v) in entries {
                if let ExprNode::Lit { value: Literal::Sym { value } } = &*k.node {
                    keys.push(value.as_str());
                    if value.as_str() == "layout" {
                        option = Some(v);
                    }
                }
            }
        }
    }
    // A renderer (`json:`, `turbo_stream:`, …) writes its own body, never a layout.
    const TEMPLATE_KEYS: &[&str] =
        &["action", "template", "file", "layout", "status", "content_type", "formats", "locals", "location", "variants", "locale", "handlers", "assigns"];
    const BARE_KEYS: &[&str] = &["body", "plain", "html", "inline", "partial"];
    if keys.iter().any(|k| !TEMPLATE_KEYS.contains(k) && !BARE_KEYS.contains(k)) {
        return;
    }
    let bare = keys.iter().any(|k| BARE_KEYS.contains(k))
        // `render @post` / `render post_object` renders a partial.
        || args.first().is_some_and(|a| !matches!(&*a.node, ExprNode::Hash { .. } | ExprNode::Lit { value: Literal::Str { .. } | Literal::Sym { .. } }));
    if bare && option.is_none() {
        return;
    }
    found(option, body.span);
}

/// Whether `body` responds by itself: a `render`, `redirect_to`, `head`
/// or `send_*` anywhere in it.
fn renders_anything(body: &Expr) -> bool {
    let mut any = false;
    fn visit(e: &Expr, any: &mut bool) {
        if *any {
            return;
        }
        if let ExprNode::Send { recv: None, method, .. } = &*e.node
            && matches!(method.as_str(), "render" | "redirect_to" | "redirect_back" | "redirect_back_or_to" | "head" | "send_data" | "send_file")
        {
            *any = true;
            return;
        }
        e.node.for_each_child(&mut |c| visit(c, any));
    }
    visit(body, &mut any);
    any
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn a_name_under_layouts_keeps_its_path() {
        assert_eq!(normalize("special"), "layouts/special");
        assert_eq!(normalize("layouts/special"), "layouts/special");
        assert_eq!(normalize("admin/layouts/narrow"), "admin/layouts/narrow");
        assert_eq!(normalize("my_layouts"), "layouts/my_layouts");
    }
}
