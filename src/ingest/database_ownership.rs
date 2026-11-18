//! Which database each model runs on, read from the app's own classes
//! (see [`crate::database`]). Never from configuration:
//! `config/database.yml` says which databases exist; the source says who
//! uses them.
//!
//! A model's database is decided by the nearest class in its ancestry
//! (itself first) that says anything about connections:
//!
//! * `connects_to database: { writing: :name, reading: :name }`, as a
//!   literal, unconditional statement of an abstract class body (Rails
//!   raises for a concrete one) → [`ModelDatabase::Named`];
//! * a class-side `connection_pool` / `lease_connection` / `connection`
//!   / `retrieve_connection` / `connection_specification_name` the app
//!   defines, on the class itself, through a concern's `class_methods`
//!   (spliced onto the includer by then), or by `extend`ing a module →
//!   [`ModelDatabase::AppDefined`]: which database a query reaches is
//!   app code. Rails consults every such method a class defines, so
//!   several are one routing, decided at the first;
//! * any other `connects_to` (computed, under a condition, `shards:`)
//!   or an `establish_connection` → [`ModelDatabase::Unresolved`].
//!
//! Otherwise the model runs on the primary. Every outcome the emitted
//! runtime does not carry becomes an [`App::database_refusals`] entry,
//! once per declaring site, counting the models it decides: a writing
//! database other than the primary (the runtime opens only the primary),
//! a name `config/database.yml` does not configure, and each of the
//! shapes above. A role or shard switch (`connected_to`,
//! `connecting_to`, `while_preventing_writes`) anywhere in the app is
//! refused at its call, except the `connected_to(role: :writing)` block
//! ingest already reads as its body (`ingest::expr`).
//!
//! One app is carried instead, on the Spinel target ([`admitted_routes`],
//! [`scoped_call`], [`restore_writing_scopes`]).

use crate::det::HashMap;

use crate::database::{Database, DatabaseRefusal, DatabaseRoutes, ModelDatabase};
use crate::dialect::{MethodReceiver, Model, ModelBodyItem};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol};
use crate::span::Span;
use crate::App;

/// Class-side methods through which Rails finds a class's connection.
/// An app that defines one routes its queries itself.
const CONNECTION_LOOKUPS: &[&str] = &[
    "connection_pool",
    "lease_connection",
    "connection",
    "with_connection",
    "retrieve_connection",
    "connection_specification_name",
];

/// Calls that switch the role or shard a block runs on.
const SWITCHES: &[&str] = &[
    "connected_to",
    "connected_to_many",
    "connecting_to",
    "while_preventing_writes",
    "prohibit_shard_swapping",
    // Not a switch: Rails' connection handler itself (its pools, by
    // connection name, role and shard), which only the multiple-database
    // runtime carries.
    "connection_handler",
];

/// What one class body itself says about its connection.
#[derive(Clone)]
enum Own {
    Named { writing: String, reading: Option<String>, span: Span },
    AppDefined { method: Symbol, defined_by: ClassId, span: Span },
    Unresolved { construct: &'static str, reason: String, span: Span },
    /// A literal `connects_to shards:`: each shard with its writing and
    /// reading configuration. Refused with `reason` unless
    /// [`admitted_routes`] takes the app.
    Shards { pools: Vec<(String, String, Option<String>)>, reason: String, span: Span },
}

fn on_self(recv: &Option<Expr>) -> bool {
    recv.as_ref().is_none_or(|r| matches!(*r.node, ExprNode::SelfRef))
}

fn sym(e: &Expr) -> Option<String> {
    match &*e.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(value.as_str().to_string()),
        _ => None,
    }
}

/// `{ writing: :a, reading: :b }` with symbol keys and values only.
fn roles(e: &Expr) -> Option<(Option<String>, Option<String>)> {
    let ExprNode::Hash { entries, .. } = &*e.node else { return None };
    let (mut writing, mut reading) = (None, None);
    for (k, v) in entries {
        let slot = match sym(k)?.as_str() {
            "writing" => &mut writing,
            "reading" => &mut reading,
            _ => return None,
        };
        if slot.is_some() {
            return None;
        }
        *slot = Some(sym(v)?);
    }
    Some((writing, reading))
}

/// A top-level `connects_to` statement, classified.
fn connects_to(model: &Model, args: &[Expr], block: &Option<Expr>, span: Span) -> Own {
    let class = model.name.0.as_str();
    let unresolved = |reason: String| Own::Unresolved { construct: "connects_to", reason, span };
    if block.is_some() {
        return unresolved(format!("`connects_to` on `{class}` passes a block"));
    }
    let [ExprNode::Hash { entries, kwargs: true }] = args.iter().map(|a| &*a.node).collect::<Vec<_>>()[..] else {
        return unresolved(format!(
            "`connects_to` on `{class}` is not a literal `database: {{ writing: :name, reading: :name }}`"
        ));
    };
    let mut database = None;
    for (k, v) in entries {
        match sym(k).as_deref() {
            Some("shards") => {
                let reason = format!("`connects_to shards:` on `{class}` declares horizontal shards; shard routing is not modeled");
                let pools = match &*v.node {
                    ExprNode::Hash { entries: shards, .. } if entries.len() == 1 && !shards.is_empty() => shards
                        .iter()
                        .map(|(shard, pool)| match (sym(shard), roles(pool)) {
                            (Some(shard), Some((Some(writing), reading))) => Some((shard, writing, reading)),
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>(),
                    _ => None,
                };
                return match pools {
                    Some(pools) if crate::lower::model_to_library::markers::is_abstract_class(model) => Own::Shards { pools, reason, span },
                    _ => unresolved(reason),
                };
            }
            Some("database") if database.is_none() => database = Some(v),
            _ => {
                return unresolved(format!(
                    "`connects_to` on `{class}` is not a literal `database: {{ writing: :name, reading: :name }}`"
                ));
            }
        }
    }
    let Some((Some(writing), reading)) = database.and_then(roles) else {
        return unresolved(format!(
            "`connects_to` on `{class}` is not a literal `database: {{ writing: :name, reading: :name }}`"
        ));
    };
    if !crate::lower::model_to_library::markers::is_abstract_class(model) {
        return Own::Unresolved {
            construct: "connects_to",
            reason: format!("`connects_to` on `{class}`, which is not an abstract class; Rails raises NotImplementedError"),
            span,
        };
    }
    Own::Named { writing, reading, span }
}

/// Whether a model-body statement is one this path decides on rather
/// than model lowering: a class-body `connects_to` or
/// `establish_connection`, which [`resolve`] reads as the model's
/// database or refuses. `model_to_library::report_unclaimed_unknowns`
/// skips these.
pub(crate) fn claims_model_statement(expr: &Expr) -> bool {
    let ExprNode::Send { recv, method, .. } = &*expr.node else { return false };
    on_self(recv) && matches!(method.as_str(), "connects_to" | "establish_connection")
}

/// Every call to one of `names` under `expr` (receiver ignored).
fn calls_named(expr: &Expr, names: &[&str], out: &mut Vec<(Span, String)>) {
    if let ExprNode::Send { method, .. } = &*expr.node {
        if names.contains(&method.as_str()) {
            out.push((expr.span, method.as_str().to_string()));
        }
    }
    expr.node.for_each_child(&mut |child| calls_named(child, names, out));
}

/// Every Active Record role or shard switch under `expr`: a [`SWITCHES`]
/// call on a class (`self`, a constant, or no receiver), and for the
/// `connected_to` family with the `role:` or `shard:` Rails requires of
/// it. A call on another object, or without those keywords, is the
/// app's own method of the same name.
fn switches_in(expr: &Expr, routes: Option<&DatabaseRoutes>, out: &mut Vec<(Span, String, String)>) {
    if let ExprNode::Send { recv, method, args, block, .. } = &*expr.node {
        let name = method.as_str();
        let on_class = recv.as_ref().is_none_or(|r| matches!(*r.node, ExprNode::SelfRef | ExprNode::Const { .. }));
        let keyed = || {
            args.iter().any(|arg| match &*arg.node {
                ExprNode::Hash { entries, .. } => entries.iter().any(|(k, _)| matches!(sym(k).as_deref(), Some("role" | "shard"))),
                _ => false,
            })
        };
        let class = recv.as_ref().and_then(|r| match &*r.node {
            ExprNode::Const { path } => Some(path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::")),
            _ => None,
        });
        // On a routed app's connection class the method is Rails', so
        // options that cannot be read (a splat, a variable) may name a
        // role or shard.
        let unread = || {
            let ours = routes.zip(class.as_deref()).is_some_and(|(routes, class)| class == "ActiveRecord::Base" || class == routes.owner.0.as_str());
            ours && !args.iter().all(|arg| matches!(&*arg.node, ExprNode::Hash { entries, .. } if entries.iter().all(|(k, _)| sym(k).is_some())))
        };
        let switch = match name {
            "connected_to" | "connected_to_many" | "connecting_to" => keyed() || unread(),
            _ => SWITCHES.contains(&name),
        };
        let detail = if !(on_class && switch) {
            // What a routed app's build cannot route: a default changed
            // or a connection registered while it runs.
            match (routes, class.as_deref()) {
                (Some(_), _) if on_class && matches!(name, "default_shard=" | "default_role=") => Some(format!(
                    "`{name}` changes the default a query runs on; the routed runtime reads the first declared shard and the writing role"
                )),
                (Some(_), Some(class)) if matches!(name, "connects_to" | "establish_connection") => Some(format!(
                    "`{class}.{name}` registers a connection while the app runs; only the class-body `connects_to` is routed"
                )),
                _ => None,
            }
        } else if name == "connection_handler" {
            Some(
                "`connection_handler` is Rails' connection handler (pools by connection name, role and shard); \
                 the emitted runtime has one connection, to the primary"
                    .to_string(),
            )
        } else {
            match routes {
                Some(routes) if scoped_call(routes, class.as_deref(), name, args, block) => None,
                Some(routes) if matches!(name, "connected_to" | "while_preventing_writes") => Some(format!(
                    "`{name}` here is not a scope the routed runtime carries: called on ActiveRecord::Base or `{}`, with a literal `role: :writing` or `:reading`, a symbol or computed `shard:`, a literal `prevent_writes:`, and a block of its own without parameters",
                    routes.owner.0.as_str()
                )),
                _ => Some(format!("`{name}` switches the role or shard a block runs on; role and shard switching is not modeled")),
            }
        };
        if let Some(detail) = detail {
            out.push((expr.span, name.to_string(), detail));
        }
    }
    expr.node.for_each_child(&mut |child| switches_in(child, routes, out));
}

/// Every switch in a body a routed app is not carried in (a view, a seed,
/// a test helper): a [`SWITCHES`] call on a class, whatever it passes,
/// and a writing block ingest read as its body.
fn switches_aside(expr: &Expr, out: &mut Vec<(Span, String, String)>) {
    let name = match &*expr.node {
        ExprNode::Send { recv, method, .. }
            if SWITCHES.contains(&method.as_str()) && recv.as_ref().is_none_or(|r| matches!(*r.node, ExprNode::SelfRef | ExprNode::Const { .. })) =>
        {
            Some(method.as_str())
        }
        _ if expr.decisions & crate::expr::ERASED_WRITING_SCOPE != 0 => Some("connected_to"),
        _ => None,
    };
    if let Some(name) = name {
        let detail = format!("`{name}` in a view, a seed or a test helper is not carried in an app that routes by role and shard; switch in a model, a controller or a library class");
        out.push((expr.span, name.to_string(), detail));
    }
    expr.node.for_each_child(&mut |child| switches_aside(child, out));
}

/// Whether a switch is one the routed runtime runs as Rails does:
/// `connected_to` or `while_preventing_writes` on ActiveRecord::Base or
/// the connection class (Rails raises for any other), with the default
/// roles as literals and its own block.
fn scoped_call(routes: &DatabaseRoutes, class: Option<&str>, name: &str, args: &[Expr], block: &Option<Expr>) -> bool {
    let literal = |e: &Expr| match &*e.node {
        ExprNode::Lit { value } => Some(value.clone()),
        _ => None,
    };
    let own_block = block.as_ref().is_some_and(|b| {
        matches!(&*b.node, ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, .. } if params.is_empty() && extra_params.is_empty())
    });
    if !own_block || !class.is_some_and(|c| c == "ActiveRecord::Base" || c == routes.owner.0.as_str()) {
        return false;
    }
    match (name, args) {
        ("while_preventing_writes", []) => true,
        ("while_preventing_writes", [enabled]) => matches!(literal(enabled), Some(Literal::Bool { .. })),
        ("connected_to", [options]) => match &*options.node {
            ExprNode::Hash { entries, .. } => entries.iter().all(|(key, value)| match (sym(key).as_deref(), literal(value)) {
                (Some("role"), Some(Literal::Sym { value })) => matches!(value.as_str(), "writing" | "reading"),
                (Some("shard"), value) => value.is_none_or(|value| matches!(value, Literal::Sym { .. })),
                (Some("prevent_writes"), Some(Literal::Bool { .. })) => true,
                _ => false,
            }),
            _ => false,
        },
        _ => false,
    }
}

/// Put back each `ActiveRecord::Base.connected_to(role: :writing)` call
/// ingest read as its block's body ([`crate::expr::ERASED_WRITING_SCOPE`]):
/// in an app that switches roles it lifts an outer reading block's guard.
fn restore_writing_scopes(expr: &mut Expr) {
    expr.node.for_each_child_mut(&mut |child| restore_writing_scopes(child));
    if expr.decisions & crate::expr::ERASED_WRITING_SCOPE == 0 {
        return;
    }
    expr.decisions &= !crate::expr::ERASED_WRITING_SCOPE;
    let span = expr.span;
    let symbol = |name: &str| Expr::new(span, ExprNode::Lit { value: Literal::Sym { value: Symbol::from(name) } });
    let body = std::mem::replace(expr, Expr::new(span, ExprNode::Lit { value: Literal::Nil }));
    let block = ExprNode::Lambda { params: Vec::new(), rest_param: None, extra_params: Vec::new(), block_param: None, body, block_style: Default::default() };
    let mut base = Expr::new(span, ExprNode::Const { path: vec![Symbol::from("ActiveRecord"), Symbol::from("Base")] });
    base.decisions |= crate::expr::GENERATED_CONST_REF;
    *expr = Expr::new(span, ExprNode::Send {
        recv: Some(base),
        method: Symbol::from("connected_to"),
        args: vec![Expr::new(span, ExprNode::Hash { entries: vec![(symbol("role"), symbol("writing"))], kwargs: true })],
        block: Some(Expr::new(span, block)),
        parenthesized: true,
    });
}

/// The routes of an app the Spinel SQLite runtime carries, with the
/// span of their declaration; `None` for any other app, whose refusals
/// stand. Rails' primary class alone declares `connects_to`, as a
/// literal `database:` or `shards:`, and every model descends from it,
/// so no query runs on the primary by default. Every configuration it
/// names is one the runtime can open ([`Database::routable`]) beside a
/// SQLite primary, and the app's schema is `db/schema.rb`. A `database:`
/// that names only the primary routes nothing.
fn admitted_routes(
    app: &App,
    own: &[Option<Own>],
    deciding: &[Option<usize>],
    environment: &[&Database],
    schema_rb: Option<&str>,
) -> Option<(DatabaseRoutes, Span)> {
    let schema_rb = schema_rb?;
    let mut declared = own.iter().enumerate().filter_map(|(j, own)| own.as_ref().map(|own| (j, own)));
    let (j, declaration) = declared.next()?;
    let owner = &app.models[j];
    let under_base = owner.parent.as_ref().is_some_and(|p| p.0.as_str().trim_start_matches("::") == "ActiveRecord::Base");
    // Rails' primary class, whose pools are ActiveRecord::Base's own: any
    // other connection class keeps pools of its own beside the primary's.
    let names_primary = |m: &Model| {
        m.body.iter().any(|item| matches!(item, ModelBodyItem::Unknown { expr, .. } if matches!(&*expr.node, ExprNode::Send { recv: None, method, .. } if method.as_str() == "primary_abstract_class")))
    };
    let primary = (owner.name.0.as_str() == "ApplicationRecord" || names_primary(owner)) && app.models.iter().filter(|m| names_primary(m)).count() <= usize::from(names_primary(owner));
    if declared.next().is_some() || !under_base || !primary || deciding.iter().any(|d| *d != Some(j)) {
        return None;
    }
    let (pools, span) = match declaration {
        Own::Named { writing, reading, span } => (vec![("default".to_string(), writing.clone(), reading.clone())], *span),
        Own::Shards { pools, span, .. } => (pools.clone(), *span),
        _ => return None,
    };
    let mut envs: Vec<&str> = environment.iter().map(|d| d.env.as_str()).collect();
    envs.dedup();
    let mut routes = Vec::new();
    let mut beyond_primary = false;
    for (shard, writing, reading) in &pools {
        for (role, name) in [("writing", Some(writing)), ("reading", reading.as_ref())] {
            let Some(name) = name else { continue };
            for env in &envs {
                let database = environment.iter().find(|d| d.env == *env && d.name == *name)?;
                if !database.routable(role == "writing", schema_rb) {
                    return None;
                }
                beyond_primary |= !database.primary;
            }
            routes.push((shard.clone(), role.to_string(), name.clone()));
        }
    }
    let sqlite_primary = |env: &&str| environment.iter().any(|d| d.env == *env && d.primary && d.adapter == crate::database::DatabaseAdapter::Sqlite3);
    if !beyond_primary || !envs.iter().all(sqlite_primary) {
        return None;
    }
    Some((DatabaseRoutes { owner: owner.name.clone(), default_shard: pools[0].0.clone(), schema_file: schema_rb.to_string(), routes }, span))
}


/// The first thing `model`'s own body says about its connection, plus
/// the refusals for anything else it says.
fn own_decision(
    model: &Model,
    modules: &HashMap<&str, &crate::dialect::LibraryClass>,
    spliced_from: Option<&HashMap<Symbol, ClassId>>,
) -> Option<Own> {
    let mut found: Vec<Own> = Vec::new();
    for item in &model.body {
        match item {
            ModelBodyItem::Unknown { expr, .. } => {
                if let ExprNode::Send { recv, method, args, block, .. } = &*expr.node {
                    if on_self(recv) {
                        match method.as_str() {
                            "connects_to" => {
                                found.push(connects_to(model, args, block, expr.span));
                                continue;
                            }
                            "establish_connection" => {
                                found.push(Own::Unresolved {
                                    construct: "establish_connection",
                                    reason: format!(
                                        "`establish_connection` on `{}` sets a connection config/database.yml does not describe; not modeled",
                                        model.name.0.as_str()
                                    ),
                                    span: expr.span,
                                });
                                continue;
                            }
                            "extend" => {
                                for arg in args {
                                    let ExprNode::Const { path } = &*arg.node else { continue };
                                    let name = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::");
                                    let Some(module) = modules.get(name.as_str()) else { continue };
                                    if let Some(m) = module.methods.iter().find(|m| {
                                        m.receiver == MethodReceiver::Instance && CONNECTION_LOOKUPS.contains(&m.name.as_str())
                                    }) {
                                        found.push(Own::AppDefined {
                                            method: m.name.clone(),
                                            defined_by: module.name.clone(),
                                            span: m.name_span,
                                        });
                                    }
                                }
                                continue;
                            }
                            _ => {}
                        }
                    }
                }
                // A connecting call nested in a statement (under `if`,
                // in a block) runs conditionally or at another time.
                let mut nested = Vec::new();
                calls_named(expr, &["connects_to", "establish_connection"], &mut nested);
                for (span, method) in nested {
                    found.push(Own::Unresolved {
                        construct: if method == "connects_to" { "connects_to" } else { "establish_connection" },
                        reason: format!(
                            "`{method}` on `{}` runs under a condition or inside another statement; only an unconditional class-body statement is modeled",
                            model.name.0.as_str()
                        ),
                        span,
                    });
                }
            }
            ModelBodyItem::Method { method, .. }
                if method.receiver == MethodReceiver::Class
                    && CONNECTION_LOOKUPS.contains(&method.name.as_str()) =>
            {
                let defined_by = spliced_from
                    .and_then(|m| m.get(&method.name))
                    .cloned()
                    .unwrap_or_else(|| model.name.clone());
                found.push(Own::AppDefined { method: method.name.clone(), defined_by, span: method.name_span });
            }
            _ => {}
        }
    }
    // Rails consults every lookup a class defines (`with_connection`
    // runs queries, `lease_connection` answers `connection`, and the
    // default `with_connection` checks out of `connection_pool`), so
    // several of them are one app-defined routing, decided at the first.
    let mut app_defined = false;
    found.retain(|own| match own {
        Own::AppDefined { .. } if app_defined => false,
        Own::AppDefined { .. } => {
            app_defined = true;
            true
        }
        _ => true,
    });
    // A class body that says two things is unresolved, at the second.
    match found.len() {
        0 => None,
        1 => found.pop(),
        _ => {
            let first = found.remove(0);
            let second_span = match &found[0] {
                Own::Named { span, .. } | Own::AppDefined { span, .. } | Own::Unresolved { span, .. } | Own::Shards { span, .. } => *span,
            };
            match first {
                Own::Unresolved { .. } => Some(first),
                _ => Some(Own::Unresolved {
                    construct: "connects_to",
                    reason: format!(
                        "`{}` declares its connection more than once; which declaration wins is not modeled",
                        model.name.0.as_str()
                    ),
                    span: second_span,
                }),
            }
        }
    }
}

/// A routed app whose configuration changes what a role or shard block
/// means keeps an error on every target: the runtime reads Rails'
/// defaults. Inspect each setter call independently.
pub(super) fn refuse_routing_settings<V: crate::vfs::Vfs + ?Sized>(app: &mut App, vfs: &V, dir: &std::path::Path) {
    const SETTINGS: &[(&str, &str)] = &[
        ("active_record.writing_role", "a configured role name"),
        ("active_record.reading_role", "a configured role name"),
        ("ActiveRecord.writing_role", "a configured role name"),
        ("ActiveRecord.reading_role", "a configured role name"),
        ("active_support.isolation_level", "an isolation level other than the default `:thread`"),
        ("active_record.database_selector", "the middleware that switches roles for a request"),
        ("active_record.database_resolver", "the middleware that switches roles for a request"),
        ("active_record.shard_selector", "the middleware that switches shards for a request"),
        ("active_record.shard_resolver", "the middleware that switches shards for a request"),
    ];
    let Some(span) = app.database_refusals.iter().find(|r| r.construct == "database_routes").map(|r| r.span) else { return };
    let mut files = vec![dir.join("config/application.rb")];
    for part in ["config/environments", "config/initializers"] {
        files.extend(super::app::read_rb_files(vfs, &dir.join(part)).unwrap_or_default());
    }
    for file in files {
        let Ok(text) = vfs.read_to_string(&file) else { continue };
        struct Assignments(Vec<(String, bool)>);
        impl<'pr> ruby_prism::Visit<'pr> for Assignments {
            fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
                let name = node.name();
                if let Some(name) = std::str::from_utf8(name.as_slice()).ok().and_then(|n| n.strip_suffix('='))
                    && let Some(receiver) = node.receiver()
                {
                    let owner = receiver.as_call_node().map(|call| super::util::constant_id_str(&call.name()).to_string())
                        .unwrap_or_else(|| String::from_utf8_lossy(receiver.location().as_slice()).into_owned());
                    let target = format!("{owner}.{name}");
                    let args: Vec<_> = node.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
                    let thread = matches!(args.as_slice(), [arg] if arg.as_symbol_node().is_some_and(|s| s.unescaped() == b"thread"));
                    self.0.push((target, thread));
                }
                ruby_prism::visit_call_node(self, node);
            }
        }
        let parsed = super::prism::parse_silent(text.as_bytes());
        let mut assignments = Assignments(Vec::new());
        ruby_prism::Visit::visit(&mut assignments, &parsed.node());
        for (target, thread) in assignments.0 {
            if thread && target.ends_with("active_support.isolation_level") { continue; }
            if let Some((key, what)) = SETTINGS.iter().find(|(key, _)| target.ends_with(*key)) {
                app.database_refusals.push(DatabaseRefusal {
                    span,
                    construct: "database_routes_setting".to_string(),
                    detail: format!("{} sets `{key}`: {what} is not carried by the routed runtime, which reads Rails' defaults", super::sources::relative_path(&file.display().to_string())),
                    warning: false,
                });
                return;
            }
        }
    }
}

/// Every body the switch walk reads.
fn each_body(app: &mut App, f: &mut dyn FnMut(&mut Expr)) {
    // A method's body and its parameters' defaults, which a call runs too.
    fn method(m: &mut crate::dialect::MethodDef, f: &mut dyn FnMut(&mut Expr)) {
        f(&mut m.body);
        m.params.iter_mut().filter_map(|p| p.default.as_mut()).for_each(f);
    }
    for model in &mut app.models {
        for item in &mut model.body {
            match item {
                ModelBodyItem::Method { method: m, .. } => method(m, f),
                ModelBodyItem::Unknown { expr, .. } => f(expr),
                ModelBodyItem::Scope { scope, .. } => f(&mut scope.body),
                _ => {}
            }
        }
    }
    for controller in &mut app.controllers {
        for item in &mut controller.body {
            match item {
                crate::dialect::ControllerBodyItem::Action { action, .. } => {
                    f(&mut action.body);
                    action.opt_params.iter_mut().for_each(|(_, default)| f(default));
                    action.kw_params.iter_mut().filter_map(|(_, default)| default.as_mut()).for_each(&mut *f);
                }
                crate::dialect::ControllerBodyItem::ClassMethod { method: m, .. } => method(m, f),
                _ => {}
            }
        }
    }
    for class in app.library_classes.iter_mut().chain(app.rails_application.iter_mut()) {
        class.methods.iter_mut().for_each(|m| method(m, f));
    }
    for module in &mut app.test_modules {
        module.tests.iter_mut().for_each(|t| f(&mut t.body));
        module.setup.iter_mut().for_each(&mut *f);
    }
}

/// The model a written superclass names, from `child`'s namespace out.
fn resolve_parent<'a>(child: &str, parent: &str, by_name: &HashMap<&'a str, usize>) -> Option<usize> {
    let parent = parent.trim_start_matches("::");
    let mut scope: Vec<&str> = child.split("::").collect();
    scope.pop();
    loop {
        let candidate =
            if scope.is_empty() { parent.to_string() } else { format!("{}::{parent}", scope.join("::")) };
        if let Some(i) = by_name.get(candidate.as_str()) {
            return Some(*i);
        }
        if scope.pop().is_none() {
            return None;
        }
    }
}

/// Record `app.model_databases` and the routing refusals. Runs after
/// every model, concern splice and library class exists, and after
/// `config/database.yml` is read into `app.databases`.
pub(super) fn resolve(app: &mut App, schema_rb: Option<&str>) {
    let modules: HashMap<&str, &crate::dialect::LibraryClass> =
        app.library_classes.iter().filter(|c| c.is_module).map(|c| (c.name.0.as_str(), c)).collect();
    let by_name: HashMap<&str, usize> =
        app.models.iter().enumerate().map(|(i, m)| (m.name.0.as_str(), i)).collect();
    let own: Vec<Option<Own>> = app
        .models
        .iter()
        .map(|m| own_decision(m, &modules, app.concern_spliced_class_methods.get(&m.name)))
        .collect();
    let parents: Vec<Option<usize>> = app
        .models
        .iter()
        .map(|m| m.parent.as_ref().and_then(|p| resolve_parent(m.name.0.as_str(), p.0.as_str(), &by_name)))
        .collect();

    // Nearest deciding class in each model's ancestry.
    let deciding: Vec<Option<usize>> = (0..app.models.len())
        .map(|i| {
            let mut at = Some(i);
            let mut steps = 0;
            while let Some(j) = at {
                if own[j].is_some() {
                    return Some(j);
                }
                at = parents[j];
                steps += 1;
                if steps > app.models.len() {
                    return None;
                }
            }
            None
        })
        .collect();

    // A name resolves against the environment the emitted tests run in,
    // `test`; an app whose file has no `test` environment, against every
    // environment it has.
    let environment: Vec<&crate::database::Database> = {
        let test: Vec<_> = app.databases.iter().filter(|d| d.env == "test").collect();
        if test.is_empty() { app.databases.iter().collect() } else { test }
    };
    let mut refusals: Vec<DatabaseRefusal> = Vec::new();
    // Refusal per deciding site (span), with the models it decides.
    let mut by_site: Vec<(Span, &'static str, String, Vec<String>)> = Vec::new();
    let mut note = |span: Span, construct: &'static str, detail: String, model: &str| {
        match by_site.iter_mut().find(|(s, c, d, _)| *s == span && *c == construct && *d == detail) {
            Some((_, _, _, models)) => models.push(model.to_string()),
            None => by_site.push((span, construct, detail, vec![model.to_string()])),
        }
    };
    let have_configurations = !app.databases.is_empty();
    let mut owners = indexmap::IndexMap::new();
    for (i, model) in app.models.iter().enumerate() {
        let name = model.name.0.as_str();
        let ownership = match deciding[i].and_then(|j| own[j].clone().map(|o| (j, o))) {
            None => ModelDatabase::Primary,
            Some((j, Own::Named { writing, reading, span })) => {
                let declared_by = app.models[j].name.clone();
                let class = declared_by.0.as_str();
                if !have_configurations {
                    note(
                        span,
                        "connects_to",
                        format!("`connects_to` on `{class}` names databases, and the app has no config/database.yml to resolve them"),
                        name,
                    );
                } else {
                    for role in std::iter::once(&writing).chain(reading.iter()) {
                        match environment.iter().find(|d| d.name == *role) {
                            None => note(
                                span,
                                "connects_to",
                                format!("`connects_to` on `{class}` names database `{role}`, which config/database.yml does not configure (or configures with values build-time resolution cannot read; see its warnings)"),
                                name,
                            ),
                            Some(db) if role == &writing && !db.primary => note(
                                span,
                                "connects_to",
                                format!(
                                    "models under `{class}` are owned by database `{role}` ({}): the emitted runtime opens only the primary database; a second database is not opened yet",
                                    db.adapter.name()
                                ),
                                name,
                            ),
                            Some(_) => {}
                        }
                    }
                }
                ModelDatabase::Named { writing, reading, declared_by, span }
            }
            Some((_, Own::AppDefined { method, defined_by, span })) => {
                // Named by its definition, not the deciding model, so a
                // concern's override is one refusal counting its includers.
                note(
                    span,
                    "connection_override",
                    format!(
                        "`{}` is app-defined (in `{}`): app-defined connection routing is not modeled, so the database the queries reach cannot be resolved",
                        method.as_str(),
                        defined_by.0.as_str()
                    ),
                    name,
                );
                ModelDatabase::AppDefined { method, defined_by, span }
            }
            Some((_, Own::Unresolved { construct, reason, span })) => {
                note(span, construct, reason.clone(), name);
                ModelDatabase::Unresolved { reason, span }
            }
            Some((_, Own::Shards { reason, span, .. })) => {
                note(span, "connects_to", reason.clone(), name);
                ModelDatabase::Unresolved { reason, span }
            }
        };
        owners.insert(model.name.clone(), ownership);
    }
    // Role and shard switches, wherever the app makes one.
    let admitted = admitted_routes(app, &own, &deciding, &environment, schema_rb);
    let mut switches = Vec::new();
    each_body(app, &mut |e| switches_in(e, None, &mut switches));
    // Read for a routed app only, where a switch in one is refused: a
    // view's block does not write its output, and a writing block read as
    // its body stays one there.
    let mut aside = Vec::new();
    let mut read_aside = |e: &Expr| switches_aside(e, &mut aside);
    app.views.iter().for_each(|view| read_aside(&view.body));
    app.seeds.iter().for_each(&mut read_aside);
    app.test_modules.iter().flat_map(|module| &module.helpers).for_each(|helper| read_aside(&helper.body));
    // The routing the Spinel SQLite runtime carries, when the app needs
    // it (a declaration or a switch refused until now), stands for those
    // refusals: one, which that target drops
    // (`session::drop_runtime_answered`).
    // Also for a switch only its routes make one: options that cannot be
    // read on the connection class, which is then refused by name below.
    let mut unread = Vec::new();
    if let Some((routes, _)) = &admitted {
        each_body(app, &mut |e| switches_in(e, Some(routes), &mut unread));
    }
    let routes = admitted.filter(|_| !by_site.is_empty() || !switches.is_empty() || !aside.is_empty() || !unread.is_empty());
    if let Some((routes, span)) = &routes {
        by_site.clear();
        switches.clear();
        refusals.push(DatabaseRefusal {
            span: *span,
            construct: "database_routes".to_string(),
            detail: format!(
                "`connects_to` on `{}` routes {} model(s) by role and shard across SQLite files: only the spinel target's runtime opens them, and no other database is substituted",
                routes.owner.0.as_str(),
                app.models.len()
            ),
            warning: false,
        });
        each_body(app, &mut |e| {
            restore_writing_scopes(e);
            switches_in(e, Some(routes), &mut switches);
        });
        switches.append(&mut aside);
    }
    let routes = routes.map(|(routes, _)| routes);
    for (span, construct, detail, models) in by_site {
        let shown: Vec<&str> = models.iter().take(4).map(String::as_str).collect();
        let more = if models.len() > 4 { format!(", and {} more", models.len() - 4) } else { String::new() };
        refusals.push(DatabaseRefusal {
            span,
            construct: construct.to_string(),
            detail: format!("{detail} ({} model(s): {}{more})", models.len(), shown.join(", ")),
            warning: false,
        });
    }

    switches.sort_by_key(|(s, _, _)| (s.file.0, s.start));
    switches.dedup_by_key(|(s, _, _)| *s);
    for (span, method, detail) in switches {
        refusals.push(DatabaseRefusal { span, construct: method, detail, warning: false });
    }

    let any_routing = owners.values().any(|o| !matches!(o, ModelDatabase::Primary));
    if any_routing {
        app.model_databases = owners;
    }
    app.database_routes = routes;
    app.database_refusals.extend(refusals);
}
