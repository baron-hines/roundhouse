//! A superclass read off a configured object as the class loads, as
//! doorkeeper 5.9 writes its controllers' parents:
//!
//! ```ruby
//! class ApplicationController < Doorkeeper.config.resolve_controller(:base)
//! ```
//!
//! Rails has run the app's initializers by then, and the host's
//! `Doorkeeper.configure do base_controller "ApplicationController" end`
//! has set the option the expression reads. Ingest records such an
//! expression ([`defer`]) instead of refusing it, and [`fold`] evaluates
//! it once the class macros have expanded (an option DSL's readers and
//! the builder's writers are ordinary methods by then).
//!
//! The evaluation is over the program's own methods, on a small heap of
//! objects: the initializer's calls on the expression's root constant
//! first, then the expression. A built-in it does not model, called on a
//! value that is not the program's (`1.year`, `Rails.env`), answers a
//! value that is carried but never tested or called on. A statement whose
//! value is not used and that it cannot run is passed over: every
//! instance variable a method it can reach may write is unsure until the
//! evaluation writes it again, and reading an unsure one stops the
//! evaluation. What stops leaves the superclass refused, as before.

use std::cell::RefCell;
use std::path::Path;

use crate::App;
use crate::det::{HashMap, HashSet};
use crate::dialect::{LibraryClass, MethodDef, MethodReceiver};
use crate::expr::{BoolOpKind, Expr, ExprNode, InterpPart, LValue, Literal, OpAssignOp, Pattern};
use crate::ident::{ClassId, Symbol};
use crate::span::Span;
use crate::vfs::Vfs;

use super::util::constant_path_of;

struct Pending {
    owner: ClassId,
    expr: Expr,
    span: Span,
}

thread_local! {
    /// Set while an app ingest runs, the one caller that folds what it defers.
    static FOLDING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PENDING: RefCell<Vec<Pending>> = const { RefCell::new(Vec::new()) };
    static CLASS_NAMES: RefCell<Vec<(Span, Expr)>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn reset() {
    FOLDING.with(|f| f.set(true));
    PENDING.with(|p| p.borrow_mut().clear());
    CLASS_NAMES.with(|p| p.borrow_mut().clear());
}

/// An association's `class_name:` written as an expression, by the declaration's span.
pub(super) fn defer_class_name(span: Span, expr: Expr) {
    if !FOLDING.with(|f| f.get()) {
        return;
    }
    CLASS_NAMES.with(|p| p.borrow_mut().push((span, expr)));
}

/// Records `node` for [`fold`] when it is a call chain rooted at a
/// constant (`Doorkeeper.config.resolve_controller(:base)`); `false`
/// leaves the superclass to the caller's refusal.
pub(super) fn defer(owner: &ClassId, node: &ruby_prism::Node<'_>, span: Span, file: &str) -> bool {
    if !FOLDING.with(|f| f.get()) {
        return false;
    }
    let mut root = node.as_call_node();
    let mut calls = 0;
    while let Some(call) = root {
        calls += 1;
        match call.receiver() {
            Some(r) if r.as_call_node().is_some() => root = r.as_call_node(),
            Some(r) if constant_path_of(&r).is_some() => break,
            _ => return false,
        }
    }
    if calls == 0 {
        return false;
    }
    let Ok(expr) = super::expr::ingest_expr(node, file) else { return false };
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        if !p.iter().any(|d| &d.owner == owner) {
            p.push(Pending { owner: owner.clone(), expr, span });
        }
    });
    true
}

/// Each deferred superclass: the constant it evaluates to, set as the
/// class's parent, or the refusal ingest would have reported.
pub(super) fn fold<V: Vfs + ?Sized>(vfs: &V, dir: &Path, app: &mut App) {
    FOLDING.with(|f| f.set(false));
    let pending = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    if pending.is_empty() {
        return;
    }
    let initializers = initializer_statements(vfs, dir, &[]);
    for item in pending {
        let answer = Eval::new(app).answer(&item.expr, &initializers);
        match answer {
            Ok(name) => {
                let parent = ClassId(Symbol::from(name.as_str()));
                for controller in app.controllers.iter_mut().filter(|c| c.name == item.owner) {
                    controller.parent = Some(parent.clone());
                }
                for lc in app.library_classes.iter_mut().filter(|c| c.name == item.owner) {
                    lc.parent = Some(parent.clone());
                }
            }
            Err(_) => app.superclass_refusals.push(crate::diagnostic::Diagnostic::unsupported(
                item.span,
                None,
                "superclass",
                format!(
                    "`{}`'s superclass is an expression evaluated when the class loads, not a constant, so the class is emitted without it",
                    item.owner.0.as_str()
                ),
            )),
        }
    }
}

/// Each association whose `class_name:` is an expression: the class it names once the initializers have run
/// (doorkeeper's `has_many :authorized_tokens, class_name: Doorkeeper.config.access_token_class.to_s`). One it
/// cannot settle keeps the class its name gives, as before.
pub(super) fn fold_class_names<V: Vfs + ?Sized>(vfs: &V, dir: &Path, app: &mut App) {
    let pending = CLASS_NAMES.with(|p| std::mem::take(&mut *p.borrow_mut()));
    if pending.is_empty() {
        return;
    }
    let initializers = initializer_statements(vfs, dir, &[]);
    let mut answers: Vec<(Span, String)> = Vec::new();
    for (span, expr) in &pending {
        let name = match Eval::new(app).answer_value(expr, &initializers) {
            Ok(Val::Str(s)) | Ok(Val::Class(s)) => s.trim_start_matches("::").to_string(),
            _ => continue,
        };
        if name.split("::").all(|seg| seg.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
            answers.push((*span, name));
        }
    }
    for model in &mut app.models {
        for item in &mut model.body {
            let crate::dialect::ModelBodyItem::Association { assoc, span, .. } = item else { continue };
            let Some((_, name)) = answers.iter().find(|(s, _)| s.file == span.file && s.start == span.start && s.end == span.end) else { continue };
            assoc.set_target(ClassId(Symbol::from(name.as_str())));
        }
    }
}

/// A model's table set from a concern's `included do self.table_name = compute_doorkeeper_table_name end`
/// (doorkeeper's ORM mixins): the name the class method answers, read against ActiveRecord's own settings,
/// binds the schema row the model's attributes come from. A setting the app writes anywhere, or a name
/// it cannot settle, leaves the model on the table ingest gave it.
pub(super) fn fold_table_names<V: Vfs + ?Sized>(vfs: &V, dir: &Path, app: &mut App) {
    let mut config = String::new();
    for file in ["config/application.rb"] {
        config.push_str(&vfs.read_to_string(&dir.join(file)).unwrap_or_default());
    }
    for sub in ["config/environments", "config/initializers"] {
        let root = dir.join(sub);
        if vfs.is_dir(&root) {
            for file in super::app::read_rb_files(vfs, &root).unwrap_or_default() {
                config.push_str(&vfs.read_to_string(&file).unwrap_or_default());
            }
        }
    }
    // Not the defaults when the app writes either: which value a model sees then turns on where and when it is set.
    if config.contains("pluralize_table_names") || config.contains("table_name_suffix") {
        return;
    }
    for index in 0..app.models.len() {
        let model = &app.models[index];
        let writes: Vec<(usize, Expr)> = model
            .body
            .iter()
            .enumerate()
            .filter_map(|(i, item)| match item {
                crate::dialect::ModelBodyItem::Unknown { expr, .. } => table_name_write(expr).map(|v| (i, v.clone())),
                _ => None,
            })
            .collect();
        let [(item, value)] = writes.as_slice() else { continue };
        let name = model.name.0.as_str().to_string();
        let conventional = crate::naming::rails_table_name(&name);
        let Some(prefix) = model.table.0.as_str().strip_suffix(conventional.as_str()) else { continue };
        let methods: Vec<MethodDef> = model.methods().filter(|m| m.receiver == MethodReceiver::Class).cloned().collect();
        let mut answers = HashMap::default();
        answers.insert("pluralize_table_names".to_string(), Val::Bool(true));
        answers.insert("table_name_prefix".to_string(), Val::Str(prefix.to_string()));
        answers.insert("table_name_suffix".to_string(), Val::Str(String::new()));
        let mut eval = Eval::new(app);
        eval.subject = Some((name.clone(), methods, answers));
        let mut frame = Frame { this: Val::Class(name.clone()), locals: HashMap::default(), block: None, scope: Some(name.clone()) };
        let Ok(Val::Str(table)) = eval.value(value, &mut frame) else { continue };
        if !table.bytes().next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_') || !table.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            continue;
        }
        let row = app.schema.tables.get(&Symbol::from(table.as_str())).map(super::model::row_from_table);
        let model = &mut app.models[index];
        model.table = crate::ident::TableRef(Symbol::from(table.as_str()));
        if let Some(row) = row {
            model.attributes = row;
        }
        model.body.remove(*item);
    }
}

fn table_name_write(expr: &Expr) -> Option<&Expr> {
    match &*expr.node {
        ExprNode::Assign { target: LValue::Attr { recv, name }, value } if matches!(&*recv.node, ExprNode::SelfRef) && name.as_str() == "table_name" => Some(value),
        ExprNode::Send { recv: Some(r), method, args, block: None, .. } if matches!(&*r.node, ExprNode::SelfRef) && method.as_str() == "table_name=" => match args.as_slice() {
            [v] => Some(v),
            _ => None,
        },
        _ => None,
    }
}

/// The top-level statements of `config/initializers/*.rb`, in the order Rails loads them.
fn initializer_statements<V: Vfs + ?Sized>(vfs: &V, dir: &Path, gem_dirs: &[std::path::PathBuf]) -> Vec<Expr> {
    // A gem compiled with the app runs its files' top-level calls as it is required, before the host's initializers (devise's `Devise.with_options model: true do |d| d.add_module …`).
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let mut gems = gem_dirs.to_vec();
    gems.sort();
    for gem in gems {
        let mut lib = super::app::read_rb_files(vfs, &gem.join("lib")).unwrap_or_default();
        lib.sort();
        files.extend(lib);
    }
    let root = dir.join("config/initializers");
    if vfs.is_dir(&root) {
        let mut own = super::app::read_rb_files(vfs, &root).unwrap_or_default();
        own.sort();
        files.extend(own);
    }
    let mut out = Vec::new();
    for file in files {
        let Ok(source) = vfs.read(&file) else { continue };
        let name = file.display().to_string();
        let parsed = super::prism::parse_silent(&source);
        let node = parsed.node();
        let Some(program) = node.as_program_node() else { continue };
        for statement in program.statements().body().iter() {
            // Not a declaration: a class or module body is the library walk's, not a call that runs here.
            if statement.as_class_node().is_some() || statement.as_module_node().is_some() || statement.as_call_node().is_some_and(|c| matches!(super::util::constant_id_str(&c.name()), "require" | "require_relative" | "autoload")) {
                continue;
            }
            let (read, _) = super::prism::scope(|| super::expr::ingest_expr(&statement, &name));
            if let Ok(expr) = read {
                out.push(expr);
            }
        }
    }
    out
}

#[derive(Clone, Debug)]
pub(super) enum Val {
    Nil,
    Bool(bool),
    Int(i64),
    Str(String),
    Sym(String),
    /// A class or module, by full name; one the program does not define is named only.
    Class(String),
    Obj(usize),
    Array(Vec<Val>),
    Hash(Vec<(Val, Val)>),
    /// Its `self` and locals are the ones where the block is written, not where it is called (`yield` in `Devise.setup`).
    Proc { params: Vec<Symbol>, rest: Option<Symbol>, body: Expr, scope: Option<String>, this: Box<Val>, locals: std::rc::Rc<HashMap<Symbol, Val>>, block: Option<Box<Val>> },
    Regex(String),
    /// What a built-in answered that this does not model: carried, never tested or called on.
    Unknown,
    /// The routing mapper a gem's routing method draws on: what is called on it is recorded, not run.
    Router,
    /// `with_options(model: true) { |d| d.add_module … }`: each call on it goes to the target with the options merged into its last Hash.
    Merger { target: Box<Val>, options: Vec<(Val, Val)> },
    /// `Hash.new { |h, k| h[k] = "#{mod}/#{k}" }`, or `Hash.new(0)`: a missing key answers the default, or what the block answers for it.
    Defaulted { entries: Vec<(Val, Val)>, default: Box<Val> },
}

impl Val {
    fn truthy(&self) -> Res<bool> {
        match self {
            Val::Unknown => Err("a value it does not model is tested".into()),
            v => Ok(!matches!(v, Val::Nil | Val::Bool(false))),
        }
    }
}

struct Object {
    class: String,
    ivars: HashMap<String, Val>,
}

/// Where a method body runs: its `self`, its locals, its block and the class its constants resolve in.
#[derive(Clone)]
struct Frame {
    this: Val,
    locals: HashMap<Symbol, Val>,
    block: Option<Val>,
    scope: Option<String>,
}

enum Flow {
    Value(Val),
    Return(Val),
}

type Res<T> = Result<T, String>;

/// An instance variable by the class whose objects (or, class side, which class itself) hold it; `*` for any.
type Tag = (String, bool, String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Holder {
    Object(usize),
    Class(String),
    Router,
    /// A constant's value, by its full name, changed in place (`Devise::ROUTES[name] = key`).
    Constant(String),
}

const MAPPER: &str = "ActionDispatch::Routing::Mapper";

/// Marks an error no statement is passed over for (a gem hook's).
const STOP: &str = "\u{0}stop ";

/// The owner a write to a constant's value is tagged with.
const CONSTANT_OWNER: &str = "::constant";

const MAX_DEPTH: usize = 40;

struct Eval<'a> {
    classes: &'a [LibraryClass],
    known: HashSet<String>,
    heap: Vec<Object>,
    /// A class or module's own instance variables (`Doorkeeper`'s `@config`).
    class_ivars: HashMap<(String, String), Val>,
    /// When a statement passed over may last have written names of a holder (a pattern: `*_secret_strategy`, `*`), and when the evaluation last wrote each name.
    unsure: HashMap<Holder, Vec<(String, usize)>>,
    written: HashMap<(Holder, String), usize>,
    tick: usize,
    depth: usize,
    /// A class outside the library classes (a model) whose class side the evaluation reads: its methods, and the framework settings it answers.
    subject: Option<(String, Vec<MethodDef>, HashMap<String, Val>)>,
    /// The methods running, innermost last: where each was found, its name and side, and what it was given, for `super`.
    calls: Vec<(String, String, MethodReceiver, Vec<Val>, Option<Val>)>,
    /// The routes drawn on a `Router`, one list per open block.
    drawn: Vec<Vec<Expr>>,
    /// Class variables, by the class whose body sets them first.
    cvars: HashMap<(String, String), Val>,
    router: RouterState,
    facts: super::gem_hooks::Facts,
    /// Constants' values once read, by full name, so a change in place (`ALL.insert`) is what the next read sees.
    consts: HashMap<String, Val>,
    /// Methods a `class_eval` string defined, by class, the latest last.
    evaled: HashMap<String, Vec<MethodDef>>,
    /// What the drawing left out, and why (`gem_hooks::omits_on_failure`).
    omitted: Vec<String>,
}

/// The mapper a routing method runs on: the program's classes that answer on it, and its instance variables.
#[derive(Default)]
struct RouterState {
    mixins: Vec<String>,
    ivars: HashMap<String, Val>,
    /// The scope the call stands in, as Rails' `@scope` holds it there: `{ path: "/admin", module: "auth", as: "admin" }`; `None` inside a resource, which the evaluation does not hold.
    enclosing: Option<Vec<(Val, Val)>>,
    /// The routing method runs: what the initializers left is replayed, and writes reached by name may be this mapper's.
    drawing: bool,
}

impl RouterState {
    /// `@scope` where the call stands.
    fn base(&self) -> Val {
        let Val::Hash(mut entries) = root_scope() else { unreachable!() };
        entries.extend(self.enclosing.clone().unwrap_or_default());
        Val::Hash(entries)
    }
}

/// Rails' `Mapper::Scope` at the top of a route set, as `@scope[:key]` reads it.
fn root_scope() -> Val {
    let names = Val::Hash(vec![(Val::Sym("new".into()), Val::Str("new".into())), (Val::Sym("edit".into()), Val::Str("edit".into()))]);
    Val::Hash(vec![(Val::Sym("path_names".into()), names)])
}

/// What `@scope` sets that a route drawn under it inherits.
const SCOPE_KEYS: &[&str] = &["path", "as", "module", "constraints", "defaults", "options"];

impl<'a> Eval<'a> {
    fn new(app: &'a App) -> Self {
        let known = app.library_classes.iter().map(|c| c.name.0.as_str().to_string()).collect();
        Eval { classes: &app.library_classes, known, heap: Vec::new(), class_ivars: HashMap::default(), unsure: HashMap::default(), written: HashMap::default(), tick: 0, depth: 0, subject: None, calls: Vec::new(), drawn: Vec::new(), cvars: HashMap::default(), router: RouterState::default(), facts: super::gem_hooks::Facts::of(app), consts: HashMap::default(), evaled: HashMap::default(), omitted: Vec::new() }
    }

    /// The constant `expr` names once the initializers that call on its root have run.
    fn answer(&mut self, expr: &Expr, initializers: &[Expr]) -> Res<String> {
        match self.answer_value(expr, initializers)? {
            Val::Class(name) => Ok(name),
            other => Err(format!("it evaluates to {other:?}, not a class")),
        }
    }

    fn answer_value(&mut self, expr: &Expr, initializers: &[Expr]) -> Res<Val> {
        let root = root_constant(expr).ok_or("no constant at the root")?;
        let top_name = root.split("::").next().unwrap_or_default().to_string();
        self.replay(initializers, &top_name)?;
        let top = Frame { this: Val::Class("Object".into()), locals: HashMap::default(), block: None, scope: None };
        self.value(expr, &mut top.clone())
    }

    /// The initializers' calls on `root`; one that names it any other way (under an `if`, in a
    /// `to_prepare`, through `Root::Config`) and may write an instance variable may set what the
    /// answer reads, so nothing folds.
    fn replay(&mut self, initializers: &[Expr], root: &str) -> Res<()> {
        let top = Frame { this: Val::Class("Object".into()), locals: HashMap::default(), block: None, scope: None };
        for statement in initializers.iter().filter(|s| !super::gem_hooks::registers_for_later(s)) {
            // Not the whole path: `Admin::Settings.configure` calls on what `Admin` holds.
            if root_constant(statement).is_some_and(|r| r.split("::").next() == Some(root)) {
                self.statement(statement, &mut top.clone())?;
            } else if let deferred = without_deferred_bodies(statement)
                && names_constant(&deferred, root)
                && let ExprNode::If { cond, .. } = &*deferred.node
                && defines_under(cond, root)
            {
                // Not refused for naming it under a guard (devise_token_auth's `if defined?(Devise::Models::…)`): the arm it takes runs, and one it cannot tell passes over with its writes unsure.
                self.statement(&deferred, &mut top.clone())?;
            } else if let deferred = without_deferred_bodies(statement)
                && names_constant(&deferred, root)
                && self.may_write(&deferred, "*", false, &mut HashSet::default()).is_none_or(|w| !w.is_empty())
            {
                return Err(format!("an initializer reaches `{root}` other than through a call on it, and may write what the answer reads"));
            }
        }
        Ok(())
    }

    /// A statement whose value is not used: one it cannot run is passed over and its possible writes kept.
    fn statement(&mut self, e: &Expr, frame: &mut Frame) -> Res<Option<Val>> {
        if super::gem_hooks::skips(&self.known, self.calls.last().map(|c| (c.0.as_str(), c.1.as_str())), e) {
            return Ok(None);
        }
        if self.router.drawing
            && let Some(why) = super::gem_hooks::omits_statement_on_failure(&self.known, e)
        {
            // As a method `omits_on_failure` names: what it drew, and the mapper state it changed, are the statement's or none of it.
            let drawn = self.drawn.last().map_or(0, Vec::len);
            let ivars = self.router.ivars.clone();
            let written: Vec<_> = self.written.iter().filter(|((h, _), _)| matches!(h, Holder::Router)).map(|(k, t)| (k.clone(), *t)).collect();
            return match self.eval(e, frame) {
                Ok(Flow::Return(v)) => Ok(Some(v)),
                Ok(Flow::Value(_)) => Ok(None),
                Err(err) => {
                    if let Some(last) = self.drawn.last_mut() {
                        last.truncate(drawn);
                    }
                    self.router.ivars = ivars;
                    self.written.retain(|(h, _), _| !matches!(h, Holder::Router));
                    self.written.extend(written);
                    self.omitted.push(format!("{why} ({err})"));
                    Ok(None)
                }
            };
        }
        match self.eval(e, frame) {
            Ok(Flow::Return(v)) => Ok(Some(v)),
            Ok(Flow::Value(_)) => Ok(None),
            // Not one a gem hook gave: what it says is not known stops the evaluation where a statement passed over would leave a guess.
            Err(why) if !matches!(&*e.node, ExprNode::Return { .. }) && !why.starts_with(STOP) => {
                let (owner, side) = self.tag_of(&frame.this);
                let devise: HashSet<Symbol> = frame.locals.iter().filter(|(_, v)| matches!(v, Val::Class(c) if c == "Devise")).map(|(k, _)| k.clone()).collect();
                let writer = |setter: &str| -> Option<Result<String, ()>> {
                    let (def, _) = self.lookup_in("Devise", setter, MethodReceiver::Class, &mut HashSet::default())?;
                    let [param] = def.params.as_slice() else { return Some(Err(())) };
                    let body = match &*def.body.node {
                        ExprNode::Seq { exprs } if exprs.len() == 1 => &exprs[0],
                        _ => &def.body,
                    };
                    match &*body.node {
                        ExprNode::Assign { target: LValue::Var { name, .. }, value }
                            if name.as_str().starts_with("@@") && matches!(&*value.node, ExprNode::Var { name: v, .. } if *v == param.name) =>
                        {
                            Some(Ok(name.as_str().to_string()))
                        }
                        _ => Some(Err(())),
                    }
                };
                let confined = super::gem_hooks::confined_writes(&self.known, self.calls.last().map(|c| (c.0.as_str(), c.1.as_str())), e, &devise, &writer);
                let Some(writes) = confined.or_else(|| self.may_write_with(e, &owner, side, &mut HashSet::default(), &frame.locals)) else {
                    return Err(format!("{why}, in a statement whose writes are not known"));
                };
                for tag in &writes {
                    self.make_unsure(tag);
                }
                // A local it may have assigned is no longer what the frame holds.
                visit(e, &mut |x| {
                    if let ExprNode::Assign { target: LValue::Var { name, .. }, .. } | ExprNode::OpAssign { target: LValue::Var { name, .. }, .. } = &*x.node {
                        frame.locals.remove(name);
                    }
                });
                Ok(None)
            }
            Err(why) => Err(why),
        }
    }

    fn tag_of(&self, this: &Val) -> (String, bool) {
        match this {
            Val::Obj(i) => (self.heap[*i].class.clone(), false),
            Val::Class(c) => (c.clone(), true),
            Val::Router => (MAPPER.into(), false),
            _ => ("*".into(), false),
        }
    }

    fn make_unsure(&mut self, (owner, side, name): &Tag) {
        self.tick += 1;
        if owner == CONSTANT_OWNER {
            // Not only a constant read already: one read later is unsure too. A written name that did not resolve may be any constant it ends with.
            self.unsure.entry(Holder::Constant(name.clone())).or_default().push(("*".into(), self.tick));
            return;
        }
        // Not for a write on an unknown receiver before the mapper is made: an initializer's runs on no mapper. While a method draws, a mixin's method reached by name may be the mapper's.
        if !*side && (owner == MAPPER || self.router.mixins.contains(owner) || (owner == "*" && self.router.drawing)) {
            self.unsure.entry(Holder::Router).or_default().push((name.clone(), self.tick));
        }
        let mut holders: Vec<Holder> = (0..self.heap.len())
            .filter(|&i| !*side && (owner == "*" || self.ancestry(&self.heap[i].class).contains(owner)))
            .map(Holder::Object)
            .collect();
        if *side || owner == "*" {
            if owner == "*" {
                holders.extend(self.known.iter().cloned().map(Holder::Class));
            } else {
                holders.push(Holder::Class(owner.clone()));
            }
        }
        for h in holders {
            self.unsure.entry(h).or_default().push((name.clone(), self.tick));
        }
    }

    fn holder(this: &Val) -> Option<Holder> {
        match this {
            Val::Router => Some(Holder::Router),
            Val::Obj(i) => Some(Holder::Object(*i)),
            Val::Class(c) => Some(Holder::Class(c.clone())),
            _ => None,
        }
    }

    fn note_read(&mut self, this: &Val, name: String) -> Res<()> {
        // Not tracked there: a `scope … do` it records is not run on the mapper, so its `@scope` is not this one.
        if matches!(this, Val::Router) && name == "scope" && self.drawn.len() > 1 {
            return Err("`@scope` read inside a routing block it records".into());
        }
        let Some(h) = Self::holder(this) else { return Ok(()) };
        let last_unsure = self.unsure.get(&h).into_iter().flatten().filter(|(p, _)| glob(p, &name)).map(|(_, t)| *t).max();
        let written = self.written.get(&(h, name.clone())).copied().unwrap_or(0);
        let shown = if name.starts_with("@@") { name.clone() } else { format!("@{name}") };
        if last_unsure.is_some_and(|u| u > written) { Err(format!("`{shown}` may have been written by a statement passed over")) } else { Ok(()) }
    }

    /// Ruby's lookup order: the class, the modules it includes (last first), then its superclass's.
    fn mro(&self, class: &str, side: MethodReceiver) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut at = Some(class.to_string());
        for _ in 0..32 {
            let Some(c) = at else { break };
            if out.contains(&c) {
                break;
            }
            out.push(c.clone());
            let decls: Vec<&LibraryClass> = self.classes.iter().filter(|d| d.name.0.as_str() == c).collect();
            if side == MethodReceiver::Instance {
                for d in decls.iter().rev() {
                    for inc in d.includes.iter().rev() {
                        if let Some(m) = self.resolve(inc.0.as_str(), Some(&c)) {
                            for x in self.mro(&m, side) {
                                if !out.contains(&x) {
                                    out.push(x);
                                }
                            }
                        }
                    }
                }
            }
            at = decls.iter().find_map(|d| d.parent.clone()).and_then(|p| self.resolve(p.0.as_str(), Some(&c)));
        }
        out
    }

    /// The class and what its lookup reaches: superclasses and included modules.
    fn ancestry(&self, class: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut todo = vec![class.to_string()];
        while let Some(c) = todo.pop() {
            if out.contains(&c) {
                continue;
            }
            out.push(c.clone());
            for decl in self.classes.iter().filter(|d| d.name.0.as_str() == c) {
                todo.extend(decl.includes.iter().filter_map(|i| self.resolve(i.0.as_str(), Some(&c))));
                todo.extend(decl.parent.iter().filter_map(|p| self.resolve(p.0.as_str(), Some(&c))));
            }
        }
        out
    }

    fn value(&mut self, e: &Expr, frame: &mut Frame) -> Res<Val> {
        match self.eval(e, frame)? {
            Flow::Value(v) | Flow::Return(v) => Ok(v),
        }
    }

    fn eval(&mut self, e: &Expr, frame: &mut Frame) -> Res<Flow> {
        let v = match &*e.node {
            ExprNode::Seq { exprs } => {
                let Some((last, rest)) = exprs.split_last() else { return Ok(Flow::Value(Val::Nil)) };
                for st in rest {
                    if let Some(v) = self.statement(st, frame)? {
                        return Ok(Flow::Return(v));
                    }
                }
                // Not by its value when the block ends in one `omits_statement_on_failure` names: what it leaves out is left out there too.
                if self.router.drawing && super::gem_hooks::omits_statement_on_failure(&self.known, last).is_some() {
                    return Ok(match self.statement(last, frame)? {
                        Some(v) => Flow::Return(v),
                        None => Flow::Value(Val::Nil),
                    });
                }
                return self.eval(last, frame);
            }
            ExprNode::Return { value } => return Ok(Flow::Return(self.value(value, frame)?)),
            ExprNode::Lit { value } => literal(value)?,
            ExprNode::SelfRef => frame.this.clone(),
            ExprNode::Var { name, .. } if name.as_str().starts_with("@@") => self.cvar_get(frame.scope.as_deref(), name.as_str())?,
            ExprNode::Var { name, .. } => frame.locals.get(name).cloned().ok_or_else(|| format!("`{}` is not bound", name.as_str()))?,
            // `defined?(@x)` as the parser can leave it, a call.
            ExprNode::Send { recv: None, method, args, block: None, .. } if method.as_str() == "defined?" && args.len() == 1 => match &*args[0].node {
                ExprNode::Ivar { name } => {
                    self.note_read(&frame.this, name.as_str().to_string())?;
                    match self.ivar(&frame.this, name.as_str()) {
                        Some(_) => Val::Str("instance-variable".into()),
                        None => Val::Nil,
                    }
                }
                // Only a constant it resolves: one it cannot may still be a gem's that is loaded without being read here.
                ExprNode::Const { path } if self.resolves_constant(path, frame.scope.as_deref()) => Val::Str("constant".into()),
                ExprNode::Const { .. } => return Err("a `defined?` of a constant it does not resolve".into()),
                _ => return Err("a `defined?` of other than an instance variable".into()),
            },
            ExprNode::Ivar { name } => {
                self.note_read(&frame.this, name.as_str().to_string())?;
                self.ivar(&frame.this, name.as_str()).unwrap_or(Val::Nil)
            }
            ExprNode::Const { path } => self.constant(path, frame.scope.as_deref())?,
            ExprNode::StringInterp { parts } => {
                let mut text = String::new();
                for part in parts {
                    match part {
                        InterpPart::Text { value } => text.push_str(value),
                        InterpPart::Expr { expr } => {
                            let v = self.value(expr, frame)?;
                            text.push_str(&self.text(&v)?);
                        }
                    }
                }
                Val::Str(text)
            }
            ExprNode::Array { elements, .. } => {
                Val::Array(elements.iter().map(|el| self.value(el, frame)).collect::<Res<_>>()?)
            }
            ExprNode::Hash { entries, .. } => Val::Hash(
                entries.iter().map(|(k, v)| Ok((self.value(k, frame)?, self.value(v, frame)?))).collect::<Res<_>>()?,
            ),
            ExprNode::BoolOp { op, left, right, .. } => {
                let l = self.value(left, frame)?;
                if l.truthy()? == matches!(op, BoolOpKind::Or) { l } else { self.value(right, frame)? }
            }
            ExprNode::If { cond, then_branch, else_branch } => {
                return if self.value(cond, frame)?.truthy()? { self.eval(then_branch, frame) } else { self.eval(else_branch, frame) };
            }
            ExprNode::Defined { operand } => match &*operand.node {
                ExprNode::Ivar { name } => {
                    self.note_read(&frame.this, name.as_str().to_string())?;
                    match self.ivar(&frame.this, name.as_str()) {
                        Some(_) => Val::Str("instance-variable".into()),
                        None => Val::Nil,
                    }
                }
                // Only a constant it resolves: one it cannot may still be a gem's that is loaded without being read here.
                ExprNode::Const { path } if self.resolves_constant(path, frame.scope.as_deref()) => Val::Str("constant".into()),
                ExprNode::Const { .. } => return Err("a `defined?` of a constant it does not resolve".into()),
                _ => return Err("a `defined?` of other than an instance variable".into()),
            },
            ExprNode::Assign { target, value } => {
                let v = self.value(value, frame)?;
                match target {
                    LValue::Var { name, .. } if name.as_str().starts_with("@@") => self.cvar_set(frame.scope.as_deref(), name.as_str(), v.clone()),
                    LValue::Var { name, .. } => {
                        frame.locals.insert(name.clone(), v.clone());
                    }
                    LValue::Ivar { name } => self.set_ivar(&frame.this, name.as_str(), v.clone())?,
                    _ => return Err("an assignment to other than a local or an instance variable".into()),
                }
                v
            }
            ExprNode::OpAssign { target, op: OpAssignOp::OrOr, value } => {
                let current = match target {
                    LValue::Var { name, .. } => frame.locals.get(name).cloned(),
                    LValue::Ivar { name } => {
                        self.note_read(&frame.this, name.as_str().to_string())?;
                        self.ivar(&frame.this, name.as_str())
                    }
                    LValue::Index { recv, index } => {
                        // `URL_HELPERS[key] ||= []`: the element a slot holds, written back through it.
                        let at = Expr::new(Span::synthetic(), ExprNode::Send { recv: Some(recv.clone()), method: Symbol::from("[]"), args: vec![index.clone()], block: None, parenthesized: true });
                        let slot = self.held_by(&at, frame).ok_or("an `||=` on an element of what it cannot hold")?;
                        let current = self.slot_get(&slot, frame)?.unwrap_or(Val::Nil);
                        if current.truthy()? {
                            return Ok(Flow::Value(current));
                        }
                        let v = self.value(value, frame)?;
                        self.slot_set(&slot, v.clone(), frame)?;
                        return Ok(Flow::Value(v));
                    }
                    _ => return Err("an `||=` on other than a local or an instance variable".into()),
                };
                let set = match &current {
                    Some(v) => v.truthy()?,
                    None => false,
                };
                match current {
                    Some(v) if set => v,
                    _ => {
                        let v = self.value(value, frame)?;
                        match target {
                            LValue::Var { name, .. } => {
                                frame.locals.insert(name.clone(), v.clone());
                            }
                            LValue::Ivar { name } => self.set_ivar(&frame.this, name.as_str(), v.clone())?,
                            _ => unreachable!(),
                        }
                        v
                    }
                }
            }
            ExprNode::Lambda { params, rest_param, extra_params, body, .. } if extra_params.is_empty() => {
                // The block's own `self` is the frame's; `instance_exec` binds another.
                Val::Proc { params: params.clone(), rest: rest_param.clone(), body: body.clone(), scope: frame.scope.clone(), this: Box::new(frame.this.clone()), locals: std::rc::Rc::new(frame.locals.clone()), block: frame.block.clone().map(Box::new) }
            }
            ExprNode::Send { recv, method, args, block, .. } => self.send(recv.as_ref(), method.as_str(), args, block.as_ref(), frame)?,
            ExprNode::Super { args } => {
                let Some((owner, name, side, given, blk)) = self.calls.last().cloned() else { return Err("a `super` outside a method".into()) };
                let argv = match args {
                    None => given,
                    Some(args) => args.iter().map(|a| self.value(a, frame)).collect::<Res<_>>()?,
                };
                let class = match (&frame.this, side) {
                    (Val::Obj(i), MethodReceiver::Instance) => self.heap[*i].class.clone(),
                    (Val::Class(c), MethodReceiver::Class) => c.clone(),
                    _ => return Err("a `super` on a receiver it does not model".into()),
                };
                let order = self.mro(&class, side);
                let after = order.iter().position(|c| *c == owner).map(|i| i + 1).unwrap_or(order.len());
                let next = order[after..].iter().find_map(|c| {
                    self.classes
                        .iter()
                        .filter(|d| d.name.0.as_str() == c)
                        .flat_map(|d| d.methods.iter())
                        .filter(|m| m.name.as_str() == name && m.receiver == side)
                        .last()
                        .map(|m| (m.clone(), m.enclosing_class.as_ref().map(|s| s.as_str().to_string()).unwrap_or_else(|| c.clone())))
                });
                match next {
                    Some((def, scope)) => {
                        let this = frame.this.clone();
                        self.invoke(&def, scope, this, argv, blk)?
                    }
                    // `Object#initialize` and the like: nothing of the program's runs.
                    None if name == "initialize" => Val::Nil,
                    None => return Err(format!("a `super` past the program's own `{name}`")),
                }
            }
            // Not a rescue it can take: nothing here raises an exception it models, so one that would stops the evaluation.
            ExprNode::BeginRescue { body, else_branch, ensure, .. } => {
                let mut v = self.value(body, frame)?;
                if let Some(e) = else_branch {
                    v = self.value(e, frame)?;
                }
                if let Some(e) = ensure {
                    self.value(e, frame)?;
                }
                v
            }
            ExprNode::Case { scrutinee, arms } => {
                let v = self.value(scrutinee, frame)?;
                for arm in arms {
                    let hit = match &arm.pattern {
                        Pattern::Wildcard => true,
                        Pattern::Lit { value } => {
                            let p = literal(value)?;
                            if !(compares(&p) && compares(&v)) {
                                return Err("a `when` on a value it does not compare".into());
                            }
                            same(&p, &v)
                        }
                        Pattern::Expr { expr } => {
                            let p = self.value(expr, frame)?;
                            self.case_eq(&p, &v, frame)?
                        }
                        _ => return Err("a `when` pattern it does not model".into()),
                    };
                    let hit = hit && match &arm.guard {
                        Some(g) => self.value(g, frame)?.truthy()?,
                        None => true,
                    };
                    if hit {
                        return self.eval(&arm.body, frame);
                    }
                }
                Val::Nil
            }
            ExprNode::MultiAssign { targets, value } => {
                let v = self.value(value, frame)?;
                let items = match &v {
                    Val::Array(items) => items.clone(),
                    Val::Unknown => return Err("a multiple assignment from a value it does not model".into()),
                    other => vec![other.clone()],
                };
                for (i, target) in targets.iter().enumerate() {
                    let item = items.get(i).cloned().unwrap_or(Val::Nil);
                    match target {
                        LValue::Var { name, .. } if !name.as_str().starts_with("@@") => {
                            frame.locals.insert(name.clone(), item);
                        }
                        LValue::Ivar { name } => self.set_ivar(&frame.this, name.as_str(), item)?,
                        _ => return Err("a multiple assignment to other than locals and instance variables".into()),
                    }
                }
                v
            }
            ExprNode::Yield { args } => {
                let proc_ = frame.block.clone().ok_or("a `yield` without a block")?;
                let argv = args.iter().map(|a| self.value(a, frame)).collect::<Res<Vec<_>>>()?;
                self.call_proc(&proc_, argv, frame)?
            }
            _ => return Err(format!("it cannot run {}", e.node.kind_str())),
        };
        Ok(Flow::Value(v))
    }

    fn send(&mut self, recv: Option<&Expr>, method: &str, args: &[Expr], block: Option<&Expr>, frame: &mut Frame) -> Res<Val> {
        let this = match recv {
            Some(r) => self.value(r, frame)?,
            None => frame.this.clone(),
        };
        let mut argv = Vec::new();
        for a in args {
            match &*a.node {
                ExprNode::Splat { value } => match self.value(value, frame)? {
                    Val::Array(items) => argv.extend(items),
                    _ => return Err("a splat of other than an Array".into()),
                },
                // `resource :registration, **options do` (devise 5): the keywords a Hash holds.
                ExprNode::KeywordSplat { value } => match self.value(value, frame)? {
                    hash @ Val::Hash(_) => argv.push(hash),
                    Val::Nil => {}
                    _ => return Err("a keyword splat of other than a Hash".into()),
                },
                _ => argv.push(self.value(a, frame)?),
            }
        }
        let blk = match block {
            None => None,
            Some(b) => Some(self.value(b, frame)?).filter(|v| !matches!(v, Val::Nil)),
        };
        self.dispatch(recv, this, method, argv, blk, frame)
    }

    /// A call whose receiver and arguments are values; `recv` is the receiver as written, when an in-place change writes back through it.
    fn dispatch(&mut self, recv: Option<&Expr>, this: Val, method: &str, mut argv: Vec<Val>, blk: Option<Val>, frame: &mut Frame) -> Res<Val> {
        if let Val::Merger { target, options } = &this {
            match argv.last_mut() {
                Some(Val::Hash(last)) => *last = deep_merge(options, last),
                _ => argv.push(Val::Hash(options.clone())),
            }
            return self.dispatch(None, (**target).clone(), method, argv, blk, frame);
        }
        // Not routes: Kernel's own methods, written receiverless in a method that runs on the mapper.
        let kernel = recv.is_none() && matches!(method, "lambda" | "proc" | "block_given?" | "raise" | "fail" | "Array" | "Hash" | "String" | "Integer" | "Float" | "format" | "sprintf" | "loop" | "catch" | "throw" | "binding" | "caller" | "freeze" | "frozen?" | "respond_to?" | "is_a?" | "kind_of?" | "instance_variable_get" | "instance_variable_set" | "instance_variable_defined?" | "send" | "public_send" | "__send__" | "tap" | "then" | "with_options");
        if matches!(this, Val::Router) {
            // A method of the program's on the mapper runs, as devise's `devise_scope` and `send("devise_#{mod}", …)` do.
            let (name, given) = match (method, argv.first()) {
                ("send" | "public_send" | "__send__", Some(Val::Sym(n) | Val::Str(n))) => (n.clone(), argv[1..].to_vec()),
                _ => (method.to_string(), argv.clone()),
            };
            if let Some((def, scope)) = self.lookup(&this, &name) {
                let owner = def.enclosing_class.as_ref().map(|s| s.as_str().to_string()).unwrap_or_else(|| scope.clone());
                let Some(why) = super::gem_hooks::omits_on_failure(&self.known, &owner, &name) else {
                    return self.invoke(&def, scope, this, given, blk);
                };
                // Not half its routes, nor the `@scope` it changed on the way: what it drew is the call's or none of it.
                let drawn = self.drawn.last().map_or(0, Vec::len);
                let ivars = self.router.ivars.clone();
                // Nor when it wrote them: a restored value keeps the uncertainty it had.
                let written: Vec<_> = self.written.iter().filter(|((h, _), _)| matches!(h, Holder::Router)).map(|(k, t)| (k.clone(), *t)).collect();
                return match self.invoke(&def, scope, this, given, blk) {
                    Ok(v) => Ok(v),
                    Err(err) => {
                        if let Some(last) = self.drawn.last_mut() {
                            last.truncate(drawn);
                        }
                        self.router.ivars = ivars;
                        self.written.retain(|(h, _), _| !matches!(h, Holder::Router));
                        self.written.extend(written);
                        self.omitted.push(format!("{why} ({err})"));
                        Ok(Val::Nil)
                    }
                };
            }
            if let Some(hook) = super::gem_hooks::router_call(&self.known, self.calls.last().map(|c| (c.0.as_str(), c.1.as_str())), method, &argv) {
                return match hook {
                    super::gem_hooks::RouterCall::RunBlock => match &blk {
                        Some(b) => self.call_proc(b, Vec::new(), frame),
                        None => Ok(Val::Nil),
                    },
                };
            }
        }
        // Not a route: `Object#with_options` on the mapper hands its routes the options.
        if matches!(this, Val::Router) && !kernel && !(method == "with_options" && matches!(argv.as_slice(), [Val::Hash(_)])) {
            return self.draw(method, argv, blk, frame);
        }
        // `@mapping.controllers.merge!(names)`: the Hash the reader answers is the ivar's, changed in place.
        if matches!(method, "merge!" | "update" | "[]=" | "delete")
            && !(method == "delete" && !matches!(this, Val::Hash(_) | Val::Defaulted { .. }))
            && let Some(r) = recv
            && let Some(slot) = self.held_by(r, frame)
            && !argv.iter().any(|a| matches!(a, Val::Unknown))
        {
            let (mut entries, default) = match self.slot_get(&slot, frame)? {
                Some(Val::Hash(entries)) => (entries, None),
                Some(Val::Defaulted { entries, default }) => (entries, Some(default)),
                _ => return Err(format!("`{method}` on other than a Hash")),
            };
            if let ("delete", [key]) = (method, argv.as_slice()) {
                if blk.is_some() {
                    return Err("a `delete` with a block".into());
                }
                let removed = entries.iter().position(|(k, _)| same(k, key)).map(|i| entries.remove(i).1);
                let kept = match default {
                    Some(default) => Val::Defaulted { entries, default },
                    None => Val::Hash(entries),
                };
                self.slot_set(&slot, kept, frame)?;
                return Ok(removed.unwrap_or(Val::Nil));
            }
            let pairs: Vec<(Val, Val)> = match (method, argv.as_slice()) {
                ("[]=", [k, v]) => vec![(k.clone(), v.clone())],
                (_, [Val::Hash(more)]) => more.clone(),
                _ => return Err(format!("a `{method}` it does not model")),
            };
            for (k, v) in pairs {
                match entries.iter_mut().find(|(ek, _)| same(ek, &k)) {
                    Some(slot) => slot.1 = v,
                    None => entries.push((k, v)),
                }
            }
            let grown = match default {
                Some(default) => Val::Defaulted { entries, default },
                None => Val::Hash(entries),
            };
            self.slot_set(&slot, grown.clone(), frame)?;
            return Ok(if method == "[]=" { argv.last().cloned().unwrap_or(Val::Nil) } else { grown });
        }
        // `@scopes.push(*names)`: the Array the ivar or local holds, grown in place.
        if matches!(method, "push" | "<<" | "concat" | "append" | "uniq!" | "compact!" | "flatten!" | "insert")
            && let Val::Array(items) = &this
            && let Some(r) = recv
            && let Some(slot) = self.held_by(r, frame)
            && !argv.iter().any(|a| matches!(a, Val::Unknown))
        {
            let mut grown = items.clone();
            let changed = match (method, argv.as_slice()) {
                ("concat", [Val::Array(more)]) => {
                    grown.extend(more.iter().cloned());
                    true
                }
                ("concat", _) => return Err("a `concat` of other than an Array".into()),
                ("insert", [Val::Int(at), values @ ..]) => {
                    // Ruby counts a negative index from past the end: `insert(-1, x)` appends.
                    let len = grown.len() as i64;
                    let at = if *at < 0 { len + at + 1 } else { *at };
                    let at = usize::try_from(at).map_err(|_| "an `insert` before the start".to_string())?;
                    if at > grown.len() {
                        grown.resize(at, Val::Nil);
                    }
                    for (n, v) in values.iter().enumerate() {
                        grown.insert(at + n, v.clone());
                    }
                    true
                }
                ("insert", _) => return Err("an `insert` at other than an Integer".into()),
                ("uniq!", []) => {
                    let before = grown.len();
                    let mut kept: Vec<Val> = Vec::new();
                    for v in grown {
                        if !kept.iter().any(|k| same(k, &v)) {
                            kept.push(v);
                        }
                    }
                    grown = kept;
                    grown.len() != before
                }
                ("compact!", []) => {
                    let before = grown.len();
                    grown.retain(|v| !matches!(v, Val::Nil));
                    grown.len() != before
                }
                ("flatten!", []) => {
                    let mut flat = Vec::new();
                    let mut nested = false;
                    for v in grown {
                        match v {
                            Val::Array(inner) => {
                                nested = true;
                                flat.extend(inner);
                            }
                            v => flat.push(v),
                        }
                    }
                    grown = flat;
                    nested
                }
                (_, []) if method.ends_with('!') => return Err(format!("`{method}` with arguments")),
                _ => {
                    grown.extend(argv.iter().cloned());
                    true
                }
            };
            let grown = Val::Array(grown);
            self.slot_set(&slot, grown.clone(), frame)?;
            // Ruby's bang forms answer nil when nothing changed.
            return Ok(if method.ends_with('!') && !changed { Val::Nil } else { grown });
        }
        // `options = resources.extract_options!`, `resources.map!(&:to_sym)`: the Array a local or ivar holds, changed in place.
        if matches!(method, "extract_options!" | "map!" | "collect!")
            && let Val::Array(items) = &this
            && let Some(r) = recv
            && matches!(&*r.node, ExprNode::Ivar { .. } | ExprNode::Var { .. })
            && argv.is_empty()
        {
            let mut kept = items.clone();
            let answer = if method == "extract_options!" {
                match kept.last() {
                    Some(Val::Hash(_)) => kept.pop().unwrap_or(Val::Nil),
                    _ => Val::Hash(Vec::new()),
                }
            } else {
                let Some(proc_) = blk.clone() else { return Err(format!("`{method}` without a block")) };
                let mut mapped = Vec::new();
                for item in kept {
                    mapped.push(self.call_proc(&proc_, vec![item], frame)?);
                }
                kept = mapped;
                Val::Array(kept.clone())
            };
            match &*r.node {
                ExprNode::Ivar { name } => self.set_ivar(&frame.this, name.as_str(), Val::Array(kept))?,
                ExprNode::Var { name, .. } => {
                    frame.locals.insert(name.clone(), Val::Array(kept));
                }
                _ => unreachable!(),
            }
            return Ok(answer);
        }
        // A method the program defines answers before anything built in.
        if let Some((def, scope)) = self.lookup(&this, method) {
            return self.invoke(&def, scope, this, argv, blk);
        }
        // Not only a model's: a subclass with no table of its own is a library class, and what `devise` gave its superclass it has too.
        if let Val::Class(c) = &this
            && self.facts.answers_for(c)
        {
            let all = match self.constant(&[Symbol::from("Devise"), Symbol::from("ALL")], None) {
                Ok(Val::Array(items)) => Some(items),
                _ => None,
            };
            let facts = std::mem::take(&mut self.facts);
            let mut config = |name: &str| self.dispatch(None, Val::Class("Devise".into()), name, Vec::new(), None, frame).ok().filter(|v| !matches!(v, Val::Unknown));
            let answer = super::gem_hooks::class_answer(&facts, c, method, &argv, all.as_deref(), &mut config);
            self.facts = facts;
            if let Some(v) = answer {
                return v.map_err(|why| format!("{STOP}{why}"));
            }
        }
        // `mattr_reader :mapping` answers on instances too, as Rails defines it, though the library walk writes only the class's.
        if let Val::Obj(i) = &this
            && argv.is_empty()
            && let class = Val::Class(self.heap[*i].class.clone())
            && let Some((def, scope)) = self.lookup(&class, method)
            && cvar_reader(&def).is_some()
        {
            return self.invoke(&def, scope, class, argv, blk);
        }
        // Not answered from a value it does not model: `nil?` or `==` on one would guess.
        let stores = method == "instance_variable_set" && matches!(argv.first(), Some(Val::Sym(_)));
        if matches!(this, Val::Unknown) || (!stores && argv.iter().any(|a| matches!(a, Val::Unknown))) {
            return Err(format!("`{method}` on or with a value it does not model"));
        }
        if let (Val::Class(c), Some((subject, _, answers))) = (&this, &self.subject)
            && c == subject
            && argv.is_empty()
            && let Some(v) = answers.get(method)
        {
            return Ok(v.clone());
        }
        if let Val::Defaulted { entries, default } = &this {
            match (method, argv.as_slice()) {
                ("[]", [key]) => {
                    return match entries.iter().find(|(k, _)| same(k, key)) {
                        Some((_, v)) => Ok(v.clone()),
                        None => match &**default {
                            proc_ @ Val::Proc { .. } => self.call_proc(proc_, vec![this.clone(), key.clone()], frame),
                            value => Ok(value.clone()),
                        },
                    };
                }
                ("merge", [Val::Hash(_)]) => {
                    let Val::Hash(merged) = self.dispatch(None, Val::Hash(entries.clone()), method, argv.clone(), None, frame)? else { unreachable!() };
                    return Ok(Val::Defaulted { entries: merged, default: default.clone() });
                }
                // Reads that do not consult the default.
                ("each" | "each_pair" | "keys" | "values" | "key?" | "has_key?" | "include?" | "member?" | "empty?" | "size" | "length" | "fetch" | "slice" | "blank?" | "present?", _) => {
                    return self.dispatch(None, Val::Hash(entries.clone()), method, argv, blk, frame);
                }
                _ => return Err(format!("`{method}` on a Hash with a default")),
            }
        }
        match (method, &this, argv.as_slice()) {
            ("block_given?", _, []) if recv.is_none() => Ok(Val::Bool(frame.block.is_some())),
            ("new", Val::Class(c), []) if c.trim_start_matches("::") == "Hash" => Ok(match blk {
                Some(proc_) => Val::Defaulted { entries: Vec::new(), default: Box::new(proc_) },
                None => Val::Hash(Vec::new()),
            }),
            ("new", Val::Class(c), [value]) if c.trim_start_matches("::") == "Hash" && blk.is_none() => {
                Ok(Val::Defaulted { entries: Vec::new(), default: Box::new(value.clone()) })
            }
            ("singularize", Val::Str(t), []) => Ok(Val::Str(crate::naming::singularize(t))),
            // Registers a file to load when the constant is first read; nothing the evaluation holds changes.
            ("autoload", Val::Class(_), [Val::Sym(_) | Val::Str(_), Val::Str(_)]) => Ok(Val::Nil),
            ("camelize" | "underscore" | "pluralize" | "singularize" | "classify", Val::Class(c), [Val::Str(t)]) if c.trim_start_matches("::") == "ActiveSupport::Inflector" => Ok(Val::Str(match method {
                "camelize" => crate::naming::camelize_path(t),
                "underscore" => crate::naming::underscore(t),
                "pluralize" => crate::naming::pluralize_snake(t),
                "singularize" => crate::naming::singularize(t),
                _ => crate::naming::classify_path(t),
            })),
            ("join", Val::Array(items), sep) if matches!(sep, [] | [Val::Str(_)]) && items.iter().all(|v| matches!(v, Val::Str(_) | Val::Sym(_) | Val::Int(_))) => {
                let sep = match sep { [Val::Str(s)] => s.as_str(), _ => "" };
                Ok(Val::Str(items.iter().map(|v| self.text(v)).collect::<Res<Vec<_>>>()?.join(sep)))
            }
            ("squeeze", Val::Str(t), [Val::Str(set)]) if set.chars().count() == 1 => {
                let c = set.chars().next().unwrap_or_default();
                let mut out = String::new();
                for ch in t.chars() {
                    if !(ch == c && out.ends_with(c)) {
                        out.push(ch);
                    }
                }
                Ok(Val::Str(out))
            }
            ("flatten", Val::Array(items), []) => {
                fn flat(items: &[Val], out: &mut Vec<Val>) {
                    for v in items {
                        match v {
                            Val::Array(inner) => flat(inner, out),
                            v => out.push(v.clone()),
                        }
                    }
                }
                let mut out = Vec::new();
                flat(items, &mut out);
                Ok(Val::Array(out))
            }
            // Devise's `Mapping.add_module`: a string of `def`s defines those methods on the class (`omniauthable?`), and runs nothing else.
            ("class_eval" | "module_eval", Val::Class(c), [Val::Str(code), ..]) if blk.is_none() && self.known.contains(c) && only_defs(code) => {
                let parsed = super::prism::parse_silent(code.as_bytes());
                let node = parsed.node();
                let program = node.as_program_node().ok_or("a `class_eval` string that does not parse")?;
                let owner = ClassId(Symbol::from(c.as_str()));
                let mut defined = Vec::new();
                for statement in program.statements().body().iter() {
                    let def = statement.as_def_node().ok_or("a `class_eval` string of other than defs")?;
                    let (read, _) = super::prism::scope(|| super::library_class::ingest_library_method(&def, &owner, "<class_eval>"));
                    defined.push(read.map_err(|_| "a `class_eval` def that does not ingest".to_string())?);
                }
                self.evaled.entry(c.clone()).or_default().extend(defined);
                Ok(Val::Nil)
            }
            // Not a replacement holding a backslash: Ruby reads `\1` and `\&` there as back-references even for a string pattern.
            ("gsub" | "sub", Val::Str(t), [Val::Str(from), Val::Str(to)]) if blk.is_none() && !from.is_empty() && !to.contains('\\') => {
                Ok(Val::Str(if method == "gsub" { t.replace(from.as_str(), to) } else { t.replacen(from.as_str(), to, 1) }))
            }
            ("tr", Val::Str(t), [Val::Str(from), Val::Str(to)]) if from.chars().count() == to.chars().count() && !from.contains(['-', '^', '\\']) => {
                let map: Vec<(char, char)> = from.chars().zip(to.chars()).collect();
                Ok(Val::Str(t.chars().map(|c| map.iter().find(|(f, _)| *f == c).map(|(_, t)| *t).unwrap_or(c)).collect()))
            }
            ("Array", _, [v]) if recv.is_none() => Ok(match v {
                Val::Nil => Val::Array(Vec::new()),
                Val::Array(_) => v.clone(),
                Val::Hash(entries) => Val::Array(entries.iter().map(|(k, v)| Val::Array(vec![k.clone(), v.clone()])).collect()),
                Val::Unknown | Val::Obj(_) | Val::Defaulted { .. } => return Err("`Array()` of a value it does not model".into()),
                other => Val::Array(vec![other.clone()]),
            }),
            ("+", Val::Array(items), [Val::Array(other)]) => Ok(Val::Array(items.iter().chain(other).cloned().collect())),
            ("&" | "-" | "|", Val::Array(items), [Val::Array(other)]) if items.iter().chain(other).all(compares) => {
                let mut out: Vec<Val> = Vec::new();
                let keep = |v: &Val, out: &Vec<Val>| !out.iter().any(|o| same(o, v));
                match method {
                    "&" => items.iter().filter(|v| other.iter().any(|o| same(o, v))).for_each(|v| if keep(v, &out) { out.push(v.clone()) }),
                    "|" => items.iter().chain(other).for_each(|v| if keep(v, &out) { out.push(v.clone()) }),
                    _ => out = items.iter().filter(|v| !other.iter().any(|o| same(o, v))).cloned().collect(),
                }
                Ok(Val::Array(out))
            }
            // Rails 7 dropped these from `ActiveSupport::Dependencies`; a gem that asks takes its other branch.
            ("respond_to?", Val::Class(c), [Val::Sym(name), ..])
                if c.trim_start_matches("::") == "ActiveSupport::Dependencies" && matches!(name.as_str(), "constantize" | "reference" | "safe_constantize") =>
            {
                Ok(Val::Bool(false))
            }
            ("is_a?" | "kind_of?", Val::Class(_), [Val::Class(c)])
                if matches!(c.trim_start_matches("::"), "String" | "Symbol" | "Integer" | "Float" | "Numeric" | "Array" | "Hash" | "NilClass" | "TrueClass" | "FalseClass" | "Proc" | "Regexp") =>
            {
                // A class or module is an instance of none of the value classes; a module it extends is not one of them.
                Ok(Val::Bool(false))
            }
            ("with_options", _, [Val::Hash(options)]) if !matches!(this, Val::Unknown) => {
                let merger = Val::Merger { target: Box::new(this.clone()), options: options.clone() };
                match &blk {
                    // A block that takes no argument runs with the merger as its `self`, as Rails' `instance_eval` does.
                    Some(Val::Proc { params, rest: None, body, scope, locals, .. }) if params.is_empty() => {
                        let mut inner = Frame { this: merger.clone(), locals: (**locals).clone(), block: None, scope: scope.clone() };
                        self.value(body, &mut inner)?;
                    }
                    Some(proc_) => {
                        self.call_proc(proc_, vec![merger.clone()], frame)?;
                    }
                    None => {}
                }
                Ok(merger)
            }
            ("pluralize", Val::Str(t), []) => Ok(Val::Str(crate::naming::pluralize(t))),
            ("underscore", Val::Str(t), []) => Ok(Val::Str(crate::naming::underscore(t))),
            ("lambda" | "proc", _, []) if recv.is_none() => blk.ok_or_else(|| format!("`{method}` without a block")),
            ("new", Val::Class(name), _) if self.known.contains(name) => {
                self.heap.push(Object { class: name.clone(), ivars: HashMap::default() });
                let obj = Val::Obj(self.heap.len() - 1);
                if let Some((def, scope)) = self.lookup(&obj, "initialize") {
                    self.invoke(&def, scope, obj.clone(), argv, blk)?;
                }
                Ok(obj)
            }
            ("instance_eval" | "instance_exec", _, _) => {
                let Some(Val::Proc { params, rest, body, scope, locals, .. }) = blk else { return Err(format!("`{method}` without a block")) };
                let mut inner = Frame { this, locals: (*locals).clone(), block: None, scope };
                bind(&mut inner.locals, &params, rest.as_ref(), argv);
                self.value(&body, &mut inner)
            }
            ("call", Val::Proc { .. }, _) => self.call_proc(&this, argv, frame),
            ("instance_variable_set", Val::Obj(_), [Val::Sym(name), value]) => {
                let bare = name.strip_prefix('@').ok_or("an ivar name without `@`")?;
                self.set_ivar(&this, bare, value.clone())?;
                Ok(value.clone())
            }
            ("instance_variable_get", _, [Val::Sym(name)]) => {
                let bare = name.strip_prefix('@').ok_or("an ivar name without `@`")?;
                self.note_read(&this, bare.to_string())?;
                Ok(self.ivar(&this, bare).unwrap_or(Val::Nil))
            }
            ("public_send" | "send" | "__send__", _, [Val::Sym(name) | Val::Str(name), ..]) => {
                let rest = argv[1..].to_vec();
                match self.lookup(&this, name) {
                    Some((def, scope)) => self.invoke(&def, scope, this, rest, blk),
                    // A built-in by its name (`send(:autoload, …)`), not a `send` again.
                    None if !matches!(name.as_str(), "send" | "public_send" | "__send__") => {
                        let name = name.clone();
                        self.dispatch(None, this, &name, rest, blk, frame)
                    }
                    None => Err(format!("no `{name}` to send")),
                }
            }
            ("respond_to?", _, [Val::Sym(name), ..]) => Ok(Val::Bool(
                self.lookup(&this, name).is_some() || (matches!(this, Val::Proc { .. }) && name == "call"),
            )),
            ("constantize", Val::Str(name), []) => Ok(Val::Class(name.trim_start_matches("::").to_string())),
            ("to_s", _, []) => Ok(Val::Str(self.text(&this)?)),
            ("to_sym", Val::Str(s) | Val::Sym(s), []) => Ok(Val::Sym(s.clone())),
            ("nil?", _, []) => Ok(Val::Bool(matches!(this, Val::Nil))),
            ("!", _, []) => Ok(Val::Bool(!this.truthy()?)),
            ("first", Val::Array(items), []) => Ok(items.first().cloned().unwrap_or(Val::Nil)),
            ("name", Val::Class(c), []) => Ok(Val::Str(c.clone())),
            ("is_a?" | "kind_of?", _, [Val::Class(c)]) => {
                let c = c.trim_start_matches("::");
                let answer = match &this {
                    Val::Obj(i) => {
                        let class = self.heap[*i].class.clone();
                        self.ancestry(&class).iter().any(|a| a == c) || matches!(c, "Object" | "BasicObject")
                    }
                    Val::Nil => c == "NilClass",
                    Val::Bool(true) => c == "TrueClass",
                    Val::Bool(false) => c == "FalseClass",
                    Val::Str(_) => matches!(c, "String" | "Comparable"),
                    Val::Sym(_) => matches!(c, "Symbol" | "Comparable"),
                    Val::Int(_) => matches!(c, "Integer" | "Numeric" | "Comparable"),
                    Val::Array(_) => matches!(c, "Array" | "Enumerable"),
                    Val::Hash(_) => matches!(c, "Hash" | "Enumerable"),
                    Val::Proc { .. } => c == "Proc",
                    _ => return Err("an `is_a?` on a value it does not model".into()),
                } || matches!(c, "Object" | "BasicObject" | "Kernel");
                Ok(Val::Bool(answer))
            }
            ("blank?" | "present?" | "presence", _, []) => {
                let blank = match &this {
                    Val::Nil | Val::Bool(false) => true,
                    Val::Str(t) => t.trim().is_empty(),
                    Val::Array(items) => items.is_empty(),
                    Val::Hash(entries) => entries.is_empty(),
                    Val::Bool(true) | Val::Int(_) | Val::Sym(_) | Val::Class(_) | Val::Proc { .. } | Val::Regex(_) => false,
                    Val::Obj(_) | Val::Unknown | Val::Router | Val::Merger { .. } | Val::Defaulted { .. } => return Err(format!("`{method}` on an object")),
                };
                Ok(match method {
                    "blank?" => Val::Bool(blank),
                    "present?" => Val::Bool(!blank),
                    _ => if blank { Val::Nil } else { this.clone() },
                })
            }
            ("classify", Val::Str(t), []) => Ok(Val::Str(crate::naming::classify_path(t))),
            ("camelize", Val::Str(t), []) => Ok(Val::Str(crate::naming::camelize_path(t))),
            ("=~" | "match?", Val::Str(t), [Val::Regex(p)]) | ("=~" | "match?", Val::Regex(p), [Val::Str(t)]) => {
                let re = regex::Regex::new(p).map_err(|_| "a pattern Rust's regex does not read".to_string())?;
                let at = re.find(t).map(|m| t[..m.start()].chars().count() as i64);
                Ok(match (method, at) {
                    ("match?", at) => Val::Bool(at.is_some()),
                    (_, Some(i)) => Val::Int(i),
                    (_, None) => Val::Nil,
                })
            }
            ("include?", Val::Array(items), [x]) => Ok(Val::Bool(items.iter().any(|i| same(i, x)))),
            ("exclude?", Val::Array(items), [x]) => Ok(Val::Bool(!items.iter().any(|i| same(i, x)))),
            ("empty?", Val::Array(items), []) => Ok(Val::Bool(items.is_empty())),
            ("empty?", Val::Str(t), []) => Ok(Val::Bool(t.is_empty())),
            ("freeze" | "dup", _, []) => Ok(this.clone()),
            ("tap", _, []) => {
                let Some(proc_) = blk.clone() else { return Err("`tap` without a block".into()) };
                self.call_proc(&proc_, vec![this.clone()], frame)?;
                Ok(this.clone())
            }
            ("all?" | "any?" | "none?" | "each" | "map" | "select" | "reject", Val::Array(items), []) => {
                let Some(proc_) = blk.clone() else { return Err(format!("`{method}` without a block")) };
                let mut answers = Vec::new();
                for item in items.clone() {
                    answers.push(self.call_proc(&proc_, vec![item], frame)?);
                }
                let truths: Vec<bool> = answers.iter().map(Val::truthy).collect::<Res<_>>()?;
                Ok(match method {
                    "all?" => Val::Bool(truths.iter().all(|t| *t)),
                    "any?" => Val::Bool(truths.iter().any(|t| *t)),
                    "none?" => Val::Bool(!truths.iter().any(|t| *t)),
                    "each" => this.clone(),
                    "map" => Val::Array(answers),
                    "select" => Val::Array(items.iter().zip(&truths).filter(|(_, t)| **t).map(|(i, _)| i.clone()).collect()),
                    _ => Val::Array(items.iter().zip(&truths).filter(|(_, t)| !**t).map(|(i, _)| i.clone()).collect()),
                })
            }
            ("merge", Val::Hash(entries), [Val::Hash(more)]) => {
                let mut merged = entries.clone();
                for (k, v) in more {
                    match merged.iter_mut().find(|(ek, _)| same(ek, k)) {
                        Some(slot) => slot.1 = v.clone(),
                        None => merged.push((k.clone(), v.clone())),
                    }
                }
                Ok(Val::Hash(merged))
            }
            ("slice" | "except", Val::Hash(entries), keys) => {
                let named = |k: &Val| keys.iter().any(|n| same(n, k));
                Ok(Val::Hash(entries.iter().filter(|(k, _)| named(k) == (method == "slice")).cloned().collect()))
            }
            ("keys", Val::Hash(entries), []) => Ok(Val::Array(entries.iter().map(|(k, _)| k.clone()).collect())),
            ("values", Val::Hash(entries), []) => Ok(Val::Array(entries.iter().map(|(_, v)| v.clone()).collect())),
            ("values_at", Val::Hash(entries), keys) => Ok(Val::Array(
                keys.iter().map(|k| entries.iter().find(|(ek, _)| same(ek, k)).map(|(_, v)| v.clone()).unwrap_or(Val::Nil)).collect(),
            )),
            ("key?" | "has_key?" | "include?" | "member?", Val::Hash(entries), [key]) => Ok(Val::Bool(entries.iter().any(|(k, _)| same(k, key)))),
            ("fetch", Val::Hash(entries), [key, rest @ ..]) if rest.len() <= 1 && blk.is_none() => match entries.iter().find(|(k, _)| same(k, key)) {
                Some((_, v)) => Ok(v.clone()),
                None => rest.first().cloned().ok_or_else(|| "a `fetch` of a key the Hash does not hold".into()),
            },
            ("empty?", Val::Hash(entries), []) => Ok(Val::Bool(entries.is_empty())),
            ("each_pair" | "each", Val::Hash(entries), []) => {
                let Some(proc_) = blk.clone() else { return Err(format!("`{method}` without a block")) };
                for (k, v) in entries.clone() {
                    self.call_proc(&proc_, vec![k, v], frame)?;
                }
                Ok(this.clone())
            }
            ("compact", Val::Array(items), []) => Ok(Val::Array(items.iter().filter(|v| !matches!(v, Val::Nil)).cloned().collect())),
            ("uniq", Val::Array(items), []) => {
                let mut kept: Vec<Val> = Vec::new();
                for v in items {
                    if !kept.iter().any(|k| same(k, v)) {
                        kept.push(v.clone());
                    }
                }
                Ok(Val::Array(kept))
            }
            ("last", Val::Array(items), []) => Ok(items.last().cloned().unwrap_or(Val::Nil)),
            ("size" | "length" | "count", Val::Array(items), []) if blk.is_none() => Ok(Val::Int(items.len() as i64)),
            ("size" | "length" | "count", Val::Hash(entries), []) if blk.is_none() => Ok(Val::Int(entries.len() as i64)),
            ("[]", Val::Hash(entries), [key]) => Ok(entries.iter().find(|(k, _)| same(k, key)).map(|(_, v)| v.clone()).unwrap_or(Val::Nil)),
            ("[]", Val::Array(items), [Val::Int(i)]) => {
                let at = if *i < 0 { i64::try_from(items.len()).ok().and_then(|n| usize::try_from(n + i).ok()) } else { usize::try_from(*i).ok() };
                Ok(at.and_then(|i| items.get(i)).cloned().unwrap_or(Val::Nil))
            }
            ("==" | "!=", _, [other]) if !(compares(&this) && compares(other)) => Err(format!("`{method}` on a value it does not compare")),
            ("==", _, [other]) => Ok(Val::Bool(same(&this, other))),
            ("!=", _, [other]) => Ok(Val::Bool(!same(&this, other))),
            ("<" | ">" | "<=" | ">=", Val::Int(a), [Val::Int(b)]) => Ok(Val::Bool(match method {
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                _ => a >= b,
            })),
            ("+" | "-" | "*", Val::Int(a), [Val::Int(b)]) => {
                let v = match method {
                    "+" => a.checked_add(*b),
                    "-" => a.checked_sub(*b),
                    _ => a.checked_mul(*b),
                };
                v.map(Val::Int).ok_or_else(|| "an Integer past 64 bits".into())
            }
            // A built-in on a value that is not the program's: it runs no method of the program and writes none of its ivars.
            _ if blk.is_none()
                && !matches!(this, Val::Obj(_) | Val::Unknown)
                && !matches!(&this, Val::Class(c) if self.known.contains(c))
                && !argv.iter().any(|a| matches!(a, Val::Proc { .. } | Val::Obj(_)))
                && !mutates(method) =>
            {
                Ok(Val::Unknown)
            }
            _ => Err(format!("it cannot call `{method}`")),
        }
    }

    /// A routing call on the mapper, recorded as written with its values folded in, its block drawn as its nested routes.
    fn draw(&mut self, method: &str, argv: Vec<Val>, blk: Option<Val>, frame: &mut Frame) -> Res<Val> {
        let span = Span::synthetic();
        let mut args: Vec<Expr> = Vec::new();
        let last = argv.len().saturating_sub(1);
        for (i, a) in argv.iter().enumerate() {
            let mut e = to_expr(a, span)?;
            if i == last
                && let ExprNode::Hash { kwargs, .. } = &mut *e.node
            {
                *kwargs = true;
            }
            args.push(e);
        }
        let block = match blk {
            None => None,
            Some(proc_ @ Val::Proc { .. }) => {
                self.drawn.push(Vec::new());
                let ran = self.call_proc(&proc_, Vec::new(), frame);
                let inner = self.drawn.pop().unwrap_or_default();
                ran?;
                Some(Expr::new(span, ExprNode::Lambda {
                    params: Vec::new(),
                    rest_param: None,
                    extra_params: Vec::new(),
                    block_param: None,
                    body: Expr::new(span, ExprNode::Seq { exprs: inner }),
                    block_style: crate::expr::BlockStyle::Do,
                }))
            }
            Some(_) => return Err("a routing block that is not a block".into()),
        };
        let mut call = Expr::new(span, ExprNode::Send { recv: None, method: Symbol::from(method), args, block, parenthesized: true });
        // What the method set on `@scope` for itself (devise's `with_devise_exclusive_scope`), as the scope the route stands in; a route in a recorded block stands in its call's.
        if self.drawn.len() == 1 {
            self.note_read(&Val::Router, "scope".into())?;
        }
        if self.drawn.len() == 1
            && let Some(Val::Hash(current)) = self.router.ivars.get("scope").cloned()
        {
            let Val::Hash(base) = self.router.base() else { unreachable!() };
            let read = |entries: &[(Val, Val)], key: &str| entries.iter().find(|(k, _)| matches!(k, Val::Sym(n) if n == key)).map(|(_, v)| v.clone());
            let changed = SCOPE_KEYS.iter().any(|key| {
                let now = read(&current, key).unwrap_or(Val::Nil);
                let was = read(&base, key).unwrap_or(Val::Nil);
                let empty = |v: &Val| matches!(v, Val::Nil) || matches!(v, Val::Hash(e) if e.is_empty());
                !(empty(&now) && empty(&was)) && !same(&now, &was)
            });
            // The whole scope, not what changed: the routes take none of the enclosing one.
            let mut set: Vec<(Val, Val)> = Vec::new();
            for key in SCOPE_KEYS {
                let now = read(&current, key).unwrap_or(Val::Nil);
                if matches!(&now, Val::Nil) || matches!(&now, Val::Hash(e) if e.is_empty()) {
                    continue;
                }
                match (*key, now) {
                    ("options", Val::Hash(entries)) => set.extend(entries),
                    ("path", Val::Str(p)) => set.push((Val::Sym("path".into()), Val::Str(p.trim_start_matches('/').to_string()))),
                    (k, v) => set.push((Val::Sym(k.to_string()), v)),
                }
            }
            if changed {
                if self.router.enclosing.is_none() {
                    return Err("a scope the method sets for itself, inside a resource".into());
                }
                let mut options = to_expr(&Val::Hash(set), span)?;
                if let ExprNode::Hash { kwargs, .. } = &mut *options.node {
                    *kwargs = true;
                }
                call = Expr::new(span, ExprNode::Send {
                    recv: None,
                    method: Symbol::from("scope"),
                    args: vec![options],
                    block: Some(Expr::new(span, ExprNode::Lambda {
                        params: Vec::new(),
                        rest_param: None,
                        extra_params: Vec::new(),
                        block_param: None,
                        body: Expr::new(span, ExprNode::Seq { exprs: vec![call] }),
                        block_style: crate::expr::BlockStyle::Do,
                    })),
                    parenthesized: true,
                });
                call = Expr::new(span, ExprNode::Send {
                    recv: None,
                    method: Symbol::from(super::routes::EXCLUSIVE_SCOPE),
                    args: Vec::new(),
                    block: Some(Expr::new(span, ExprNode::Lambda {
                        params: Vec::new(),
                        rest_param: None,
                        extra_params: Vec::new(),
                        block_param: None,
                        body: Expr::new(span, ExprNode::Seq { exprs: vec![call] }),
                        block_style: crate::expr::BlockStyle::Do,
                    })),
                    parenthesized: true,
                });
            }
        }
        self.drawn.last_mut().ok_or("a route drawn outside the mapper")?.push(call);
        Ok(Val::Nil)
    }

    /// Where a receiver expression's value lives: `@x`, `@@x`, or a reader of the program's that answers one.
    fn held_by(&mut self, recv: &Expr, frame: &mut Frame) -> Option<Slot> {
        match &*recv.node {
            ExprNode::Ivar { name } => Some(Slot::Ivar(frame.this.clone(), name.as_str().to_string())),
            ExprNode::Var { name, .. } if name.as_str().starts_with("@@") => Some(Slot::Cvar(frame.scope.clone(), name.as_str().to_string())),
            ExprNode::Var { name, .. } if frame.locals.contains_key(name) => Some(Slot::Local(name.clone())),
            ExprNode::Const { path } => {
                let scope = frame.scope.clone();
                let key = self.constant_slot(path, scope.as_deref())?;
                // Read once so the slot holds its value.
                self.constant(path, scope.as_deref()).ok()?;
                Some(Slot::Constant(key))
            }
            ExprNode::Send { recv: Some(inner), method, args, block: None, .. } if method.as_str() == "[]" && args.len() == 1 => {
                let holder = self.held_by(inner, frame)?;
                let key = self.value(&args[0], frame).ok()?;
                Some(Slot::Index(Box::new(holder), key))
            }
            ExprNode::Send { recv: inner, method, args, block: None, .. } if args.is_empty() => {
                let mut holder = match inner {
                    Some(r) => self.value(r, frame).ok()?,
                    None => frame.this.clone(),
                };
                let (def, scope) = match self.lookup(&holder, method.as_str()) {
                    Some(found) => found,
                    None => {
                        let Val::Obj(i) = &holder else { return None };
                        holder = Val::Class(self.heap[*i].class.clone());
                        self.lookup(&holder, method.as_str()).filter(|(d, _)| cvar_reader(d).is_some())?
                    }
                };
                if let Some(cvar) = cvar_reader(&def) {
                    return Some(Slot::Cvar(Some(scope), cvar));
                }
                let body = match &*def.body.node {
                    ExprNode::Seq { exprs } if exprs.len() == 1 => &exprs[0],
                    _ => &def.body,
                };
                match &*body.node {
                    ExprNode::Ivar { name } => Some(Slot::Ivar(holder, name.as_str().to_string())),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn slot_get(&mut self, slot: &Slot, frame: &Frame) -> Res<Option<Val>> {
        Ok(match slot {
            Slot::Local(name) => frame.locals.get(name).cloned(),
            Slot::Constant(key) => {
                self.note_constant_read(key)?;
                self.consts.get(key).cloned()
            }
            Slot::Index(holder, key) => match self.slot_get(holder, frame)? {
                Some(Val::Hash(entries)) => Some(entries.iter().find(|(k, _)| same(k, key)).map(|(_, v)| v.clone()).unwrap_or(Val::Nil)),
                Some(Val::Defaulted { entries, .. }) => match entries.iter().find(|(k, _)| same(k, key)) {
                    Some((_, v)) => Some(v.clone()),
                    // Not the default: what it answers is not stored, so a change to it is not the Hash's.
                    None => return Err("an element a Hash's default answers, changed in place".into()),
                },
                Some(Val::Array(items)) => match key {
                    Val::Int(i) => usize::try_from(*i).ok().and_then(|i| items.get(i).cloned()),
                    _ => return Err("an Array element by other than an Integer".into()),
                },
                _ => return Err("an element of other than a Hash or an Array".into()),
            },
            Slot::Ivar(holder, name) => self.ivar(holder, name),
            Slot::Cvar(scope, name) => Some(self.cvar_get(scope.as_deref(), name)?),
        })
    }

    fn slot_set(&mut self, slot: &Slot, v: Val, frame: &mut Frame) -> Res<()> {
        match slot {
            Slot::Local(name) => {
                frame.locals.insert(name.clone(), v);
                Ok(())
            }
            Slot::Constant(key) => {
                self.tick += 1;
                self.written.insert((Holder::Constant(key.clone()), String::new()), self.tick);
                self.consts.insert(key.clone(), v);
                Ok(())
            }
            Slot::Index(holder, key) => {
                let grown = match self.slot_get(holder, frame)? {
                    Some(Val::Hash(mut entries)) => {
                        match entries.iter_mut().find(|(k, _)| same(k, key)) {
                            Some(e) => e.1 = v,
                            None => entries.push((key.clone(), v)),
                        }
                        Val::Hash(entries)
                    }
                    Some(Val::Defaulted { mut entries, default }) => {
                        match entries.iter_mut().find(|(k, _)| same(k, key)) {
                            Some(e) => e.1 = v,
                            None => entries.push((key.clone(), v)),
                        }
                        Val::Defaulted { entries, default }
                    }
                    Some(Val::Array(mut items)) => {
                        let Val::Int(i) = key else { return Err("an Array element by other than an Integer".into()) };
                        let i = usize::try_from(*i).map_err(|_| "a negative Array index written".to_string())?;
                        if i >= items.len() {
                            items.resize(i + 1, Val::Nil);
                        }
                        items[i] = v;
                        Val::Array(items)
                    }
                    _ => return Err("an element of other than a Hash or an Array".into()),
                };
                self.slot_set(holder, grown, frame)
            }
            Slot::Ivar(holder, name) => self.set_ivar(holder, name, v),
            Slot::Cvar(scope, name) => {
                self.cvar_set(scope.as_deref(), name, v);
                Ok(())
            }
        }
    }

    /// The class in `scope`'s chain whose body set `name` first, as Ruby shares a class variable down the hierarchy.
    fn cvar_home(&self, scope: Option<&str>, name: &str) -> Option<String> {
        let scope = scope?;
        self.mro(scope, MethodReceiver::Instance).into_iter().find(|c| {
            self.cvars.contains_key(&(c.clone(), name.to_string()))
                || self.classes.iter().filter(|d| d.name.0.as_str() == c).any(|d| {
                    d.class_ivar_initializers.iter().any(|i| matches!(&*i.node, ExprNode::Assign { target: LValue::Var { name: n, .. }, .. } if n.as_str() == name))
                        || d.unknown_calls.iter().any(|c| self_write(c, name.trim_start_matches("@@")).is_some())
                })
        })
    }

    fn cvar_get(&mut self, scope: Option<&str>, name: &str) -> Res<Val> {
        let home = self.cvar_home(scope, name).ok_or_else(|| format!("`{name}` is not set"))?;
        // Not only ivars: a statement passed over may have set the class variable (devise's `config.omniauth` writes `@@omniauth_configs`).
        self.note_read(&Val::Class(home.clone()), name.to_string())?;
        if let Some(v) = self.cvars.get(&(home.clone(), name.to_string())) {
            return Ok(v.clone());
        }
        let init = self
            .classes
            .iter()
            .filter(|d| d.name.0.as_str() == home)
            .flat_map(|d| d.class_ivar_initializers.iter())
            .find_map(|i| match &*i.node {
                ExprNode::Assign { target: LValue::Var { name: n, .. }, value } if n.as_str() == name => Some(value.clone()),
                _ => None,
            });
        // Not only `@@x = …`: the body can set it through its `mattr_accessor` writer (devise_token_auth's `self.omniauth_prefix = '/omniauth'`), the last such write standing.
        let init = match init {
            Some(init) => init,
            None => {
                let attr = name.trim_start_matches("@@");
                let writes_cvar = self.lookup_in(&home, &format!("{attr}="), MethodReceiver::Class, &mut HashSet::default()).is_some_and(|(def, _)| {
                    let body = match &*def.body.node {
                        ExprNode::Seq { exprs } if exprs.len() == 1 => &exprs[0],
                        _ => &def.body,
                    };
                    matches!((def.params.as_slice(), &*body.node), ([param], ExprNode::Assign { target: LValue::Var { name: n, .. }, value })
                        if n.as_str() == name && matches!(&*value.node, ExprNode::Var { name: v, .. } if *v == param.name))
                });
                self.classes
                    .iter()
                    .filter(|d| writes_cvar && d.name.0.as_str() == home)
                    .flat_map(|d| d.unknown_calls.iter())
                    .filter_map(|c| self_write(c, attr))
                    .last()
                    .ok_or_else(|| format!("`{name}` is not set"))?
            }
        };
        let mut frame = Frame { this: Val::Class(home.clone()), locals: HashMap::default(), block: None, scope: Some(home.clone()) };
        let v = self.value(&init, &mut frame)?;
        self.cvars.insert((home, name.to_string()), v.clone());
        Ok(v)
    }

    fn cvar_set(&mut self, scope: Option<&str>, name: &str, v: Val) {
        let home = self.cvar_home(scope, name).or_else(|| scope.map(str::to_string)).unwrap_or_default();
        self.tick += 1;
        self.written.insert((Holder::Class(home.clone()), name.to_string()), self.tick);
        self.cvars.insert((home, name.to_string()), v);
    }

    /// `pattern === value`, as a `when` tests it.
    fn case_eq(&mut self, pattern: &Val, value: &Val, frame: &mut Frame) -> Res<bool> {
        match pattern {
            Val::Class(_) => self.dispatch(None, value.clone(), "is_a?", vec![pattern.clone()], None, frame)?.truthy(),
            Val::Proc { .. } => self.call_proc(pattern, vec![value.clone()], frame)?.truthy(),
            Val::Regex(_) => Ok(matches!(self.dispatch(None, pattern.clone(), "match?", vec![value.clone()], None, frame)?, Val::Bool(true))),
            p if compares(p) && compares(value) => Ok(same(p, value)),
            _ => Err("a `when` on a value it does not compare".into()),
        }
    }

    fn call_proc(&mut self, proc_: &Val, argv: Vec<Val>, frame: &Frame) -> Res<Val> {
        let _ = frame;
        let Val::Proc { params, rest, body, scope, this, locals, block } = proc_ else { return Err("a block that is not a block".into()) };
        // A `yield` in a block yields to the block of the method it is written in.
        let mut inner = Frame { this: (**this).clone(), locals: (**locals).clone(), block: block.as_deref().cloned(), scope: scope.clone() };
        bind(&mut inner.locals, params, rest.as_ref(), argv);
        self.value(body, &mut inner)
    }

    fn invoke(&mut self, def: &MethodDef, scope: String, this: Val, argv: Vec<Val>, blk: Option<Val>) -> Res<Val> {
        if self.depth > MAX_DEPTH {
            return Err("its calls nest too deep".into());
        }
        let mut frame = Frame { this, locals: HashMap::default(), block: blk.clone(), scope: Some(scope) };
        let takes_keywords = def.params.iter().any(|p| p.keyword || p.from_keyword);
        let mut keywords: Vec<(Val, Val)> = Vec::new();
        let mut argv = argv;
        if takes_keywords && matches!(argv.last(), Some(Val::Hash(_))) {
            if let Some(Val::Hash(entries)) = argv.pop() {
                keywords = entries;
            }
        }
        let mut given = argv.into_iter();
        for p in &def.params {
            if p.from_kwrest {
                return Err("a keyword rest parameter".into());
            }
            if p.keyword || p.from_keyword {
                let found = keywords.iter().position(|(k, _)| matches!(k, Val::Sym(n) if n == p.name.as_str()));
                let v = match (found, &p.default) {
                    (Some(i), _) => keywords.remove(i).1,
                    (None, Some(d)) => {
                        let d = d.clone();
                        self.value(&d, &mut frame)?
                    }
                    (None, None) => return Err(format!("`{}:` is not given", p.name.as_str())),
                };
                frame.locals.insert(p.name.clone(), v);
                continue;
            }
            if p.rest {
                frame.locals.insert(p.name.clone(), Val::Array(given.by_ref().collect()));
                continue;
            }
            let v = match (given.next(), &p.default) {
                (Some(v), _) => v,
                (None, Some(d)) => {
                    let d = d.clone();
                    self.value(&d, &mut frame)?
                }
                (None, None) => return Err(format!("`{}` is not given", p.name.as_str())),
            };
            frame.locals.insert(p.name.clone(), v);
        }
        if let Some(b) = &def.block_param {
            frame.locals.insert(b.name.clone(), blk.clone().unwrap_or(Val::Nil));
        }
        // What a bare `super` passes: the parameters as bound, a default filled in, not the arguments as given.
        let mut given_for_super = Vec::new();
        let mut passed_keywords = Vec::new();
        for p in &def.params {
            let Some(v) = frame.locals.get(&p.name).cloned() else { continue };
            if p.keyword || p.from_keyword {
                passed_keywords.push((Val::Sym(p.name.as_str().to_string()), v));
            } else if p.rest {
                if let Val::Array(items) = v {
                    given_for_super.extend(items);
                }
            } else {
                given_for_super.push(v);
            }
        }
        if !passed_keywords.is_empty() {
            given_for_super.push(Val::Hash(passed_keywords));
        }
        self.depth += 1;
        let owner = frame.scope.clone().unwrap_or_default();
        let side = def.receiver;
        self.calls.push((owner, def.name.as_str().to_string(), side, given_for_super, blk));
        let ran = self.value(&def.body, &mut frame);
        self.calls.pop();
        self.depth -= 1;
        ran
    }

    /// The method `name` answers on `this`, and the class its body's constants resolve in.
    fn lookup(&self, this: &Val, name: &str) -> Option<(MethodDef, String)> {
        if let (Val::Class(c), Some((subject, methods, _))) = (this, &self.subject)
            && c == subject
            && let Some(m) = methods.iter().rev().find(|m| m.name.as_str() == name)
        {
            let scope = m.enclosing_class.as_ref().map(|s| s.as_str().to_string()).unwrap_or_else(|| c.clone());
            return Some((m.clone(), scope));
        }
        if matches!(this, Val::Router) {
            return self.router.mixins.iter().find_map(|c| self.lookup_in(c, name, MethodReceiver::Instance, &mut HashSet::default()));
        }
        let (class, receiver) = match this {
            Val::Obj(i) => (self.heap[*i].class.clone(), MethodReceiver::Instance),
            Val::Class(c) if self.known.contains(c) => (c.clone(), MethodReceiver::Class),
            _ => return None,
        };
        let mut seen = HashSet::default();
        self.lookup_in(&class, name, receiver, &mut seen)
    }

    fn lookup_in(&self, class: &str, name: &str, receiver: MethodReceiver, seen: &mut HashSet<String>) -> Option<(MethodDef, String)> {
        if !seen.insert(class.to_string()) {
            return None;
        }
        // The mapper answers with what the gems mixed into it, as well as what reopens it.
        if class == MAPPER && receiver == MethodReceiver::Instance {
            for mixin in self.router.mixins.iter().filter(|m| m.as_str() != MAPPER) {
                if let Some(found) = self.lookup_in(mixin, name, receiver, seen) {
                    return Some(found);
                }
            }
        }
        if let Some(m) = self.evaled.get(class).and_then(|ms| ms.iter().rev().find(|m| m.name.as_str() == name && m.receiver == receiver)) {
            return Some((m.clone(), class.to_string()));
        }
        let decls: Vec<&LibraryClass> = self.classes.iter().filter(|c| c.name.0.as_str() == class).collect();
        for c in decls.iter().rev() {
            if let Some(m) = c.methods.iter().rev().find(|m| m.name.as_str() == name && m.receiver == receiver) {
                let scope = m.enclosing_class.as_ref().map(|s| s.as_str().to_string()).unwrap_or_else(|| class.to_string());
                return Some((m.clone(), scope));
            }
        }
        // Not `extend`ed modules on the class side: what reaches a class there is its own `def self.`s and its ancestors'.
        if receiver == MethodReceiver::Instance {
            for c in &decls {
                for inc in c.includes.iter().rev() {
                    if let Some(full) = self.resolve(inc.0.as_str(), Some(class))
                        && let Some(found) = self.lookup_in(&full, name, MethodReceiver::Instance, seen)
                    {
                        return Some(found);
                    }
                }
            }
        }
        let parent = decls.iter().find_map(|c| c.parent.clone())?;
        let parent = self.resolve(parent.0.as_str(), Some(class))?;
        self.lookup_in(&parent, name, receiver, seen)
    }

    fn resolve(&self, written: &str, scope: Option<&str>) -> Option<String> {
        let rooted = written.starts_with("::");
        let written = written.trim_start_matches("::");
        let mut at = if rooted { None } else { scope.map(str::to_string) };
        while let Some(outer) = at {
            let candidate = format!("{outer}::{written}");
            if self.known.contains(&candidate) {
                return Some(candidate);
            }
            at = outer.rsplit_once("::").map(|(o, _)| o.to_string());
        }
        self.known.contains(written).then(|| written.to_string())
    }

    /// A constant: a class this program defines, a constant one of them assigns, or a name it does not define.
    /// Whether `path` names a class the program defines or a constant one holds; `constant` answers any other name as a class of that name.
    fn resolves_constant(&mut self, path: &[Symbol], scope: Option<&str>) -> bool {
        let written = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::");
        let rooted = if path.first().is_some_and(|s| s.as_str().is_empty()) { format!("::{written}") } else { written.clone() };
        if self.resolve(&rooted, scope).is_some() {
            return true;
        }
        match self.constant(path, scope) {
            Ok(Val::Class(c)) => c != written,
            Ok(Val::Unknown) | Err(_) => false,
            Ok(_) => true,
        }
    }

    fn constant(&mut self, path: &[Symbol], scope: Option<&str>) -> Res<Val> {
        let written = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::");
        let written = if path.first().is_some_and(|s| s.as_str().is_empty()) { format!("::{written}") } else { written };
        if let Some(full) = self.resolve(&written, scope) {
            return Ok(Val::Class(full));
        }
        let (owner, last) = match written.trim_start_matches("::").rsplit_once("::") {
            Some((o, l)) => (self.resolve(o, scope), l.to_string()),
            None => (None, written.trim_start_matches("::").to_string()),
        };
        let mut at = owner.or_else(|| scope.map(str::to_string));
        while let Some(class) = at {
            let value = self
                .classes
                .iter()
                .filter(|c| c.name.0.as_str() == class)
                .flat_map(|c| c.constants.iter())
                .find(|(n, _)| n.as_str() == last)
                .map(|(_, v)| v.clone());
            if let Some(value) = value {
                let key = format!("{class}::{last}");
                self.note_constant_read(&key)?;
                if let Some(v) = self.consts.get(&key) {
                    return Ok(v.clone());
                }
                let mut frame = Frame { this: Val::Class(class.clone()), locals: HashMap::default(), block: None, scope: Some(class.clone()) };
                let v = self.value(&value, &mut frame)?;
                self.consts.insert(key, v.clone());
                return Ok(v);
            }
            at = class.rsplit_once("::").map(|(o, _)| o.to_string());
        }
        Ok(Val::Class(written.trim_start_matches("::").to_string()))
    }

    /// A constant's tag by the name it resolves to from where it is written; one that does not resolve keeps its written path.
    fn full_constant_tag(&self, tag: Tag, owner: &str) -> Tag {
        if tag.0 != CONSTANT_OWNER {
            return tag;
        }
        let path: Vec<Symbol> = tag.2.split("::").map(Symbol::from).collect();
        match self.constant_slot(&path, (owner != "*").then_some(owner)) {
            Some(key) => (CONSTANT_OWNER.to_string(), false, key),
            None => tag,
        }
    }

    fn note_constant_read(&self, key: &str) -> Res<()> {
        let h = Holder::Constant(key.to_string());
        let last_unsure = self
            .unsure
            .iter()
            .filter(|(holder, _)| matches!(holder, Holder::Constant(n) if n == key || key.ends_with(&format!("::{n}"))))
            .flat_map(|(_, marks)| marks.iter().map(|(_, t)| *t))
            .max();
        let written = self.written.get(&(h, String::new())).copied().unwrap_or(0);
        if last_unsure.is_some_and(|u| u > written) { Err(format!("`{key}` may have been changed by a statement passed over")) } else { Ok(()) }
    }

    /// Where a constant's value lives, by full name, and its value now.
    fn constant_slot(&self, path: &[Symbol], scope: Option<&str>) -> Option<String> {
        let written = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::");
        let (owner, last) = match written.rsplit_once("::") {
            Some((o, l)) => (self.resolve(o, scope), l.to_string()),
            None => (None, written.clone()),
        };
        let mut at = if path.first().is_some_and(|s| s.as_str().is_empty()) && owner.is_none() { None } else { owner.or_else(|| scope.map(str::to_string)) };
        while let Some(class) = at {
            if self.classes.iter().filter(|c| c.name.0.as_str() == class).any(|c| c.constants.iter().any(|(n, _)| n.as_str() == last)) {
                return Some(format!("{class}::{last}"));
            }
            at = class.rsplit_once("::").map(|(o, _)| o.to_string());
        }
        None
    }

    fn ivar(&self, this: &Val, name: &str) -> Option<Val> {
        match this {
            Val::Router => self.router.ivars.get(name).cloned().or_else(|| (name == "scope").then(|| self.router.base())),
            Val::Obj(i) => self.heap[*i].ivars.get(name).cloned(),
            Val::Class(c) => self.class_ivars.get(&(c.clone(), name.to_string())).cloned(),
            _ => None,
        }
    }

    fn set_ivar(&mut self, this: &Val, name: &str, v: Val) -> Res<()> {
        if let Some(h) = Self::holder(this) {
            self.tick += 1;
            self.written.insert((h, name.to_string()), self.tick);
        }
        match this {
            Val::Router => {
                self.router.ivars.insert(name.to_string(), v);
            }
            Val::Obj(i) => {
                self.heap[*i].ivars.insert(name.to_string(), v);
            }
            Val::Class(c) => {
                self.class_ivars.insert((c.clone(), name.to_string()), v);
            }
            _ => return Err("an instance variable of other than an object".into()),
        }
        Ok(())
    }

    fn text(&self, v: &Val) -> Res<String> {
        match v {
            Val::Str(s) | Val::Sym(s) | Val::Class(s) => Ok(s.clone()),
            Val::Int(i) => Ok(i.to_string()),
            Val::Nil => Ok(String::new()),
            Val::Bool(b) => Ok(b.to_string()),
            _ => Err("text of other than a literal".into()),
        }
    }

    // A class with `method_missing` may answer any name with any of its methods.
    fn answers_missing(&self, class: &str) -> bool {
        self.ancestry(class).iter().any(|a| {
            self.classes.iter().any(|c| c.name.0.as_str() == a && c.methods.iter().any(|m| m.name.as_str() == "method_missing"))
        })
    }

    fn owner_tag(&self, c: &LibraryClass, m: &MethodDef) -> (String, bool) {
        // Not the module's own: an included or extended module's method writes the ivars of whoever it runs on.
        if c.is_module && m.receiver == MethodReceiver::Instance { ("*".into(), false) } else { (c.name.0.as_str().to_string(), m.receiver == MethodReceiver::Class) }
    }

    fn all_method_writes(&self) -> Option<HashSet<Tag>> {
        let mut out = HashSet::default();
        for c in self.classes {
            for m in &c.methods {
                let (owner, side) = self.owner_tag(c, m);
                if !self.direct_writes(&m.body, &owner, side, &mut out) {
                    return None;
                }
            }
        }
        Some(out)
    }

    /// The writes `e` makes itself, without the methods it calls; `false` for one whose name is known only when it runs.
    fn direct_writes(&self, e: &Expr, owner: &str, side: bool, out: &mut HashSet<Tag>) -> bool {
        let bounded = true;
        visit(e, &mut |x| match &*x.node {
            ExprNode::Assign { target: LValue::Ivar { name }, .. } | ExprNode::OpAssign { target: LValue::Ivar { name }, .. } => {
                out.insert((owner.to_string(), side, name.as_str().to_string()));
            }
            // A class variable is the class's, whichever side writes it.
            ExprNode::Assign { target: LValue::Var { name, .. }, .. } | ExprNode::OpAssign { target: LValue::Var { name, .. }, .. } if name.as_str().starts_with("@@") => {
                out.insert((owner.to_string(), true, name.as_str().to_string()));
            }
            // `@scopes << :read`, `URL_HELPERS[key].concat(value)`: the object the ivar or constant holds changes, which a read of it sees; a program's own method is followed instead.
            ExprNode::Send { recv: Some(r), method, .. }
                if !self.classes.iter().any(|c| c.methods.iter().any(|m| &m.name == method))
                    && (method.as_str().ends_with('!')
                        || matches!(method.as_str(), "<<" | "push" | "[]=" | "concat" | "update" | "delete" | "clear" | "replace" | "insert" | "unshift" | "store" | "append" | "prepend")) =>
            {
                if let Some(tag) = changed_root(r, owner, side) {
                    out.insert(self.full_constant_tag(tag, owner));
                }
            }
            ExprNode::Assign { target: LValue::Index { recv, .. }, .. } | ExprNode::OpAssign { target: LValue::Index { recv, .. }, .. } => {
                if let Some(tag) = changed_root(recv, owner, side) {
                    out.insert(self.full_constant_tag(tag, owner));
                }
            }
            ExprNode::Send { recv, method, args, .. } if method.as_str() == "instance_variable_set" => match args.first().map(|a| &*a.node) {
                Some(ExprNode::Lit { value: Literal::Sym { value } }) => {
                    let name = value.as_str().trim_start_matches('@').to_string();
                    match recv.as_ref().map(|r| &*r.node) {
                        None | Some(ExprNode::SelfRef) => out.insert((owner.to_string(), side, name)),
                        Some(_) => out.insert(("*".into(), false, name)),
                    };
                }
                // A name known only when it runs: the names its literal parts allow.
                arg => {
                    let pattern = arg.map(name_pattern).unwrap_or_else(|| "*".into());
                    match recv.as_ref().map(|r| &*r.node) {
                        None | Some(ExprNode::SelfRef) => out.insert((owner.to_string(), side, pattern)),
                        Some(_) => out.insert(("*".into(), false, pattern)),
                    };
                }
            },
            _ => {}
        });
        bounded
    }

    /// The instance variables a statement may write, through every method of the program it may call: `None` when unbounded.
    fn may_write(&self, e: &Expr, owner: &str, side: bool, visited: &mut HashSet<String>) -> Option<HashSet<Tag>> {
        self.may_write_with(e, owner, side, visited, &HashMap::default())
    }

    /// `locals` are what the statement's own locals hold as it runs (`config`, the class `Devise.setup` yields), each a receiver its lookup answers for, unless the statement assigns it.
    fn may_write_with(&self, e: &Expr, owner: &str, side: bool, visited: &mut HashSet<String>, locals: &HashMap<Symbol, Val>) -> Option<HashSet<Tag>> {
        let mut out = HashSet::default();
        let mut assigned: HashSet<Symbol> = HashSet::default();
        visit(e, &mut |x| {
            if let ExprNode::Assign { target: LValue::Var { name, .. }, .. } | ExprNode::OpAssign { target: LValue::Var { name, .. }, .. } = &*x.node {
                assigned.insert(name.clone());
            }
        });
        // `Devise.setup do |config| … end`, with `def self.setup; yield self; end`: the block's parameter is the receiver.
        let mut locals = locals.clone();
        let mut yielded: Option<Symbol> = None;
        if let ExprNode::Send { recv: Some(r), method, block: Some(b), .. } = &*e.node
            && let ExprNode::Const { path } = &*r.node
            && let ExprNode::Lambda { params, .. } = &*b.node
            && let [param] = params.as_slice()
            && let Some(class) = self.resolve(&path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::"), (owner != "*").then_some(owner))
            && let Some((def, _)) = self.lookup_in(&class, method.as_str(), MethodReceiver::Class, &mut HashSet::default())
            && yields_self(&def.body)
        {
            locals.insert(param.clone(), Val::Class(class));
            yielded = Some(param.clone());
        }
        // Not a name a block in the statement binds again (`each { |config| … }`): there it is the block's value.
        let mut bound: HashMap<Symbol, usize> = HashMap::default();
        visit(e, &mut |x| {
            if let ExprNode::Lambda { params, rest_param, .. } = &*x.node {
                for p in params.iter().chain(rest_param.iter()) {
                    *bound.entry(p.clone()).or_default() += 1;
                }
            }
        });
        locals.retain(|name, _| match bound.get(name) {
            None => true,
            Some(n) => yielded.as_ref() == Some(name) && *n == 1,
        });
        let locals = &locals;
        // Not a lambda handed as a value (`setup: ->(env) { … }`): it runs when called, not as the statement does.
        let e = &without_value_lambdas(e);
        // Not by recursion: a large program's call chains run deeper than a stack bound, and each method is read once.
        let mut work: Vec<(Expr, String, bool, Option<String>, bool)> = vec![(e.clone(), owner.to_string(), side, (owner != "*").then(|| owner.to_string()), true)];
        while let Some((body, owner, side, class, top)) = work.pop() {
            if !self.direct_writes(&body, &owner, side, &mut out) {
                return None;
            }
            let mut calls: Vec<(String, Option<Expr>, bool)> = Vec::new();
            visit(&body, &mut |x| {
                if let ExprNode::Send { recv, method, args, .. } = &*x.node {
                    // Not `class_eval do … end`: its block is walked here as the call's own body, and a string is what would name methods only when it runs.
                    let dynamic = matches!(method.as_str(), "send" | "public_send" | "__send__" | "class_eval" | "define_method")
                        && !matches!(args.first().map(|a| &*a.node), Some(ExprNode::Lit { value: Literal::Sym { .. } }))
                        && !(method.as_str() == "class_eval" && args.is_empty());
                    // `send(:autoload, …)` calls `autoload`, not every method named `send`.
                    let named = match (method.as_str(), args.first().map(|a| &*a.node)) {
                        ("send" | "public_send" | "__send__", Some(ExprNode::Lit { value: Literal::Sym { value } })) => value.as_str().to_string(),
                        _ => method.as_str().to_string(),
                    };
                    calls.push((named, recv.clone(), dynamic));
                }
                // `each(&method(:apply))` calls `apply` as it runs, though no Send names it.
                if let ExprNode::MethodRef { recv, name } = &*x.node {
                    calls.push((name.as_str().to_string(), recv.clone(), false));
                }
            });
            for (name, recv, dynamic) in calls {
                if dynamic {
                    // On self, a name known only when it runs is still one of the class's own methods.
                    let own = matches!(recv.as_ref().map(|r| &*r.node), None | Some(ExprNode::SelfRef)).then_some(class.as_ref()).flatten();
                    match own {
                        Some(c) => {
                            let receiver = if side { MethodReceiver::Class } else { MethodReceiver::Instance };
                            for ancestor in self.ancestry(c) {
                                for decl in self.classes.iter().filter(|d| d.name.0.as_str() == ancestor) {
                                    for m in decl.methods.iter().filter(|m| m.receiver == receiver || decl.is_module) {
                                        let key = format!("{}#{}#{:?}#{:?}", decl.name.0.as_str(), m.name.as_str(), m.receiver, m.body.span);
                                        if visited.insert(key) {
                                            let (o, s) = self.owner_tag(decl, m);
                                            work.push((m.body.clone(), o, s, Some(ancestor.clone()), false));
                                        }
                                    }
                                }
                            }
                        }
                        None => {
                            out.extend(self.all_method_writes()?)
                        }
                    }
                    continue;
                }
                // Not a program object: `Rails.application.config` answers the framework's, whose methods write none of the program's ivars.
                if recv.as_ref().is_some_and(|r| {
                    matches!(&*r.node, ExprNode::Send { .. }) && root_constant(r).is_some_and(|c| self.resolve(&c, class.as_deref()).is_none())
                }) {
                    continue;
                }
                // The class the call is on, when the receiver says: then its own lookup answers, and no method there is a built-in.
                let on: Option<(String, MethodReceiver)> = match recv.as_ref().map(|r| &*r.node) {
                    None | Some(ExprNode::SelfRef) => {
                        class.clone().map(|c| (c, if side { MethodReceiver::Class } else { MethodReceiver::Instance }))
                    }
                    Some(ExprNode::Const { path }) => {
                        let written = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::");
                        let resolved = self.resolve(&written, class.as_deref());
                        // Not a class the program defines: a framework's, whose methods write none of its ivars.
                        let Some(resolved) = resolved else { continue };
                        Some((resolved, MethodReceiver::Class))
                    }
                    Some(ExprNode::Var { name, .. }) if top && !assigned.contains(name) => match locals.get(name) {
                        Some(Val::Class(c)) if self.known.contains(c) => Some((c.clone(), MethodReceiver::Class)),
                        Some(Val::Obj(i)) => Some((self.heap[*i].class.clone(), MethodReceiver::Instance)),
                        _ => None,
                    },
                    _ => None,
                };
                let exact = match &on {
                    Some((c, MethodReceiver::Class)) if name == "new" => self.lookup_in(c, "initialize", MethodReceiver::Instance, &mut HashSet::default()),
                    Some((c, receiver)) => self.lookup_in(c, &name, *receiver, &mut HashSet::default()),
                    None => None,
                };
                if on.is_some() && exact.is_none() && !self.answers_missing(on.as_ref().map(|(c, _)| c.as_str()).unwrap_or("")) {
                    continue;
                }
                let callees: Vec<(&LibraryClass, &MethodDef)> = match exact {
                    Some((def, _)) => self
                        .classes
                        .iter()
                        .flat_map(|c| c.methods.iter().map(move |m| (c, m)))
                        .filter(|(_, m)| m.name == def.name && m.receiver == def.receiver && m.body.span == def.body.span)
                        .take(1)
                        .collect(),
                    // Every method of the program by that name: which one the call reaches is not known here.
                    None => self.classes.iter().flat_map(|c| c.methods.iter().filter(|m| m.name.as_str() == name).map(move |m| (c, m))).collect(),
                };
                for (c, m) in callees {
                    let key = format!("{}#{}#{:?}#{:?}", c.name.0.as_str(), m.name.as_str(), m.receiver, m.body.span);
                    if visited.insert(key) {
                        let (o, s) = self.owner_tag(c, m);
                        let at = (!c.is_module || m.receiver == MethodReceiver::Class).then(|| c.name.0.as_str().to_string());
                        work.push((m.body.clone(), o, s, at, false));
                    }
                }
            }
        }
        Some(out)
    }
}

/// A statement with each lambda literal it passes as a value made nil; a call's block stays, as the call may run it.
fn without_value_lambdas(e: &Expr) -> Expr {
    fn strip(e: &mut Expr, as_block: bool) {
        let literal = matches!(&*e.node, ExprNode::Send { recv: None, method, block: Some(_), args, .. } if args.is_empty() && matches!(method.as_str(), "lambda" | "proc"));
        if !as_block && (literal || matches!(&*e.node, ExprNode::Lambda { .. })) {
            *e = Expr::new(e.span, ExprNode::Lit { value: Literal::Nil });
            return;
        }
        if let ExprNode::Send { recv, args, block, .. } = &mut *e.node {
            if let Some(r) = recv {
                strip(r, false);
            }
            for a in args {
                strip(a, false);
            }
            if let Some(b) = block
                && let ExprNode::Lambda { body, .. } = &mut *b.node
            {
                strip(body, false);
            }
            return;
        }
        e.node.for_each_child_mut(&mut |c| strip(c, false));
    }
    let mut out = e.clone();
    strip(&mut out, false);
    out
}

/// A body that yields its own `self` and nothing else (`yield self`), so a block's parameter is the receiver.
fn yields_self(body: &Expr) -> bool {
    let mut yields = Vec::new();
    visit(body, &mut |e| {
        if let ExprNode::Yield { args } = &*e.node {
            yields.push(matches!(args.as_slice(), [a] if matches!(&*a.node, ExprNode::SelfRef)));
        }
    });
    !yields.is_empty() && yields.iter().all(|y| *y)
}

/// A statement without what a `proc`/`lambda` literal would run later: `OmniAuth.config.on_failure = proc do |env| … end` runs none of its body as it loads.
fn without_deferred_bodies(e: &Expr) -> Expr {
    let mut out = e.clone();
    fn strip(e: &mut Expr) {
        let deferred = match &*e.node {
            ExprNode::Send { recv: None, method, block: Some(_), args, .. } => args.is_empty() && matches!(method.as_str(), "proc" | "lambda"),
            ExprNode::Send { recv: Some(r), method, block: Some(_), .. } => method.as_str() == "new" && matches!(&*r.node, ExprNode::Const { path } if path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).eq(["Proc"])),
            _ => false,
        };
        if deferred {
            *e = Expr::new(e.span, ExprNode::Lit { value: Literal::Nil });
            return;
        }
        e.node.for_each_child_mut(&mut |c| strip(c));
    }
    strip(&mut out);
    out
}

/// Source made only of method definitions (and blank lines), as a `class_eval` string of them is.
fn only_defs(code: &str) -> bool {
    let mut depth = 0usize;
    for line in code.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        if depth == 0 && !line.starts_with("def ") {
            return false;
        }
        if line.starts_with("def ") && !line.contains(';') {
            depth += 1;
        } else if line == "end" {
            depth = depth.checked_sub(1).unwrap_or(usize::MAX);
            if depth == usize::MAX {
                return false;
            }
        } else if depth == 1 && line.starts_with("def ") {
            return false;
        }
    }
    depth == 0
}

/// What holds the object a receiver reads, through any `[key]` (`@routes[:a]`, `URL_HELPERS[key]`): its ivar, or its constant by last segment.
fn changed_root(r: &Expr, owner: &str, side: bool) -> Option<Tag> {
    match &*r.node {
        ExprNode::Ivar { name } => Some((owner.to_string(), side, name.as_str().to_string())),
        ExprNode::Var { name, .. } if name.as_str().starts_with("@@") => Some((owner.to_string(), true, name.as_str().to_string())),
        ExprNode::Const { path } => Some((CONSTANT_OWNER.to_string(), false, path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::"))),
        ExprNode::Send { recv: Some(inner), method, .. } if method.as_str() == "[]" => changed_root(inner, owner, side),
        _ => None,
    }
}

/// `Hash#deep_merge`: a key both hold with Hash values merges them, otherwise `more`'s value wins.
fn deep_merge(base: &[(Val, Val)], more: &[(Val, Val)]) -> Vec<(Val, Val)> {
    let mut merged = base.to_vec();
    for (k, v) in more {
        match merged.iter_mut().find(|(ek, _)| same(ek, k)) {
            Some(slot) => {
                slot.1 = match (&slot.1, v) {
                    (Val::Hash(a), Val::Hash(b)) => Val::Hash(deep_merge(a, b)),
                    _ => v.clone(),
                };
            }
            None => merged.push((k.clone(), v.clone())),
        }
    }
    merged
}

fn visit(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    e.node.for_each_child(&mut |c| visit(c, f));
}

fn bind(locals: &mut HashMap<Symbol, Val>, params: &[Symbol], rest: Option<&Symbol>, argv: Vec<Val>) {
    let mut given = argv.into_iter();
    for p in params {
        locals.insert(p.clone(), given.next().unwrap_or(Val::Nil));
    }
    if let Some(r) = rest {
        locals.insert(r.clone(), Val::Array(given.collect()));
    }
}

fn literal(value: &Literal) -> Res<Val> {
    Ok(match value {
        Literal::Nil => Val::Nil,
        Literal::Bool { value } => Val::Bool(*value),
        Literal::Int { value } => Val::Int(*value),
        Literal::Str { value } => Val::Str(value.clone()),
        Literal::Sym { value } => Val::Sym(value.as_str().to_string()),
        // Ruby's `^` and `$` are line anchors; flags other than those it can carry are not read here.
        Literal::Regex { pattern, flags } if flags.chars().all(|f| matches!(f, 'i' | 'm' | 'x')) => {
            let mut inline = String::from("m");
            if flags.contains('i') {
                inline.push('i');
            }
            if flags.contains('x') {
                inline.push('x');
            }
            if flags.contains('m') {
                inline.push('s');
            }
            Val::Regex(format!("(?{inline}){pattern}"))
        }
        _ => return Err("a literal of another kind".into()),
    })
}

fn same(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Nil, Val::Nil) => true,
        (Val::Bool(x), Val::Bool(y)) => x == y,
        (Val::Int(x), Val::Int(y)) => x == y,
        (Val::Str(x), Val::Str(y)) | (Val::Sym(x), Val::Sym(y)) | (Val::Class(x), Val::Class(y)) => x == y,
        (Val::Obj(x), Val::Obj(y)) => x == y,
        (Val::Array(x), Val::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b)),
        (Val::Hash(x), Val::Hash(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.iter().any(|(k2, v2)| same(k, k2) && same(v, v2))),
        _ => false,
    }
}

/// Whether `same` answers `==` as Ruby does: not for a value it carries without modeling (a Regexp's options, a Proc's identity).
fn compares(v: &Val) -> bool {
    match v {
        Val::Array(items) => items.iter().all(compares),
        Val::Hash(entries) => entries.iter().all(|(k, v)| compares(k) && compares(v)),
        Val::Regex(_) | Val::Proc { .. } | Val::Unknown | Val::Router | Val::Merger { .. } | Val::Defaulted { .. } => false,
        _ => true,
    }
}

/// The value a class body's `self.attr = value` writes, however the parser left it.
fn self_write(e: &Expr, attr: &str) -> Option<Expr> {
    match &*e.node {
        ExprNode::Assign { target: LValue::Attr { recv, name }, value } if name.as_str() == attr && matches!(&*recv.node, ExprNode::SelfRef) => Some(value.clone()),
        ExprNode::Send { recv: Some(recv), method, args, block: None, .. }
            if method.as_str().strip_suffix('=') == Some(attr) && args.len() == 1 && matches!(&*recv.node, ExprNode::SelfRef) => Some(args[0].clone()),
        _ => None,
    }
}

/// The constant a call chain starts from: `Doorkeeper` in `Doorkeeper.config.resolve_controller(:base)`.
fn root_constant(e: &Expr) -> Option<String> {
    match &*e.node {
        ExprNode::Send { recv: Some(r), .. } => root_constant(r),
        ExprNode::Const { path } => Some(path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::")),
        _ => None,
    }
}

// Not answered as `Unknown`: a built-in that changes its receiver changes a value the program may read back.
fn mutates(method: &str) -> bool {
    method.ends_with('!')
        || matches!(method, "<<" | "push" | "[]=" | "concat" | "update" | "delete" | "clear" | "replace" | "insert" | "unshift" | "store" | "append" | "prepend" | "send" | "public_send" | "__send__" | "instance_variable_set" | "instance_eval" | "instance_exec" | "class_eval" | "define_method" | "extend" | "include")
}

/// The ivar names an `instance_variable_set` name expression can spell, as a pattern (`@#{type}_secret_strategy` -> `*_secret_strategy`).
fn name_pattern(node: &ExprNode) -> String {
    let parts = match node {
        ExprNode::Send { recv: Some(r), method, .. } if method.as_str() == "to_sym" => match &*r.node {
            ExprNode::StringInterp { parts } => parts,
            _ => return "*".into(),
        },
        ExprNode::StringInterp { parts } => parts,
        ExprNode::Lit { value: Literal::Str { value } } => return value.trim_start_matches('@').to_string(),
        _ => return "*".into(),
    };
    let mut out = String::new();
    for part in parts {
        match part {
            InterpPart::Text { value } => out.push_str(value),
            InterpPart::Expr { .. } => {
                if !out.ends_with('*') {
                    out.push('*');
                }
            }
        }
    }
    let out = out.trim_start_matches('@').to_string();
    if out.is_empty() { "*".into() } else { out }
}

fn glob(pattern: &str, name: &str) -> bool {
    let pieces: Vec<&str> = pattern.split('*').collect();
    if pieces.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (pieces[0], pieces[pieces.len() - 1]);
    if !name.starts_with(first) || !name[first.len()..].ends_with(last) || name.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for piece in &pieces[1..pieces.len() - 1] {
        match rest.find(piece) {
            Some(i) => rest = &rest[i + piece.len()..],
            None => return false,
        }
    }
    true
}

fn to_expr(v: &Val, span: Span) -> Res<Expr> {
    let lit = |value: Literal| Expr::new(span, ExprNode::Lit { value });
    Ok(match v {
        Val::Nil => lit(Literal::Nil),
        Val::Bool(b) => lit(Literal::Bool { value: *b }),
        Val::Int(i) => lit(Literal::Int { value: *i }),
        Val::Str(t) => lit(Literal::Str { value: t.clone() }),
        Val::Sym(t) => lit(Literal::Sym { value: Symbol::from(t.as_str()) }),
        Val::Class(c) => Expr::new(span, ExprNode::Const { path: c.split("::").map(Symbol::from).collect() }),
        Val::Array(items) => Expr::new(span, ExprNode::Array { elements: items.iter().map(|i| to_expr(i, span)).collect::<Res<_>>()?, style: Default::default() }),
        Val::Hash(entries) => Expr::new(span, ExprNode::Hash {
            entries: entries.iter().map(|(k, v)| Ok((to_expr(k, span)?, to_expr(v, span)?))).collect::<Res<_>>()?,
            kwargs: false,
        }),
        // `constraints(lambda { |request| … })` (devise's `authenticate`): the lambda, with each local it
        // closes over written in as its value; one it cannot write, or `self`, is no routing argument.
        // The block it closed over is the one a `yield` inside would run; a body with none does not need it.
        Val::Proc { params, rest: None, body, locals, block, .. } if block.is_none() || !yields(body) => {
            // A nested block that binds a name the outer one closes over: its reads are its own, which
            // a substitution by name would replace.
            if let Some(name) = rebound(body).into_iter().find(|n| locals.contains_key(n) && !params.contains(n)) {
                return Err(format!("a block that rebinds `{}`, a local it closes over", name.as_str()));
            }
            let unwritable = std::cell::RefCell::new(None::<String>);
            let body = crate::lower::controller_to_library::util::map_expr(body, &|e| match &*e.node {
                ExprNode::Var { name, .. } if !params.contains(name) => {
                    let value = locals.get(name)?;
                    match to_expr(value, e.span) {
                        Ok(written) => Some(written),
                        Err(why) => {
                            unwritable.borrow_mut().get_or_insert(format!("{why} (`{}` in a block)", name.as_str()));
                            None
                        }
                    }
                }
                ExprNode::SelfRef => {
                    unwritable.borrow_mut().get_or_insert_with(|| "a block that reads `self`".into());
                    None
                }
                _ => None,
            });
            if let Some(why) = unwritable.into_inner() {
                return Err(why);
            }
            // Checked on the result, not trusted to the walk: a read the substitution did not reach
            // (a `when` pattern) would run unbound, and a pattern match can bind a name of its own.
            if let Some(name) = reads(&body).into_iter().find(|n| locals.contains_key(n) && !params.contains(n)) {
                return Err(format!("a block that reads `{}`, a local it closes over, where it cannot be written in", name.as_str()));
            }
            if matches_patterns(&body) {
                return Err("a block with a pattern match, whose patterns may bind a local it closes over".into());
            }
            Expr::new(span, ExprNode::Lambda {
                params: params.clone(),
                rest_param: None,
                extra_params: Vec::new(),
                block_param: None,
                body,
                block_style: Default::default(),
            })
        }
        other => return Err(format!("a routing argument it cannot write ({})", val_kind(other))),
    })
}

fn reads(e: &Expr) -> Vec<Symbol> {
    let mut out = Vec::new();
    if let ExprNode::Var { name, .. } = &*e.node {
        out.push(name.clone());
    }
    e.node.for_each_child(&mut |c| out.extend(reads(c)));
    out
}

fn matches_patterns(e: &Expr) -> bool {
    let mut found = matches!(&*e.node, ExprNode::CaseMatch { .. } | ExprNode::MatchPredicate { .. } | ExprNode::MatchRequired { .. });
    e.node.for_each_child(&mut |c| found |= matches_patterns(c));
    found
}

/// The names `e` binds: nested blocks' parameters and the locals it assigns, either of which makes a
/// read of that name something other than the value the outer block closed over.
fn rebound(e: &Expr) -> Vec<Symbol> {
    let mut out = Vec::new();
    if let ExprNode::Lambda { params, rest_param, block_param, .. } = &*e.node {
        out.extend(params.iter().cloned());
        out.extend(rest_param.iter().cloned());
        out.extend(block_param.iter().cloned());
    }
    if let ExprNode::Assign { target: crate::expr::LValue::Var { name, .. }, .. }
    | ExprNode::OpAssign { target: crate::expr::LValue::Var { name, .. }, .. } = &*e.node
    {
        out.push(name.clone());
    }
    if let ExprNode::MultiAssign { targets, .. } = &*e.node {
        out.extend(targets.iter().filter_map(|t| match t {
            crate::expr::LValue::Var { name, .. } => Some(name.clone()),
            _ => None,
        }));
    }
    e.node.for_each_child(&mut |c| out.extend(rebound(c)));
    out
}

fn yields(e: &Expr) -> bool {
    let mut found = matches!(&*e.node, ExprNode::Yield { .. });
    e.node.for_each_child(&mut |c| found |= yields(c));
    found
}

fn val_kind(v: &Val) -> &'static str {
    match v {
        Val::Proc { rest: Some(_), .. } => "a block with a rest parameter",
        Val::Proc { block: Some(_), .. } => "a block that takes a block",
        Val::Proc { .. } => "a block",
        Val::Obj(_) => "an object",
        Val::Regex(_) => "a regexp",
        Val::Unknown => "a value it does not model",
        Val::Router => "the mapper",
        Val::Merger { .. } => "a with_options target",
        Val::Defaulted { .. } => "a Hash with a default",
        _ => "a value",
    }
}

/// The routes a gem's routing method draws (`use_doorkeeper do … end`), run with the mapper as the
/// recording `Router` after the initializers that call on `root` have run.
/// `enclosing` is the scope the call stands in (`path`, `module`, `as`, as Rails accumulates them), `None` inside a resource.
pub(super) fn draw_routes(app: &App, def: &MethodDef, scope: String, call: &Expr, initializers: &[Expr], roots: &[String], mixins: Vec<String>, enclosing: Option<(Option<String>, Option<String>, Option<String>)>) -> Res<(Vec<Expr>, Vec<String>)> {
    let mut eval = Eval::new(app);
    let enclosing = enclosing.map(|(path, module, as_)| {
        [("path", path), ("module", module), ("as", as_)]
            .into_iter()
            .filter_map(|(k, v)| v.map(|v| (Val::Sym(k.into()), Val::Str(v))))
            .collect()
    });
    eval.router = RouterState { mixins, ivars: HashMap::default(), enclosing, drawing: false };
    for root in roots {
        eval.replay(initializers, root)?;
    }
    // Not what an initializer passed over may have written: the mapper the call runs on is the evaluation's own, made after them.
    eval.unsure.remove(&Holder::Router);
    eval.router.drawing = true;
    let ExprNode::Send { args, block, .. } = &*call.node else { return Err("not a call".into()) };
    let mut frame = Frame { this: Val::Router, locals: HashMap::default(), block: None, scope: None };
    let argv = args.iter().map(|a| eval.value(a, &mut frame)).collect::<Res<Vec<_>>>()?;
    let blk = match block {
        Some(b) => Some(eval.value(b, &mut frame)?),
        None => None,
    };
    eval.drawn.push(Vec::new());
    eval.invoke(def, scope, Val::Router, argv, blk).map_err(|why| why.trim_start_matches(STOP).to_string())?;
    Ok((eval.drawn.pop().unwrap_or_default(), eval.omitted))
}

pub(super) fn initializers_of<V: Vfs + ?Sized>(vfs: &V, dir: &Path, gem_dirs: &[std::path::PathBuf]) -> Vec<Expr> {
    initializer_statements(vfs, dir, gem_dirs)
}

/// A place a value lives, which a method on it can change in place.
enum Slot {
    Ivar(Val, String),
    Cvar(Option<String>, String),
    Local(Symbol),
    Constant(String),
    /// `holder[key]`, the element of a Hash or Array another slot holds.
    Index(Box<Slot>, Val),
}

/// The class variable a `mattr_reader`-made reader answers (`def self.mapping; @@mapping; end`).
fn cvar_reader(def: &MethodDef) -> Option<String> {
    if !def.params.is_empty() {
        return None;
    }
    let body = match &*def.body.node {
        ExprNode::Seq { exprs } if exprs.len() == 1 => &exprs[0],
        _ => &def.body,
    };
    match &*body.node {
        ExprNode::Var { name, .. } if name.as_str().starts_with("@@") => Some(name.as_str().to_string()),
        _ => None,
    }
}

/// Whether `e` names the constant `root` or one under it anywhere.
/// `defined?(Root::…)`, as a gem guards a write to another's constant: not any condition, whose arms the replay would run every time.
fn defines_under(cond: &Expr, root: &str) -> bool {
    let operand = match &*cond.node {
        ExprNode::Defined { operand } => operand,
        ExprNode::Send { recv: None, method, args, block: None, .. } if method.as_str() == "defined?" && args.len() == 1 => &args[0],
        _ => return false,
    };
    matches!(&*operand.node, ExprNode::Const { path } if path.iter().map(|s| s.as_str()).find(|s| !s.is_empty()) == Some(root))
}

fn names_constant(e: &Expr, root: &str) -> bool {
    let mut found = false;
    visit(e, &mut |x| {
        if let ExprNode::Const { path } = &*x.node
            && path.iter().map(|s| s.as_str()).find(|s| !s.is_empty()) == Some(root)
        {
            found = true;
        }
    });
    found
}


#[cfg(test)]
mod tests {
    use super::*;

    fn statement(source: &str) -> Expr {
        let parsed = super::super::prism::parse_silent(source.as_bytes());
        let node = parsed.node();
        let program = node.as_program_node().expect("program");
        let first = program.statements().body().iter().next().expect("a statement");
        let (expr, _) = super::super::prism::scope(|| super::super::expr::ingest_expr(&first, "t.rb"));
        expr.expect("ingest")
    }

    /// A block parameter that shadows a local the frame knows is the block's value, not the local's.
    #[test]
    fn a_block_parameter_shadowing_a_known_local_is_not_that_local() {
        let tree: std::collections::HashMap<std::path::PathBuf, Vec<u8>> = [
            ("db/schema.rb", "ActiveRecord::Schema.define do\nend\n"),
            ("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\nend\n"),
            ("lib/thing.rb", "class Thing\n  def update\n    @state = 1\n  end\nend\n"),
            ("lib/other.rb", "class Other\n  def update\n    @other = 2\n  end\nend\n"),
            ("config/routes.rb", "Rails.application.routes.draw do\nend\n"),
        ]
        .into_iter()
        .map(|(p, c)| (std::path::PathBuf::from(p), c.as_bytes().to_vec()))
        .collect();
        let app = crate::ingest::ingest_app_from_tree(tree).expect("ingest");
        let mut eval = Eval::new(&app);
        eval.heap.push(Object { class: "Thing".into(), ivars: HashMap::default() });
        let mut locals: HashMap<Symbol, Val> = HashMap::default();
        locals.insert(Symbol::from("config"), Val::Obj(0));
        let shadowed = statement("items.each { |config| config.update }\n");
        let writes = eval.may_write_with(&shadowed, "*", false, &mut HashSet::default(), &locals).expect("bounded");
        assert!(writes.iter().any(|(owner, _, name)| owner == "Other" && name == "other"), "{writes:?}");
    }
}
