//! A class's settings methods defined from `config/site_settings.yml` as it loads (discourse's `SiteSetting`).
//!
//! The class `extend`s a module whose `load_settings(file)` yields each entry of the file to `setting(name, default,
//! opts)`, which calls `setup_methods(name)`: `define_singleton_method` blocks chosen by the setting's type, closing over
//! locals computed from it. No call to `SiteSetting.title` resolved, since nothing names the method in source.
//!
//! Here `setup_methods` is evaluated per entry, as the class loads it: its locals and conditions from the entry's
//! type, enum class and options, as `SiteSettings::TypeSupervisor` computes them, and each `define_singleton_method`
//! reached becomes the `def self.…` it defines, its block's body with the closed-over locals written as their values.
//! The class still runs the loader, which defines the same methods over these as it loads (or, for a setting
//! `GlobalSetting` shadows, the shadowed ones).
//!
//! An entry this reads no value for (a YAML scalar Psych may resolve otherwise, a type from a locale default, an enum
//! class answering `wrap` by a method it defines as it loads) gets no methods; neither does the whole file when
//! `setup_methods` holds anything this does not evaluate. Only settings the app's source names after a `.` are written.

use crate::det::{HashMap, HashSet};
use crate::dialect::{MethodReceiver, ModelBodyItem};
use crate::span::{SourceFile, Span};

use super::util::{constant_id_str, flatten_statements};
use crate::ingest::prism_api::PrismLocation;

use yaml_rust2::parser::{Event, Parser};
use yaml_rust2::scanner::TScalarStyle;

const FILE: &str = "config/site_settings.yml";

/// `SiteSettings::TypeSupervisor.types`' names.
const TYPES: &[&str] = &[
    "string", "time", "integer", "float", "bool", "null", "enum", "list", "url_list", "host_list", "category_list",
    "value_list", "regex", "email", "username", "category", "uploaded_image_list", "upload", "group", "group_list",
    "tag_list", "color", "simple_list", "emoji_list", "html_deprecated", "tag_group_list", "file_size_restriction",
    "objects", "locale_enum", "topic", "datetime", "icon", "date",
];

#[derive(Clone, Debug, PartialEq)]
enum Node {
    /// A scalar with its Ruby value, or `None` where Psych's reading is not certain here.
    Scalar(Option<Value>),
    Map(Vec<(String, Node)>),
    Seq(Vec<Node>),
}

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Nil,
    Bool(bool),
    Int,
    Float,
    Str(String),
    Sym(String),
    Const(String),
    List(Vec<Value>),
}

/// What Psych 4's `ScalarScanner#tokenize` (legacy integers, symbols parsed) answers for a plain scalar, by class:
/// `None` for a Time, Date, special float, Symbol or base-60 number, which no setting's type is read from here.
fn plain(text: &str) -> Option<Value> {
    use std::sync::LazyLock;
    static WORD: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"^[^\d.:-]?[[:alpha:]_\s!@#$%\^&*(){}<>|/\\~;=]+").unwrap());
    static OTHER: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
        [
            r"^-?\d{4}-\d{1,2}-\d{1,2}(?:[Tt]|\s+)\d{1,2}:\d\d:\d\d(?:\.\d*)?(?:\s*(?:Z|[-+]\d{1,2}:?(?:\d\d)?))?$",
            r"^\d{4}-(?:1[012]|0\d|\d)-(?:[12]\d|3[01]|0\d|\d)$",
            r"(?i)^\+?\.inf$",
            r"(?i)^-\.inf$",
            r"(?i)^\.nan$",
            r"^:.",
            r"^[-+]?[0-9][0-9_]*(:[0-5]?[0-9]){1,2}$",
            r"^[-+]?[0-9][0-9_]*(:[0-5]?[0-9]){1,2}\.[0-9_]*$",
        ]
        .iter()
        .map(|r| regex::Regex::new(r).unwrap())
        .collect()
    });
    static FLOAT: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"^(?:[-+]?([0-9][0-9_,]*)?\.[0-9]*([eE][-+][0-9]+)?)$").unwrap());
    static INTEGER: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^(?:[-+]?0b[_,]*[0-1][0-1_,]*|[-+]?0[_,]*[0-7][0-7_,]*|[-+]?(?:0|[1-9](?:[0-9]|,[0-9]|_[0-9])*)|[-+]?0x[_,]*[0-9a-fA-F][0-9a-fA-F_,]*)$").unwrap()
    });
    if text.is_empty() {
        return Some(Value::Nil);
    }
    if WORD.is_match(text) || text.contains('\n') {
        if text.chars().count() > 5 {
            return Some(Value::Str(text.to_string()));
        }
        let lower = text.to_ascii_lowercase();
        return Some(if !lower.starts_with(['y', 't', 'o', 'n', 'f', '~']) {
            Value::Str(text.to_string())
        } else if text == "~" || lower == "null" {
            Value::Nil
        } else if matches!(lower.as_str(), "yes" | "true" | "on") {
            Value::Bool(true)
        } else if matches!(lower.as_str(), "no" | "false" | "off") {
            Value::Bool(false)
        } else {
            Value::Str(text.to_string())
        });
    }
    if OTHER.iter().any(|r| r.is_match(text)) {
        return None;
    }
    if FLOAT.is_match(text) {
        return Some(if matches!(text, "." | "+." | "-.") { Value::Str(text.to_string()) } else { Value::Float });
    }
    if INTEGER.is_match(text) {
        return Some(Value::Int);
    }
    Some(Value::Str(text.to_string()))
}

fn read_yaml(source: &str) -> Result<Node, String> {
    struct Reader<'a> {
        parser: Parser<std::str::Chars<'a>>,
        anchors: HashMap<usize, Node>,
        /// Nodes left to copy out of aliases: each copies its anchor's whole subtree.
        budget: usize,
    }
    fn size(node: &Node) -> usize {
        match node {
            Node::Scalar(_) => 1,
            Node::Map(entries) => 1 + entries.iter().map(|(_, v)| size(v)).sum::<usize>(),
            Node::Seq(items) => 1 + items.iter().map(size).sum::<usize>(),
        }
    }
    impl Reader<'_> {
        fn next(&mut self) -> Result<Event, String> {
            self.parser.next_token().map(|(e, _)| e).map_err(|e| e.to_string())
        }
        fn node(&mut self, depth: usize) -> Result<Node, String> {
            if depth > 64 {
                return Err("nesting".into());
            }
            let (node, anchor) = match self.next()? {
                Event::Scalar(text, style, anchor, tag) => {
                    let value = match (tag, style) {
                        (Some(_), _) => None,
                        (None, TScalarStyle::Plain) => plain(&text),
                        (None, _) => Some(Value::Str(text)),
                    };
                    (Node::Scalar(value), anchor)
                }
                Event::MappingStart(anchor, tag) => {
                    if tag.is_some() {
                        return Err("a tagged mapping".into());
                    }
                    let mut entries = Vec::new();
                    while self.parser.peek().map_err(|e| e.to_string())?.0 != Event::MappingEnd {
                        let key = match self.next()? {
                            Event::Scalar(text, style, 0, None) if style != TScalarStyle::Plain || matches!(plain(&text), Some(Value::Str(_))) => text,
                            _ => return Err("a key other than a String".into()),
                        };
                        if key == "<<" {
                            return Err("a merge key".into());
                        }
                        let value = self.node(depth + 1)?;
                        // Psych's Hash keeps a repeated key where it first stood, with its last value.
                        match entries.iter_mut().find(|(k, _): &&mut (String, Node)| *k == key) {
                            Some(entry) => entry.1 = value,
                            None => entries.push((key, value)),
                        }
                    }
                    self.next()?;
                    (Node::Map(entries), anchor)
                }
                Event::SequenceStart(anchor, _) => {
                    let mut items = Vec::new();
                    while self.parser.peek().map_err(|e| e.to_string())?.0 != Event::SequenceEnd {
                        items.push(self.node(depth + 1)?);
                    }
                    self.next()?;
                    (Node::Seq(items), anchor)
                }
                Event::Alias(id) => {
                    let node = self.anchors.get(&id).ok_or_else(|| "an alias".to_string())?;
                    self.budget = self.budget.checked_sub(size(node)).ok_or_else(|| "aliases copying more than the file holds".to_string())?;
                    return Ok(node.clone());
                }
                other => return Err(format!("{other:?}")),
            };
            if anchor != 0 {
                self.anchors.insert(anchor, node.clone());
            }
            Ok(node)
        }
    }
    let mut reader = Reader { parser: Parser::new_from_str(source), anchors: HashMap::default(), budget: source.len() };
    if reader.next()? != Event::StreamStart || reader.next()? != Event::DocumentStart {
        return Err("no document".into());
    }
    reader.node(0)
}

struct Entry {
    name: String,
    /// `TypeSupervisor#get_type`'s answer, or none where it is not read here.
    ty: Option<String>,
    opts: Vec<(String, Node)>,
}

impl Entry {
    fn opt(&self, key: &str) -> Option<&Node> {
        self.opts.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    fn opt_str(&self, key: &str) -> Option<String> {
        match self.opt(key) {
            Some(Node::Scalar(Some(Value::Str(s)))) => Some(s.clone()),
            _ => None,
        }
    }
}

fn value_type(v: &Value) -> Option<&'static str> {
    Some(match v {
        Value::Nil => "null",
        Value::Str(_) => "string",
        Value::Int => "integer",
        Value::Float => "float",
        Value::Bool(_) => "bool",
        _ => return None,
    })
}

/// The file's entries as `SiteSettings::YamlLoader` yields them, each with its type.
fn entries(source: &str) -> Result<Vec<Entry>, String> {
    let Node::Map(categories) = read_yaml(source)? else { return Err("not a mapping".into()) };
    let mut out = Vec::new();
    for (_, settings) in categories {
        let Node::Map(settings) = settings else { return Err("a category other than a mapping".into()) };
        for (name, node) in settings {
            let (default, opts) = match node {
                Node::Map(opts) => {
                    let default = opts.iter().find(|(k, _)| k == "default").map(|(_, v)| v.clone());
                    (default, opts.into_iter().filter(|(k, _)| k != "default").collect::<Vec<_>>())
                }
                scalar => (Some(scalar), Vec::new()),
            };
            let mut entry = Entry { name, ty: None, opts };
            let st = entry.opt_str("type").or_else(|| entry.opt("enum").map(|_| "enum".to_string()));
            entry.ty = match st.filter(|t| TYPES.contains(&t.as_str())) {
                Some(t) => Some(t),
                None => {
                    let ty = match default {
                        Some(Node::Scalar(Some(v))) => value_type(&v).map(str::to_string),
                        _ => None,
                    };
                    // The default the type is read from is the site locale's, when the entry has one per locale.
                    let locales_agree = match entry.opt("locale_default") {
                        None => true,
                        Some(Node::Map(per)) => per.iter().all(|(_, v)| matches!(v, Node::Scalar(Some(v)) if value_type(v).map(str::to_string) == ty)),
                        Some(_) => false,
                    };
                    ty.filter(|_| locales_agree)
                }
            };
            out.push(entry);
        }
    }
    Ok(out)
}

/// The class that loads the file, the module it extends for it, and that module's `setup_methods` source.
struct Loader<'a> {
    class: String,
    module_source: &'a SourceFile,
    setup: (usize, usize),
}

fn find_loader(sources: &[SourceFile]) -> Option<Loader<'_>> {
    let call = format!("load_settings(Rails.root.join(\"{FILE}\").to_s)");
    let class_source = sources.iter().find(|s| s.text.contains(&call))?;
    let parsed = super::prism::parse_silent(class_source.text.as_bytes());
    let mut found = Vec::new();
    super::active_model_serializer::collect_classes(&parsed.node(), &mut Vec::new(), &mut found);
    let (class, module) = found.iter().find_map(|(path, class)| {
        let stmts = class.body().map(flatten_statements).unwrap_or_default();
        stmts.iter().any(|s| &class_source.text[s.location().start_offset()..s.location().end_offset()] == call).then_some(())?;
        let module = stmts.iter().find_map(|s| {
            let c = s.as_call_node().filter(|c| c.receiver().is_none() && constant_id_str(&c.name()) == "extend")?;
            let args: Vec<_> = c.arguments()?.arguments().iter().collect();
            let [arg] = args.as_slice() else { return None };
            let name = super::util::constant_path_of(arg)?.join("::");
            let module_source = sources.iter().find(|m| m.text.contains(&format!("module {name}\n")) && m.text.contains("def setup_methods(name)"))?;
            Some((module_source, name))
        })?;
        Some((path.join("::"), module))
    })?;
    let (module, module_name) = module;
    // The module's loader is the one that yields each entry to `setting`, which defines through `setup_methods`.
    let text = &module.text;
    if !(text.contains(".load do |category, name, default, opts|") && text.contains("setting(name, default, opts.merge(") && text.contains("setup_methods(name)")) {
        return None;
    }
    let mparsed = super::prism::parse_silent(text.as_bytes());
    let program = mparsed.node();
    let def = program.as_program_node()?.statements().body().iter().filter_map(|s| s.as_module_node()).filter(|m| {
        super::util::constant_path_of(&m.constant_path()).is_some_and(|p| p.join("::") == module_name)
    }).find_map(|m| {
        m.body().map(flatten_statements).unwrap_or_default().into_iter().find_map(|s| {
            s.as_def_node().filter(|d| d.receiver().is_none() && constant_id_str(&d.name()) == "setup_methods").map(|d| (d.location().start_offset(), d.location().end_offset()))
        })
    })?;
    Some(Loader { class, module_source: module, setup: def })
}

/// What the app's source says of a constant's class for `klass.respond_to?(:wrap)`: true where it or an ancestor
/// writes `def self.wrap`, false where none does and its body calls nothing that may define one; none otherwise.
fn answers_wrap(class: &str, sources: &[SourceFile]) -> Option<bool> {
    let mut current = class.to_string();
    for _ in 0..8 {
        let source = sources.iter().find(|s| s.text.contains(&format!("class {current}")))?;
        let parsed = super::prism::parse_silent(source.text.as_bytes());
        let mut found = Vec::new();
        super::active_model_serializer::collect_classes(&parsed.node(), &mut Vec::new(), &mut found);
        let (_, node) = found.into_iter().find(|(p, _)| p.join("::") == current)?;
        for stmt in node.body().map(flatten_statements).unwrap_or_default() {
            if let Some(def) = stmt.as_def_node() {
                if def.receiver().is_some() && constant_id_str(&def.name()) == "wrap" {
                    return Some(true);
                }
                continue;
            }
            // `wrap_values!`: a class method run as the class loads may define anything.
            if let Some(call) = stmt.as_call_node() {
                let visibility = call.receiver().is_none() && matches!(constant_id_str(&call.name()), "private_class_method" | "public_class_method" | "private_constant");
                let names_wrap = call.arguments().is_some_and(|a| a.arguments().iter().any(|x| super::util::symbol_value(&x).as_deref() == Some("wrap")));
                if !visibility || names_wrap {
                    return None;
                }
            }
        }
        let Some(parent) = node.superclass() else { return Some(false) };
        let parent = super::util::constant_path_of(&parent)?.join("::");
        if parent == "Object" {
            return Some(false);
        }
        current = parent;
    }
    None
}

/// `setup_methods(name)` run for one entry: each method it defines, as `(name, params, body)` source, or why not.
fn evaluate(entry: &Entry, setup: &ruby_prism::DefNode<'_>, text: &str, sources: &[SourceFile]) -> Result<Vec<(String, String, String, (usize, usize))>, String> {
    let mut env: HashMap<String, Value> = HashMap::default();
    let param = setup
        .parameters()
        .and_then(|p| p.requireds().iter().next())
        .and_then(|r| r.as_required_parameter_node().map(|r| constant_id_str(&r.name()).to_string()))
        .ok_or("`setup_methods` without its name")?;
    env.insert(param, Value::Sym(entry.name.clone()));
    let mut out = Vec::new();
    let body = setup.body().map(flatten_statements).unwrap_or_default();
    run(&body, entry, &mut env, &mut out, text, sources)?;
    Ok(out)
}

type Defined = Vec<(String, String, String, (usize, usize))>;

fn run(stmts: &[ruby_prism::Node<'_>], entry: &Entry, env: &mut HashMap<String, Value>, out: &mut Defined, text: &str, sources: &[SourceFile]) -> Result<Option<Value>, String> {
    let mut last = Some(Value::Nil);
    for stmt in stmts {
        last = Some(statement(stmt, entry, env, out, text, sources)?);
    }
    Ok(last)
}

fn statement(stmt: &ruby_prism::Node<'_>, entry: &Entry, env: &mut HashMap<String, Value>, out: &mut Defined, text: &str, sources: &[SourceFile]) -> Result<Value, String> {
    if let Some(w) = stmt.as_local_variable_write_node() {
        // A block already defined reads the local as it is when called: the later value, not the one written in.
        if !out.is_empty() && env.contains_key(constant_id_str(&w.name())) {
            return Err("a local rebound after a method closes over it".into());
        }
        let v = statement(&w.value(), entry, env, out, text, sources)?;
        env.insert(constant_id_str(&w.name()).to_string(), v.clone());
        return Ok(v);
    }
    if let Some(i) = stmt.as_if_node() {
        let cond = expr(&i.predicate(), entry, env, sources)?;
        let truthy = !matches!(cond, Value::Nil | Value::Bool(false));
        if truthy {
            let stmts = i.statements().map(|s| s.body().iter().collect::<Vec<_>>()).unwrap_or_default();
            return Ok(run(&stmts, entry, env, out, text, sources)?.unwrap_or(Value::Nil));
        }
        return match i.subsequent() {
            None => Ok(Value::Nil),
            Some(next) if next.as_if_node().is_some() => statement(&next, entry, env, out, text, sources),
            Some(next) => {
                let e = next.as_else_node().ok_or("an `if` tail other than `else`")?;
                let stmts = e.statements().map(|s| s.body().iter().collect::<Vec<_>>()).unwrap_or_default();
                Ok(run(&stmts, entry, env, out, text, sources)?.unwrap_or(Value::Nil))
            }
        };
    }
    if let Some(call) = stmt.as_call_node()
        && call.receiver().is_none()
        && constant_id_str(&call.name()) == "define_singleton_method"
    {
        let args: Vec<_> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
        let [arg] = args.as_slice() else { return Err("`define_singleton_method` of other than a name".into()) };
        let name = match expr(arg, entry, env, sources)? {
            Value::Sym(s) | Value::Str(s) => s,
            _ => return Err("a method name other than a Symbol or String".into()),
        };
        let block = call.block().and_then(|b| b.as_block_node()).ok_or("`define_singleton_method` without a block")?;
        let params = match block.parameters().and_then(|p| p.as_block_parameters_node()).and_then(|p| p.parameters()) {
            Some(p) => {
                struct Reads<'a>(&'a HashMap<String, Value>, bool);
                impl<'pr> ruby_prism::Visit<'pr> for Reads<'_> {
                    fn visit_local_variable_read_node(&mut self, node: &ruby_prism::LocalVariableReadNode<'pr>) {
                        self.1 |= node.depth() > 0 && self.0.contains_key(constant_id_str(&node.name()));
                    }
                }
                let mut reads = Reads(env, false);
                ruby_prism::Visit::visit(&mut reads, &p.as_node());
                if reads.1 {
                    return Err("a parameter default reading a local the block closes over".into());
                }
                let l = p.location();
                text[l.start_offset()..l.end_offset()].to_string()
            }
            None => String::new(),
        };
        let body = match block.body() {
            Some(b) => substitute(&b, env, text)?,
            None => "nil".to_string(),
        };
        out.push((name, params, body, (call.location().start_offset(), call.location().end_offset())));
        return Ok(Value::Nil);
    }
    expr(stmt, entry, env, sources)
}

/// A block's body with each closed-over local written as its value.
fn substitute(body: &ruby_prism::Node<'_>, env: &HashMap<String, Value>, text: &str) -> Result<String, String> {
    // A read reaches past the `define_singleton_method` block when its depth passes the blocks nested inside it.
    struct Locals<'a> {
        env: &'a HashMap<String, Value>,
        text: &'a str,
        reads: Vec<(usize, usize, String)>,
        writes: bool,
        nesting: u32,
        failed: Option<String>,
    }
    impl Locals<'_> {
        /// A closed-over local's value, where `node` reads one.
        fn closed(&self, node: &ruby_prism::Node<'_>) -> Option<&Value> {
            let r = node.as_local_variable_read_node()?;
            (r.depth() > self.nesting).then(|| self.env.get(constant_id_str(&r.name()))).flatten()
        }
        /// `return x.wrap(v) if enum_wrapper` with the wrapper nil: the branch its value takes, as the method runs it.
        fn fold(&mut self, node: &ruby_prism::Node<'_>, taken: Option<ruby_prism::Node<'_>>) {
            let text = match taken {
                None => "nil".to_string(),
                Some(branch) => {
                    let mut inner = Locals { env: self.env, text: self.text, reads: Vec::new(), writes: false, nesting: self.nesting, failed: None };
                    ruby_prism::Visit::visit(&mut inner, &branch);
                    self.writes |= inner.writes;
                    if inner.failed.is_some() {
                        self.failed = inner.failed;
                    }
                    format!("begin\n{}\nend", spliced(self.text, &branch, inner.reads))
                }
            };
            self.reads.push((node.location().start_offset(), node.location().end_offset(), text));
        }
    }
    impl<'pr> ruby_prism::Visit<'pr> for Locals<'_> {
        fn visit_if_node(&mut self, node: &ruby_prism::IfNode<'pr>) {
            if let Some(v) = self.closed(&node.predicate()).cloned() {
                if matches!(v, Value::Nil | Value::Bool(_)) {
                    let taken = if truthy(&v) {
                        node.statements().map(|s| s.as_node())
                    } else {
                        match node.subsequent() {
                            None => None,
                            Some(e) => match e.as_else_node() {
                                Some(e) => e.statements().map(|s| s.as_node()),
                                None => {
                                    self.failed = Some("an `elsif` on a closed-over local".into());
                                    return;
                                }
                            },
                        }
                    };
                    self.fold(&node.as_node(), taken);
                    return;
                }
            }
            ruby_prism::visit_if_node(self, node);
        }
        fn visit_local_variable_read_node(&mut self, node: &ruby_prism::LocalVariableReadNode<'pr>) {
            if node.depth() > self.nesting
                && let Some(v) = self.env.get(constant_id_str(&node.name()))
            {
                self.reads.push((node.location().start_offset(), node.location().end_offset(), literal(v)));
            }
        }
        fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'pr>) {
            self.writes |= node.depth() > self.nesting;
            ruby_prism::visit_local_variable_write_node(self, node);
        }
        fn visit_block_node(&mut self, node: &ruby_prism::BlockNode<'pr>) {
            self.nesting += 1;
            ruby_prism::visit_block_node(self, node);
            self.nesting -= 1;
        }
        fn visit_lambda_node(&mut self, node: &ruby_prism::LambdaNode<'pr>) {
            self.nesting += 1;
            ruby_prism::visit_lambda_node(self, node);
            self.nesting -= 1;
        }
    }
    let mut locals = Locals { env, text, reads: Vec::new(), writes: false, nesting: 0, failed: None };
    ruby_prism::Visit::visit(&mut locals, body);
    if locals.writes {
        return Err("a block writing a local it closes over".into());
    }
    if let Some(reason) = locals.failed {
        return Err(reason);
    }
    Ok(spliced(text, body, locals.reads))
}

/// `node`'s source with each `(start, end, text)` written in.
fn spliced(text: &str, node: &ruby_prism::Node<'_>, mut reads: Vec<(usize, usize, String)>) -> String {
    let (start, end) = (node.location().start_offset(), node.location().end_offset());
    let mut out = text[start..end].to_string();
    reads.sort();
    for (a, b, lit) in reads.into_iter().rev() {
        out.replace_range(a - start..b - start, &lit);
    }
    out
}

fn literal(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Bool(b) => b.to_string(),
        Value::Sym(s) => format!(":{}", serde_json::to_string(s).unwrap().replace('#', "\\#")),
        Value::Str(s) => serde_json::to_string(s).unwrap().replace('#', "\\#"),
        Value::Const(c) => format!("::{c}"),
        Value::Int | Value::Float | Value::List(_) => unreachable!("closed-over values are names, constants or nil"),
    }
}

fn truthy(v: &Value) -> bool {
    !matches!(v, Value::Nil | Value::Bool(false))
}

fn expr(node: &ruby_prism::Node<'_>, entry: &Entry, env: &HashMap<String, Value>, sources: &[SourceFile]) -> Result<Value, String> {
    let unknown = || Err(format!("`{}` is not evaluated", String::from_utf8_lossy(node.location().as_slice())));
    if let Some(r) = node.as_local_variable_read_node() {
        return env.get(constant_id_str(&r.name())).cloned().ok_or_else(|| "an unbound local".to_string());
    }
    if let Some(s) = node.as_symbol_node() {
        return Ok(Value::Sym(String::from_utf8_lossy(s.unescaped()).into_owned()));
    }
    if let Some(s) = node.as_string_node() {
        return Ok(Value::Str(String::from_utf8_lossy(s.unescaped()).into_owned()));
    }
    if node.as_nil_node().is_some() {
        return Ok(Value::Nil);
    }
    if let Some(p) = node.as_parentheses_node() {
        let stmts: Vec<_> = p.body().map(flatten_statements).unwrap_or_default();
        let [one] = stmts.as_slice() else { return unknown() };
        return expr(one, entry, env, sources);
    }
    if let Some(s) = node.as_interpolated_string_node() {
        let mut out = String::new();
        for part in s.parts().iter() {
            if let Some(t) = part.as_string_node() {
                out.push_str(&String::from_utf8_lossy(t.unescaped()));
                continue;
            }
            let e = part.as_embedded_statements_node().ok_or("an interpolation")?;
            let stmts: Vec<_> = e.statements().map(|s| s.body().iter().collect()).unwrap_or_default();
            let [one] = stmts.as_slice() else { return unknown() };
            match expr(one, entry, env, sources)? {
                Value::Sym(s) | Value::Str(s) => out.push_str(&s),
                _ => return unknown(),
            }
        }
        return Ok(Value::Str(out));
    }
    if let Some(a) = node.as_array_node() {
        return Ok(Value::List(a.elements().iter().map(|e| expr(&e, entry, env, sources)).collect::<Result<_, _>>()?));
    }
    if let Some(a) = node.as_and_node() {
        let l = expr(&a.left(), entry, env, sources)?;
        return if truthy(&l) { expr(&a.right(), entry, env, sources) } else { Ok(l) };
    }
    if let Some(o) = node.as_or_node() {
        let l = expr(&o.left(), entry, env, sources)?;
        return if truthy(&l) { Ok(l) } else { expr(&o.right(), entry, env, sources) };
    }
    if let Some(i) = node.as_if_node() {
        // `klass if klass.respond_to?(:wrap)`, and an `if` read for its value.
        let cond = expr(&i.predicate(), entry, env, sources)?;
        if i.subsequent().is_some() {
            return unknown();
        }
        let stmts: Vec<_> = i.statements().map(|s| s.body().iter().collect()).unwrap_or_default();
        if !truthy(&cond) {
            return Ok(Value::Nil);
        }
        let mut scratch = env.clone();
        let mut last = Value::Nil;
        for s in &stmts {
            if let Some(w) = s.as_local_variable_write_node() {
                last = expr(&w.value(), entry, &scratch, sources)?;
                scratch.insert(constant_id_str(&w.name()).to_string(), last.clone());
            } else {
                last = expr(s, entry, &scratch, sources)?;
            }
        }
        return Ok(last);
    }
    let Some(call) = node.as_call_node() else { return unknown() };
    let method = constant_id_str(&call.name());
    let args: Vec<_> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
    if call.block().is_some() {
        return unknown();
    }
    let Some(recv) = call.receiver() else { return unknown() };
    // `type_supervisor.get_type(name)` and its kin, read from the entry.
    if recv.as_call_node().is_some_and(|r| r.receiver().is_none() && r.arguments().is_none() && constant_id_str(&r.name()) == "type_supervisor") {
        let [arg] = args.as_slice() else { return unknown() };
        if expr(arg, entry, env, sources)? != Value::Sym(entry.name.clone()) {
            return unknown();
        }
        return match method {
            "get_type" => entry.ty.clone().map(Value::Sym).ok_or_else(|| "the setting's type is not read".to_string()),
            "get_enum_class" => Ok(entry.opt_str("enum").map(Value::Const).unwrap_or(Value::Nil)),
            "get_list_type" => Ok(match (entry.opt_str("type").as_deref(), entry.opt_str("list_type")) {
                (Some("list"), Some(t)) => Value::Str(t),
                _ => Value::Nil,
            }),
            _ => unknown(),
        };
    }
    if super::util::constant_path_of(&recv).is_some_and(|p| p.join("::") == "UpcomingChanges") && method == "exists?" {
        let [arg] = args.as_slice() else { return unknown() };
        if expr(arg, entry, env, sources)? != Value::Sym(entry.name.clone()) {
            return unknown();
        }
        return Ok(Value::Bool(matches!(entry.opt("upcoming_change"), Some(Node::Map(m)) if !m.is_empty())));
    }
    let r = expr(&recv, entry, env, sources)?;
    if call.is_safe_navigation() && r == Value::Nil {
        return Ok(Value::Nil);
    }
    let a: Vec<Value> = args.iter().map(|x| expr(x, entry, env, sources)).collect::<Result<_, _>>()?;
    Ok(match (method, &r, a.as_slice()) {
        ("to_s", Value::Sym(s) | Value::Str(s), []) => Value::Str(s.clone()),
        ("to_s", Value::Nil, []) => Value::Str(String::new()),
        ("to_sym", Value::Str(s) | Value::Sym(s), []) => Value::Sym(s.clone()),
        ("sub", Value::Str(s), [Value::Str(from), Value::Str(to)]) => Value::Str(s.replacen(from.as_str(), to, 1)),
        ("==", x, [y]) => Value::Bool(x == y),
        ("nil?", x, []) => Value::Bool(*x == Value::Nil),
        ("include?", Value::List(items), [x]) => Value::Bool(items.iter().any(|i| {
            // `%i[list]` holds Symbols and `%w[simple]` Strings: a Symbol is no String.
            i == x
        })),
        ("respond_to?", Value::Const(c), [Value::Sym(m)]) if m == "wrap" => {
            Value::Bool(answers_wrap(c, sources).ok_or_else(|| format!("whether {c} answers `wrap` as it loads is not read"))?)
        }
        ("respond_to?", Value::Nil, [Value::Sym(m)]) if m == "wrap" => Value::Bool(false),
        _ => return unknown(),
    })
}

/// Write the settings methods into the class that loads the file.
pub(super) fn lower(app: &mut crate::App, sources: &[SourceFile], yml: Option<String>) {
    let Some(yml) = yml else { return };
    let Some(loader) = find_loader(sources) else { return };
    let ledger = |message: String| {
        super::survey::record(&super::IngestError::Unsupported { file: FILE.to_string(), message });
    };
    let entries = match entries(&yml) {
        Ok(e) => e,
        Err(e) => return ledger(format!("{FILE} not read: {e}")),
    };
    let text = &loader.module_source.text;
    let parsed = super::prism::parse_silent(text.as_bytes());
    let program = parsed.node();
    let Some(setup) = program.as_program_node().into_iter().flat_map(|p| p.statements().body().iter().collect::<Vec<_>>()).filter_map(|s| s.as_module_node()).find_map(|m| {
        m.body().map(flatten_statements).unwrap_or_default().into_iter().find_map(|d| d.as_def_node().filter(|d| d.location().start_offset() == loader.setup.0))
    }) else {
        return;
    };
    // Only what some source reads after a `.`: the rest no call site names.
    let mut named: HashSet<String> = HashSet::default();
    for s in sources {
        let bytes = s.text.as_bytes();
        let mut i = 0;
        while let Some(off) = s.text[i..].find('.') {
            let start = i + off + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > start {
                named.insert(s.text[start..end].to_string());
            }
            i = start;
        }
    }
    let Some(model) = app.models.iter().position(|m| m.name.0.as_str() == loader.class) else { return };
    let own: HashSet<String> = app.models[model].methods().filter(|m| m.receiver == MethodReceiver::Class).map(|m| m.name.as_str().to_string()).collect();
    let mut seen: HashSet<String> = HashSet::default();
    let mut methods: Vec<(String, (usize, usize))> = Vec::new();
    let mut defined: HashSet<String> = HashSet::default();
    for entry in &entries {
        let clean = entry.name.replacen('?', "", 1);
        // `setting` raises for it, and a repeated name redefines what the first defined.
        if entry.name == "default_locale" || !seen.insert(entry.name.clone()) {
            continue;
        }
        if !named.contains(&clean) && !named.contains(&format!("{clean}_map")) && !named.contains(&format!("{clean}_groups_map")) {
            continue;
        }
        let defs = match evaluate(entry, &setup, text, sources) {
            Ok(d) => d,
            Err(reason) => {
                ledger(format!("SiteSetting `{}`: {reason}", entry.name));
                continue;
            }
        };
        for (name, params, body, span) in defs {
            if own.contains(&name) || !defined.insert(name.clone()) {
                continue;
            }
            let params = if params.is_empty() { String::new() } else { format!("({params})") };
            methods.push((format!("  def self.{name}{params}\n    {body}\n  end\n"), span));
        }
    }
    if methods.is_empty() {
        return;
    }
    let src = format!("class {} < ActiveRecord::Base\n{}end\n", loader.class, methods.iter().map(|(m, _)| m.as_str()).collect::<String>());
    let (ingested, diags) = super::prism::scope(|| super::model::ingest_model(src.as_bytes(), "<site_settings>", &app.schema, &Default::default()));
    let synth = match ingested {
        Ok(Some(m)) if diags.is_empty() => m,
        _ => return ledger(format!("{FILE}: the settings methods did not ingest")),
    };
    let file = super::sources::file_id(&loader.module_source.path);
    // A constant keeps no span: its resolution reads the reference at its position, which is the block's here.
    fn mark(expr: &mut crate::expr::Expr, span: Span) {
        expr.span = if matches!(&*expr.node, crate::expr::ExprNode::Const { .. }) { Span::synthetic() } else { span };
        expr.node.for_each_child_mut(&mut |c| mark(c, span));
    }
    let mut spans = methods.iter().map(|(_, s)| *s);
    for item in synth.body {
        let ModelBodyItem::Method { mut method, .. } = item else { continue };
        let Some((start, end)) = spans.next() else { break };
        let span = Span { file, start: start as u32, end: end as u32 };
        mark(&mut method.body, span);
        method.name_span = span;
        app.models[model].body.push(ModelBodyItem::Method { method, leading_comments: Vec::new(), leading_blank_line: false });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_scalar_resolves_as_psych_does() {
        for (text, want) in [
            ("", Some(Value::Nil)),
            ("~", Some(Value::Nil)),
            ("yes", Some(Value::Bool(true))),
            ("Off", Some(Value::Bool(false))),
            ("5", Some(Value::Int)),
            ("1,000", Some(Value::Int)),
            ("0x1f", Some(Value::Int)),
            ("1.5", Some(Value::Float)),
            ("hello", Some(Value::Str("hello".into()))),
            ("s3.amazonaws.com", Some(Value::Str("s3.amazonaws.com".into()))),
            ("https://x.org/a", Some(Value::Str("https://x.org/a".into()))),
            ("2024-01-02", None),
            (":sym", None),
            ("1:30", None),
            (".inf", None),
        ] {
            assert_eq!(plain(text), want, "{text}");
        }
    }

    #[test]
    fn a_repeated_key_keeps_its_last_value() {
        let entries = entries("basic:\n  tags:\n    default: \"a|b\"\n    type: string\n    type: list\n").unwrap();
        assert_eq!(entries[0].ty.as_deref(), Some("list"));
    }

    #[test]
    fn aliases_copying_past_the_file_are_refused() {
        let mut yml = String::from("a: &a [x, x, x, x, x, x, x, x]\n");
        for i in 0..12 {
            yml.push_str(&format!("b{i}: &b{i} [*{p}, *{p}, *{p}, *{p}]\n", p = if i == 0 { "a".to_string() } else { format!("b{}", i - 1) }));
        }
        assert!(read_yaml(&yml).is_err());
    }
}
