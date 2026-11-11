//! A mailer's `before_action` / `after_action` / `around_action`.
//! actionmailer 8.1.4 runs them as one chain, in declaration order and
//! parent class first: a before callback runs and then the rest, an
//! around callback runs with the rest as its block, and an after callback
//! runs the rest and then itself, so afters run in reverse. The action
//! ends the chain. `only:` / `except:` pick actions by name, `if:` /
//! `unless:` take a Symbol or a lambda, and `with_options` merges its
//! options into the calls in its block. Nothing halts the chain
//! (`throw :abort` isn't caught).
//!
//! On the ruby family ([`expand`]) each public action of a mailer with a
//! chain keeps its name and gains the chain, resolved for that action,
//! as source ingested like the class's own:
//!
//! ```ruby
//! def hello(a)
//!   return __mailer_action_hello(a) unless __start_action("hello")
//!   load_params
//!   wrap do
//!     __mailer_action_hello(a)   # the action's own body, now private
//!     after_one
//!   end
//!   message
//! end
//! ```
//!
//! A block callback and a lambda condition become private methods of
//! the class that declares them. The chain runs when the action is
//! called (`Mailer.hello(...)`), not when the mail is delivered: the
//! runtime builds the mail eagerly, so a callback's errors and side
//! effects happen at that call rather than at `deliver_now` or
//! `.message`.
//!
//! A chain it doesn't take (`skip_before_action`, a non-literal option,
//! a callback method the class doesn't define, an action with keyword or
//! block parameters, a name the generated methods would take) keeps the
//! class's calls reported, and its subclasses' with it. Other targets
//! report every callback ([`refuse`]).

use std::collections::BTreeSet;
use crate::det::HashMap;

use crate::app::App;
use crate::dialect::{LibraryClass, MethodReceiver, MethodVisibility};
use crate::expr::{Expr, ExprNode, Literal};
use crate::span::Span;

const ACTION_PREFIX: &str = "__mailer_action_";
const CALLBACK_PREFIX: &str = "__mailer_callback_";
const CONDITION_PREFIX: &str = "__mailer_condition_";

/// The action a mailer method answers for: its own name, or the action
/// whose body it holds under the chain.
pub(crate) fn action_of(name: &str) -> &str {
    name.strip_prefix(ACTION_PREFIX).unwrap_or(name)
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Before,
    After,
    Around,
}

#[derive(Clone)]
enum Call {
    /// A method of the mailer, by name.
    Method(String),
    /// A block or lambda body, generated as a private method.
    Body(String, Expr),
}

#[derive(Clone)]
struct Callback {
    kind: Kind,
    /// A delivery callback (`before_deliver`, …), run by `deliver_now`
    /// rather than with the action.
    deliver: bool,
    target: Call,
    only: Option<Vec<String>>,
    except: Option<Vec<String>>,
    ifs: Vec<Call>,
    unlesses: Vec<Call>,
}

impl Callback {
    fn applies_to(&self, action: &str) -> bool {
        self.only.as_ref().is_none_or(|names| names.iter().any(|n| n == action))
            && !self.except.as_ref().is_some_and(|names| names.iter().any(|n| n == action))
    }
}

/// One class's callbacks, the unknown calls they came from, and the
/// generated bodies it declares.
struct Parsed {
    callbacks: Vec<Callback>,
    handlers: Vec<Handler>,
    consumed: Vec<usize>,
}

/// One `rescue_from` handler: the classes it takes, written as given, and
/// what it calls, with the exception when it takes one.
#[derive(Clone)]
struct Handler {
    classes: Vec<String>,
    call: Call,
    /// For a block handler, its parameter (the generated method's).
    param: Option<String>,
}

const RESCUE_PREFIX: &str = "__mailer_rescue_";

/// A `rescue_from Klass, …, with: :name` or `rescue_from(Klass) { |e| … }`.
fn parse_rescue(call: &Expr, counter: &mut usize) -> Result<Handler, String> {
    let ExprNode::Send { args, block, .. } = &*call.node else { unreachable!() };
    let mut classes = Vec::new();
    let mut with = None;
    for arg in args {
        match &*arg.node {
            ExprNode::Const { path } => classes.push(path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("::")),
            ExprNode::Hash { entries, .. } => {
                for (k, v) in entries {
                    match (&*k.node, &*v.node) {
                        (ExprNode::Lit { value: Literal::Sym { value: key } }, ExprNode::Lit { value: Literal::Sym { value } })
                            if key.as_str() == "with" =>
                        {
                            with = Some(value.as_str().to_string())
                        }
                        _ => return Err("a `rescue_from` option other than `with:` a Symbol".into()),
                    }
                }
            }
            _ => return Err("a `rescue_from` class that isn't a constant".into()),
        }
    }
    if classes.is_empty() {
        return Err("a `rescue_from` naming no class".into());
    }
    match (with, block) {
        (Some(name), None) => Ok(Handler { classes, call: Call::Method(name), param: None }),
        (None, Some(block)) => {
            let ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, body, .. } = &*block.node
            else {
                return Err("a `rescue_from` block with other than one plain parameter".into());
            };
            if params.len() > 1 || !extra_params.is_empty() {
                return Err("a `rescue_from` block with other than one plain parameter".into());
            }
            *counter += 1;
            let name = format!("{RESCUE_PREFIX}{counter}");
            // A block that takes no exception is still given one, unused.
            let param = params.first().map(|p| p.as_str().to_string()).unwrap_or_else(|| "__exception".into());
            Ok(Handler { classes, call: Call::Body(name, body.clone()), param: Some(param) })
        }
        _ => Err("a `rescue_from` with both or neither of a block and `with:`".into()),
    }
}

fn is_rescue(call: &Expr) -> bool {
    matches!(&*call.node, ExprNode::Send { recv: None, method, .. } if method.as_str() == "rescue_from")
}

/// The callback's kind, and whether it is a delivery callback.
fn kind_of(method: &str) -> Option<(Kind, bool)> {
    match method {
        "before_action" | "append_before_action" => Some((Kind::Before, false)),
        "after_action" | "append_after_action" => Some((Kind::After, false)),
        "around_action" | "append_around_action" => Some((Kind::Around, false)),
        "before_deliver" => Some((Kind::Before, true)),
        "after_deliver" => Some((Kind::After, true)),
        "around_deliver" => Some((Kind::Around, true)),
        _ => None,
    }
}

fn is_callback_macro(method: &str) -> bool {
    kind_of(method).is_some()
        || matches!(
            method,
            "skip_before_action"
                | "skip_after_action"
                | "skip_around_action"
                | "prepend_before_action"
                | "prepend_after_action"
                | "prepend_around_action"
                | "append_before_action"
                | "append_after_action"
                | "append_around_action"
        )
}

/// The calls in a `with_options` block, each with whether the block's
/// options merge into it, or None when it isn't one. In the yielded form
/// (`do |o| o.before_action :x end`) a call on the block's parameter is
/// the class's own with the options merged, as the option merger forwards
/// it; a receiverless call there is the class's own as written, without
/// them. Without a parameter, every call takes the options.
fn with_options_calls(call: &Expr) -> Option<(&[(Expr, Expr)], Vec<(Expr, bool)>)> {
    let ExprNode::Send { recv: None, method, args, block: Some(block), .. } = &*call.node else { return None };
    if method.as_str() != "with_options" {
        return None;
    }
    let [hash] = args.as_slice() else { return None };
    let ExprNode::Hash { entries, .. } = &*hash.node else { return None };
    let ExprNode::Lambda { params, body, .. } = &*block.node else { return None };
    let param = match params.as_slice() {
        [] => None,
        [param] => Some(param),
        _ => return None,
    };
    let stmts: Vec<&Expr> = match &*body.node {
        ExprNode::Seq { exprs } => exprs.iter().collect(),
        _ => vec![body],
    };
    let stmts = stmts
        .into_iter()
        .map(|stmt| {
            let mut stmt = stmt.clone();
            let mut merged = param.is_none();
            if let (Some(param), ExprNode::Send { recv, .. }) = (param, &mut *stmt.node)
                && recv.as_ref().is_some_and(|r| matches!(&*r.node, ExprNode::Var { name, .. } if name == param))
            {
                *recv = None;
                merged = true;
            }
            (stmt, merged)
        })
        .collect();
    Some((entries, stmts))
}

fn callback_method(call: &Expr) -> Option<&str> {
    match &*call.node {
        ExprNode::Send { recv: None, method, .. } if is_callback_macro(method.as_str()) => Some(method.as_str()),
        _ => None,
    }
}

/// Does this unknown call declare (or merge into) a callback?
fn declares_callbacks(call: &Expr) -> bool {
    callback_method(call).is_some()
        || is_rescue(call)
        || with_options_calls(call).is_some_and(|(_, stmts)| stmts.iter().any(|(s, _)| callback_method(s).is_some()))
}

fn names(value: &Expr) -> Option<Vec<String>> {
    let one = |e: &Expr| match &*e.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(value.as_str().to_string()),
        ExprNode::Lit { value: Literal::Str { value } } => Some(value.to_string()),
        _ => None,
    };
    match &*value.node {
        ExprNode::Array { elements, .. } => elements.iter().map(one).collect(),
        _ => one(value).map(|n| vec![n]),
    }
}

fn condition(value: &Expr, counter: &mut usize) -> Result<Call, String> {
    match &*value.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Ok(Call::Method(value.as_str().to_string())),
        ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, body, .. }
            if params.is_empty() && extra_params.is_empty() =>
        {
            *counter += 1;
            Ok(Call::Body(format!("{CONDITION_PREFIX}{counter}"), body.clone()))
        }
        _ => Err("a condition other than a Symbol or a lambda without parameters".into()),
    }
}

/// One `before_action` (…) call, with a `with_options` block's options
/// under its own.
fn parse_call(call: &Expr, outer: &[(Expr, Expr)], counter: &mut usize) -> Result<Vec<Callback>, String> {
    let ExprNode::Send { method, args, block, .. } = &*call.node else { unreachable!() };
    let Some((kind, deliver)) = kind_of(method.as_str()) else {
        return Err(format!("`{}` is not lowered", method.as_str()));
    };
    let mut targets: Vec<Call> = Vec::new();
    let mut options: Vec<(Expr, Expr)> = outer.to_vec();
    for arg in args {
        match &*arg.node {
            ExprNode::Lit { value: Literal::Sym { value } } => targets.push(Call::Method(value.as_str().to_string())),
            ExprNode::Hash { entries, .. } => options.extend(entries.iter().cloned()),
            _ => return Err("a callback other than a Symbol or a block".into()),
        }
    }
    if let Some(block) = block {
        let ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, body, .. } = &*block.node
        else {
            return Err("a block callback with parameters".into());
        };
        if !params.is_empty() || !extra_params.is_empty() || !targets.is_empty() || kind == Kind::Around {
            return Err("a block callback with parameters, or beside named callbacks".into());
        }
        *counter += 1;
        targets.push(Call::Body(format!("{CALLBACK_PREFIX}{counter}"), body.clone()));
    }
    if targets.is_empty() {
        return Err("a callback call naming no callback".into());
    }
    let (mut only, mut except, mut ifs, mut unlesses) = (None, None, Vec::new(), Vec::new());
    // A later key wins, as a Hash merge does.
    for (key, value) in &options {
        let ExprNode::Lit { value: Literal::Sym { value: key } } = &*key.node else {
            return Err("a non-Symbol option key".into());
        };
        match key.as_str() {
            "only" => only = Some(names(value).ok_or("an `only:` other than literal names")?),
            "except" => except = Some(names(value).ok_or("an `except:` other than literal names")?),
            "if" => ifs = vec![condition(value, counter)?],
            "unless" => unlesses = vec![condition(value, counter)?],
            other => return Err(format!("the `{other}:` option")),
        }
    }
    // Rails' delivery callbacks are plain ActiveSupport callbacks: no
    // `only:` / `except:`.
    if deliver && (only.is_some() || except.is_some()) {
        return Err("`only:` / `except:` on a delivery callback".into());
    }
    Ok(targets
        .into_iter()
        .map(|target| Callback { kind, deliver, target, only: only.clone(), except: except.clone(), ifs: ifs.clone(), unlesses: unlesses.clone() })
        .collect())
}

fn parse_class(lc: &LibraryClass, counter: &mut usize) -> Result<Parsed, String> {
    let mut parsed = Parsed { callbacks: Vec::new(), handlers: Vec::new(), consumed: Vec::new() };
    for (i, call) in lc.unknown_calls.iter().enumerate() {
        if is_rescue(call) {
            parsed.handlers.push(parse_rescue(call, counter)?);
            parsed.consumed.push(i);
        } else if callback_method(call).is_some() {
            parsed.callbacks.extend(parse_call(call, &[], counter)?);
            parsed.consumed.push(i);
        } else if let Some((outer, stmts)) = with_options_calls(call) {
            if !stmts.iter().any(|(s, _)| callback_method(s).is_some()) {
                continue;
            }
            if !stmts.iter().all(|(s, _)| callback_method(s).is_some()) {
                return Err("a `with_options` block with callbacks and other calls".into());
            }
            for (stmt, merged) in &stmts {
                parsed.callbacks.extend(parse_call(stmt, if *merged { outer } else { &[] }, counter)?);
            }
            parsed.consumed.push(i);
        }
    }
    Ok(parsed)
}

/// The ruby family: each mailer's actions gain their chains. Returns
/// the warnings for the chains it doesn't take.
pub(crate) fn expand(app: &mut App, target: &str) -> Vec<crate::diagnostic::Diagnostic> {
    let mailers = crate::lower::mailer_class_side::mailer_names(app);
    if mailers.is_empty() {
        return Vec::new();
    }
    let parents: HashMap<String, String> = app
        .library_classes
        .iter()
        .filter_map(|lc| lc.parent.as_ref().map(|p| (lc.name.0.as_str().to_string(), p.0.as_str().to_string())))
        .collect();
    // Mailers nearest the base first, so a parent is decided before its
    // subclasses.
    let depth = |name: &str| {
        let mut n = 0;
        let mut cur = name.to_string();
        while let Some(p) = parents.get(&cur).filter(|p| mailers.contains(*p)) {
            n += 1;
            cur = p.clone();
            if n > 64 {
                break;
            }
        }
        n
    };
    let mut order: Vec<usize> =
        (0..app.library_classes.len()).filter(|&i| mailers.contains(app.library_classes[i].name.0.as_str())).collect();
    order.sort_by_key(|&i| depth(app.library_classes[i].name.0.as_str()));

    let mut counter = 0usize;
    // Actions given a chain, by class: a callback naming one calls its own
    // body, as Rails calls the method, not the chain again.
    let mut wrapped: HashMap<String, BTreeSet<String>> = HashMap::default();
    // Every name the generated chains took.
    let mut generated: BTreeSet<String> = BTreeSet::new();
    let mut chains: HashMap<String, (Vec<Callback>, Vec<Handler>)> = HashMap::default();
    let mut refused: BTreeSet<String> = BTreeSet::new();
    let mut gaps: Vec<(Span, String, String)> = Vec::new();
    for i in order {
        let name = app.library_classes[i].name.0.as_str().to_string();
        let parent = parents.get(&name).filter(|p| mailers.contains(*p)).cloned();
        let decided = (|| -> Result<Option<Generated>, String> {
            if parent.as_ref().is_some_and(|p| refused.contains(p)) {
                return Err("its parent mailer's callbacks aren't lowered".into());
            }
            let parsed = parse_class(&app.library_classes[i], &mut counter)?;
            let (mut chain, mut handlers) = parent.as_ref().and_then(|p| chains.get(p)).cloned().unwrap_or_default();
            chain.extend(parsed.callbacks.iter().cloned());
            handlers.extend(parsed.handlers.iter().cloned());
            chains.insert(name.clone(), (chain.clone(), handlers.clone()));
            // A mailer with no callbacks still gets its actions' wrappers:
            // each action starts on a fresh mail, as Rails' fresh mailer
            // does, which two actions through one `with(...)` can tell.
            // Its actions are its own and its modules' (`action_methods`).
            if chain.is_empty()
                && handlers.is_empty()
                && actions(&app.library_classes[i]).is_empty()
                && module_actions(app, &name).is_empty()
            {
                return Ok(None);
            }
            let made = generated_methods(app, i, &chain, &handlers, &parsed, &parents, &mailers, &wrapped, &generated)?;
            Ok(Some(made))
        })();
        match decided {
            Ok(None) => {}
            Ok(Some(Generated { methods, own: actions, unwrapped })) => {
                for (span, action) in unwrapped {
                    gaps.push((
                        span,
                        format!("mailer action `{action}`"),
                        "an action with keyword, rest, block or destructured parameters, or a required one after an optional one, gets no callback chain".into(),
                    ));
                }
                let lc = &mut app.library_classes[i];
                wrapped.insert(name.clone(), actions.iter().cloned().collect());
                generated.extend(actions.iter().map(|a| format!("{ACTION_PREFIX}{a}")));
                generated.extend(
                    methods
                        .iter()
                        .map(|m| m.name.as_str().to_string())
                        .filter(|n| {
                            n.starts_with(CALLBACK_PREFIX)
                                || n.starts_with(CONDITION_PREFIX)
                                || n.starts_with(RESCUE_PREFIX)
                                || n == "__deliver"
                                || n == "__rescue"
                        }),
                );
                for m in lc.methods.iter_mut() {
                    if m.receiver == MethodReceiver::Instance && actions.contains(&m.name.as_str().to_string()) {
                        m.name = crate::ident::Symbol::from(format!("{ACTION_PREFIX}{}", m.name.as_str()));
                        m.visibility = MethodVisibility::Private;
                    }
                }
                lc.methods.extend(methods);
                let consumed: Vec<usize> = lc
                    .unknown_calls
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| declares_callbacks(c))
                    .map(|(i, _)| i)
                    .collect();
                remove_calls(lc, &consumed);
            }
            Err(reason) => {
                refused.insert(name.clone());
                let lc = &mut app.library_classes[i];
                let mine: Vec<usize> =
                    lc.unknown_calls.iter().enumerate().filter(|(_, c)| declares_callbacks(c)).map(|(i, _)| i).collect();
                for &k in &mine {
                    gaps.push((lc.unknown_calls[k].span, construct(&lc.unknown_calls[k]), reason.clone()));
                }
                remove_calls(lc, &mine);
            }
        }
    }
    let mut out = Vec::new();
    for (span, construct, reason) in gaps {
        out.extend(super::app::validates_gap_diagnostics(
            app,
            target,
            vec![(span, construct)],
            &format!("mailer callback not lowered: {reason}"),
        ));
    }
    out
}

/// A target without the chain: every mailer callback is reported, and
/// its action runs without it, as before.
pub(crate) fn refuse(app: &mut App, target: &str) -> Vec<crate::diagnostic::Diagnostic> {
    let mailers = crate::lower::mailer_class_side::mailer_names(app);
    let mut gaps: Vec<(Span, String)> = Vec::new();
    let mut rescues: Vec<(Span, String)> = Vec::new();
    for lc in &app.library_classes {
        if !mailers.contains(lc.name.0.as_str()) {
            continue;
        }
        for call in lc.unknown_calls.iter().filter(|c| declares_callbacks(c)) {
            if is_rescue(call) { &mut rescues } else { &mut gaps }.push((call.span, construct(call)));
        }
    }
    let mut out = super::app::validates_gap_diagnostics(
        app,
        target,
        gaps,
        "mailer callback not lowered: this target has no mailer callback chain, so the action runs without it",
    );
    out.extend(super::app::validates_gap_diagnostics(
        app,
        target,
        rescues,
        "mailer rescue_from not lowered: this target doesn't handle a mailer's exceptions, so they propagate",
    ));
    out
}

fn construct(call: &Expr) -> String {
    match &*call.node {
        ExprNode::Send { method, .. } => format!("mailer {}", method.as_str()),
        _ => "mailer callback".into(),
    }
}

fn remove_calls(lc: &mut LibraryClass, indices: &[usize]) {
    let mut i = 0;
    lc.unknown_calls.retain(|_| {
        let keep = !indices.contains(&i);
        i += 1;
        keep
    });
}

/// The class's own public actions: what ActionMailer's class-side proxy
/// answers (`action_methods`).
fn actions(lc: &LibraryClass) -> Vec<String> {
    lc.methods
        .iter()
        .filter(|m| {
            m.receiver == MethodReceiver::Instance
                && m.visibility == MethodVisibility::Public
                && m.name.as_str() != "initialize"
                && !m.name.as_str().starts_with("__")
        })
        .map(|m| m.name.as_str().to_string())
        .collect()
}

/// Every instance method the class and its mailer ancestors define, or
/// a module they include.
fn defined(app: &App, i: usize, parents: &HashMap<String, String>, mailers: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut cur = Some(app.library_classes[i].name.0.as_str().to_string());
    let mut guard = 0;
    while let Some(name) = cur {
        guard += 1;
        if guard > 64 {
            break;
        }
        if let Some(lc) = app.library_classes.iter().find(|c| c.name.0.as_str() == name) {
            out.extend(lc.methods.iter().filter(|m| m.receiver == MethodReceiver::Instance).map(|m| m.name.as_str().to_string()));
            // A module it includes (a mailer concern), and the modules that
            // one includes.
            let mut modules: Vec<String> = lc.includes.iter().map(|id| id.0.as_str().to_string()).collect();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            while let Some(module) = modules.pop() {
                if !seen.insert(module.clone()) {
                    continue;
                }
                if let Some(m) = app.library_classes.iter().find(|c| c.is_module && c.name.0.as_str() == module) {
                    out.extend(m.methods.iter().filter(|d| d.receiver == MethodReceiver::Instance).map(|d| d.name.as_str().to_string()));
                    modules.extend(m.includes.iter().map(|id| id.0.as_str().to_string()));
                }
            }
        }
        cur = parents.get(&name).filter(|p| mailers.contains(*p)).cloned();
    }
    out
}

/// The public actions a module `class_name` includes defines (a mailer
/// concern, or a plain module), and the modules that one includes: Rails
/// counts them in `action_methods`, and runs the chain for them.
fn module_actions<'a>(app: &'a App, class_name: &str) -> Vec<(String, &'a crate::dialect::MethodDef)> {
    let mut out: Vec<(String, &crate::dialect::MethodDef)> = Vec::new();
    let Some(lc) = app.library_classes.iter().find(|c| c.name.0.as_str() == class_name) else { return out };
    // Last included first, as Ruby's ancestors list them.
    let mut modules: Vec<String> = lc.includes.iter().map(|id| id.0.as_str().to_string()).collect();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some(module) = modules.pop() {
        if !seen.insert(module.clone()) {
            continue;
        }
        if let Some(m) = app.library_classes.iter().find(|c| c.is_module && c.name.0.as_str() == module) {
            for a in actions(m) {
                if !out.iter().any(|(n, _)| *n == a) {
                    let d = m.methods.iter().find(|d| d.receiver == MethodReceiver::Instance && d.name.as_str() == a).expect("an action is a method");
                    out.push((a, d));
                }
            }
            modules.extend(m.includes.iter().map(|id| id.0.as_str().to_string()));
        }
    }
    out
}

/// What [`generated_methods`] makes for a class: the methods, the class's
/// own actions given a wrapper (whose bodies `expand` renames), and the
/// actions it can't wrap, each reported.
struct Generated {
    methods: Vec<crate::dialect::MethodDef>,
    own: Vec<String>,
    unwrapped: Vec<(Span, String)>,
}

/// A wrapper's formals for `m`, and how it forwards to the body, or None
/// for a keyword, rest, forwarding, block or destructured formal, or a
/// required one after an optional one.
///
/// A default isn't the wrapper's to evaluate: Rails runs the callbacks and
/// then the action with only the arguments its caller gave, so the body
/// evaluates its defaults after them (`def welcome(user = @user)` beside a
/// `before_action` setting `@user`). Each optional formal defaults to a
/// marker that it was omitted, and the forward passes the given ones only;
/// an explicit nil is still passed.
struct Forward {
    formals: String,
    required: Vec<String>,
    optional: Vec<String>,
}

impl Forward {
    /// The call to `target` with the arguments the caller gave.
    fn call(&self, target: &str) -> String {
        let with = |n: usize| {
            let args: Vec<&str> =
                self.required.iter().map(String::as_str).chain(self.optional.iter().take(n).map(String::as_str)).collect();
            format!("{target}({})", args.join(", "))
        };
        if self.optional.is_empty() {
            return with(0);
        }
        let mut text = String::from("(");
        for (i, name) in self.optional.iter().enumerate() {
            text.push_str(&format!("{} __omitted_{name} then {} ", if i == 0 { "if" } else { "elsif" }, with(i)));
        }
        text.push_str(&format!("else {} end)", with(self.optional.len())));
        text
    }
}

fn wrapper_params(m: &crate::dialect::MethodDef) -> Option<Forward> {
    // A destructured or other formal ingest can't hold (`unsupported_formals`)
    // would be dropped from the wrapper's list.
    if m.block_param.is_some()
        || m.unsupported_formals.is_some()
        || m.params.iter().any(|p| p.keyword || p.forwarding || p.rest)
    {
        return None;
    }
    let mut formals = Vec::new();
    let mut required = Vec::new();
    let mut optional: Vec<String> = Vec::new();
    for p in &m.params {
        let name = p.name.as_str().to_string();
        match &p.default {
            None => {
                if !optional.is_empty() {
                    return None;
                }
                formals.push(name.clone());
                required.push(name);
            }
            Some(_) => {
                formals.push(format!("{name} = (__omitted_{name} = true; nil)"));
                optional.push(name);
            }
        }
    }
    Some(Forward { formals: formals.join(", "), required, optional })
}

#[allow(clippy::too_many_arguments)]
fn generated_methods(
    app: &App,
    i: usize,
    chain: &[Callback],
    handlers: &[Handler],
    parsed: &Parsed,
    parents: &HashMap<String, String>,
    mailers: &BTreeSet<String>,
    wrapped: &HashMap<String, BTreeSet<String>>,
    generated: &BTreeSet<String>,
) -> Result<Generated, String> {
    let lc = &app.library_classes[i];
    let defined = defined(app, i, parents, mailers);
    // The ancestors' actions with a chain: their bodies now carry the
    // prefix, and a callback naming one calls the body.
    let mut ancestors_wrapped: BTreeSet<String> = BTreeSet::new();
    let mut cur = parents.get(lc.name.0.as_str()).filter(|p| mailers.contains(*p)).cloned();
    while let Some(p) = cur {
        if let Some(names) = wrapped.get(&p) {
            ancestors_wrapped.extend(names.iter().cloned());
        }
        cur = parents.get(&p).filter(|q| mailers.contains(*q) && **q != p).cloned();
    }
    // The own actions that get a wrapper, and so a prefixed body: an
    // action that can't (`wrapper_params`) keeps its name, and a callback
    // naming it calls it as written.
    let own_actions: BTreeSet<String> = actions(lc)
        .into_iter()
        .filter(|a| {
            lc.methods
                .iter()
                .find(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == a)
                .is_some_and(|m| wrapper_params(m).is_some())
        })
        .collect();
    // A name the generated methods take must not be the app's own (one an
    // ancestor's chain generated is ours).
    if let Some(taken) = defined.iter().find(|n| {
        (n.starts_with(ACTION_PREFIX)
            || n.starts_with(CALLBACK_PREFIX)
            || n.starts_with(CONDITION_PREFIX)
            || n.starts_with(RESCUE_PREFIX)
            || n.as_str() == "__rescue"
            || n.as_str() == "__deliver")
            && !generated.contains(*n)
    }) {
        return Err(format!("the class defines `{taken}`, a name the generated chain would take"));
    }
    // Each generated private method: its name, its body, and its parameter.
    let mut generated_bodies: Vec<(String, Expr, Option<String>)> = Vec::new();
    // Called on `self`: a bare name would read an action's formal of that
    // name instead (`def notify(stamp)` beside `after_action :stamp`).
    let callable = |call: &Call| -> Result<String, String> {
        match call {
            Call::Method(name) if own_actions.contains(name) || ancestors_wrapped.contains(name) => {
                Ok(format!("self.{ACTION_PREFIX}{name}"))
            }
            Call::Method(name) if defined.contains(name) => Ok(format!("self.{name}")),
            Call::Method(name) => Err(format!("the callback `{name}` is not a method the mailer defines")),
            Call::Body(name, _) => Ok(format!("self.{name}")),
        }
    };
    let mut text = format!("class {}\n", lc.name.0.as_str());
    let mut own = Vec::new();
    let mut unwrapped = Vec::new();
    // The class's own actions, then those it inherits from a mailer
    // ancestor and doesn't redefine: Rails runs the receiving class's
    // chain for an inherited action, so it gets this class's wrapper,
    // around the ancestor's body.
    let own_methods: BTreeSet<String> =
        lc.methods.iter().filter(|m| m.receiver == MethodReceiver::Instance).map(|m| m.name.as_str().to_string()).collect();
    // (action, its method, the class's own, its body is a module's)
    let mut targets: Vec<(String, &crate::dialect::MethodDef, bool, bool)> = Vec::new();
    for action in actions(lc) {
        let m = lc
            .methods
            .iter()
            .find(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == action)
            .expect("an action is a method");
        targets.push((action, m, true, false));
    }
    // An action a module the class includes defines: the wrapper is the
    // class's own method, so it reaches the module's with `super`, and
    // the body keeps its name. Ahead of an ancestor's action, as Ruby's
    // ancestors order them.
    for (action, m) in module_actions(app, lc.name.0.as_str()) {
        if !own_methods.contains(&action) {
            targets.push((action, m, false, true));
        }
    }
    for action in &ancestors_wrapped {
        if own_methods.contains(action) || targets.iter().any(|(a, ..)| a == action) {
            continue;
        }
        let body = format!("{ACTION_PREFIX}{action}");
        let mut cur = parents.get(lc.name.0.as_str()).filter(|p| mailers.contains(*p)).cloned();
        let mut found = None;
        while let Some(p) = cur {
            if let Some(m) = app
                .library_classes
                .iter()
                .find(|c| c.name.0.as_str() == p)
                .and_then(|c| c.methods.iter().find(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == body))
            {
                found = Some(m);
                break;
            }
            cur = parents.get(&p).filter(|q| mailers.contains(*q) && **q != p).cloned();
        }
        if let Some(m) = found {
            targets.push((action.clone(), m, false, false));
        }
    }
    // An ancestor's module actions: the ancestor's wrapper reaches the
    // body, and this one runs this class's chain around it.
    let mut cur = parents.get(lc.name.0.as_str()).filter(|p| mailers.contains(*p)).cloned();
    while let Some(p) = cur {
        for (action, m) in module_actions(app, &p) {
            if !own_methods.contains(&action) && !targets.iter().any(|(a, ..)| *a == action) {
                targets.push((action, m, false, true));
            }
        }
        cur = parents.get(&p).filter(|q| mailers.contains(*q) && **q != p).cloned();
    }
    for (action, m, is_own, via_module) in targets {
        let Some(forward) = wrapper_params(m) else {
            // Its callbacks don't run, and it isn't wrapped: the rest of
            // the class's actions still are.
            unwrapped.push((m.name_span, action.clone()));
            continue;
        };
        if is_own {
            own.push(action.clone());
        }
        let applies: Vec<&Callback> = chain.iter().filter(|c| !c.deliver && c.applies_to(&action)).collect();
        let resolved = resolve(&applies, &callable)?;
        let formals = forward.formals.clone();
        let called = if via_module { forward.call("super") } else { forward.call(&format!("self.{ACTION_PREFIX}{action}")) };
        // The class side too, so an argument the caller leaves out stays
        // out (`mailer_class_side` would forward every formal, nil and
        // all); it builds the mailer that processes the action. A
        // module's action gets one always: `mailer_class_side` reads only
        // the class's own methods.
        if !forward.optional.is_empty() || via_module {
            text.push_str(&format!(
                "  def self.{action}({formals})\n    {}\n  end\n\n",
                forward.call(&format!("new.__processing.{action}"))
            ));
        }
        let body = nested(&resolved, &called, 6);
        // Called from inside a running action, the method is a plain call.
        // The chain's state is cleared when it ends, however it ends, so
        // the next action on this mailer (two through one `with`) runs its
        // own chain on a fresh mail.
        // With `rescue_from` handlers, an exception the chain raises is
        // offered to them (`__rescue`); one they take leaves the mail
        // `mail(...)` built, or the null mail if it wasn't called.
        let body = if handlers.is_empty() {
            body
        } else {
            format!("      begin\n{body}      rescue => __error\n        raise unless __rescue(__error)\n        return self.message || self.__null_mail\n      end\n")
        };
        text.push_str(&format!(
            "  def {action}({formals})\n    return {called} unless self.__start_action(\"{action}\")\n    begin\n{body}      self.message || self.__null_mail\n    ensure\n      self.__end_action\n    end\n  end\n\n"
        ));
    }
    // The handlers, last declared first (an ancestor's before this
    // class's), each taking the exception when its class matches; with
    // none, the exception's cause is tried, as `rescue_with_handler` does.
    if !handlers.is_empty() {
        let mut arms = String::new();
        for h in handlers.iter().rev() {
            let call = match &h.call {
                Call::Body(name, _) => format!("{name}(__current)"),
                Call::Method(name) if defined.contains(name) => match method_arity(app, i, parents, mailers, name) {
                    Some(0) => name.clone(),
                    Some(1) => format!("{name}(__current)"),
                    _ => return Err(format!("the handler `{name}` takes other than one or no argument")),
                },
                Call::Method(name) => return Err(format!("the handler `{name}` is not a method the mailer defines")),
            };
            arms.push_str(&format!("      when {}\n        {call}\n        return true\n", h.classes.join(", ")));
        }
        text.push_str(&format!(
            "  def __rescue(__exception)\n    __seen = []\n    __current = __exception\n    until __current.nil? || __seen.include?(__current)\n      case __current\n{arms}      end\n      __seen << __current\n      __current = __current.cause\n    end\n    false\n  end\n\n"
        ));
    }
    // The delivery callbacks run by `deliver_now` (`Message#deliver_now`
    // asks the mailer that built the mail); a `throw :abort` in one stops
    // the delivery, which answers nil.
    let delivering: Vec<&Callback> = chain.iter().filter(|c| c.deliver).collect();
    if !delivering.is_empty() {
        let resolved = resolve(&delivering, &callable)?;
        let body = nested(&resolved, "__delivered = __mail.__push", 6);
        let rescue = if handlers.is_empty() { "" } else { "  rescue => __error\n    raise unless __rescue(__error)\n    nil\n" };
        text.push_str(&format!(
            "  def __deliver(__mail)\n    __delivered = nil\n    catch(:abort) do\n{body}    end\n    __delivered\n{rescue}  end\n\n"
        ));
    }
    // This class's own block callbacks and lambda conditions.
    for c in &parsed.callbacks {
        for call in std::iter::once(&c.target).chain(&c.ifs).chain(&c.unlesses) {
            if let Call::Body(name, body) = call
                && !generated_bodies.iter().any(|(n, _, _)| n == name)
            {
                generated_bodies.push((name.clone(), body.clone(), None));
            }
        }
    }
    // This class's own block handlers.
    for h in &parsed.handlers {
        if let Call::Body(name, body) = &h.call {
            generated_bodies.push((name.clone(), body.clone(), h.param.clone()));
        }
    }
    if !generated_bodies.is_empty() {
        text.push_str("  private\n\n");
        for (name, _, param) in &generated_bodies {
            let params = param.as_ref().map(|p| format!("({p})")).unwrap_or_default();
            text.push_str(&format!("  def {name}{params}\n    nil\n  end\n\n"));
        }
    }
    text.push_str("end\n");
    let (classes, diags) = super::prism::scope(|| super::library_class::ingest_library_classes(text.as_bytes(), "<mailer_callbacks>"));
    let classes = classes.map_err(|_| "the generated chain did not ingest".to_string())?;
    if !diags.is_empty() {
        return Err("the generated chain did not ingest".into());
    }
    let mut methods: Vec<crate::dialect::MethodDef> = classes.into_iter().flat_map(|c| c.methods).collect();
    for m in methods.iter_mut() {
        if let Some((_, body, _)) = generated_bodies.iter().find(|(n, _, _)| n == m.name.as_str()) {
            m.body = body.clone();
        }
        m.enclosing_class = Some(lc.name.0.clone());
    }
    Ok(Generated { methods, own, unwrapped })
}

/// The number of parameters of the instance method `name` the class or
/// a mailer ancestor defines.
fn method_arity(
    app: &App,
    i: usize,
    parents: &HashMap<String, String>,
    mailers: &BTreeSet<String>,
    name: &str,
) -> Option<usize> {
    let mut cur = Some(app.library_classes[i].name.0.as_str().to_string());
    let mut guard = 0;
    while let Some(class) = cur {
        guard += 1;
        if guard > 64 {
            break;
        }
        if let Some(lc) = app.library_classes.iter().find(|c| c.name.0.as_str() == class) {
            if let Some(m) = lc.methods.iter().find(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == name) {
                return Some(m.params.len());
            }
            // A module it includes (a mailer concern), as `defined` reads
            // it, and the modules that one includes.
            let mut modules: Vec<String> = lc.includes.iter().map(|id| id.0.as_str().to_string()).collect();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            while let Some(module) = modules.pop() {
                if !seen.insert(module.clone()) {
                    continue;
                }
                if let Some(m) = app.library_classes.iter().find(|c| c.is_module && c.name.0.as_str() == module) {
                    if let Some(d) = m.methods.iter().find(|d| d.receiver == MethodReceiver::Instance && d.name.as_str() == name) {
                        return Some(d.params.len());
                    }
                    modules.extend(m.includes.iter().map(|id| id.0.as_str().to_string()));
                }
            }
        }
        cur = parents.get(&class).filter(|p| mailers.contains(*p)).cloned();
    }
    None
}

/// Each callback's method and its conditions, as the chain calls them.
fn resolve(
    callbacks: &[&Callback],
    callable: &dyn Fn(&Call) -> Result<String, String>,
) -> Result<Vec<(Kind, String, String)>, String> {
    let mut resolved = Vec::new();
    for c in callbacks {
        let target = callable(&c.target)?;
        let mut conds = Vec::new();
        for cond in &c.ifs {
            conds.push(format!("({})", callable(cond)?));
        }
        for cond in &c.unlesses {
            conds.push(format!("!({})", callable(cond)?));
        }
        resolved.push((c.kind, target, conds.join(" && ")));
    }
    Ok(resolved)
}

/// The chain, from `resolved[0]` inward to `action`, at `indent`.
fn nested(resolved: &[(Kind, String, String)], action: &str, indent: usize) -> String {
    fn go(rest: &[(Kind, String, String)], action: &str, indent: usize) -> String {
        let pad = " ".repeat(indent);
        let Some(((kind, name, cond), rest)) = rest.split_first() else {
            return format!("{pad}{action}\n");
        };
        let guard = if cond.is_empty() { String::new() } else { format!(" if {cond}") };
        match kind {
            Kind::Before => format!("{pad}{name}{guard}\n{}", go(rest, action, indent)),
            Kind::After => format!("{}{pad}{name}{guard}\n", go(rest, action, indent)),
            Kind::Around if cond.is_empty() => {
                format!("{pad}{name} do\n{}{pad}end\n", go(rest, action, indent + 2))
            }
            Kind::Around => format!(
                "{pad}if {cond}\n{pad}  {name} do\n{}{pad}  end\n{pad}else\n{}{pad}end\n",
                go(rest, action, indent + 4),
                go(rest, action, indent + 2)
            ),
        }
    }
    go(resolved, action, indent)
}
