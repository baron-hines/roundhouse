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
}

struct Own {
    stmts: Vec<Stmt>,
    defs: Vec<Def>,
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
            match collect(&class, source, &name, &tree) {
                Ok((stmts, defs)) => {
                    let span = Span { file: super::sources::file_id(&source.path), start: offset as u32, end: offset as u32 };
                    owns.insert(name, Own { stmts, defs, span, source_index: index, offset });
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
    // A refused class takes down its descendants: they inherit what it would have defined.
    loop {
        let more: Vec<String> = chains
            .iter()
            .filter(|(n, chain)| !refused.contains_key(*n) && chain.iter().any(|a| refused.contains_key(a)))
            .map(|(n, _)| n.clone())
            .collect();
        if more.is_empty() {
            break;
        }
        for name in more {
            let chain = &chains[&name];
            let ancestor = chain.iter().find(|a| refused.contains_key(*a)).cloned().unwrap_or_default();
            refused.insert(
                name.clone(),
                IngestError::Unsupported {
                    file: owns.get(&name).map(|o| sources[o.source_index].path.clone()).unwrap_or_default(),
                    message: format!("ActiveModel::Serializer subset: {name} inherits from {ancestor}, which is refused"),
                },
            );
        }
    }
    // A class whose association names a refused serializer would call the gem's class.
    loop {
        let more: Vec<(String, String)> = owns
            .iter()
            .filter(|(n, _)| !refused.contains_key(*n))
            .filter_map(|(n, own)| {
                own.stmts.iter().find_map(|s| match s {
                    Stmt::Association(a, _) if refused.contains_key(&a.serializer) => Some((n.clone(), a.serializer.clone())),
                    _ => None,
                })
            })
            .collect();
        if more.is_empty() {
            break;
        }
        for (name, target) in more {
            let own = &owns[&name];
            refused.insert(
                name,
                refuse(&sources[own.source_index], own.offset, &format!("an association serializes through {target}, which is refused")),
            );
        }
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
    let done: HashSet<String> = lowered.into_iter().filter(|n| !failed.contains(n)).collect();
    for_each_rewritten_body(app, &mut |body| rewrite_arrays(body, &done));
    Ok(())
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

fn collect(
    class: &ruby_prism::ClassNode<'_>,
    source: &SourceFile,
    name: &str,
    tree: &HashSet<String>,
) -> IngestResult<(Vec<Stmt>, Vec<Def>)> {
    let mut stmts = Vec::new();
    let mut defs = Vec::new();
    let Some(body) = class.body() else { return Ok((stmts, defs)) };
    let class_text = &source.text[class.location().start_offset()..class.location().end_offset()];
    if class_text.contains("@ams_") {
        return Err(refuse(source, class.location().start_offset(), "an instance variable named `@ams_…` would be the expansion's"));
    }
    let mut visible = true;
    for (at, stmt) in flatten_statements(body).into_iter().enumerate() {
        let offset = stmt.location().start_offset();
        let fail = |reason: &str| refuse(source, offset, reason);
        if let Some(def) = stmt.as_def_node() {
            let method = constant_id_str(&def.name()).to_string();
            if def.receiver().is_some() {
                continue;
            }
            if method == "initialize" || method.starts_with("ams_") || GEM_ONLY.contains(&method.as_str()) {
                return Err(fail(&format!("`def {method}` would be replaced by the expansion")));
            }
            let mut calls = GemCalls { found: None };
            ruby_prism::Visit::visit(&mut calls, &stmt);
            if let Some((call, at)) = calls.found {
                return Err(refuse(source, at, &format!("`{call}` is the gem's, outside the subset")));
            }
            defs.push(Def { name: method, at, visible });
            continue;
        }
        if stmt.as_class_node().is_some() || stmt.as_module_node().is_some() || stmt.as_constant_write_node().is_some() {
            continue;
        }
        let Some(call) = stmt.as_call_node() else {
            return Err(fail("a serializer body may hold only the gem's declarations, methods, constants and classes"));
        };
        let method = constant_id_str(&call.name());
        let args: Vec<_> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
        let on_self = call.receiver().is_some_and(|r| r.as_self_node().is_some());
        if on_self && method == "root=" {
            stmts.push(Stmt::Root(root_value(&args[0]).ok_or_else(|| fail("a root other than a literal"))?));
            continue;
        }
        if call.receiver().is_some() {
            return Err(fail("a receiver on a serializer declaration"));
        }
        if call.block().is_some() {
            return Err(fail(&format!("a block on `{method}`")));
        }
        match method {
            "private" | "protected" | "public" if args.is_empty() => visible = method != "private",
            "attributes" => {
                for arg in &args {
                    if let Some(attr) = symbol_value(arg) {
                        stmts.push(attribute(&attr, None, offset, at, source)?);
                    } else if let Some(pairs) = hash_pairs(arg) {
                        for (attr, key) in pairs.map_err(|r| fail(&r))? {
                            stmts.push(attribute(&attr, Some(key), offset, at, source)?);
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
                stmts.push(attribute(&attr, key, offset, at, source)?);
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
                    if !is_plain(&n) || DEFINED.contains(&n.as_str()) {
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
                            let target = if rooted { written.clone() } else { lexical(&written, name, tree) };
                            if !tree.contains(&target) {
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
                    stmts.push(Stmt::Association(
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
                stmts.push(Stmt::Embed(embed, include));
            }
            "root" => {
                let [value] = args.as_slice() else { return Err(fail("`root` takes one value")) };
                stmts.push(Stmt::Root(root_value(value).ok_or_else(|| fail("a root other than a literal"))?));
            }
            other => return Err(fail(&format!("`{other}` is outside the ActiveModel::Serializer 0.8 subset"))),
        }
    }
    // A private `def` the declaration's own reader or gate replaces, which the expansion would keep beside it.
    for stmt in &stmts {
        let (attr, at) = match stmt {
            Stmt::Attribute { name, at, .. } => (name.as_str(), *at),
            Stmt::Association(a, at) => (a.name.as_str(), *at),
            _ => continue,
        };
        let gate = format!("include_{attr}?");
        if defs.iter().any(|d| !d.visible && d.at < at && (d.name == attr || d.name == gate)) {
            return Err(refuse(source, class.location().start_offset(), &format!("a private `{attr}` before its declaration is outside the subset")));
        }
    }
    Ok((stmts, defs))
}

fn attribute(attr: &str, key: Option<Key>, offset: usize, at: usize, source: &SourceFile) -> IngestResult<Stmt> {
    let fail = |reason: &str| refuse(source, offset, reason);
    let reader = attr.strip_suffix('?').unwrap_or(attr);
    if !is_plain(reader) || (DEFINED.contains(&attr) && !PROVIDED.contains(&attr)) {
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
    if chain.len() == 1 {
        src.push_str(ROOT_API);
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
    src.push_str("\n  def ams_attributes(h)\n");
    for (attr, key) in &flat.attributes {
        let gate = if attr.ends_with('?') { String::new() } else { format!(" && include_{attr}?") };
        src.push_str(&format!("    h[{}] = {attr} if include?({}){gate}\n", Key::Sym(key.clone()).ruby(), Key::Sym(attr.clone()).ruby()));
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
    let sym = Key::Sym(name.clone()).ruby();
    let target = format!("::{}", a.serializer);
    let embed = a.embed.unwrap_or(flat.embed);
    let nested = |value: &str| format!("{target}.new({value}, @options).ams_adopt(@ams_side, @ams_unique)");
    let mut out = format!("    if include?({sym}) && include_{name}?\n");
    match (embed, a.many) {
        (Embed::Objects, true) => {
            let key = a.key.clone().unwrap_or(Key::Sym(name.clone())).ruby();
            out.push_str(&format!("      node[{key}] = {name}.map {{ |item| {}.serializable_hash }}\n", nested("item")));
        }
        (Embed::Objects, false) => {
            let key = a.key.clone().unwrap_or(Key::Sym(name.clone())).ruby();
            out.push_str(&format!("      value = {name}\n      node[{key}] = value ? {}.serializable_hash : nil\n", nested("value")));
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
                out.push_str(&format!("      node[{key}] = {name}.map {{ |item| item.{embed_key} }}\n"));
            } else {
                let key = a.key.clone().unwrap_or_else(|| Key::Sym(format!("{name}_id"))).ruby();
                out.push_str(&format!("      included = ams_include_option({sym}, {default})\n"));
                out.push_str(&format!("      node[{key}] = {name} ? {name}.{embed_key} : nil\n"));
            }
            out.push_str("      if included\n");
            out.push_str("        side = @ams_side\n");
            out.push_str(&format!(
                "        raise \"Cannot serialize {name} when #{{self.class}} does not have a root!\" if side.nil?\n"
            ));
            // has_many sideloads nothing for a nil collection; has_one always opens its root.
            if many {
                out.push_str(&format!("        ams_merge(side, {root}, {name}.map {{ |item| {} }}) unless {name}.nil?\n", nested("item")));
            } else {
                out.push_str(&format!("        value = {name}\n        ams_merge(side, {root}, value ? [{}] : [])\n", nested("value")));
            }
            out.push_str("      end\n");
        }
    }
    out.push_str("    end\n");
    out
}

/// The gem's instance API, on the class under `ActiveModel::Serializer`.
/// The Hash the root and the sideloaded records share is held apart from
/// `options`, where the gem keeps it, so the Hash the app passes keeps the
/// type it was built with.
const ROOT_API: &str = r#"
  def initialize(object, options = {})
    @object = object
    @options = options
    @ams_side = nil
    @ams_unique = nil
  end

  def object
    @object
  end

  def options
    @options
  end

  def scope
    @options[:scope]
  end

  def ams_adopt(side, unique)
    @ams_side = side
    @ams_unique = unique
    self
  end

  def root_name
    ams_root_name
  end

  def meta_key
    key = @options[:meta_key]
    key.respond_to?(:to_sym) ? key.to_sym : :meta
  end

  def include_meta(hash)
    hash[meta_key] = @options[:meta] if @options.has_key?(:meta)
  end

  def include?(name)
    return false if @options.key?(:only) && !Array(@options[:only]).include?(name)
    return false if @options.key?(:except) && Array(@options[:except]).include?(name)
    true
  end

  def ams_include_option(name, default)
    if @options.key?(:include)
      @options[:include].include?(name)
    elsif @options.include?(:exclude)
      !@options[:exclude].include?(name)
    else
      default
    end
  end

  def ams_merge(side, key, serializables)
    unique = (@ams_unique ||= {})
    seen = (unique[key] ||= {})
    list = (side[key] ||= [])
    serializables.each do |serializable|
      unless seen.include?(serializable.object)
        seen[serializable.object] = true
        list << serializable.serializable_hash
      end
    end
  end

  def attributes
    h = {}
    ams_attributes(h)
  end

  def serializable_hash
    return nil if @object.nil?
    node = attributes
    ams_associations(node) if @object.present?
    node
  end

  def as_json(options = {})
    options ||= {}
    root = options.fetch(:root, @options.fetch(:root, root_name))
    if root
      hash = {}
      @ams_side = hash
      @ams_unique = {}
      body = serializable_hash
      hash[root] = body
      include_meta(hash)
      hash
    else
      serializable_hash
    end
  end
"#;

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
