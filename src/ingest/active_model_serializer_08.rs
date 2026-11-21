//! `ActiveModel::Serializer` as active_model_serializers 0.8 has it (the
//! version discourse runs), expanded before inference into ordinary
//! methods the way `active_model_serializer.rs` expands a 0.10 one.
//!
//! ```ruby
//! class ApplicationSerializer < ActiveModel::Serializer
//!   embed :ids, include: true
//! end
//!
//! class ArticleSerializer < ApplicationSerializer
//!   attributes :id, :title
//!   has_many :comments, serializer: CommentSerializer
//!   has_one :author, serializer: UserSerializer, embed: :objects
//!
//!   def include_title? = scope.nil?
//! end
//! ```
//!
//! 0.8 differs from 0.10 throughout: an attribute is read through a method
//! the class defines unless one is already defined, each name has an
//! `include_<name>?` gate the app may override, a key loses a trailing
//! `?`, `as_json` wraps the hash in a root named after the class, and an
//! association embeds its records, or their ids with the records
//! sideloaded beside the root, the way `embed` and `include:` say. The
//! classes keep their app parents (0.8 apps share a base serializer); the
//! one under `ActiveModel::Serializer` loses that parent and gains the
//! gem's instance API instead: `object`, `options`, `scope`, `include?`,
//! `attributes`, `serializable_hash`, `as_json`, `root_name`.
//!
//! `ActiveModel::ArraySerializer.new(xs, each_serializer: S, …).as_json`,
//! the serializer a literal constant and the options a literal Hash,
//! becomes `S.ams_array_as_json(xs, {…})`; any other use of
//! `ActiveModel::ArraySerializer` stays the gem's, the reported residue.
//!
//! STRICT: a declaration or option outside the subset fails the class (a
//! survey line under `--survey`) and its whole hierarchy, never a partial
//! lowering.

use crate::det::{HashMap, HashSet};

use crate::dialect::{LibraryClass, LibraryClassOrigin};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;
use crate::span::{SourceFile, Span};

use super::active_model_serializer::{collect_classes, for_each_rewritten_body, is_rooted, refuse};
use super::util::{constant_id_str, flatten_statements, symbol_value};
use crate::ingest::prism_api::PrismLocation;
use super::{IngestError, IngestResult};

const SERIALIZER: &str = "ActiveModel::Serializer";
const ARRAY_SERIALIZER: [&str; 2] = ["ActiveModel", "ArraySerializer"];

/// The public and protected instance methods an `ActiveModel::Serializer`
/// has under Rails (Ruby 4.0, active_model_serializers 0.8.4): the gem
/// defines no reader or `include_` method over one of them
/// (`method_defined?`), so an attribute of that name reads it.
const DEFINED: &[&str] = &[
    "!", "!=", "!~", "<=>", "==", "===", "__id__", "__send__", "_associations", "_associations=", "_associations?",
    "_attributes", "_attributes=", "_attributes?", "_embed", "_embed=", "_embed?", "_root", "_root=", "_root?",
    "_root_embed", "_root_embed=", "_root_embed?", "_serializable_hash", "acts_like?", "as_json", "attributes",
    "blank?", "cache", "cache=", "cache?", "class", "clone", "deep_dup", "define_singleton_method", "display", "dup",
    "duplicable?", "enum_for", "eql?", "equal?", "expand_cache_key", "extend", "freeze", "frozen?", "hash", "in?",
    "include!", "include?", "include_associations!", "include_meta", "inspect", "instance_eval", "instance_exec",
    "instance_of?", "instance_values", "instance_variable_defined?", "instance_variable_get",
    "instance_variable_names", "instance_variable_set", "instance_variables", "instrument", "is_a?", "itself",
    "kind_of?", "merge_association", "meta_key", "method", "methods", "nil?", "object", "object_id", "options",
    "perform_caching", "perform_caching=", "perform_caching?", "presence", "presence_in", "present?",
    "private_methods", "protected_methods", "public_method", "public_methods", "public_send",
    "read_attribute_for_serialization", "remove_instance_variable", "require_dependency", "respond_to?",
    "root_name", "scope", "send", "serializable_hash", "singleton_class", "singleton_method", "singleton_methods",
    "tap", "then", "to_enum", "to_json", "to_param", "to_query", "to_s", "to_yaml", "try", "try!", "url_options",
    "with", "with_options", "yield_self",
];

/// The gem's methods the expansion writes: an attribute of one of these
/// names reads it, as it reads the gem's.
const PROVIDED: &[&str] =
    &["object", "options", "scope", "include?", "attributes", "serializable_hash", "as_json", "root_name", "meta_key", "include_meta"];

/// Whether the app's lockfile pins active_model_serializers 0.8.
pub(super) fn applies(app: &crate::App) -> bool {
    app.gem_lock
        .as_ref()
        .and_then(|lock| lock.version_of("active_model_serializers"))
        .is_some_and(|v| v.starts_with("0.8."))
}

#[derive(Clone, PartialEq, Debug)]
enum Key {
    Sym(String),
    Str(String),
}

impl Key {
    fn ruby(&self) -> String {
        match self {
            Key::Sym(s) => format!(":{}", ruby_string(s)),
            Key::Str(s) => ruby_string(s),
        }
    }
}

/// A double-quoted Ruby literal; `#` escaped so nothing interpolates.
fn ruby_string(s: &str) -> String {
    serde_json::to_string(s).unwrap().replace('#', "\\#")
}

#[derive(Clone, Copy, PartialEq)]
enum Embed {
    Ids,
    Objects,
}

#[derive(Clone)]
enum Root {
    Unset,
    False,
    True,
    Name(Key),
}

#[derive(Clone)]
struct Assoc {
    many: bool,
    name: String,
    /// The target serializer's full name.
    serializer: String,
    embed: Option<Embed>,
    include: Option<bool>,
    key: Option<Key>,
    /// Where an included record is sideloaded: `root:`, else what the gem's
    /// `refine` computes as the class loads.
    root: Key,
    embed_key: String,
}

/// What a class body declares, in order.
enum Stmt {
    Attribute { name: String, key: String, at: usize },
    Association(Assoc, usize),
    Embed(Embed, bool),
    Root(Root),
}

/// A `def` in the class body: its name, where it sits, and whether it is
/// public or protected (what `method_defined?` sees).
struct Def {
    name: String,
    at: usize,
    visible: bool,
    /// Whether its body calls `super`.
    supers: bool,
    /// From an included mixin: an ancestor of the class, not the class.
    from_module: bool,
}

struct Own {
    stmts: Vec<Stmt>,
    defs: Vec<Def>,
    /// The mixins whose `self.included` declared into the class.
    hooked: Vec<String>,
    span: Span,
    source_index: usize,
    offset: usize,
}

/// What the class holds once its body and its ancestors' have run: the
/// gem's class attributes.
#[derive(Clone)]
struct Flat {
    attributes: Vec<(String, String)>,
    assocs: Vec<Assoc>,
    embed: Embed,
    root_embed: bool,
    root: Root,
}

impl Default for Flat {
    fn default() -> Self {
        Flat { attributes: Vec::new(), assocs: Vec::new(), embed: Embed::Objects, root_embed: false, root: Root::Unset }
    }
}

pub(super) fn lower(app: &mut crate::App, sources: &[SourceFile]) -> IngestResult<()> {
    if !applies(app) {
        return Ok(());
    }
    let names: HashSet<String> = app.library_classes.iter().map(|c| c.name.0.as_str().to_string()).collect();
    let parent_of: HashMap<String, String> = app
        .library_classes
        .iter()
        .filter_map(|c| {
            let written = c.parent.as_ref()?.0.as_str();
            Some((c.name.0.as_str().to_string(), resolve(written, c.name.0.as_str(), &names)))
        })
        .collect();
    // Each class whose app ancestry reaches the gem's class, with its chain from the root down.
    let mut chains: HashMap<String, Vec<String>> = HashMap::default();
    for name in &names {
        let mut chain = vec![name.clone()];
        let mut current = name.clone();
        let reached = loop {
            let Some(parent) = parent_of.get(&current) else { break false };
            if parent == SERIALIZER {
                break true;
            }
            if !names.contains(parent) || chain.contains(parent) || chain.len() > 32 {
                break false;
            }
            chain.push(parent.clone());
            current = parent.clone();
        };
        if reached {
            chain.reverse();
            chains.insert(name.clone(), chain);
        }
    }
    if chains.is_empty() {
        return Ok(());
    }
    let tree: HashSet<String> = chains.keys().cloned().collect();
    // Where each module of the app is defined, for a serializer's `include`.
    let mut modules: HashMap<String, usize> = HashMap::default();
    for lc in app.library_classes.iter().filter(|c| c.is_module) {
        let name = lc.name.0.as_str();
        let leaf = name.rsplit("::").next().unwrap_or(name);
        let found = sources.iter().position(|src| {
            src.text.contains(&format!("module {leaf}")) && module_body(&super::prism::parse_silent(src.text.as_bytes()).node(), name).is_some()
        });
        if let Some(index) = found {
            modules.insert(name.to_string(), index);
        }
    }
    let mut owns: HashMap<String, Own> = HashMap::default();
    let mut refused: HashMap<String, IngestError> = HashMap::default();
    for (index, source) in sources.iter().enumerate() {
        if !tree.iter().any(|n| source.text.contains(n.rsplit("::").next().unwrap_or(n))) {
            continue;
        }
        let parsed = super::prism::parse_silent(source.text.as_bytes());
        let mut found = Vec::new();
        collect_classes(&parsed.node(), &mut Vec::new(), &mut found);
        for (path, class) in found {
            let name = path.join("::");
            if !tree.contains(&name) || refused.contains_key(&name) {
                continue;
            }
            let offset = class.location().start_offset();
            if owns.remove(&name).is_some() {
                refused.insert(name, refuse(source, offset, "a reopened serializer is not supported"));
                continue;
            }
            match collect(&class, source, &name, &tree, &modules, sources) {
                Ok((stmts, defs, hooked)) => {
                    let span = Span { file: super::sources::file_id(&source.path), start: offset as u32, end: offset as u32 };
                    owns.insert(name, Own { stmts, defs, hooked, span, source_index: index, offset });
                }
                Err(e) => {
                    refused.insert(name, e);
                }
            }
        }
    }
    for name in &tree {
        if !owns.contains_key(name) && !refused.contains_key(name) {
            refused.insert(
                name.clone(),
                IngestError::Unsupported {
                    file: "<active_model_serializers>".into(),
                    message: format!("ActiveModel::Serializer subset: cannot find the source of {name}"),
                },
            );
        }
    }
    // The other targets refuse each reference by its top-level constant, which a namespace shared with other classes would over-reach.
    for name in &tree {
        if let Some((top, _)) = name.split_once("::")
            && !tree.contains(top)
            && let Some(own) = owns.get(name)
        {
            let source = &sources[own.source_index];
            refused.insert(
                name.clone(),
                refuse(source, own.offset, &format!("a serializer nested in `{top}`, which is not a serializer, is not supported")),
            );
        }
    }
    // The root's `super` from an override of the gem's API reaches the gem's class, which the expansion removes.
    for (name, chain) in &chains {
        let (1, Some(own)) = (chain.len(), owns.get(name)) else { continue };
        let api = PER_CLASS_API.iter().chain(ROOT_API).map(|(m, _)| *m).chain(["include?"]);
        if let Some(d) = own.defs.iter().find(|d| d.supers && api.clone().any(|m| m == d.name)) {
            let reason = format!("`super` in `def {}` reaches the gem's class, which the expansion removes", d.name);
            refused.entry(name.clone()).or_insert_with(|| refuse(&sources[own.source_index], own.offset, &reason));
        }
    }
    // A mixin's hook declares into whatever includes it, and the expansion removes it: a class outside the tree that
    // includes it would lose what the hook gave it.
    let includers: Vec<(String, String)> = app
        .library_classes
        .iter()
        .flat_map(|c| c.includes.iter().map(move |i| (c.name.0.as_str().to_string(), i.0.as_str().to_string())))
        .chain(app.models.iter().flat_map(|m| crate::analyze::model_includes(m).into_iter().map(move |i| (m.name.0.as_str().to_string(), i.0.as_str().to_string()))))
        .collect();
    for (name, own) in &owns {
        for module in &own.hooked {
            let leaf = module.rsplit("::").next().unwrap_or(module);
            if let Some((other, _)) = includers.iter().find(|(c, i)| {
                !tree.contains(c) && (i == module || i.rsplit("::").next() == Some(leaf))
            }) {
                let reason = format!("`include {module}`, whose `self.included` also runs for {other}, which is not a serializer");
                refused.entry(name.clone()).or_insert_with(|| refuse(&sources[own.source_index], own.offset, &reason));
            }
        }
    }
    // The gem reads an attribute through the record's `read_attribute_for_serialization`, which the expansion calls
    // by the attribute's own reader: an app that defines its own would read differently.
    if let Some((source, offset)) = sources.iter().find_map(|s| own_serialization_reader(s, &tree)) {
        for name in &tree {
            refused.entry(name.clone()).or_insert_with(|| {
                refuse(source, offset, "the app defines its own `read_attribute_for_serialization`, which the gem reads attributes through")
            });
        }
    }
    // One fixed point over both: a refused class takes down its descendants, which inherit what it would have
    // defined, and every class with an association serializing through it, which would call the gem's class.
    loop {
        let mut more: Vec<(String, IngestError)> = Vec::new();
        for (name, chain) in &chains {
            if refused.contains_key(name) || more.iter().any(|(n, _)| n == name) {
                continue;
            }
            let file = owns.get(name).map(|o| sources[o.source_index].path.clone()).unwrap_or_default();
            if let Some(ancestor) = chain.iter().find(|a| refused.contains_key(*a)) {
                more.push((
                    name.clone(),
                    IngestError::Unsupported {
                        file,
                        message: format!("ActiveModel::Serializer subset: {name} inherits from {ancestor}, which is refused"),
                    },
                ));
                continue;
            }
            let through = chain.iter().filter_map(|c| owns.get(c)).find_map(|own| {
                own.stmts.iter().find_map(|s| match s {
                    Stmt::Association(a, _) if refused.contains_key(&a.serializer) => Some(a.serializer.clone()),
                    _ => None,
                })
            });
            if let (Some(target), Some(own)) = (through, owns.get(name)) {
                more.push((
                    name.clone(),
                    refuse(&sources[own.source_index], own.offset, &format!("an association serializes through {target}, which is refused")),
                ));
            }
        }
        if more.is_empty() {
            break;
        }
        refused.extend(more);
    }
    let refused_names: HashSet<String> = refused.keys().cloned().collect();
    if !refused.is_empty() {
        let mut ordered: Vec<(String, IngestError)> = refused.into_iter().collect();
        ordered.sort_by(|a, b| a.0.cmp(&b.0));
        if !super::survey::is_active() {
            return Err(ordered.remove(0).1);
        }
        for (_, e) in &ordered {
            super::survey::record(e);
        }
    }
    let refused = refused_names;
    let lowered: Vec<String> = {
        let mut v: Vec<String> = tree.iter().filter(|n| !refused.contains(*n)).cloned().collect();
        v.sort_by_key(|n| chains[n].len());
        v
    };
    let mut flats: HashMap<String, Flat> = HashMap::default();
    for name in &lowered {
        let chain = &chains[name];
        let mut flat = if chain.len() > 1 { flats[&chain[chain.len() - 2]].clone() } else { Flat::default() };
        for stmt in &owns[name].stmts {
            match stmt {
                Stmt::Attribute { name, key, .. } => match flat.attributes.iter_mut().find(|(n, _)| n == name) {
                    Some(entry) => entry.1 = key.clone(),
                    None => flat.attributes.push((name.clone(), key.clone())),
                },
                Stmt::Association(a, _) => match flat.assocs.iter_mut().find(|x| x.name == a.name) {
                    Some(entry) => *entry = a.clone(),
                    None => flat.assocs.push(a.clone()),
                },
                Stmt::Embed(embed, include) => {
                    flat.embed = *embed;
                    if *include {
                        flat.root_embed = true;
                    }
                }
                Stmt::Root(root) => flat.root = root.clone(),
            }
        }
        flats.insert(name.clone(), flat);
    }
    let mut failed: HashSet<String> = HashSet::default();
    for name in &lowered {
        if chains[name].iter().any(|a| failed.contains(a)) {
            failed.insert(name.clone());
            continue;
        }
        let src = synthesize(name, &chains[name], &owns, &flats[name]);
        let Some(index) = app.library_classes.iter().position(|c| c.name.0.as_str() == name) else { continue };
        let before = app.library_classes[index].clone();
        if let Err(e) = install(&mut app.library_classes[index], &src, owns[name].span, chains[name].len() == 1) {
            app.library_classes[index] = before;
            failed.insert(name.clone());
            if !super::survey::is_active() {
                return Err(e);
            }
            super::survey::record(&e);
        }
    }
    let mut done: HashSet<String> = lowered.into_iter().filter(|n| !failed.contains(n)).collect();
    // The hook's declarations are expanded into each includer; replayed, `klass.attributes` has no gem to answer it.
    let unhook: HashSet<String> = owns.iter().filter(|(n, _)| done.contains(*n)).flat_map(|(_, o)| o.hooked.iter().cloned()).collect();
    for lc in app.library_classes.iter_mut().filter(|c| c.is_module && unhook.contains(c.name.0.as_str())) {
        lc.methods.retain(|m| !(m.receiver == crate::dialect::MethodReceiver::Class && m.name.as_str() == "included"));
    }
    // The rewrite reads `each_serializer:` by its spelling, not from where it is written: a name another
    // namespace also defines (`Admin::ItemSerializer` beside `ItemSerializer`) may be that one, so it stays the gem's.
    let all: Vec<String> = app
        .library_classes
        .iter()
        .map(|c| c.name.0.as_str().to_string())
        .chain(app.models.iter().map(|m| m.name.0.as_str().to_string()))
        .chain(app.controllers.iter().map(|c| c.name.0.as_str().to_string()))
        .collect();
    done.retain(|n| !all.iter().any(|o| o != n && o.ends_with(&format!("::{n}"))));
    if all.iter().any(|o| o == "ActiveModel::ArraySerializer" || o.ends_with("::ActiveModel::ArraySerializer")) {
        done.clear();
    }
    for_each_rewritten_body(app, &mut |body| rewrite_arrays(body, &done));
    Ok(())
}

/// Where `source` defines a `read_attribute_for_serialization` other than one that sends the name, as Active Record's
/// alias does (`alias read_attribute_for_serialization send`, `def …(name) = public_send(name)`): the expansion reads
/// an attribute by its reader, which only those match. A serializer's own is refused with its class.
fn own_serialization_reader<'s>(source: &'s SourceFile, tree: &HashSet<String>) -> Option<(&'s SourceFile, usize)> {
    if !source.text.contains("read_attribute_for_serialization") {
        return None;
    }
    let sends = regex::Regex::new(
        r"^\s*(?:alias\s+read_attribute_for_serialization\s+(?:send|__send__|public_send)|alias_method\s+:read_attribute_for_serialization,\s*:(?:send|__send__|public_send))\s*$",
    )
    .unwrap();
    let def = regex::Regex::new(
        r"^\s*def read_attribute_for_serialization\((\w+)\)\s*\n\s*(?:send|__send__|public_send)\((\w+)\)\s*\n\s*end",
    )
    .unwrap();
    let parsed = super::prism::parse_silent(source.text.as_bytes());
    let mut classes = Vec::new();
    collect_classes(&parsed.node(), &mut Vec::new(), &mut classes);
    let in_serializer = |offset: usize| {
        classes.iter().any(|(path, class)| {
            tree.contains(&path.join("::"))
                && class.location().start_offset() <= offset
                && offset < class.location().end_offset()
        })
    };
    let mut at = 0;
    while let Some(found) = source.text[at..].find("read_attribute_for_serialization") {
        let offset = at + found;
        at = offset + 1;
        let line_start = source.text[..offset].rfind('\n').map_or(0, |i| i + 1);
        let line_end = source.text[offset..].find('\n').map_or(source.text.len(), |i| offset + i);
        let line = &source.text[line_start..line_end];
        if !line.trim_start().starts_with("def ") && !line.trim_start().starts_with("alias") {
            continue;
        }
        if sends.is_match(line) || in_serializer(offset) {
            continue;
        }
        if let Some(c) = def.captures(&source.text[line_start..]) {
            if c[1] == c[2] {
                continue;
            }
        }
        return Some((source, offset));
    }
    None
}

/// The class a superclass reference names from inside `owner`: the
/// innermost enclosing namespace that has it, else the top level.
fn resolve(written: &str, owner: &str, names: &HashSet<String>) -> String {
    let written = written.trim_start_matches("::");
    let mut scope: Vec<&str> = owner.split("::").collect();
    scope.pop();
    while !scope.is_empty() {
        let candidate = format!("{}::{written}", scope.join("::"));
        if names.contains(&candidate) {
            return candidate;
        }
        scope.pop();
    }
    written.to_string()
}

struct Walk<'a> {
    name: &'a str,
    tree: &'a HashSet<String>,
    /// Each module of the app, with the index of the source that defines it.
    modules: &'a HashMap<String, usize>,
    sources: &'a [SourceFile],
    stmts: Vec<Stmt>,
    defs: Vec<Def>,
    visible: bool,
    at: usize,
    /// The mixins whose `self.included` hook the walk read as declarations.
    hooked: Vec<String>,
    hook_seen: bool,
}

enum Mode<'m> {
    Class,
    Module,
    /// The body of a mixin's `def self.included(klass)`, `klass` its parameter.
    Hook(&'m str),
}

fn collect(
    class: &ruby_prism::ClassNode<'_>,
    source: &SourceFile,
    name: &str,
    tree: &HashSet<String>,
    modules: &HashMap<String, usize>,
    sources: &[SourceFile],
) -> IngestResult<(Vec<Stmt>, Vec<Def>, Vec<String>)> {
    let class_text = &source.text[class.location().start_offset()..class.location().end_offset()];
    if class_text.contains("@ams_") {
        return Err(refuse(source, class.location().start_offset(), "an instance variable named `@ams_…` would be the expansion's"));
    }
    let mut ctx = Walk { name, tree, modules, sources, stmts: Vec::new(), defs: Vec::new(), visible: true, at: 0, hooked: Vec::new(), hook_seen: false };
    if let Some(body) = class.body() {
        walk(&mut ctx, flatten_statements(body), source, Mode::Class)?;
    }
    let Walk { stmts, defs, hooked, .. } = ctx;
    // A private `def` the declaration's own reader or gate replaces, which the expansion would keep beside it.
    for stmt in &stmts {
        let (attr, at) = match stmt {
            Stmt::Attribute { name, at, .. } => (name.as_str(), *at),
            Stmt::Association(a, at) => (a.name.as_str(), *at),
            _ => continue,
        };
        let gate = format!("include_{attr}?");
        if defs.iter().any(|d| !d.visible && !d.from_module && d.at < at && (d.name == attr || d.name == gate)) {
            return Err(refuse(source, class.location().start_offset(), &format!("a private `{attr}` before its declaration is outside the subset")));
        }
    }
    Ok((stmts, defs, hooked))
}

fn walk(ctx: &mut Walk<'_>, body: Vec<ruby_prism::Node<'_>>, source: &SourceFile, mode: Mode<'_>) -> IngestResult<()> {
    for stmt in body {
        ctx.at += 1;
        let at = ctx.at;
        let offset = stmt.location().start_offset();
        let fail = |reason: &str| refuse(source, offset, reason);
        if let Some(def) = stmt.as_def_node() {
            let method = constant_id_str(&def.name()).to_string();
            if matches!(mode, Mode::Hook(_)) {
                return Err(fail("a `def` in a mixin's `self.included`"));
            }
            if def.receiver().is_some() {
                if !matches!(mode, Mode::Module) || method != "included" {
                    continue;
                }
                let params: Vec<String> = def
                    .parameters()
                    .map(|p| p.requireds().iter().filter_map(|r| r.as_required_parameter_node().map(|r| constant_id_str(&r.name()).to_string())).collect())
                    .unwrap_or_default();
                let [param] = params.as_slice() else { return Err(fail("a `self.included` taking other than the class")) };
                if let Some(hook) = def.body() {
                    walk(ctx, flatten_statements(hook), source, Mode::Hook(param))?;
                }
                ctx.hook_seen = true;
                continue;
            }
            if method == "initialize" || method.starts_with("ams_") || GEM_ONLY.contains(&method.as_str()) {
                return Err(fail(&format!("`def {method}` would be replaced by the expansion")));
            }
            let mut calls = GemCalls { found: None, supers: false };
            ruby_prism::Visit::visit(&mut calls, &stmt);
            if let Some((call, at)) = calls.found {
                return Err(refuse(source, at, &format!("`{call}` is the gem's, outside the subset")));
            }
            ctx.defs.push(Def { name: method, at, visible: ctx.visible, supers: calls.supers, from_module: matches!(mode, Mode::Module) });
            continue;
        }
        if !matches!(mode, Mode::Hook(_))
            && (stmt.as_class_node().is_some() || stmt.as_module_node().is_some() || stmt.as_constant_write_node().is_some())
        {
            continue;
        }
        let Some(call) = stmt.as_call_node() else {
            return Err(fail("a serializer body may hold only the gem's declarations, methods, constants and classes"));
        };
        let method = constant_id_str(&call.name());
        let args: Vec<_> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
        if call.block().is_some() {
            return Err(fail(&format!("a block on `{method}`")));
        }
        match mode {
            Mode::Hook(param) => {
                let on_klass = call
                    .receiver()
                    .and_then(|r| r.as_local_variable_read_node())
                    .is_some_and(|r| constant_id_str(&r.name()) == param);
                if !on_klass {
                    return Err(fail(&format!("a mixin's `self.included` may only declare on `{param}`")));
                }
            }
            Mode::Module => {
                if call.receiver().is_none() && matches!(method, "private" | "protected" | "public") && args.is_empty() {
                    ctx.visible = method != "private";
                    continue;
                }
                return Err(fail("a mixin body may hold only methods, constants, visibility and `self.included`"));
            }
            Mode::Class => {
                let on_self = call.receiver().is_some_and(|r| r.as_self_node().is_some());
                if on_self && method == "root=" {
                    ctx.stmts.push(Stmt::Root(root_value(&args[0]).ok_or_else(|| fail("a root other than a literal"))?));
                    continue;
                }
                if call.receiver().is_some() {
                    return Err(fail("a receiver on a serializer declaration"));
                }
                if matches!(method, "private" | "protected" | "public") && args.is_empty() {
                    ctx.visible = method != "private";
                    continue;
                }
                if method == "include" {
                    let [arg] = args.as_slice() else { return Err(fail("`include` of other than one module")) };
                    let written = super::util::constant_path_of(arg).map(|p| p.join("::")).ok_or_else(|| fail("`include` of other than a constant"))?;
                    let module = if is_rooted(arg) { written.clone() } else { lexical_in(&written, ctx.name, ctx.modules) };
                    let Some(&index) = ctx.modules.get(&module) else {
                        return Err(fail(&format!("`include {written}`: not a module of the app")));
                    };
                    let msource = &ctx.sources[index];
                    let parsed = super::prism::parse_silent(msource.text.as_bytes());
                    let Some(mbody) = module_body(&parsed.node(), &module) else {
                        return Err(fail(&format!("`include {written}`: its source is not found")));
                    };
                    ctx.hook_seen = false;
                    // A module's own `private` does not carry into the class.
                    let visible = std::mem::replace(&mut ctx.visible, true);
                    walk(ctx, mbody, msource, Mode::Module)?;
                    ctx.visible = visible;
                    if ctx.hook_seen && !ctx.hooked.contains(&module) {
                        ctx.hooked.push(module);
                    }
                    continue;
                }
            }
        }
        match method {
            "attributes" => {
                for arg in &args {
                    if let Some(attr) = symbol_value(arg) {
                        ctx.stmts.push(attribute(&attr, None, offset, at, source)?);
                    } else if let Some(pairs) = hash_pairs(arg) {
                        for (attr, key) in pairs.map_err(|r| fail(&r))? {
                            ctx.stmts.push(attribute(&attr, Some(key), offset, at, source)?);
                        }
                    } else {
                        return Err(fail("`attributes` takes Symbols and a Hash of keys"));
                    }
                }
            }
            "attribute" => {
                let (attr, key) = match args.as_slice() {
                    [arg] | [arg, _] if symbol_value(arg).is_some() => {
                        let key = match args.get(1) {
                            None => None,
                            Some(opts) => {
                                let pairs = hash_pairs(opts).ok_or_else(|| fail("`attribute` options other than a Hash"))?;
                                let pairs = pairs.map_err(|r| fail(&r))?;
                                match pairs.as_slice() {
                                    [(k, v)] if k == "key" => Some(v.clone()),
                                    _ => return Err(fail("an `attribute` option other than `key:`")),
                                }
                            }
                        };
                        (symbol_value(arg).unwrap(), key)
                    }
                    [arg] => match hash_pairs(arg).map(|p| p.map_err(|r| fail(&r))) {
                        Some(Ok(pairs)) if pairs.len() == 1 => pairs.into_iter().next().map(|(a, k)| (a, Some(k))).unwrap(),
                        _ => return Err(fail("`attribute` takes a Symbol, or one name with its key")),
                    },
                    _ => return Err(fail("`attribute` takes a Symbol, or one name with its key")),
                };
                ctx.stmts.push(attribute(&attr, key, offset, at, source)?);
            }
            "has_one" | "has_many" => {
                let many = method == "has_many";
                let (names, options) = match args.split_last() {
                    Some((last, rest)) if last.as_keyword_hash_node().is_some() || last.as_hash_node().is_some() => (rest, Some(last)),
                    _ => (args.as_slice(), None),
                };
                let mut assoc_names = Vec::new();
                for n in names {
                    let n = symbol_value(n).ok_or_else(|| fail(&format!("`{method}` takes Symbols")))?;
                    if !is_plain(&n) || n.starts_with("ams_") || DEFINED.contains(&n.as_str()) {
                        return Err(fail(&format!("an association named `{n}` is outside the subset")));
                    }
                    assoc_names.push(n);
                }
                let Some(first) = assoc_names.first() else { return Err(fail(&format!("`{method}` without a name"))) };
                let mut serializer = None;
                let mut embed = None;
                let mut include = None;
                let mut key = None;
                let mut root = None;
                let mut embed_key = "id".to_string();
                for el in options.map(|o| option_elements(o)).unwrap_or_default() {
                    let assoc = el.as_assoc_node().ok_or_else(|| fail("a splatted association option"))?;
                    let opt = symbol_value(&assoc.key()).ok_or_else(|| fail("an association option that isn't a Symbol"))?;
                    let value = assoc.value();
                    match opt.as_str() {
                        "serializer" => {
                            let written = super::util::constant_path_of(&value)
                                .map(|p| p.join("::"))
                                .or_else(|| value.as_string_node().map(|s| String::from_utf8_lossy(s.unescaped()).into_owned()))
                                .ok_or_else(|| fail("a `serializer:` other than a constant"))?;
                            let rooted = is_rooted(&value) || value.as_string_node().is_some();
                            let target = if rooted { written.clone() } else { lexical(&written, ctx.name, ctx.tree) };
                            if !ctx.tree.contains(&target) {
                                return Err(fail(&format!("`serializer: {written}` is not a serializer of the app")));
                            }
                            serializer = Some(target);
                        }
                        "embed" => embed = Some(embed_value(&value).ok_or_else(|| fail("an `embed:` other than :ids or :objects"))?),
                        "include" => {
                            include = Some(if value.as_true_node().is_some() {
                                true
                            } else if value.as_false_node().is_some() {
                                false
                            } else {
                                return Err(fail("an `include:` other than true or false"));
                            })
                        }
                        "key" => key = Some(key_value(&value).ok_or_else(|| fail("a `key:` other than a Symbol or String"))?),
                        "root" => root = Some(key_value(&value).ok_or_else(|| fail("a `root:` other than a Symbol or String"))?),
                        "embed_key" => {
                            embed_key = symbol_value(&value).filter(|k| is_plain(k)).ok_or_else(|| fail("an `embed_key:` other than a Symbol"))?
                        }
                        other => return Err(fail(&format!("the association option `{other}:` is outside the subset"))),
                    }
                }
                let serializer = serializer.ok_or_else(|| {
                    fail("an association without `serializer:` (the record's own `active_model_serializer`) is outside the subset")
                })?;
                // The gem's `refine` writes the first name's root into the options all the names share.
                let root = root.unwrap_or_else(|| {
                    Key::Sym(if many { first.clone() } else { crate::naming::pluralize(first) })
                });
                for n in assoc_names {
                    ctx.stmts.push(Stmt::Association(
                        Assoc { many, name: n, serializer: serializer.clone(), embed, include, key: key.clone(), root: root.clone(), embed_key: embed_key.clone() },
                        at,
                    ));
                }
            }
            "embed" => {
                let (kind, opts) = match args.as_slice() {
                    [kind] => (kind, None),
                    [kind, opts] => (kind, Some(opts)),
                    _ => return Err(fail("`embed` takes a type and options")),
                };
                let embed = embed_value(kind).ok_or_else(|| fail("an `embed` other than :ids or :objects"))?;
                if opts.is_some_and(|o| o.as_keyword_hash_node().is_none() && o.as_hash_node().is_none()) {
                    return Err(fail("`embed` options other than a literal Hash"));
                }
                let mut include = false;
                for el in opts.map(|o| option_elements(o)).unwrap_or_default() {
                    let assoc = el.as_assoc_node().ok_or_else(|| fail("a splatted `embed` option"))?;
                    if symbol_value(&assoc.key()).as_deref() != Some("include") {
                        return Err(fail("an `embed` option other than `include:`"));
                    }
                    let value = assoc.value();
                    include = if value.as_true_node().is_some() {
                        true
                    } else if value.as_false_node().is_some() || value.as_nil_node().is_some() {
                        false
                    } else {
                        return Err(fail("an `include:` other than true or false"));
                    };
                }
                ctx.stmts.push(Stmt::Embed(embed, include));
            }
            "root" => {
                let [value] = args.as_slice() else { return Err(fail("`root` takes one value")) };
                ctx.stmts.push(Stmt::Root(root_value(value).ok_or_else(|| fail("a root other than a literal"))?));
            }
            other => return Err(fail(&format!("`{other}` is outside the ActiveModel::Serializer 0.8 subset"))),
        }
    }
    Ok(())
}

/// The statements of module `path`'s body in a parsed file.
fn module_body<'pr>(node: &ruby_prism::Node<'pr>, path: &str) -> Option<Vec<ruby_prism::Node<'pr>>> {
    fn find<'pr>(node: &ruby_prism::Node<'pr>, scope: &mut Vec<String>, path: &str) -> Option<Vec<ruby_prism::Node<'pr>>> {
        let stmts: Vec<ruby_prism::Node<'pr>> = if let Some(p) = node.as_program_node() {
            p.statements().body().iter().collect()
        } else if let Some(s) = node.as_statements_node() {
            s.body().iter().collect()
        } else if let Some(m) = node.as_module_node() {
            let name = super::util::constant_path_of(&m.constant_path())?;
            let depth = scope.len();
            scope.extend(name);
            let found = if scope.join("::") == path {
                Some(m.body().map(flatten_statements).unwrap_or_default())
            } else {
                m.body().and_then(|b| find(&b, scope, path))
            };
            scope.truncate(depth);
            return found;
        } else if let Some(c) = node.as_class_node() {
            let name = super::util::constant_path_of(&c.constant_path())?;
            let depth = scope.len();
            scope.extend(name);
            let found = c.body().and_then(|b| find(&b, scope, path));
            scope.truncate(depth);
            return found;
        } else {
            return None;
        };
        stmts.iter().find_map(|s| find(s, scope, path))
    }
    find(node, &mut Vec::new(), path)
}

/// A module written inside `owner`'s body: its lexical scopes, then the top level.
fn lexical_in(written: &str, owner: &str, modules: &HashMap<String, usize>) -> String {
    let mut scope: Vec<&str> = owner.split("::").collect();
    while !scope.is_empty() {
        let candidate = format!("{}::{written}", scope.join("::"));
        if modules.contains_key(&candidate) {
            return candidate;
        }
        scope.pop();
    }
    written.to_string()
}

fn attribute(attr: &str, key: Option<Key>, offset: usize, at: usize, source: &SourceFile) -> IngestResult<Stmt> {
    let fail = |reason: &str| refuse(source, offset, reason);
    let reader = attr.strip_suffix('?').unwrap_or(attr);
    if !is_plain(reader) || reader.starts_with("ams_") || (DEFINED.contains(&attr) && !PROVIDED.contains(&attr)) {
        return Err(fail(&format!("an attribute named `{attr}` is outside the subset")));
    }
    let key = match key {
        None => attr.strip_suffix('?').unwrap_or(attr).to_string(),
        Some(Key::Sym(k) | Key::Str(k)) => k,
    };
    Ok(Stmt::Attribute { name: attr.to_string(), key, at })
}

/// A lowercase identifier the expansion can name a method by.
fn is_plain(s: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "alias", "and", "begin", "break", "case", "class", "def", "defined", "do", "else", "elsif", "end", "ensure", "false",
        "for", "if", "in", "module", "next", "nil", "not", "or", "redo", "rescue", "retry", "return", "self", "super", "then",
        "true", "undef", "unless", "until", "when", "while", "yield", "__method__", "__FILE__", "__LINE__",
    ];
    if KEYWORDS.contains(&s) {
        return false;
    }
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A constant written inside `owner`'s body: its lexical scopes, then the top level.
fn lexical(written: &str, owner: &str, tree: &HashSet<String>) -> String {
    let mut scope: Vec<&str> = owner.split("::").collect();
    while !scope.is_empty() {
        let candidate = format!("{}::{written}", scope.join("::"));
        if tree.contains(&candidate) {
            return candidate;
        }
        scope.pop();
    }
    written.to_string()
}

fn option_elements<'pr>(node: &ruby_prism::Node<'pr>) -> Vec<ruby_prism::Node<'pr>> {
    if let Some(h) = node.as_keyword_hash_node() {
        h.elements().iter().collect()
    } else if let Some(h) = node.as_hash_node() {
        h.elements().iter().collect()
    } else {
        Vec::new()
    }
}

/// `{name => key}` pairs, Symbol names and Symbol or String keys.
fn hash_pairs(node: &ruby_prism::Node<'_>) -> Option<Result<Vec<(String, Key)>, String>> {
    if node.as_keyword_hash_node().is_none() && node.as_hash_node().is_none() {
        return None;
    }
    let mut out = Vec::new();
    for el in option_elements(node) {
        let Some(assoc) = el.as_assoc_node() else { return Some(Err("a splatted Hash".into())) };
        let (Some(k), Some(v)) = (symbol_value(&assoc.key()), key_value(&assoc.value())) else {
            return Some(Err("a Hash entry other than a Symbol and its Symbol or String".into()));
        };
        out.push((k, v));
    }
    Some(Ok(out))
}

fn key_value(node: &ruby_prism::Node<'_>) -> Option<Key> {
    if let Some(s) = symbol_value(node) {
        return Some(Key::Sym(s));
    }
    node.as_string_node().map(|s| Key::Str(String::from_utf8_lossy(s.unescaped()).into_owned()))
}

fn embed_value(node: &ruby_prism::Node<'_>) -> Option<Embed> {
    match symbol_value(node)?.as_str() {
        "ids" | "id" => Some(Embed::Ids),
        "objects" | "object" => Some(Embed::Objects),
        _ => None,
    }
}

fn root_value(node: &ruby_prism::Node<'_>) -> Option<Root> {
    if node.as_false_node().is_some() {
        Some(Root::False)
    } else if node.as_true_node().is_some() {
        Some(Root::True)
    } else if node.as_nil_node().is_some() {
        Some(Root::Unset)
    } else {
        key_value(node).map(Root::Name)
    }
}

/// The gem's own methods the expansion doesn't write, called on self.
const GEM_ONLY: &[&str] = &[
    "_associations", "_attributes", "_embed", "_root", "_root_embed", "_serializable_hash", "cache", "expand_cache_key",
    "include!", "include_associations!", "instrument", "merge_association", "perform_caching", "perform_caching?",
    "read_attribute_for_serialization", "url_options", "_fast_attributes",
];

struct GemCalls {
    found: Option<(String, usize)>,
    supers: bool,
}

impl<'pr> ruby_prism::Visit<'pr> for GemCalls {
    fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
        let name = constant_id_str(&node.name());
        let on_self = node.receiver().is_none_or(|r| r.as_self_node().is_some());
        if self.found.is_none() && on_self && (GEM_ONLY.contains(&name) || name.starts_with("ams_")) {
            self.found = Some((name.to_string(), node.location().start_offset()));
        }
        ruby_prism::visit_call_node(self, node);
    }
    fn visit_super_node(&mut self, node: &ruby_prism::SuperNode<'pr>) {
        self.supers = true;
        ruby_prism::visit_super_node(self, node);
    }
    fn visit_forwarding_super_node(&mut self, node: &ruby_prism::ForwardingSuperNode<'pr>) {
        self.supers = true;
        ruby_prism::visit_forwarding_super_node(self, node);
    }
}

/// Whether `name` is defined where `method_defined?` looks when the
/// declaration at `at` in `class` runs: a public or protected `def` of an
/// ancestor, one earlier in the class, a reader or gate an ancestor's
/// declaration generated, or the gem's.
fn defined_before(name: &str, class: &str, at: usize, chain: &[String], owns: &HashMap<String, Own>, generated: &HashSet<(String, String)>) -> bool {
    if DEFINED.contains(&name) {
        return true;
    }
    for ancestor in chain.iter().take_while(|a| a.as_str() != class) {
        if owns[ancestor].defs.iter().any(|d| d.name == name && d.visible) || generated.contains(&(ancestor.clone(), name.to_string())) {
            return true;
        }
    }
    generated.contains(&(class.to_string(), name.to_string())) || owns[class].defs.iter().any(|d| d.name == name && d.visible && d.at < at)
}

fn synthesize(name: &str, chain: &[String], owns: &HashMap<String, Own>, flat: &Flat) -> String {
    let mut src = format!("class {name}\n");
    // An own `def` of the name is the class's, as it overrides the gem's.
    let own: HashSet<&str> = owns[name].defs.iter().map(|d| d.name.as_str()).collect();
    // Per class, not once on the root: an `object` the whole tree shares is a union of every serializer's records,
    // which the analysis reads per class but the lowering of `object.comments.order(:id)` does not.
    for (method, body) in PER_CLASS_API.iter().chain(if chain.len() == 1 { ROOT_API } else { &[] }) {
        if !own.contains(method) {
            src.push('\n');
            src.push_str(body);
        }
    }
    // What an ancestor's declarations defined, which `method_defined?` sees here.
    let mut generated: HashSet<(String, String)> = HashSet::default();
    for ancestor in chain {
        for stmt in &owns[ancestor].stmts {
            let (method, at) = match stmt {
                Stmt::Attribute { name, at, .. } => (name.clone(), *at),
                Stmt::Association(a, at) => (a.name.clone(), *at),
                _ => continue,
            };
            let mut write = |m: &str, body: &str, generated: &mut HashSet<(String, String)>| {
                if defined_before(m, ancestor, at, chain, owns, generated) {
                    return;
                }
                generated.insert((ancestor.clone(), m.to_string()));
                // A later `def` of the name replaces what the declaration defined.
                if ancestor == name && !owns[ancestor].defs.iter().any(|d| d.name == m && d.at > at) {
                    src.push_str(&format!("\n  def {m}\n    {body}\n  end\n"));
                }
            };
            let body = format!("object.{method}");
            write(&method, &body, &mut generated);
            if !method.ends_with('?') {
                write(&format!("include_{method}?"), "true", &mut generated);
            }
        }
    }
    let class_name = crate::naming::model_param_key(name.rsplit("::").next().unwrap_or(name));
    let class_name = class_name.strip_suffix("_serializer").unwrap_or(&class_name).to_string();
    let root = match &flat.root {
        Root::False => "false".to_string(),
        Root::Unset | Root::True => Key::Sym(class_name).ruby(),
        Root::Name(key) => key.ruby(),
    };
    src.push_str(&format!("\n  def ams_root_name\n    {root}\n  end\n"));
    if !own.contains("include?") {
        src.push_str("\n  def include?(name)\n");
        src.push_str("    return false if @options.key?(:only) && !Array(@options[:only]).include?(name)\n");
        src.push_str("    return false if @options.key?(:except) && Array(@options[:except]).include?(name)\n");
        let mut gated: Vec<&str> = flat.attributes.iter().map(|(a, _)| a.as_str()).collect();
        gated.extend(flat.assocs.iter().map(|a| a.name.as_str()));
        let mut seen = HashSet::default();
        gated.retain(|a| seen.insert(*a));
        let unknown = "raise NoMethodError, \"undefined method 'include_#{name}?' for an instance of #{self.class}\"";
        if gated.is_empty() {
            src.push_str(&format!("    {unknown}\n  end\n"));
        } else {
            src.push_str("    case name\n");
            for attr in gated {
                // `include_long??` is no name a `def` can write: the gate the gem defines for it is always true.
                let gate = if attr.ends_with('?') { "true".to_string() } else { format!("include_{attr}?") };
                src.push_str(&format!("    when {} then {gate}\n", Key::Sym(attr.to_string()).ruby()));
            }
            src.push_str(&format!("    else\n      {unknown}\n    end\n  end\n"));
        }
    }
    src.push_str("\n  def ams_attributes(h)\n");
    for (attr, key) in &flat.attributes {
        // `self.h`, not `h()`: the Ruby emit drops the parentheses, and an attribute named `h` then reads the Hash.
        src.push_str(&format!("    h[{}] = self.{attr} if include?({})\n", Key::Sym(key.clone()).ruby(), Key::Sym(attr.clone()).ruby()));
    }
    src.push_str("    h\n  end\n");
    src.push_str("\n  def ams_associations(node)\n");
    for a in &flat.assocs {
        src.push_str(&association(a, flat));
    }
    src.push_str("    node\n  end\n");
    src.push_str(&format!(
        r#"
  def self.ams_array_as_json(objects, options)
    hash = {{}}
    unique = {{}}
    root = options[:root]
    items = objects.map {{ |item| new(item, options).ams_adopt(hash, unique).serializable_hash }}
    if root
      hash[root] = items
      meta_key = options[:meta_key]
      hash[meta_key.respond_to?(:to_sym) ? meta_key.to_sym : :meta] = options[:meta] if options.has_key?(:meta)
      hash
    else
      items
    end
  end
"#
    ));
    src.push_str("end\n");
    src
}

fn association(a: &Assoc, flat: &Flat) -> String {
    let name = &a.name;
    // `self.node`, not `node`: an association called `node` or `side` would read the local.
    let read = format!("self.{name}");
    let sym = Key::Sym(name.clone()).ruby();
    let target = format!("::{}", a.serializer);
    let embed = a.embed.unwrap_or(flat.embed);
    let nested = |value: &str| format!("{target}.new({value}, @options).ams_adopt(@ams_side, @ams_unique)");
    let mut out = format!("    if include?({sym})\n");
    match (embed, a.many) {
        (Embed::Objects, true) => {
            let key = a.key.clone().unwrap_or(Key::Sym(name.clone())).ruby();
            out.push_str(&format!("      node[{key}] = {read}.map {{ |item| {}.serializable_hash }}\n", nested("item")));
        }
        (Embed::Objects, false) => {
            let key = a.key.clone().unwrap_or(Key::Sym(name.clone())).ruby();
            out.push_str(&format!("      value = {read}\n      node[{key}] = value ? {}.serializable_hash : nil\n", nested("value")));
        }
        (Embed::Ids, many) => {
            let default = match a.include {
                Some(b) => b.to_string(),
                None if flat.root_embed => "true".to_string(),
                None => "nil".to_string(),
            };
            let embed_key = &a.embed_key;
            let root = a.root.ruby();
            if many {
                let key = a.key.clone().unwrap_or_else(|| Key::Sym(format!("{}_ids", crate::naming::singularize(name)))).ruby();
                out.push_str(&format!("      included = ams_include_option({sym}, {default})\n"));
                out.push_str(&format!("      node[{key}] = {read}.map {{ |item| item.{embed_key} }}\n"));
            } else {
                let key = a.key.clone().unwrap_or_else(|| Key::Sym(format!("{name}_id"))).ruby();
                out.push_str(&format!("      included = ams_include_option({sym}, {default})\n"));
                out.push_str(&format!("      node[{key}] = {read} ? {read}.{embed_key} : nil\n"));
            }
            out.push_str("      if included\n");
            out.push_str("        side = @ams_side\n");
            out.push_str(&format!(
                "        raise \"Cannot serialize {name} when #{{self.class}} does not have a root!\" if side.nil?\n"
            ));
            // has_many sideloads nothing for a nil collection; has_one always opens its root.
            if many {
                out.push_str(&format!("        ams_merge(side, {root}, {read}.map {{ |item| {} }}) unless {read}.nil?\n", nested("item")));
            } else {
                out.push_str(&format!("        value = {read}\n        ams_merge(side, {root}, value ? [{}] : [])\n", nested("value")));
            }
            out.push_str("      end\n");
        }
    }
    out.push_str("    end\n");
    out
}

/// The gem's constructor and readers, on every class of the tree, by name.
const PER_CLASS_API: &[(&str, &str)] = &[
    ("initialize", "  def initialize(object, options = {})\n    @object = object\n    @options = options\n    @ams_side = nil\n    @ams_unique = nil\n  end\n"),
    ("object", "  def object\n    @object\n  end\n"),
    ("options", "  def options\n    @options\n  end\n"),
    ("scope", "  def scope\n    @options[:scope]\n  end\n"),
];

/// The rest of the gem's instance API, on the class under `ActiveModel::Serializer`, by name. The Hash the root
/// and the sideloaded records share is held apart from `options`, where the gem keeps it, so the Hash the app
/// passes keeps the type it was built with.
const ROOT_API: &[(&str, &str)] = &[
    ("ams_adopt", "  def ams_adopt(side, unique)\n    @ams_side = side\n    @ams_unique = unique\n    self\n  end\n"),
    ("root_name", "  def root_name\n    ams_root_name\n  end\n"),
    ("meta_key", "  def meta_key\n    key = @options[:meta_key]\n    key.respond_to?(:to_sym) ? key.to_sym : :meta\n  end\n"),
    ("include_meta", "  def include_meta(hash)\n    hash[meta_key] = @options[:meta] if @options.has_key?(:meta)\n  end\n"),
    ("ams_include_option", "  def ams_include_option(name, default)\n    if @options.key?(:include)\n      @options[:include].include?(name)\n    elsif @options.include?(:exclude)\n      !@options[:exclude].include?(name)\n    else\n      default\n    end\n  end\n"),
    ("ams_merge", "  def ams_merge(side, key, serializables)\n    unique = (@ams_unique ||= {})\n    seen = (unique[key] ||= {})\n    list = (side[key] ||= [])\n    serializables.each do |serializable|\n      unless seen.include?(serializable.object)\n        seen[serializable.object] = true\n        list << serializable.serializable_hash\n      end\n    end\n  end\n"),
    ("attributes", "  def attributes\n    h = {}\n    ams_attributes(h)\n  end\n"),
    ("serializable_hash", "  def serializable_hash\n    return nil if @object.nil?\n    node = attributes\n    ams_associations(node) if @object.present?\n    node\n  end\n"),
    ("as_json", "  def as_json(options = {})\n    options ||= {}\n    root = options.fetch(:root, @options.fetch(:root, root_name))\n    if root\n      hash = {}\n      @ams_side = hash\n      @ams_unique = {}\n      body = serializable_hash\n      hash[root] = body\n      include_meta(hash)\n      hash\n    else\n      serializable_hash\n    end\n  end\n"),
];

fn install(class: &mut LibraryClass, src: &str, span: Span, root: bool) -> IngestResult<()> {
    let (parsed, diags) = super::prism::scope(|| super::ingest_library_classes(src.as_bytes(), "<active_model_serializers>"));
    let mut parsed = parsed?;
    if !diags.is_empty() || parsed.is_empty() {
        return Err(IngestError::Parse {
            file: "<active_model_serializers>".into(),
            message: format!("invalid synthesized serializer for {}: {diags:?}\n{src}", class.name.0),
        });
    }
    fn mark(expr: &mut Expr, span: Span) {
        expr.span = if matches!(&*expr.node, ExprNode::Const { .. }) { Span::synthetic() } else { span };
        expr.node.for_each_child_mut(&mut |child| mark(child, span));
    }
    let mut methods = parsed.remove(0).methods;
    for method in &mut methods {
        mark(&mut method.body, span);
        method.name_span = span;
    }
    class.methods.append(&mut methods);
    if root {
        class.parent = None;
    }
    class.unknown_calls.clear();
    class.origin = Some(LibraryClassOrigin::ActiveModelSerializer08);
    Ok(())
}

/// `ActiveModel::ArraySerializer.new(xs, {each_serializer: S, …}).as_json`
/// → `S.ams_array_as_json(xs, {each_serializer: S, …})`, S a lowered
/// serializer written as a constant; the options, `each_serializer:`
/// among them, reach each item's serializer as the gem passes them.
fn rewrite_arrays(expr: &mut Expr, done: &HashSet<String>) {
    expr.node.for_each_child_mut(&mut |c| rewrite_arrays(c, done));
    let ExprNode::Send { recv: Some(array), method, args, block: None, .. } = &*expr.node else { return };
    if method.as_str() != "as_json" || !args.is_empty() {
        return;
    }
    let ExprNode::Send { recv: Some(class), method, args, block: None, .. } = &*array.node else { return };
    let ExprNode::Const { path } = &*class.node else { return };
    let written: Vec<&str> = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
    if method.as_str() != "new" || written != ARRAY_SERIALIZER {
        return;
    }
    let [objects, options] = args.as_slice() else { return };
    let ExprNode::Hash { entries, .. } = &*options.node else { return };
    let mut serializer = None;
    for (k, v) in entries {
        let ExprNode::Lit { value: Literal::Sym { value } } = &*k.node else { return };
        if value.as_str() == "each_serializer" {
            let ExprNode::Const { path } = &*v.node else { return };
            serializer = Some(path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::"));
        }
    }
    let Some(serializer) = serializer.filter(|s| done.contains(s)) else { return };
    let span = expr.span;
    let recv = Expr::new(
        Span::synthetic(),
        ExprNode::Const { path: std::iter::once(Symbol::from("")).chain(serializer.split("::").map(Symbol::from)).collect() },
    );
    let mut options = options.clone();
    if let ExprNode::Hash { kwargs, .. } = &mut *options.node {
        *kwargs = false;
    }
    let objects = objects.clone();
    *expr = Expr::new(
        span,
        ExprNode::Send { recv: Some(recv), method: Symbol::from("ams_array_as_json"), args: vec![objects, options], block: None, parenthesized: true },
    );
}
