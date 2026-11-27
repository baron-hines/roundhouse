//! A mailer's `helper :name` / `helper SomeHelper`. actionmailer 8.1.4
//! gives a mailer's templates only the helpers its class chain declares
//! (and the modules those include), each class's own default
//! `<Name>Helper` module, and Rails' own (`ActionMailer::MailHelper`,
//! the routes' url helpers): not even `ApplicationHelper` otherwise. A
//! template calling another app helper raises `ActionView::Template::Error`.
//!
//! Every app helper resolves in every template here, so a declaration
//! needs no lowering: [`apply`] takes the `helper` calls as modelled, and
//! warns where a mailer's own template calls an app helper its chain
//! doesn't make visible, which this build renders and Rails doesn't.
//! `helper :all` names `AllHelper` in a mailer (it is special only in a
//! controller), which raises when the class loads; it and a `helper do …
//! end` block, whose methods aren't emitted, stay reported.

use std::collections::BTreeSet;
use crate::det::HashMap;

use crate::app::App;
use crate::expr::{Expr, ExprNode, Literal};
use crate::span::Span;

/// The ruby family: consume each mailer's `helper` calls, and report the
/// app helpers its templates call that Rails wouldn't give them.
pub(crate) fn apply(app: &mut App, target: &str) -> Vec<crate::diagnostic::Diagnostic> {
    let mailers = crate::lower::mailer_class_side::mailer_names(app);
    if mailers.is_empty() {
        return Vec::new();
    }
    let parents: HashMap<String, String> = app
        .library_classes
        .iter()
        .filter_map(|lc| lc.parent.as_ref().map(|p| (lc.name.0.as_str().to_string(), p.0.as_str().to_string())))
        .collect();
    let mut includes: HashMap<String, Vec<String>> = app
        .library_classes
        .iter()
        .map(|lc| (lc.name.0.as_str().to_string(), lc.includes.iter().map(|i| i.0.as_str().to_string()).collect()))
        .collect();
    // A helper with no methods of its own (`module OuterHelper; include
    // InnerHelper; end`) isn't kept as a class: read its includes from
    // its source.
    for (module, included) in source_helper_includes(app) {
        let entry = includes.entry(module).or_default();
        for inc in included {
            if !entry.contains(&inc) {
                entry.push(inc);
            }
        }
    }
    let mut gaps: Vec<(Span, String, String)> = Vec::new();
    // Each mailer's declared helper modules, its own calls consumed.
    let mut declared: HashMap<String, BTreeSet<String>> = HashMap::default();
    for lc in app.library_classes.iter_mut() {
        if !mailers.contains(lc.name.0.as_str()) {
            continue;
        }
        let mut own = BTreeSet::new();
        lc.unknown_calls.retain(|call| {
            let ExprNode::Send { recv: None, method, args, block, .. } = &*call.node else { return true };
            if method.as_str() != "helper" {
                return true;
            }
            if block.is_some() {
                gaps.push((call.span, "mailer helper block".into(), "a `helper do … end` block defines template methods this build doesn't emit".into()));
                return false;
            }
            for arg in args {
                match module_name(arg) {
                    Some(name) if name == "AllHelper" => gaps.push((
                        call.span,
                        "mailer helper :all".into(),
                        "`helper :all` names `AllHelper` in a mailer, which Rails can't load (NameError)".into(),
                    )),
                    Some(name) => {
                        own.insert(name);
                    }
                    None => gaps.push((call.span, "mailer helper".into(), "a `helper` argument other than a name or a module".into())),
                }
            }
            false
        });
        declared.insert(lc.name.0.as_str().to_string(), own);
    }
    // What each mailer's templates see: its chain's declarations, each
    // class's own `<Name>Helper`, and the modules those include.
    let visible = |mailer: &str| -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut cur = Some(mailer.to_string());
        let mut guard = 0;
        while let Some(class) = cur {
            guard += 1;
            if guard > 64 || !mailers.contains(&class) {
                break;
            }
            out.extend(declared.get(&class).into_iter().flatten().cloned());
            out.insert(format!("{class}Helper"));
            cur = parents.get(&class).cloned();
        }
        let mut pending: Vec<String> = out.iter().cloned().collect();
        while let Some(module) = pending.pop() {
            for inner in includes.get(&module).into_iter().flatten() {
                if out.insert(inner.clone()) {
                    pending.push(inner.clone());
                }
            }
        }
        out
    };
    let app_helpers: BTreeSet<String> = app.helper_modules.iter().map(|m| m.0.as_str().to_string()).collect();
    let mut undeclared: Vec<(Span, String, String)> = Vec::new();
    for mailer in &mailers {
        let dir = view_dir(mailer);
        let seen = visible(mailer);
        for view in &app.views {
            let name = view.name.as_str();
            if name.rsplit_once('/').map(|(d, _)| d) != Some(dir.as_str()) {
                continue;
            }
            let mut reported: BTreeSet<String> = BTreeSet::new();
            calls(&view.body, &mut |method, span| {
                let Some(module) = app.helper_method_index.get(&crate::ident::Symbol::from(method)) else { return };
                let module = module.0.as_str();
                if app_helpers.contains(module) && !seen.contains(module) && reported.insert(method.to_string()) {
                    undeclared.push((
                        span,
                        format!("mailer template helper `{method}`"),
                        format!(
                            "`{name}` calls `{method}` from `{module}`, which `{mailer}` doesn't declare (`helper`): Rails raises ActionView::Template::Error; this build renders it"
                        ),
                    ));
                }
            });
        }
    }
    gaps.extend(undeclared);
    let mut out = Vec::new();
    for (span, construct, detail) in gaps {
        out.extend(super::app::validates_gap_diagnostics(app, target, vec![(span, construct)], &detail));
    }
    out
}

/// Each `app/**/helpers/**/*_helper.rb` module's `include`d constants, by
/// module name, from the source.
fn source_helper_includes(app: &App) -> Vec<(String, Vec<String>)> {
    fn walk(node: &ruby_prism::Node<'_>, scope: &mut Vec<String>, out: &mut Vec<(String, Vec<String>)>) {
        let name_of = |path: ruby_prism::Node<'_>| -> Option<String> {
            let loc = path.location();
            std::str::from_utf8(loc.as_slice()).ok().map(|s| s.trim_start_matches("::").to_string())
        };
        if let Some(module) = node.as_module_node() {
            let Some(name) = name_of(module.constant_path()) else { return };
            scope.push(name);
            let full = scope.join("::");
            let mut included = Vec::new();
            if let Some(body) = module.body() {
                for stmt in super::util::flatten_statements(body) {
                    if let Some(call) = stmt.as_call_node()
                        && call.receiver().is_none()
                        && call.name().as_slice() == b"include"
                    {
                        for arg in call.arguments().into_iter().flat_map(|a| a.arguments().iter().collect::<Vec<_>>()) {
                            if (arg.as_constant_read_node().is_some() || arg.as_constant_path_node().is_some())
                                && let Some(n) = name_of(arg)
                            {
                                included.push(n);
                            }
                        }
                    } else {
                        walk(&stmt, scope, out);
                    }
                }
            }
            out.push((full, included));
            scope.pop();
        } else if let Some(program) = node.as_program_node() {
            for stmt in program.statements().body().iter() {
                walk(&stmt, scope, out);
            }
        }
    }
    let mut out = Vec::new();
    for file in &app.sources {
        if !(file.path.contains("/helpers/") && file.path.ends_with("_helper.rb")) {
            continue;
        }
        let result = ruby_prism::parse(file.text.as_bytes());
        walk(&result.node(), &mut Vec::new(), &mut out);
    }
    out
}

/// `:accounts` / `"admin/accounts"` → `AccountsHelper` /
/// `Admin::AccountsHelper`; a module constant as written.
fn module_name(arg: &Expr) -> Option<String> {
    match &*arg.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(helper_for(value.as_str())),
        ExprNode::Lit { value: Literal::Str { value } } => Some(helper_for(value)),
        ExprNode::Const { path } => Some(path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::")),
        _ => None,
    }
}

fn helper_for(name: &str) -> String {
    format!("{}Helper", crate::naming::camelize_path(name))
}

/// `Admin::UserMailer` → `admin/user_mailer`, its templates' directory.
fn view_dir(class: &str) -> String {
    class.split("::").map(crate::naming::snake_case).collect::<Vec<_>>().join("/")
}

/// Each receiverless call in `expr`, with its span.
fn calls(expr: &Expr, f: &mut dyn FnMut(&str, Span)) {
    if let ExprNode::Send { recv: None, method, .. } = &*expr.node {
        f(method.as_str(), expr.span);
    }
    expr.node.for_each_child(&mut |c| calls(c, f));
}
