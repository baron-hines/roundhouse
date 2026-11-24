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
    /// The target serializer's full name; empty where the association names none.
    serializer: String,
    /// Without `serializer:`, the record's own `active_model_serializer`: each class whose instances have one, with
    /// it, most derived first.
    implicit: Option<Vec<(String, String)>>,
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
    /// Methods a literal-list loop in the body defines, as Ruby source.
    extra: Vec<String>,
    /// The class macros the body ran: (defining class, name).
    expanded: Vec<(String, String)>,
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
    /// The app's `include!` (0.9's): a sideload with no root to merge into is skipped, not raised.
    rootless_skips: bool,
}

impl Default for Flat {
    fn default() -> Self {
        Flat { attributes: Vec::new(), assocs: Vec::new(), embed: Embed::Objects, root_embed: false, root: Root::Unset, rootless_skips: false }
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
            if !names.contains(parent) || chain.contains(parent) {
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
    // Class macros the tree's classes define: `def self.staff_attributes(*attrs)` with a rest parameter alone.
    let mut macros: HashMap<String, Vec<(String, usize)>> = HashMap::default();
    // The constants the tree's class bodies assign, by full name: one may stand for a serializer (`Item = Other`).
    let mut consts: HashSet<String> = HashSet::default();
    for (index, source) in sources.iter().enumerate() {
        if !tree.iter().any(|n| source.text.contains(n.rsplit("::").next().unwrap_or(n))) {
            continue;
        }
        let parsed = super::prism::parse_silent(source.text.as_bytes());
        let mut found = Vec::new();
        collect_classes(&parsed.node(), &mut Vec::new(), &mut found);
        for (path, class) in found {
            let name = path.join("::");
            if !tree.contains(&name) {
                continue;
            }
            for stmt in class.body().map(flatten_statements).unwrap_or_default() {
                if let Some(w) = stmt.as_constant_write_node() {
                    consts.insert(format!("{name}::{}", constant_id_str(&w.name())));
                }
                if let Some(def) = stmt.as_def_node()
                    && def.receiver().is_some_and(|r| r.as_self_node().is_some())
                    && (macro_rest(&def).is_some() || macro_single(&def).is_some())
                {
                    macros.entry(constant_id_str(&def.name()).to_string()).or_default().push((name.clone(), index));
                }
            }
        }
    }
    // `active_model_serializer`: Active Record's records (and a class including `ActiveModel::SerializerSupport`)
    // answer `"#{self.class.name}Serializer".safe_constantize`, each class its own, a subclass included.
    let implicit: (Option<Vec<(String, String)>>, String) = {
        let support: Vec<String> = app
            .library_classes
            .iter()
            .filter(|c| c.includes.iter().any(|i| i.0.as_str().trim_start_matches("::") == "ActiveModel::SerializerSupport"))
            .map(|c| c.name.0.as_str().to_string())
            .collect();
        let depth = |m: &str| {
            let mut d = 0;
            let mut current = app.models.iter().find(|x| x.name.0.as_str() == m);
            while let Some(model) = current {
                d += 1;
                current = model.parent.as_ref().and_then(|p| app.models.iter().find(|x| &x.name == p));
            }
            d
        };
        let mut pairs: Vec<(String, String)> = app
            .models
            .iter()
            .map(|m| m.name.0.as_str().to_string())
            .chain(support)
            .filter_map(|m| {
                let s = format!("{m}Serializer");
                tree.contains(&s).then_some((m, s))
            })
            .collect();
        pairs.sort_by(|a, b| depth(&b.0).cmp(&depth(&a.0)).then(a.0.cmp(&b.0)));
        match sources.iter().find(|s| s.text.contains("active_model_serializer") && defines_method(s, "active_model_serializer")) {
            Some(_) => (None, "an association without `serializer:`, where the app defines its own `active_model_serializer`".to_string()),
            None => (Some(pairs), String::new()),
        }
    };
    let mut owns: HashMap<String, Own> = HashMap::default();
    let mut refused: HashMap<String, IngestError> = HashMap::default();
    let (reopened, rootless_skips) = match reopened_gem(sources) {
        Ok(found) => found,
        Err((index, offset, reason)) => {
            for name in &tree {
                refused.insert(name.clone(), refuse(&sources[index], offset, &reason));
            }
            (false, false)
        }
    };
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
            let lineage: Vec<String> = chains[&name].iter().rev().cloned().collect();
            match collect(&class, source, &name, &tree, &modules, sources, &macros, &consts, &implicit, lineage) {
                Ok((stmts, defs, hooked, extra, expanded)) => {
                    let span = Span { file: super::sources::file_id(&source.path), start: offset as u32, end: offset as u32 };
                    owns.insert(name, Own { stmts, defs, hooked, extra, expanded, span, source_index: index, offset });
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
        // `initialize`'s `super` is the expansion's own, below.
        let api = PER_CLASS_API.iter().chain(ROOT_API).map(|(m, _)| *m).filter(|m| *m != "initialize").chain(["include?"]);
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
    // `scope_name:` makes the gem alias the scope under that name on the instance, which the expansion does not.
    if let Some(source) = sources.iter().find(|s| s.text.contains("scope_name:") || s.text.contains(":scope_name")) {
        let offset = source.text.find("scope_name").unwrap_or(0);
        for name in &tree {
            refused.entry(name.clone()).or_insert_with(|| refuse(source, offset, "the app passes `scope_name:`, which the gem aliases the scope under"));
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
                    // Not an implicit one's candidates: which record reaches it is not known here, and a refused
                    // candidate is reported on its own.
                    Stmt::Association(a, _) if a.implicit.is_none() && refused.contains_key(&a.serializer) => Some(a.serializer.clone()),
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
        flat.rootless_skips = rootless_skips;
        flats.insert(name.clone(), flat);
    }
    let mut failed: HashSet<String> = HashSet::default();
    let mut lazy_clash: HashSet<String> = HashSet::default();
    for name in &lowered {
        if chains[name].iter().any(|a| failed.contains(a)) {
            failed.insert(name.clone());
            continue;
        }
        let src = synthesize(name, &chains[name], &owns, &flats[name]);
        let Some(index) = app.library_classes.iter().position(|c| c.name.0.as_str() == name) else { continue };
        let before = app.library_classes[index].clone();
        // An ancestor's own `initialize` is what the class's `super` reaches, not the gem's constructor.
        let inherits_initialize = chains[name].iter().take_while(|a| *a != name).any(|a| owns[a].defs.iter().any(|d| d.name == "initialize"));
        match install(&mut app.library_classes[index], &src, owns[name].span, chains[name].len() == 1, inherits_initialize) {
            // Not a name the app already gives a class: its own stays, and the array is not deferred.
            Ok(nested) => {
                for class in nested {
                    if app.library_classes.iter().any(|c| c.name == class.name) {
                        lazy_clash.insert(name.clone());
                    } else {
                        app.library_classes.push(class);
                    }
                }
            }
            Err(e) => {
                app.library_classes[index] = before;
                failed.insert(name.clone());
                if !super::survey::is_active() {
                    return Err(e);
                }
                super::survey::record(&e);
            }
        }
    }
    let mut done: HashSet<String> = lowered.into_iter().filter(|n| !failed.contains(n)).collect();
    // A macro the expansion ran leaves its class: it ran as the class loaded, and kept, its `define_method` reads to
    // the analysis as a class whose constructor it cannot verify.
    let ran: HashSet<(String, String)> = owns.iter().filter(|(n, _)| done.contains(*n)).flat_map(|(_, o)| o.expanded.iter().cloned()).collect();
    for lc in app.library_classes.iter_mut().filter(|c| done.contains(c.name.0.as_str())) {
        let class = lc.name.0.as_str().to_string();
        lc.methods.retain(|m| !(m.receiver == crate::dialect::MethodReceiver::Class && ran.contains(&(class.clone(), m.name.as_str().to_string()))));
    }
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
    let lazy_ok: HashSet<String> = done.iter().filter(|n| !lazy_clash.contains(*n)).cloned().collect();
    for_each_rewritten_body(app, &mut |body| rewrite_arrays(body, &done, &lazy_ok));
    // The app's copy of `include!` is the gem's, which nothing reaches once every serializer is expanded; a refused one
    // still runs through it.
    if reopened && tree.iter().all(|n| done.contains(n)) {
        app.library_classes.retain(|c| c.name.0.as_str() != SERIALIZER);
    }
    // `include ActiveModel::SerializerSupport` gives `active_model_serializer`, which the implicit pairs above
    // answered, and `read_attribute_for_serialization`, `send` by another name, which the expansion reads as a
    // reader. The gem's module is not emitted, so the include would be an `uninitialized constant` at class load;
    // an app's own call of either is a method the class does not have, reported as one.
    for lc in app.library_classes.iter_mut() {
        let spanned = lc.include_spans.len() == lc.includes.len();
        let mut at = 0;
        let spans = std::mem::take(&mut lc.include_spans);
        lc.includes.retain(|i| {
            let keep = i.0.as_str().trim_start_matches("::") != "ActiveModel::SerializerSupport";
            if spanned && keep {
                lc.include_spans.push(spans[at]);
            }
            at += 1;
            keep
        });
        if !spanned {
            lc.include_spans = spans;
        }
    }
    Ok(())
}

/// A method name written as a Symbol or a String (`define_method("x")`).
fn method_name_value(node: &ruby_prism::Node<'_>) -> Option<String> {
    symbol_value(node).or_else(|| node.as_string_node().map(|s| String::from_utf8_lossy(s.unescaped()).into_owned()))
}

/// Whether `source` defines a method `name` anywhere, on either side: a `def` however spaced or wrapped, an `alias`,
/// or an `alias_method` / `define_method` / `define_singleton_method` naming it.
fn defines_method(source: &SourceFile, name: &str) -> bool {
    struct Find<'n> {
        name: &'n str,
        found: bool,
    }
    impl<'pr> ruby_prism::Visit<'pr> for Find<'_> {
        fn visit_def_node(&mut self, node: &ruby_prism::DefNode<'pr>) {
            self.found |= constant_id_str(&node.name()) == self.name;
            ruby_prism::visit_def_node(self, node);
        }
        fn visit_alias_method_node(&mut self, node: &ruby_prism::AliasMethodNode<'pr>) {
            self.found |= method_name_value(&node.new_name()).as_deref() == Some(self.name);
            ruby_prism::visit_alias_method_node(self, node);
        }
        fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
            if matches!(constant_id_str(&node.name()), "alias_method" | "define_method" | "define_singleton_method") {
                let first = node.arguments().and_then(|a| a.arguments().iter().next());
                self.found |= first.as_ref().and_then(method_name_value).as_deref() == Some(self.name);
            }
            ruby_prism::visit_call_node(self, node);
        }
    }
    let parsed = super::prism::parse_silent(source.text.as_bytes());
    let mut find = Find { name, found: false };
    ruby_prism::Visit::visit(&mut find, &parsed.node());
    find.found
}

/// Where `source` defines a `read_attribute_for_serialization` other than one that sends the name, as Active Record's
/// alias does (`alias read_attribute_for_serialization send`, `def …(name) = public_send(name)`): the expansion reads
/// an attribute by its reader, which only those match. A serializer's own is refused with its class.
fn own_serialization_reader<'s>(source: &'s SourceFile, tree: &HashSet<String>) -> Option<(&'s SourceFile, usize)> {
    const NAME: &str = "read_attribute_for_serialization";
    const SENDS: &[&str] = &["send", "__send__", "public_send"];
    if !source.text.contains(NAME) {
        return None;
    }
    // Every definition, whatever wraps it (`private def …`, an endless `def … = …`), read from the tree.
    struct Find {
        found: Vec<(usize, bool)>,
    }
    impl<'pr> ruby_prism::Visit<'pr> for Find {
        fn visit_def_node(&mut self, node: &ruby_prism::DefNode<'pr>) {
            if constant_id_str(&node.name()) == NAME {
                let param = node.parameters().and_then(|p| {
                    let req: Vec<_> = p.requireds().iter().collect();
                    let only = p.optionals().iter().next().is_none() && p.rest().is_none() && p.keywords().iter().next().is_none();
                    match (req.as_slice(), only) {
                        ([one], true) => one.as_required_parameter_node().map(|r| constant_id_str(&r.name()).to_string()),
                        _ => None,
                    }
                });
                let body: Vec<_> = node.body().map(flatten_statements).unwrap_or_default();
                let sends = match (param, body.as_slice()) {
                    (Some(param), [one]) => one.as_call_node().is_some_and(|c| {
                        SENDS.contains(&constant_id_str(&c.name()))
                            && c.receiver().is_none_or(|r| r.as_self_node().is_some())
                            && c.block().is_none()
                            && c.arguments().is_some_and(|a| {
                                let args: Vec<_> = a.arguments().iter().collect();
                                matches!(args.as_slice(), [arg] if arg.as_local_variable_read_node().is_some_and(|r| constant_id_str(&r.name()) == param))
                            })
                    }),
                    _ => false,
                };
                self.found.push((node.location().start_offset(), sends));
            }
            ruby_prism::visit_def_node(self, node);
        }
        fn visit_alias_method_node(&mut self, node: &ruby_prism::AliasMethodNode<'pr>) {
            let name = |n: ruby_prism::Node<'pr>| n.as_symbol_node().map(|s| String::from_utf8_lossy(s.unescaped()).into_owned());
            if name(node.new_name()).as_deref() == Some(NAME) {
                let sends = name(node.old_name()).is_some_and(|o| SENDS.contains(&o.as_str()));
                self.found.push((node.location().start_offset(), sends));
            }
            ruby_prism::visit_alias_method_node(self, node);
        }
        fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
            let method = constant_id_str(&node.name());
            if matches!(method, "alias_method" | "define_method") {
                let args: Vec<_> = node.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
                if args.first().and_then(method_name_value).as_deref() == Some(NAME) {
                    let sends = method == "alias_method" && args.get(1).and_then(method_name_value).is_some_and(|o| SENDS.contains(&o.as_str()));
                    self.found.push((node.location().start_offset(), sends));
                }
            }
            ruby_prism::visit_call_node(self, node);
        }
    }
    let parsed = super::prism::parse_silent(source.text.as_bytes());
    let mut find = Find { found: Vec::new() };
    ruby_prism::Visit::visit(&mut find, &parsed.node());
    let mut classes = Vec::new();
    collect_classes(&parsed.node(), &mut Vec::new(), &mut classes);
    let in_serializer = |offset: usize| {
        classes.iter().any(|(path, class)| {
            tree.contains(&path.join("::")) && class.location().start_offset() <= offset && offset < class.location().end_offset()
        })
    };
    find.found.into_iter().find(|(offset, sends)| !sends && !in_serializer(*offset)).map(|(offset, _)| (source, offset))
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
    /// Methods a literal-list loop defines (`define_method`, `attr_accessor`), as Ruby source for the expansion.
    extra: Vec<String>,
    /// Constants the body assigns a literal list of Symbols, by name.
    lists: HashMap<String, Vec<String>>,
    /// Each class macro a tree class defines (`def self.staff_attributes(*attrs)`): its classes and their sources.
    macros: &'a HashMap<String, Vec<(String, usize)>>,
    /// The constants the tree's class bodies assign, by full name.
    consts: &'a HashSet<String>,
    /// The classes whose records answer `active_model_serializer` with a lowered serializer, or none and why.
    implicit: Option<Vec<(String, String)>>,
    implicit_refusal: String,
    /// The class and its ancestors, nearest first.
    lineage: Vec<String>,
    /// The macros the body ran, by the class that defines each.
    expanded: Vec<(String, String)>,
    /// Whether the module being walked does `extend ActiveSupport::Concern`.
    concern: bool,
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
    macros: &HashMap<String, Vec<(String, usize)>>,
    consts: &HashSet<String>,
    implicit: &(Option<Vec<(String, String)>>, String),
    lineage: Vec<String>,
) -> IngestResult<(Vec<Stmt>, Vec<Def>, Vec<String>, Vec<String>, Vec<(String, String)>)> {
    let class_text = &source.text[class.location().start_offset()..class.location().end_offset()];
    if class_text.contains("@ams_") {
        return Err(refuse(source, class.location().start_offset(), "an instance variable named `@ams_…` would be the expansion's"));
    }
    let mut ctx = Walk { name, tree, modules, sources, stmts: Vec::new(), defs: Vec::new(), visible: true, at: 0, hooked: Vec::new(), hook_seen: false, extra: Vec::new(), lists: HashMap::default(), macros, consts, lineage, expanded: Vec::new(), concern: false, implicit: implicit.0.clone(), implicit_refusal: implicit.1.clone() };
    if let Some(body) = class.body() {
        walk(&mut ctx, flatten_statements(body), source, Mode::Class)?;
    }
    let Walk { stmts, defs, hooked, extra, expanded, .. } = ctx;
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
    Ok((stmts, defs, hooked, extra, expanded))
}

fn walk(ctx: &mut Walk<'_>, body: Vec<ruby_prism::Node<'_>>, source: &SourceFile, mode: Mode<'_>) -> IngestResult<()> {
    let module = matches!(mode, Mode::Module);
    // A mixin's hook runs once every method of it is the class's ancestor's: `include` appends the features first.
    let (hooks, body): (Vec<_>, Vec<_>) = body.into_iter().partition(|stmt| {
        module && (stmt.as_def_node().is_some_and(|d| d.receiver().is_some() && constant_id_str(&d.name()) == "included")
            || concern_included(stmt).is_some())
    });
    for stmt in body.into_iter().chain(hooks) {
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
                if matches!(mode, Mode::Module) && method == "append_features" {
                    return Err(fail("a mixin's `self.append_features` changes what including it adds"));
                }
                // `def self.can_attributes = %i[…]`: a list the body may read as it loads.
                if matches!(mode, Mode::Class) && def.receiver().is_some_and(|r| r.as_self_node().is_some()) {
                    // A later definition replaces the earlier: Ruby calls the latest.
                    ctx.lists.remove(&format!("self.{method}"));
                    if def.parameters().is_none()
                        && let Some(body) = def.body()
                        && let [only] = flatten_statements(body).as_slice()
                        && let Some(names) = symbol_list(only)
                    {
                        ctx.lists.insert(format!("self.{method}"), names);
                    }
                }
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
            if method.starts_with("ams_") || GEM_ONLY.contains(&method.as_str()) {
                return Err(fail(&format!("`def {method}` would be replaced by the expansion")));
            }
            // `def initialize(object, options = {})`: its second parameter is the options Hash.
            let option_locals: Vec<String> = if method == "initialize" {
                def.parameters().into_iter().flat_map(|p| {
                    let names: Vec<String> = p.requireds().iter().filter_map(|r| r.as_required_parameter_node().map(|r| constant_id_str(&r.name()).to_string()))
                        .chain(p.optionals().iter().filter_map(|o| o.as_optional_parameter_node().map(|o| constant_id_str(&o.name()).to_string())))
                        .collect();
                    names.into_iter().nth(1)
                }).collect()
            } else {
                Vec::new()
            };
            let mut calls = GemCalls::new(option_locals, &ctx.lists, ctx.name);
            ruby_prism::Visit::visit(&mut calls, &stmt);
            if let Some((call, at)) = calls.found {
                return Err(refuse(source, at, &format!("`{call}` is the gem's, outside the subset")));
            }
            ctx.defs.push(Def { name: method, at, visible: ctx.visible, supers: calls.supers, from_module: matches!(mode, Mode::Module) });
            continue;
        }
        // Every write replaces the list: `FIELDS = [].freeze` after `FIELDS = [:title]` declares nothing.
        if let Some(w) = stmt.as_constant_write_node() {
            let name = constant_id_str(&w.name()).to_string();
            match symbol_list(&w.value()) {
                Some(names) => ctx.lists.insert(name, names),
                None => ctx.lists.remove(&name),
            };
        }
        if !matches!(mode, Mode::Hook(_))
            && (stmt.as_class_node().is_some() || stmt.as_module_node().is_some() || stmt.as_constant_write_node().is_some())
        {
            continue;
        }
        if matches!(mode, Mode::Class)
            && let Some(call) = stmt.as_call_node()
            && constant_id_str(&call.name()) == "each"
            && call.block().is_some()
        {
            unroll(ctx, &call, source, offset)?;
            continue;
        }
        if matches!(mode, Mode::Class)
            && let Some(alias) = stmt.as_alias_method_node()
        {
            let (Some(to), Some(from)) = (symbol_value(&alias.new_name()), symbol_value(&alias.old_name())) else {
                return Err(fail("`alias` of other than two names"));
            };
            if to.starts_with("ams_") || GEM_ONLY.contains(&to.as_str()) {
                return Err(fail(&format!("`alias {to}` would be replaced by the expansion")));
            }
            let Some(origin) = ctx.defs.iter().rev().find(|d| d.name == from && !d.from_module) else {
                return Err(fail(&format!("`alias {to} {from}` of a method the body does not define")));
            };
            let visible = origin.visible;
            ctx.defs.push(Def { name: to, at, visible, supers: false, from_module: false });
            continue;
        }
        // `if Rails.env.test? … end` writing the module's constants: nothing the class including it defines or declares.
        if module && (stmt.as_if_node().is_some() || stmt.as_unless_node().is_some()) && !declares_anything(&stmt) {
            struct Writes(Vec<String>);
            impl<'pr> ruby_prism::Visit<'pr> for Writes {
                fn visit_constant_write_node(&mut self, node: &ruby_prism::ConstantWriteNode<'pr>) {
                    self.0.push(constant_id_str(&node.name()).to_string());
                    ruby_prism::visit_constant_write_node(self, node);
                }
            }
            let mut writes = Writes(Vec::new());
            ruby_prism::Visit::visit(&mut writes, &stmt);
            if let Some(name) = writes.0.iter().find(|n| ctx.lists.contains_key(n.as_str())) {
                return Err(fail(&format!("a conditional write of `{name}`, a list the body declares from")));
            }
            continue;
        }
        let Some(call) = stmt.as_call_node() else {
            return Err(fail("a serializer body may hold only the gem's declarations, methods, constants and classes"));
        };
        let method = constant_id_str(&call.name());
        let args: Vec<_> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
        // ActiveSupport::Concern's `included do … end`: the block runs in the class once the module's methods are its.
        if module && let Some(block) = concern_included(&stmt) {
            if !ctx.concern {
                return Err(fail("`included` with a block in a module that does not `extend ActiveSupport::Concern`"));
            }
            let visible = std::mem::replace(&mut ctx.visible, true);
            walk(ctx, block.body().map(flatten_statements).unwrap_or_default(), source, Mode::Class)?;
            ctx.visible = visible;
            ctx.hook_seen = true;
            continue;
        }
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
                // `klass.include UserFlairMixin`: the class includes it as the hook runs.
                if method == "include" {
                    let [arg] = args.as_slice() else { return Err(fail("`include` of other than one module")) };
                    include_into(ctx, arg, source, offset)?;
                    continue;
                }
            }
            Mode::Module => {
                if call.receiver().is_none() && method == "extend" && args.len() == 1
                    && super::util::constant_path_of(&args[0]).is_some_and(|p| p.join("::") == "ActiveSupport::Concern")
                {
                    ctx.concern = true;
                    continue;
                }
                if call.receiver().is_none() && method == "private_constant" && args.iter().all(|a| symbol_value(a).is_some()) {
                    continue;
                }
                // `extend self` gives the module its own methods; the class including it is untouched.
                if call.receiver().is_none() && method == "extend" && matches!(args.as_slice(), [a] if a.as_self_node().is_some()) {
                    continue;
                }
                if call.receiver().is_none() && matches!(method, "private" | "protected" | "public") && args.is_empty() {
                    ctx.visible = method != "private";
                    continue;
                }
                // A mixin's own `include`: its methods are the class's ancestors too. Not one with a hook, which
                // Ruby runs for the including mixin, not the class.
                if call.receiver().is_none() && method == "include" && let [arg] = args.as_slice() {
                    let outer = ctx.hooked.len();
                    let outer_hook = ctx.hook_seen;
                    include_into(ctx, arg, source, offset)?;
                    if ctx.hooked.len() != outer {
                        return Err(fail("a mixin including a mixin with its own `self.included`"));
                    }
                    ctx.hook_seen = outer_hook;
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
                // The ingest writes their methods; `method_defined?` sees them from here on.
                if matches!(method, "attr_reader" | "attr_writer" | "attr_accessor") {
                    for arg in &args {
                        let name = symbol_value(arg).ok_or_else(|| fail(&format!("`{method}` of other than Symbols")))?;
                        if method != "attr_writer" {
                            ctx.defs.push(Def { name: name.clone(), at, visible: ctx.visible, supers: false, from_module: false });
                        }
                        if method != "attr_reader" {
                            ctx.defs.push(Def { name: format!("{name}="), at, visible: ctx.visible, supers: false, from_module: false });
                        }
                    }
                    continue;
                }
                if method == "include" {
                    let [arg] = args.as_slice() else { return Err(fail("`include` of other than one module")) };
                    include_into(ctx, arg, source, offset)?;
                    continue;
                }
                // The declaration stays for the app's delegate expansion, which runs after this one.
                if method == "delegate" {
                    for (name, visible) in delegated_names(&args).map_err(|r| fail(&r))? {
                        if name.starts_with("ams_") || GEM_ONLY.contains(&name.as_str()) || DEFINED.contains(&name.as_str()) {
                            return Err(fail(&format!("`delegate` of `{name}`, which the expansion defines")));
                        }
                        ctx.defs.push(Def { name, at, visible, supers: false, from_module: false });
                    }
                    continue;
                }
                // The ingest copies an alias of a method the body defined: a method of its own from here on.
                if method == "alias_method" {
                    let names: Option<Vec<String>> = args.iter().map(|a| symbol_value(a).or_else(|| a.as_string_node().map(|s| String::from_utf8_lossy(s.unescaped()).into_owned()))).collect();
                    let Some([to, from]) = names.as_deref() else { return Err(fail("`alias_method` of other than two names")) };
                    if to.starts_with("ams_") || GEM_ONLY.contains(&to.as_str()) {
                        return Err(fail(&format!("`alias_method :{to}` would be replaced by the expansion")));
                    }
                    let Some(origin) = ctx.defs.iter().rev().find(|d| d.name == *from && !d.from_module) else {
                        return Err(fail(&format!("`alias_method :{to}, :{from}` of a method the body does not define")));
                    };
                    let visible = origin.visible;
                    ctx.defs.push(Def { name: to.clone(), at, visible, supers: false, from_module: false });
                    continue;
                }
            }
        }
        match method {
            "attributes" => {
                for arg in &args {
                    if let Some(attr) = symbol_value(arg) {
                        ctx.stmts.push(attribute(&attr, None, offset, at, source)?);
                    } else if let Some(names) = arg.as_splat_node().and_then(|s| s.expression()).and_then(|e| listed_names(ctx, &e)) {
                        for attr in names {
                            ctx.stmts.push(attribute(&attr, None, offset, at, source)?);
                        }
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
                            let target = if rooted { Some(written.clone()) } else { constant_in(&written, ctx) };
                            let Some(target) = target.filter(|t| ctx.tree.contains(t)) else {
                                return Err(fail(&format!("`serializer: {written}` is not a serializer of the app")));
                            };
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
                        // `option(:root) || @name`: a false or nil root is no root given.
                        "root" if value.as_false_node().is_some() || value.as_nil_node().is_some() => {}
                        "root" => root = Some(key_value(&value).ok_or_else(|| fail("a `root:` other than a Symbol or String"))?),
                        "embed_key" => {
                            embed_key = symbol_value(&value).filter(|k| is_plain(k)).ok_or_else(|| fail("an `embed_key:` other than a Symbol"))?
                        }
                        // 0.8.4 reads none of these (`if:` is 0.10's), so the class loading only builds the value.
                        other if !matches!(other, "name" | "value" | "polymorphic")
                            && (value.as_lambda_node().is_some() || value.as_symbol_node().is_some() || value.as_string_node().is_some()
                                || value.as_true_node().is_some() || value.as_false_node().is_some() || value.as_nil_node().is_some()) => {}
                        other => return Err(fail(&format!("the association option `{other}:` is outside the subset"))),
                    }
                }
                let implicit = match serializer {
                    Some(_) => None,
                    None => Some(ctx.implicit.clone().ok_or_else(|| fail(ctx.implicit_refusal.as_str()))?),
                };
                let serializer = serializer.unwrap_or_default();
                // The gem's `refine` writes the first name's root into the options all the names share.
                let root = root.unwrap_or_else(|| {
                    Key::Sym(if many { first.clone() } else { crate::naming::pluralize(first) })
                });
                for n in assoc_names {
                    ctx.stmts.push(Stmt::Association(
                        Assoc { many, name: n, serializer: serializer.clone(), implicit: implicit.clone(), embed, include, key: key.clone(), root: root.clone(), embed_key: embed_key.clone() },
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
            other => {
                let owner = ctx.lineage.iter().find_map(|c| ctx.macros.get(other)?.iter().find(|(n, _)| n == c).cloned());
                match (owner, args.iter().map(symbol_value).collect::<Option<Vec<String>>>()) {
                    (Some((class, index)), Some(names)) if matches!(mode, Mode::Class) => {
                        ctx.expanded.push((class.clone(), other.to_string()));
                        expand_macro(ctx, other, &class, index, &names).map_err(|e| {
                            let inner = e.to_string();
                            let inner = inner.split("ActiveModel::Serializer subset: ").last().unwrap_or(&inner).to_string();
                            fail(&format!("`{other}`: {inner}"))
                        })?;
                    }
                    _ => return Err(fail(&format!("`{other}` is outside the ActiveModel::Serializer 0.8 subset"))),
                }
            }
        }
    }
    Ok(())
}

/// The methods a literal `delegate :a, :b, to: :t` defines, each with whether it is public: Rails' `module_eval`
/// leaves a surrounding `private` alone, so only `private: true` hides them.
fn delegated_names(args: &[ruby_prism::Node<'_>]) -> Result<Vec<(String, bool)>, String> {
    let Some((options, names)) = args.split_last().filter(|(o, _)| o.as_keyword_hash_node().is_some()) else {
        return Err("`delegate` without `to:`".into());
    };
    let names: Vec<String> = names.iter().map(symbol_value).collect::<Option<_>>().ok_or("`delegate` of other than Symbols")?;
    let (mut to, mut prefix, mut public) = (None, None, true);
    for el in option_elements(options) {
        let assoc = el.as_assoc_node().ok_or("a splatted `delegate` option")?;
        let value = assoc.value();
        match symbol_value(&assoc.key()).as_deref() {
            Some("to") => to = Some(symbol_value(&value).ok_or("a `to:` other than a Symbol")?),
            Some("prefix") if value.as_true_node().is_some() => prefix = Some(None),
            Some("prefix") if value.as_false_node().is_some() || value.as_nil_node().is_some() => prefix = None,
            Some("prefix") => prefix = Some(Some(symbol_value(&value).ok_or("a `prefix:` other than true or a Symbol")?)),
            Some("allow_nil") if value.as_true_node().is_some() || value.as_false_node().is_some() => {}
            Some("private") if value.as_true_node().is_some() || value.as_false_node().is_some() => public = value.as_false_node().is_some(),
            _ => return Err("a `delegate` option other than a literal `to:`, `prefix:`, `allow_nil:` or `private:`".into()),
        }
    }
    let to = to.ok_or("`delegate` without `to:`")?;
    let prefix = match prefix {
        None => String::new(),
        Some(None) => format!("{to}_"),
        Some(Some(p)) => format!("{p}_"),
    };
    Ok(names.into_iter().map(|n| (format!("{prefix}{n}"), public)).collect())
}

/// The gem's `include!` as 0.8.4 writes it, comments dropped, with its rootless branch's body left out: the
/// `raise IncludeError` there is what an app's copy (discourse's `ams_include_without_root.rb`, 0.9's behavior) drops.
const GEM_INCLUDE: &str = r#"def include!(name, options={})
unique_values = if hash = options[:hash] if @options[:hash] == hash @options[:unique_values] ||= {} else {} end else hash = @options[:hash] @options[:unique_values] ||= {} end
node = options[:node] ||= @node value = options[:value]
if options[:include] == nil if @options.key?(:include) options[:include] = @options[:include].include?(name) elsif @options.include?(:exclude) options[:include] = !@options[:exclude].include?(name) end end
association_class = if klass = _associations[name] klass elsif value.respond_to?(:to_ary) Associations::HasMany else Associations::HasOne end
association = association_class.new(name, self, options)
if association.embed_ids? node[association.key] = association.serialize_ids
if association.embed_in_root? && hash.nil? RAISE elsif association.embed_in_root? && association.embeddable? merge_association hash, association.root, association.serializables, unique_values end
elsif association.embed_objects? node[association.key] = association.serialize end end"#;

/// Whether the app reopens the gem's class with a copy of its `include!`, and whether that copy skips a rootless
/// sideload; any other reopening is refused, with where.
fn reopened_gem(sources: &[SourceFile]) -> Result<(bool, bool), (usize, usize, String)> {
    let normal = |text: &str| -> String {
        text.lines()
            .map(|l| l.split_once('#').filter(|(_, rest)| !rest.starts_with('{')).map_or(l, |(code, _)| code))
            .flat_map(|l| l.split_whitespace())
            .collect::<Vec<_>>()
            .join(" ")
            .replace("options = {}", "options={}")
    };
    let (mut found_any, mut skips) = (false, false);
    for (index, source) in sources.iter().enumerate() {
        if !source.text.contains("Serializer") {
            continue;
        }
        let parsed = super::prism::parse_silent(source.text.as_bytes());
        let mut found = Vec::new();
        collect_classes(&parsed.node(), &mut Vec::new(), &mut found);
        for (path, class) in found.into_iter().filter(|(p, _)| p.join("::") == SERIALIZER) {
            let _ = path;
            let offset = class.location().start_offset();
            let reopened = || (index, offset, "the app reopens `ActiveModel::Serializer`, which the expansion replaces".to_string());
            let stmts = class.body().map(flatten_statements).unwrap_or_default();
            let [only] = stmts.as_slice() else { return Err(reopened()) };
            let Some(def) = only.as_def_node().filter(|d| d.receiver().is_none() && constant_id_str(&d.name()) == "include!") else {
                return Err(reopened());
            };
            let text = normal(&source.text[def.location().start_offset()..def.location().end_offset()]);
            // Which runs last is the files' load order, which is not read here.
            if found_any {
                return Err((index, offset, "`ActiveModel::Serializer#include!` reopened more than once".to_string()));
            }
            found_any = true;
            if text == normal(&GEM_INCLUDE.replace("RAISE ", "raise IncludeError.new(self.class, association.name) ")) {
                continue;
            }
            if text == normal(&GEM_INCLUDE.replace("RAISE ", "")) {
                skips = true;
                continue;
            }
            return Err(reopened());
        }
    }
    Ok((found_any, skips))
}

/// A class macro's one required parameter, when it takes that alone.
fn macro_single(def: &ruby_prism::DefNode<'_>) -> Option<String> {
    let params = def.parameters()?;
    if params.optionals().iter().next().is_some() || params.keywords().iter().next().is_some() || params.rest().is_some() || params.keyword_rest().is_some() || params.block().is_some() {
        return None;
    }
    let requireds: Vec<_> = params.requireds().iter().collect();
    let [one] = requireds.as_slice() else { return None };
    Some(constant_id_str(&one.as_required_parameter_node()?.name()).to_string())
}

/// A class macro's rest parameter, when it takes that alone.
fn macro_rest(def: &ruby_prism::DefNode<'_>) -> Option<String> {
    let params = def.parameters()?;
    if params.requireds().iter().next().is_some() || params.optionals().iter().next().is_some() || params.keywords().iter().next().is_some() || params.keyword_rest().is_some() || params.block().is_some() {
        return None;
    }
    let rest = params.rest()?.as_rest_parameter_node()?;
    Some(constant_id_str(&rest.name()?).to_string())
}

/// `staff_attributes :a, :b` in a body: the macro's own body with its rest parameter the literal names, as the
/// class runs it when it loads. `attributes(*attrs)` declares them, and `attrs.each do |attr| … end` runs per name.
fn expand_macro(ctx: &mut Walk<'_>, method: &str, class: &str, index: usize, names: &[String]) -> IngestResult<()> {
    let source = &ctx.sources[index];
    let parsed = super::prism::parse_silent(source.text.as_bytes());
    let mut found = Vec::new();
    collect_classes(&parsed.node(), &mut Vec::new(), &mut found);
    let Some((_, node)) = found.into_iter().find(|(p, _)| p.join("::") == class) else {
        return Err(refuse(source, 0, "the macro's class is not found"));
    };
    let stmts = node.body().map(flatten_statements).unwrap_or_default();
    let defs: Vec<_> = stmts.iter().filter_map(|s| s.as_def_node()).filter(|d| d.receiver().is_some() && constant_id_str(&d.name()) == method).collect();
    // Not the first of several: which one a call runs depends on where it sits among them.
    let openings = ctx.macros.get(method).map_or(0, |found| found.iter().filter(|(c, _)| c == class).count());
    if defs.len() > 1 || openings > 1 {
        return Err(refuse(source, 0, &format!("the macro `{method}` is defined more than once")));
    }
    let Some(def) = defs.into_iter().next() else {
        return Err(refuse(source, 0, "the macro's `def` is not found"));
    };
    // `add_compared_field :wiki`: the body runs as a loop's would, its parameter the one name.
    if let Some(param) = macro_single(&def) {
        let [name] = names else { return Err(refuse(source, def.location().start_offset(), "a one-parameter macro called with other than one name")) };
        let stmts = def.body().map(flatten_statements).unwrap_or_default();
        return unroll_stmts(ctx, std::slice::from_ref(name), &param, &stmts, source, def.location().start_offset());
    }
    let rest = macro_rest(&def).ok_or_else(|| refuse(source, def.location().start_offset(), "a macro taking other than a rest parameter"))?;
    let is_rest = |n: &ruby_prism::Node<'_>| n.as_local_variable_read_node().is_some_and(|r| constant_id_str(&r.name()) == rest);
    for stmt in def.body().map(flatten_statements).unwrap_or_default() {
        let offset = stmt.location().start_offset();
        let fail = |reason: &str| refuse(source, offset, reason);
        let call = stmt.as_call_node().ok_or_else(|| fail("a macro body holding other than calls"))?;
        let name = constant_id_str(&call.name());
        let args: Vec<_> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
        ctx.at += 1;
        let at = ctx.at;
        match (name, call.receiver(), args.as_slice()) {
            ("attributes", None, [splat]) if splat.as_splat_node().and_then(|s| s.expression()).is_some_and(|e| is_rest(&e)) => {
                for n in names {
                    ctx.stmts.push(attribute(n, None, offset, at, source)?);
                }
            }
            // `[list].flatten`: the names are Symbols, which flatten leaves as they are.
            ("each", Some(r), []) if is_rest(&r) || flattened(&r).is_some_and(|e| is_rest(&e)) => {
                let block = call.block().and_then(|b| b.as_block_node()).ok_or_else(|| fail("`each` without a literal block"))?;
                unroll_block(ctx, names, &block, source, offset)?;
            }
            _ => return Err(fail(&format!("`{name}` in a class macro is outside the subset"))),
        }
    }
    Ok(())
}


/// Whether a statement holds a `def`, a call on no receiver (a declaration on the module), or a write to anything but
/// a constant or a local.
fn declares_anything(node: &ruby_prism::Node<'_>) -> bool {
    struct Finds(bool);
    impl<'pr> ruby_prism::Visit<'pr> for Finds {
        fn visit_def_node(&mut self, _: &ruby_prism::DefNode<'pr>) {
            self.0 = true;
        }
        fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
            self.0 |= node.receiver().is_none() && !node.is_variable_call();
            ruby_prism::visit_call_node(self, node);
        }
        fn visit_instance_variable_write_node(&mut self, _: &ruby_prism::InstanceVariableWriteNode<'pr>) {
            self.0 = true;
        }
        fn visit_class_variable_write_node(&mut self, _: &ruby_prism::ClassVariableWriteNode<'pr>) {
            self.0 = true;
        }
        fn visit_global_variable_write_node(&mut self, _: &ruby_prism::GlobalVariableWriteNode<'pr>) {
            self.0 = true;
        }
        fn visit_class_node(&mut self, _: &ruby_prism::ClassNode<'pr>) {
            self.0 = true;
        }
        fn visit_module_node(&mut self, _: &ruby_prism::ModuleNode<'pr>) {
            self.0 = true;
        }
        fn visit_singleton_class_node(&mut self, _: &ruby_prism::SingletonClassNode<'pr>) {
            self.0 = true;
        }
    }
    let mut finds = Finds(false);
    ruby_prism::Visit::visit(&mut finds, node);
    finds.0
}

/// `[x].flatten`'s `x`.
fn flattened<'pr>(node: &ruby_prism::Node<'pr>) -> Option<ruby_prism::Node<'pr>> {
    let call = node.as_call_node().filter(|c| constant_id_str(&c.name()) == "flatten" && c.arguments().is_none() && c.block().is_none())?;
    let array = call.receiver()?.as_array_node()?;
    let mut elements = array.elements().iter();
    let (Some(only), None) = (elements.next(), elements.next()) else { return None };
    match only.as_splat_node() {
        Some(splat) => splat.expression(),
        None => Some(only),
    }
}

/// The block of a module's `included do … end`.
fn concern_included<'pr>(stmt: &ruby_prism::Node<'pr>) -> Option<ruby_prism::BlockNode<'pr>> {
    let call = stmt.as_call_node()?;
    if call.receiver().is_some() || call.arguments().is_some() || constant_id_str(&call.name()) != "included" {
        return None;
    }
    call.block()?.as_block_node().filter(|b| b.parameters().is_none())
}

/// The Symbols a list the body reads holds: a literal, a constant the body assigned one, or a class method of the body
/// answering one, called on no receiver.
fn listed_names(ctx: &Walk<'_>, node: &ruby_prism::Node<'_>) -> Option<Vec<String>> {
    if let Some(names) = symbol_list(node) {
        return Some(names);
    }
    if let Some(c) = node.as_constant_read_node() {
        return ctx.lists.get(constant_id_str(&c.name())).cloned();
    }
    let call = node.as_call_node()?;
    if call.receiver().is_some() || call.arguments().is_some() || call.block().is_some() {
        return None;
    }
    ctx.lists.get(&format!("self.{}", constant_id_str(&call.name()))).cloned()
}

/// The Symbols a literal list holds (`%i[a b]`, `[:a, :b]`).
fn symbol_list(node: &ruby_prism::Node<'_>) -> Option<Vec<String>> {
    let array = node.as_array_node()?;
    array.elements().iter().map(|e| symbol_value(&e)).collect()
}

/// `%i[a b].each { |name| … }` (or a constant the body assigned such a list): its body once per name, the block's
/// parameter read as that Symbol. Declarations land as written; `define_method` and `attr_accessor` are written as
/// methods for the expansion, as the loop defines them when the class loads.
fn unroll(ctx: &mut Walk<'_>, call: &ruby_prism::CallNode<'_>, source: &SourceFile, offset: usize) -> IngestResult<()> {
    let fail = |reason: &str| refuse(source, offset, reason);
    let receiver = call.receiver().ok_or_else(|| fail("`each` without a list"))?;
    let names = listed_names(ctx, &receiver).ok_or_else(|| fail("`each` over other than a literal list of Symbols"))?;
    let block = call.block().and_then(|b| b.as_block_node()).ok_or_else(|| fail("`each` without a literal block"))?;
    unroll_block(ctx, &names, &block, source, offset)
}

/// A loop's block run once per name in `names`.
fn unroll_block(ctx: &mut Walk<'_>, names: &[String], block: &ruby_prism::BlockNode<'_>, source: &SourceFile, offset: usize) -> IngestResult<()> {
    let fail = |reason: &str| refuse(source, offset, reason);
    if !ctx.visible {
        return Err(fail("a loop defining methods after `private`"));
    }
    let params: Vec<String> = block
        .parameters()
        .and_then(|p| p.as_block_parameters_node())
        .and_then(|p| p.parameters())
        .map(|p| p.requireds().iter().filter_map(|r| r.as_required_parameter_node().map(|r| constant_id_str(&r.name()).to_string())).collect())
        .unwrap_or_default();
    let [param] = params.as_slice() else { return Err(fail("an `each` block taking other than one name")) };
    let Some(body) = block.body() else { return Ok(()) };
    unroll_stmts(ctx, names, param, &flatten_statements(body), source, offset)
}

/// Statements run once per name in `names`, `param` read as that name's Symbol.
fn unroll_stmts(ctx: &mut Walk<'_>, names: &[String], param: &str, stmts: &[ruby_prism::Node<'_>], source: &SourceFile, offset: usize) -> IngestResult<()> {
    let fail = |reason: &str| refuse(source, offset, reason);
    if !ctx.visible {
        return Err(fail("a loop defining methods after `private`"));
    }
    // The source of `node` with each read of the parameter written as `name`'s Symbol.
    let text_of = |node: &ruby_prism::Node<'_>, name: &str| -> String {
        struct Reads<'a> {
            param: &'a str,
            found: Vec<(usize, usize)>,
        }
        impl<'pr> ruby_prism::Visit<'pr> for Reads<'_> {
            fn visit_local_variable_read_node(&mut self, node: &ruby_prism::LocalVariableReadNode<'pr>) {
                if constant_id_str(&node.name()) == self.param {
                    self.found.push((node.location().start_offset(), node.location().end_offset()));
                }
            }
        }
        let mut reads = Reads { param, found: Vec::new() };
        ruby_prism::Visit::visit(&mut reads, node);
        let (start, end) = (node.location().start_offset(), node.location().end_offset());
        let mut text = source.text[start..end].to_string();
        reads.found.sort();
        for (a, b) in reads.found.into_iter().rev() {
            text.replace_range(a - start..b - start, &format!(":{name}"));
        }
        text
    };
    let is_param = |n: &ruby_prism::Node<'_>| n.as_local_variable_read_node().is_some_and(|r| constant_id_str(&r.name()) == param);
    // The name a `"include_#{attr}?"` spells for `name`, or none if it reads anything but the loop's name.
    let spelled = |node: &ruby_prism::Node<'_>, name: &str, locals: &HashMap<String, String>| -> Option<String> {
        let read = |n: &ruby_prism::Node<'_>| -> Option<String> {
            if is_param(n) {
                return Some(name.to_string());
            }
            locals.get(constant_id_str(&n.as_local_variable_read_node()?.name())).cloned()
        };
        if let Some(value) = read(node) {
            return Some(value);
        }
        let parts: Vec<_> = match (node.as_interpolated_string_node(), node.as_interpolated_symbol_node()) {
            (Some(s), _) => s.parts().iter().collect(),
            (_, Some(s)) => s.parts().iter().collect(),
            _ => return None,
        };
        let mut out = String::new();
        for part in parts {
            if let Some(t) = part.as_string_node() {
                out.push_str(&String::from_utf8_lossy(t.unescaped()));
            } else {
                let e = part.as_embedded_statements_node()?;
                let st = e.statements()?;
                let body: Vec<_> = st.body().iter().collect();
                let [one] = body.as_slice() else { return None };
                out.push_str(&read(one)?);
            }
        }
        Some(out)
    };
    for name in names {
        let mut locals: HashMap<String, String> = HashMap::default();
        for stmt in stmts {
            let fail = |reason: &str| refuse(source, stmt.location().start_offset(), reason);
            if let Some(w) = stmt.as_local_variable_write_node() {
                if constant_id_str(&w.name()) == param {
                    return Err(fail("a declaring loop reassigning its name"));
                }
                let value = spelled(&w.value(), name, &locals).ok_or_else(|| fail("a loop local other than a name spelled from the loop's"))?;
                locals.insert(constant_id_str(&w.name()).to_string(), value);
                continue;
            }
            let Some(inner) = stmt.as_call_node() else { return Err(fail("a loop body holding other than calls")) };
            if inner.receiver().is_some() {
                return Err(fail("a receiver in a declaring loop"));
            }
            let method = constant_id_str(&inner.name());
            let args: Vec<_> = inner.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
            ctx.at += 1;
            let at = ctx.at;
            match (method, args.as_slice()) {
                ("attributes" | "attribute", [arg]) if inner.block().is_none() && (is_param(arg) || arg.as_local_variable_read_node().is_some()) => {
                    let attr = spelled(arg, name, &locals).ok_or_else(|| fail("an attribute named by other than the loop's name"))?;
                    ctx.stmts.push(attribute(&attr, None, stmt.location().start_offset(), at, source)?);
                }
                ("define_method", [arg]) => {
                    // The name as written, its interpolations of the parameter answered.
                    let method_name = spelled(arg, name, &locals).ok_or_else(|| fail("a `define_method` name spelled from other than the loop's name"))?;
                    let Some(mblock) = inner.block().and_then(|b| b.as_block_node()) else {
                        return Err(fail("a `define_method` without a literal block"));
                    };
                    if mblock.parameters().is_some() {
                        return Err(fail("a `define_method` block with parameters"));
                    }
                    struct LocalReads<'a>(&'a HashMap<String, String>, bool);
                    impl<'pr> ruby_prism::Visit<'pr> for LocalReads<'_> {
                        fn visit_local_variable_read_node(&mut self, node: &ruby_prism::LocalVariableReadNode<'pr>) {
                            self.1 |= self.0.contains_key(constant_id_str(&node.name()));
                        }
                    }
                    let mut reads = LocalReads(&locals, false);
                    ruby_prism::Visit::visit(&mut reads, &mblock.as_node());
                    if reads.1 {
                        return Err(fail("a `define_method` body reading a loop local"));
                    }
                    let body = mblock.body().map(|b| text_of(&b, name)).unwrap_or_else(|| "nil".to_string());
                    if method_name.starts_with("ams_") || method_name == "initialize" || GEM_ONLY.contains(&method_name.as_str()) {
                        return Err(fail(&format!("`define_method` of `{method_name}`, which the expansion writes")));
                    }
                    let mut calls = GemCalls::new(Vec::new(), &HashMap::default(), "");
                    ruby_prism::Visit::visit(&mut calls, &mblock.as_node());
                    if let Some((call, _)) = calls.found {
                        return Err(fail(&format!("`{call}` in a `define_method` block is the gem's, outside the subset")));
                    }
                    ctx.defs.push(Def { name: method_name.clone(), at, visible: true, supers: calls.supers, from_module: false });
                    ctx.extra.push(format!("  def {method_name}\n    {body}\n  end\n"));
                }
                // `class_eval %{def #{attribute} … end}`: the source the string spells, which must be methods alone.
                ("class_eval", [arg]) if inner.block().is_none() && arg.as_interpolated_string_node().is_some() => {
                    let text = spelled(arg, name, &locals).ok_or_else(|| fail("a `class_eval` string interpolating other than the loop's name"))?;
                    let parsed = super::prism::parse_silent(text.as_bytes());
                    if parsed.errors().count() != 0 {
                        return Err(fail("a `class_eval` string that does not parse"));
                    }
                    let program = parsed.node();
                    let body: Vec<_> = program.as_program_node().map(|p| p.statements().body().iter().collect()).unwrap_or_default();
                    let mut defined = Vec::new();
                    for d in &body {
                        let Some(def) = d.as_def_node().filter(|d| d.receiver().is_none()) else {
                            return Err(fail("a `class_eval` string holding other than methods"));
                        };
                        let mut calls = GemCalls::new(Vec::new(), &HashMap::default(), "");
                        ruby_prism::Visit::visit(&mut calls, d);
                        if let Some((call, _)) = calls.found {
                            return Err(fail(&format!("`{call}` in a `class_eval` string is the gem's, outside the subset")));
                        }
                        defined.push((constant_id_str(&def.name()).to_string(), calls.supers));
                    }
                    for (m, supers) in defined {
                        if m.starts_with("ams_") || m == "initialize" || GEM_ONLY.contains(&m.as_str()) {
                            return Err(fail(&format!("`class_eval` of `{m}`, which the expansion writes")));
                        }
                        ctx.defs.push(Def { name: m, at, visible: true, supers, from_module: false });
                    }
                    ctx.extra.push(format!("{text}\n"));
                }
                ("public_send" | "send", [what, arg]) if symbol_value(what).as_deref() == Some("attr_accessor") && is_param(arg) => {
                    for (m, body) in [(name.clone(), format!("@{name}")), (format!("{name}="), String::new())] {
                        ctx.defs.push(Def { name: m.clone(), at, visible: true, supers: false, from_module: false });
                        if body.is_empty() {
                            ctx.extra.push(format!("  def {m}(value)\n    @{name} = value\n  end\n"));
                        } else {
                            ctx.extra.push(format!("  def {m}\n    {body}\n  end\n"));
                        }
                    }
                }
                _ => return Err(fail(&format!("`{method}` in a declaring loop is outside the subset"))),
            }
        }
    }
    Ok(())
}

/// `include M` at the walk's place: M's methods are an ancestor's from here on, and its `self.included` declares here.
fn include_into(ctx: &mut Walk<'_>, arg: &ruby_prism::Node<'_>, source: &SourceFile, offset: usize) -> IngestResult<()> {
    let fail = |reason: &str| refuse(source, offset, reason);
    let written = super::util::constant_path_of(arg).map(|p| p.join("::")).ok_or_else(|| fail("`include` of other than a constant"))?;
    let module = if is_rooted(arg) { written.clone() } else { lexical_in(&written, ctx.name, ctx.modules) };
    // A module of no methods (`module NoteHelpers; include NoteBase; end`) is no class the ingest keeps: read where
    // its source defines it.
    let leaf = module.rsplit("::").next().unwrap_or(&module);
    let found = ctx.modules.get(&module).copied().or_else(|| {
        ctx.sources.iter().position(|src| {
            src.text.contains(&format!("module {leaf}"))
                && module_body(&super::prism::parse_silent(src.text.as_bytes()).node(), &module).is_some()
        })
    });
    let Some(index) = found else {
        return Err(fail(&format!("`include {written}`: not a module of the app")));
    };
    // One body only: a later one may define another `self.included` or methods, and Ruby takes them all.
    let bodies = ctx
        .sources
        .iter()
        .filter(|src| src.text.contains(&format!("module {leaf}")))
        .map(|src| count_module_bodies(&super::prism::parse_silent(src.text.as_bytes()).node(), &module))
        .sum::<usize>();
    if bodies > 1 {
        return Err(fail(&format!("`include {written}`: a module reopened in more than one body")));
    }
    let msource = &ctx.sources[index];
    let parsed = super::prism::parse_silent(msource.text.as_bytes());
    let Some(mbody) = module_body(&parsed.node(), &module) else {
        return Err(fail(&format!("`include {written}`: its source is not found")));
    };
    let outer_hook = std::mem::replace(&mut ctx.hook_seen, false);
    let outer_concern = std::mem::replace(&mut ctx.concern, false);
    // A module's own `private` does not carry into the class.
    let visible = std::mem::replace(&mut ctx.visible, true);
    // Ledgered at the `include`, not in the mixin: a gap there would claim the mixin file's own findings.
    walk(ctx, mbody, msource, Mode::Module).map_err(|e| {
        let inner = e.to_string();
        let inner = inner.split("ActiveModel::Serializer subset: ").last().unwrap_or(&inner).to_string();
        fail(&format!("`include {written}`: {inner}"))
    })?;
    ctx.visible = visible;
    if ctx.hook_seen && !ctx.hooked.contains(&module) {
        ctx.hooked.push(module);
    }
    ctx.hook_seen = outer_hook;
    ctx.concern = outer_concern;
    Ok(())
}

/// How many bodies of module `path` a parsed file holds.
fn count_module_bodies(node: &ruby_prism::Node<'_>, path: &str) -> usize {
    fn go(node: &ruby_prism::Node<'_>, scope: &mut Vec<String>, path: &str) -> usize {
        let stmts: Vec<ruby_prism::Node<'_>> = if let Some(p) = node.as_program_node() {
            p.statements().body().iter().collect()
        } else if let Some(s) = node.as_statements_node() {
            s.body().iter().collect()
        } else if let Some(m) = node.as_module_node() {
            let Some(name) = super::util::constant_path_of(&m.constant_path()) else { return 0 };
            let depth = scope.len();
            scope.extend(name);
            let here = usize::from(scope.join("::") == path) + m.body().map_or(0, |b| go(&b, scope, path));
            scope.truncate(depth);
            return here;
        } else if let Some(c) = node.as_class_node() {
            let Some(name) = super::util::constant_path_of(&c.constant_path()) else { return 0 };
            let depth = scope.len();
            scope.extend(name);
            let here = c.body().map_or(0, |b| go(&b, scope, path));
            scope.truncate(depth);
            return here;
        } else {
            return 0;
        };
        stmts.iter().map(|s| go(s, scope, path)).sum()
    }
    go(node, &mut Vec::new(), path)
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

/// The serializer a constant written in the class body names, as Ruby looks it up: the lexical scopes, then the
/// class's ancestors, then the top level. None where a constant the tree assigns comes first, which may stand for
/// any class.
fn constant_in(written: &str, ctx: &Walk<'_>) -> Option<String> {
    let mut scopes: Vec<String> = Vec::new();
    let mut scope: Vec<&str> = ctx.name.split("::").collect();
    while !scope.is_empty() {
        scopes.push(scope.join("::"));
        scope.pop();
    }
    scopes.extend(ctx.lineage.iter().skip(1).cloned());
    for scope in scopes {
        let candidate = format!("{scope}::{written}");
        if ctx.consts.contains(&candidate) {
            return None;
        }
        if ctx.tree.contains(&candidate) {
            return Some(candidate);
        }
    }
    Some(written.to_string())
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

/// `options`, `self.options`, `@options` or a local assigned one of them: the Hash the serializer was built with.
fn options_read(node: &ruby_prism::Node<'_>, locals: &[String]) -> bool {
    if let Some(ivar) = node.as_instance_variable_read_node() {
        return constant_id_str(&ivar.name()) == "@options";
    }
    if let Some(local) = node.as_local_variable_read_node() {
        return locals.iter().any(|l| l == constant_id_str(&local.name()));
    }
    node.as_call_node().is_some_and(|c| {
        constant_id_str(&c.name()) == "options" && c.receiver().is_none_or(|r| r.as_self_node().is_some()) && c.arguments().is_none()
    })
}

struct GemCalls {
    found: Option<(String, usize)>,
    supers: bool,
    option_locals: Vec<String>,
    /// The class's literal Symbol-list constants that hold none of the keys the gem adds, and the class's own name.
    safe_lists: HashSet<String>,
    owner: String,
    /// The parameter of each `LIST.each` block the walk is inside: a key from one of those lists.
    listed: Vec<String>,
    /// The listed parameters the block assigns: no longer a key from the list.
    rebound: HashSet<String>,
}

impl GemCalls {
    fn new(option_locals: Vec<String>, lists: &HashMap<String, Vec<String>>, owner: &str) -> Self {
        let safe_lists = lists.iter().filter(|(_, names)| !names.iter().any(|n| matches!(n.as_str(), "hash" | "unique_values"))).map(|(k, _)| k.clone()).collect();
        GemCalls { found: None, supers: false, option_locals, safe_lists, owner: owner.rsplit("::").next().unwrap_or(owner).to_string(), listed: Vec::new(), rebound: HashSet::default() }
    }

    fn rebind(&mut self, name: String) {
        if self.listed.contains(&name) {
            self.rebound.insert(name);
        }
    }

    /// `LIST` or `Owner::LIST`, one of the class's safe lists.
    fn safe_list(&self, node: &ruby_prism::Node<'_>) -> bool {
        if let Some(read) = node.as_constant_read_node() {
            return self.safe_lists.contains(constant_id_str(&read.name()));
        }
        let Some(path) = node.as_constant_path_node() else { return false };
        let named = path.name().is_some_and(|n| self.safe_lists.contains(constant_id_str(&n)));
        let own = path.parent().is_some_and(|p| p.as_constant_read_node().is_some_and(|r| constant_id_str(&r.name()) == self.owner));
        named && own
    }
}

impl<'pr> ruby_prism::Visit<'pr> for GemCalls {
    fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
        let name = constant_id_str(&node.name());
        let on_self = node.receiver().is_none_or(|r| r.as_self_node().is_some());
        if self.found.is_none() && on_self && (GEM_ONLY.contains(&name) || name.starts_with("ams_")) {
            self.found = Some((name.to_string(), node.location().start_offset()));
        }
        // The gem's `as_json` writes these into the options a serializer is built with; the expansion holds them apart.
        let args: Vec<_> = node.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
        // Every key of a multi-key reader; only the first of `fetch` and the like, whose second is a default.
        let keys: &[ruby_prism::Node<'_>] = match name {
            "values_at" | "slice" | "except" => &args,
            // `options[:include_status] = true` after `super`: the Hash is the caller's, as the gem's `@options = options` keeps it.
            "[]" | "[]=" | "store" | "fetch" | "key?" | "has_key?" | "include?" | "member?" | "dig" | "delete" => &args[..args.len().min(1)],
            _ => &[],
        };
        let shared = keys.iter().filter_map(symbol_value).find(|k| matches!(k.as_str(), "hash" | "unique_values"));
        let on_options = node.receiver().is_some_and(|r| options_read(&r, &self.option_locals));
        // `PostSerializer::INSTANCE_VARS.each { |name| opts[name] }`: a key from a literal list the gem's keys are not in.
        let listed = |k: &ruby_prism::Node<'_>| {
            k.as_local_variable_read_node().is_some_and(|l| {
                let name = constant_id_str(&l.name());
                self.listed.iter().any(|n| n == name) && !self.rebound.contains(name)
            })
        };
        if self.found.is_none() && on_options {
            if let Some(key) = shared {
                self.found = Some((format!("{name}(:{key})"), node.location().start_offset()));
            } else if keys.is_empty() || !keys.iter().all(|k| symbol_value(k).is_some() || listed(k)) {
                // The whole Hash (`opts.size`, `opts.keys`) or a computed key: it may see the keys the gem adds.
                self.found = Some((format!("options.{name}"), node.location().start_offset()));
            }
        }
        let param = (name == "each" && node.arguments().is_none() && node.receiver().is_some_and(|r| self.safe_list(&r)))
            .then(|| node.block().and_then(|b| b.as_block_node()))
            .flatten()
            .and_then(|b| b.parameters())
            .and_then(|p| p.as_block_parameters_node())
            .and_then(|p| p.parameters())
            .and_then(|p| match p.requireds().iter().collect::<Vec<_>>().as_slice() {
                [only] => only.as_required_parameter_node().map(|r| constant_id_str(&r.name()).to_string()),
                _ => None,
            });
        let pushed = param.is_some();
        self.listed.extend(param);
        ruby_prism::visit_call_node(self, node);
        if pushed {
            self.listed.pop();
        }
    }
    fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'pr>) {
        self.rebind(constant_id_str(&node.name()).to_string());
        if options_read(&node.value(), &self.option_locals) {
            self.option_locals.push(constant_id_str(&node.name()).to_string());
        }
        ruby_prism::visit_local_variable_write_node(self, node);
    }
    fn visit_local_variable_operator_write_node(&mut self, node: &ruby_prism::LocalVariableOperatorWriteNode<'pr>) {
        self.rebind(constant_id_str(&node.name()).to_string());
        ruby_prism::visit_local_variable_operator_write_node(self, node);
    }
    fn visit_local_variable_or_write_node(&mut self, node: &ruby_prism::LocalVariableOrWriteNode<'pr>) {
        self.rebind(constant_id_str(&node.name()).to_string());
        ruby_prism::visit_local_variable_or_write_node(self, node);
    }
    fn visit_local_variable_and_write_node(&mut self, node: &ruby_prism::LocalVariableAndWriteNode<'pr>) {
        self.rebind(constant_id_str(&node.name()).to_string());
        ruby_prism::visit_local_variable_and_write_node(self, node);
    }
    fn visit_local_variable_target_node(&mut self, node: &ruby_prism::LocalVariableTargetNode<'pr>) {
        self.rebind(constant_id_str(&node.name()).to_string());
        ruby_prism::visit_local_variable_target_node(self, node);
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
    for method in &owns[name].extra {
        src.push('\n');
        src.push_str(method);
    }
    // An own `def` of the name is the class's, as it overrides the gem's.
    // And an ancestor's: the class inherits it, as it does from the gem's base, and a copy here would hide it.
    let own: HashSet<&str> = chain.iter().flat_map(|c| owns[c].defs.iter().map(|d| d.name.as_str())).collect();
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
                // Not a mixin's: one included after the declaration sits above the class, whose reader wins.
                if ancestor == name && !owns[ancestor].defs.iter().any(|d| d.name == m && d.at > at && !d.from_module) {
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
    // Each Hash built in the one method that fills it: one a callee writes into types without what it wrote.
    src.push_str("\n  def ams_attributes\n    h = {}\n");
    for (attr, key) in &flat.attributes {
        // `self.h`, not `h()`: the Ruby emit drops the parentheses, and an attribute named `h` then reads the Hash.
        src.push_str(&format!("    h[{}] = self.{attr} if include?({})\n", Key::Sym(key.clone()).ruby(), Key::Sym(attr.clone()).ruby()));
    }
    src.push_str("    h\n  end\n");
    for a in &flat.assocs {
        let Some(pairs) = &a.implicit else { continue };
        // `instance_of?`, not `is_a?`: the gem asks for the record's own class's serializer, a subclass's by its name.
        src.push_str(&format!("\n  def ams_find_{}(value)\n", a.name));
        for (i, (class, serializer)) in pairs.iter().enumerate() {
            let kw = if i == 0 { "if" } else { "elsif" };
            src.push_str(&format!("    {kw} value.instance_of?(::{class})\n      ::{serializer}.new(value, @options).ams_adopt(@ams_side, @ams_unique)\n"));
        }
        if pairs.is_empty() {
            src.push_str("    value\n  end\n");
        } else {
            src.push_str("    else\n      value\n    end\n  end\n");
        }
    }
    src.push_str("\n  def ams_serializable_hash\n    node = attributes\n");
    if !flat.assocs.is_empty() {
        src.push_str("    if @object.present?\n");
        for a in &flat.assocs {
            src.push_str(&association(a, flat).lines().map(|l| format!("  {l}\n")).collect::<String>());
        }
        src.push_str("    end\n");
    }
    src.push_str("    node\n  end\n");
    src.push_str(&format!(
        r#"
  def self.ams_array_as_json(objects, options)
    hash = {{}}
    unique = {{}}
    root = options[:root]
    items = objects.map {{ |item| ::{name}.new(item, options).ams_adopt(hash, unique).serializable_hash }}
    if root
      hash[root] = items
      ams_array_meta(hash, options)
      hash
    else
      items
    end
  end

  def self.ams_array_meta(hash, options)
    meta_key = options[:meta_key]
    hash[meta_key.respond_to?(:to_sym) ? meta_key.to_sym : :meta] = options[:meta] if options.has_key?(:meta)
  end

  class AmsArray
    def initialize(objects, options)
      @objects = objects
      @options = options
    end

    def as_json(*args)
      ::{name}.ams_array_as_json(@objects, @options)
    end

    def to_json(*args)
      ActiveSupport::JSON.encode(as_json)
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
    let nested = |value: &str| match &a.implicit {
        Some(_) => format!("ams_find_{name}({value})"),
        None => format!("{target}.new({value}, @options).ams_adopt(@ams_side, @ams_unique)"),
    };
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
            if flat.rootless_skips {
                out.push_str("      side = @ams_side\n      if included && !side.nil?\n");
            } else {
                out.push_str("      if included\n");
                out.push_str("        side = @ams_side\n");
                out.push_str(&format!(
                    "        raise \"Cannot serialize {name} when #{{self.class}} does not have a root!\" if side.nil?\n"
                ));
            }
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

/// The gem's constructor, readers and the methods answering the class's Hash, on every class of the tree, by name:
/// one on the root alone is typed with the root's own declarations, whatever subclass it answers for.
const PER_CLASS_API: &[(&str, &str)] = &[
    ("ams_initialize", "  def ams_initialize(object, options = {})\n    @object = object\n    @options = options\n    @ams_side = nil\n    @ams_unique = nil\n  end\n"),
    ("initialize", "  def initialize(object, options = {})\n    ams_initialize(object, options)\n  end\n"),
    ("object", "  def object\n    @object\n  end\n"),
    ("options", "  def options\n    @options\n  end\n"),
    ("scope", "  def scope\n    @options[:scope]\n  end\n"),
    ("ams_adopt", "  def ams_adopt(side, unique)\n    @ams_side = side\n    @ams_unique = unique\n    self\n  end\n"),
    ("root_name", "  def root_name\n    ams_root_name\n  end\n"),
    ("attributes", "  def attributes\n    ams_attributes\n  end\n"),
    ("serializable_hash", "  def serializable_hash\n    return nil if @object.nil?\n    ams_serializable_hash\n  end\n"),
    // The gem's `to_json` is ActiveSupport's `Object#to_json`, `ActiveSupport::JSON.encode(self)`, which encodes what `as_json` answers.
    ("to_json", "  def to_json(options = nil)\n    ActiveSupport::JSON.encode(as_json(options || {}))\n  end\n"),
    ("as_json", "  def as_json(options = {})\n    options ||= {}\n    root = options.fetch(:root, @options.fetch(:root, root_name))\n    if root\n      hash = {}\n      @ams_side = hash\n      @ams_unique = {}\n      body = serializable_hash\n      hash[root] = body\n      include_meta(hash)\n      hash\n    else\n      serializable_hash\n    end\n  end\n"),
];

/// The rest of the gem's instance API, on the class under `ActiveModel::Serializer`, by name. The Hash the root
/// and the sideloaded records share is held apart from `options`, where the gem keeps it, so the Hash the app
/// passes keeps the type it was built with.
const ROOT_API: &[(&str, &str)] = &[
    ("meta_key", "  def meta_key\n    key = @options[:meta_key]\n    key.respond_to?(:to_sym) ? key.to_sym : :meta\n  end\n"),
    ("include_meta", "  def include_meta(hash)\n    hash[meta_key] = @options[:meta] if @options.has_key?(:meta)\n  end\n"),
    ("ams_include_option", "  def ams_include_option(name, default)\n    if @options.key?(:include)\n      @options[:include].include?(name)\n    elsif @options.include?(:exclude)\n      !@options[:exclude].include?(name)\n    else\n      default\n    end\n  end\n"),
    ("ams_merge", "  def ams_merge(side, key, serializables)\n    unique = (@ams_unique ||= {})\n    seen = (unique[key] ||= {})\n    list = (side[key] ||= [])\n    serializables.each do |serializable|\n      unless seen.include?(serializable.object)\n        seen[serializable.object] = true\n        list << serializable.serializable_hash\n      end\n    end\n  end\n"),
];

fn install(class: &mut LibraryClass, src: &str, span: Span, root: bool, inherits_initialize: bool) -> IngestResult<Vec<LibraryClass>> {
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
    // The classes the source nests in the serializer (`AmsArray`), the program's beside it.
    for nested in &mut parsed {
        for method in &mut nested.methods {
            mark(&mut method.body, span);
            method.name_span = span;
        }
    }
    // An own `initialize`'s `super` is the gem's constructor, which `ams_initialize` is here: it writes the class's
    // own ivars, so each class types its record apart.
    for method in class.methods.iter_mut().filter(|m| !inherits_initialize && m.name.as_str() == "initialize" && m.receiver == crate::dialect::MethodReceiver::Instance) {
        let params: Vec<Symbol> = method.params.iter().map(|p| p.name.clone()).collect();
        fn bare_super(e: &Expr) -> bool {
            let mut found = matches!(&*e.node, ExprNode::Super { args: None });
            e.node.for_each_child(&mut |c| found |= bare_super(c));
            found
        }
        // A bare `super` hands on every parameter as it was declared: a keyword or splat is not one positional argument.
        if method.params.iter().any(|p| p.keyword || p.rest || p.forwarding) && bare_super(&method.body) {
            return Err(IngestError::Unsupported {
                file: "<active_model_serializers>".into(),
                message: format!("ActiveModel::Serializer subset: {}'s `initialize` hands a keyword or splat on through `super`", class.name.0),
            });
        }
        fn rewrite(e: &mut Expr, params: &[Symbol]) {
            e.node.for_each_child_mut(&mut |c| rewrite(c, params));
            if let ExprNode::Super { args } = &*e.node {
                let args = args.clone().unwrap_or_else(|| {
                    params
                        .iter()
                        .map(|p| Expr::new(Span::synthetic(), ExprNode::Var { id: crate::ident::VarId(0), name: p.clone() }))
                        .collect()
                });
                *e.node = ExprNode::Send { recv: None, method: Symbol::from("ams_initialize"), args, block: None, parenthesized: true };
            }
        }
        rewrite(&mut method.body, &params);
    }
    if root {
        class.parent = None;
    }
    class.unknown_calls.retain(super::delegate::is_delegate_declaration);
    class.origin = Some(LibraryClassOrigin::ActiveModelSerializer08);
    for nested in &mut parsed {
        nested.origin = Some(LibraryClassOrigin::ActiveModelSerializer08);
    }
    Ok(parsed)
}

/// `opts[:each_serializer] = S`, S a constant: the local and S's name.
fn each_serializer_write(e: &Expr) -> Option<(String, String)> {
    let (recv, index, value) = match &*e.node {
        ExprNode::Assign { target: crate::expr::LValue::Index { recv, index }, value } => (recv, index, value),
        ExprNode::Send { recv: Some(recv), method, args, .. } if method.as_str() == "[]=" && args.len() == 2 => (recv, &args[0], &args[1]),
        _ => return None,
    };
    let ExprNode::Var { name, .. } = &*recv.node else { return None };
    let ExprNode::Lit { value: Literal::Sym { value: key } } = &*index.node else { return None };
    let ExprNode::Const { path } = &*value.node else { return None };
    (key.as_str() == "each_serializer")
        .then(|| (name.as_str().to_string(), path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::")))
}

/// Whether `e` writes an `:each_serializer` key through any receiver.
fn writes_each_serializer(e: &Expr) -> bool {
    let key = |k: &Expr| matches!(&*k.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "each_serializer");
    let here = match &*e.node {
        ExprNode::Assign { target: crate::expr::LValue::Index { index, .. }, .. }
        | ExprNode::OpAssign { target: crate::expr::LValue::Index { index, .. }, .. } => key(index),
        ExprNode::Send { method, args, .. } => matches!(method.as_str(), "[]=" | "store") && args.first().is_some_and(key),
        _ => false,
    };
    let mut found = here;
    e.node.for_each_child(&mut |c| found |= writes_each_serializer(c));
    found
}

/// The locals `e` hands on (an assignment, an argument, an element) rather than only calling on, and the locals it
/// assigns one of them: either name may then write the Hash.
fn escaped_locals(e: &Expr, out: &mut HashSet<String>) {
    fn walk(e: &Expr, as_receiver: bool, out: &mut HashSet<String>) {
        match &*e.node {
            ExprNode::Var { name, .. } => {
                if !as_receiver {
                    out.insert(name.as_str().to_string());
                }
            }
            ExprNode::Send { recv: Some(r), args, block, .. } => {
                walk(r, true, out);
                args.iter().for_each(|a| walk(a, false, out));
                if let Some(b) = block {
                    walk(b, false, out);
                }
            }
            ExprNode::Assign { target: crate::expr::LValue::Index { recv, index }, value }
            | ExprNode::OpAssign { target: crate::expr::LValue::Index { recv, index }, value, .. } => {
                walk(recv, true, out);
                walk(index, false, out);
                walk(value, false, out);
            }
            ExprNode::Assign { target: crate::expr::LValue::Var { name, .. }, value } => {
                walk(value, false, out);
                let mut held = HashSet::default();
                value_locals(value, &mut held);
                if !held.is_empty() {
                    out.insert(name.as_str().to_string());
                }
            }
            _ => e.node.for_each_child(&mut |c| walk(c, false, out)),
        }
    }
    walk(e, false, out);
}

/// The locals whose Hash `e` may evaluate to: itself, a branch of a condition, or the receiver of a call answering
/// its receiver (`opts.merge!(…)`), not a copy (`{ a: 1 }.merge!(opts)`).
fn value_locals(e: &Expr, out: &mut HashSet<String>) {
    match &*e.node {
        ExprNode::Var { name, .. } => {
            out.insert(name.as_str().to_string());
        }
        ExprNode::BoolOp { .. } | ExprNode::If { .. } | ExprNode::Seq { .. } => e.node.for_each_child(&mut |c| value_locals(c, out)),
        ExprNode::Send { recv: Some(r), method, .. }
            if matches!(method.as_str(), "merge!" | "update" | "tap" | "itself" | "reverse_merge!" | "with_indifferent_access" | "compact!" | "reject!" | "select!" | "delete_if" | "keep_if" | "except!" | "slice!" | "freeze" | "then" | "replace") =>
        {
            value_locals(r, out)
        }
        _ => {}
    }
}

fn local_reads(e: &Expr, out: &mut HashSet<String>) {
    match &*e.node {
        ExprNode::Var { name, .. } => {
            out.insert(name.as_str().to_string());
        }
        // `opts ||= {}` reads `opts`, which its target, not a child, holds.
        ExprNode::OpAssign { target: crate::expr::LValue::Var { name, .. }, .. } => {
            out.insert(name.as_str().to_string());
        }
        _ => {}
    }
    e.node.for_each_child(&mut |c| local_reads(c, out));
}

/// Whether `e` rebinds `local` or writes into it other than a constant `each_serializer:`.
fn writes_local(e: &Expr, local: &str) -> bool {
    let here = match &*e.node {
        ExprNode::Assign { target: crate::expr::LValue::Var { name, .. }, .. }
        | ExprNode::OpAssign { target: crate::expr::LValue::Var { name, .. }, .. } => name.as_str() == local,
        ExprNode::Assign { target: crate::expr::LValue::Index { recv, .. }, .. }
        | ExprNode::OpAssign { target: crate::expr::LValue::Index { recv, .. }, .. } => {
            matches!(&*recv.node, ExprNode::Var { name, .. } if name.as_str() == local)
        }
        ExprNode::Send { recv: Some(r), method, .. } => {
            matches!(&*r.node, ExprNode::Var { name, .. } if name.as_str() == local)
                && matches!(method.as_str(), "merge!" | "update" | "delete" | "store" | "clear" | "replace" | "[]=" | "freeze")
        }
        _ => false,
    };
    let mut found = here;
    e.node.for_each_child(&mut |c| found |= writes_local(c, local));
    found
}

/// `ActiveModel::ArraySerializer.new(xs, {each_serializer: S, …}).as_json`
/// → `S.ams_array_as_json(xs, {each_serializer: S, …})`, S a lowered
/// serializer written as a constant; the options, `each_serializer:`
/// among them, reach each item's serializer as the gem passes them.
fn rewrite_arrays(expr: &mut Expr, done: &HashSet<String>, lazy_ok: &HashSet<String>) {
    rewrite_arrays_in(expr, done, lazy_ok, &HashMap::default(), &HashSet::default(), &HashSet::default());
}

/// `locals`: the options locals an earlier statement gave a constant `each_serializer:` (`opts[:each_serializer] = S`),
/// which no statement since has written over. `read_after`: the locals a later statement reads.
fn rewrite_arrays_in(
    expr: &mut Expr,
    done: &HashSet<String>,
    lazy_ok: &HashSet<String>,
    locals: &HashMap<String, String>,
    read_after: &HashSet<String>,
    outer_escaped: &HashSet<String>,
) {
    // What a statement or an earlier child leaves known: not a local it writes, hands on (another name may then
    // write the Hash), or freezes (the gem's `as_json` then raises FrozenError).
    let survive = |e: &Expr, known: &mut HashMap<String, String>, escaped: &mut HashSet<String>| {
        if let Some((local, class)) = each_serializer_write(e) {
            known.retain(|l, _| *l == local);
            known.insert(local, class);
        } else {
            if writes_each_serializer(e) {
                known.clear();
            }
            known.retain(|local, _| !writes_local(e, local));
        }
        escaped_locals(e, escaped);
        known.retain(|local, _| !escaped.contains(local));
    };
    if let ExprNode::Seq { exprs } = &mut *expr.node {
        let mut known = locals.clone();
        // Locals whose Hash another name may hold, here or in an enclosing statement: never kept as known.
        let mut escaped: HashSet<String> = outer_escaped.clone();
        for i in 0..exprs.len() {
            let mut after = read_after.clone();
            for later in &exprs[i + 1..] {
                local_reads(later, &mut after);
            }
            rewrite_arrays_in(&mut exprs[i], done, lazy_ok, &known, &after, &escaped);
            survive(&exprs[i], &mut known, &mut escaped);
        }
        return;
    }
    let mut base = read_after.clone();
    // Not a loop body's: a read before the call in the source may run after it.
    if matches!(&*expr.node, ExprNode::While { .. }) || matches!(&*expr.node, ExprNode::Lambda { .. }) {
        local_reads(expr, &mut base);
    }
    // Each child sees what its later siblings read: `{ body: …as_json, shared: opts[:hash] }`.
    let mut reads: Vec<HashSet<String>> = Vec::new();
    expr.node.for_each_child(&mut |c| {
        let mut r = HashSet::default();
        local_reads(c, &mut r);
        reads.push(r);
    });
    // A condition runs before either branch; the branches are alternatives, neither after the other.
    let branches = matches!(&*expr.node, ExprNode::If { .. } | ExprNode::Case { .. });
    let mut known = locals.clone();
    let mut escaped = outer_escaped.clone();
    let mut at = 0usize;
    expr.node.for_each_child_mut(&mut |c| {
        let mut after = base.clone();
        if !branches || at == 0 {
            for r in reads.iter().skip(at + 1) {
                after.extend(r.iter().cloned());
            }
        }
        rewrite_arrays_in(c, done, lazy_ok, &known, &after, &escaped);
        // Not past a branch: only the condition runs before the others.
        if !branches || at == 0 {
            survive(c, &mut known, &mut escaped);
        }
        at += 1;
    });
    // `S::AmsArray.new(xs, opts).as_json`, the constructor rewritten below: the value now, as the gem's is.
    if let ExprNode::Send { recv: Some(array), method, args, block: None, .. } = &*expr.node
        && method.as_str() == "as_json"
        && args.is_empty()
        && let ExprNode::Send { recv: Some(class), method: new, args: built, block: None, .. } = &*array.node
        && new.as_str() == "new"
        && let ExprNode::Const { path } = &*class.node
        && path.last().is_some_and(|l| l.as_str() == "AmsArray")
        && class.span.is_synthetic()
        && let [objects, options] = built.as_slice()
    {
        let serializer: Vec<Symbol> = path[..path.len() - 1].to_vec();
        let (objects, options) = (objects.clone(), options.clone());
        *expr = Expr::new(
            expr.span,
            ExprNode::Send {
                recv: Some(Expr::new(Span::synthetic(), ExprNode::Const { path: serializer })),
                method: Symbol::from("ams_array_as_json"),
                args: vec![objects, options],
                block: None,
                parenthesized: true,
            },
        );
        return;
    }
    // The constructor alone: an object whose `as_json` / `to_json` answer as the gem's do, when they are asked.
    let (span, objects, options, lazy) = match &*expr.node {
        ExprNode::Send { recv: Some(array), method, args, block: None, .. } if method.as_str() == "as_json" && args.is_empty() => {
            let ExprNode::Send { recv: Some(class), method, args, block: None, .. } = &*array.node else { return };
            let ExprNode::Const { path } = &*class.node else { return };
            let written: Vec<&str> = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
            if method.as_str() != "new" || written != ARRAY_SERIALIZER {
                return;
            }
            let [objects, options] = args.as_slice() else { return };
            (expr.span, objects, options, false)
        }
        ExprNode::Send { recv: Some(class), method, args, block: None, .. } if method.as_str() == "new" => {
            let ExprNode::Const { path } = &*class.node else { return };
            let written: Vec<&str> = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
            if written != ARRAY_SERIALIZER {
                return;
            }
            let [objects, options] = args.as_slice() else { return };
            // Not an options local: the gem reads `:each_serializer` when `as_json` runs, which a write to the Hash
            // after this (a block's, a callee's) changes; a literal is a Hash nothing else holds.
            if !matches!(&*options.node, ExprNode::Hash { .. }) {
                return;
            }
            (expr.span, objects, options, true)
        }
        _ => return,
    };
    let mut serializer = None;
    let entries = match &*options.node {
        ExprNode::Hash { entries, .. } => entries.as_slice(),
        // Not one read again: the gem's `as_json` writes `:hash` and `:unique_values` into it, the expansion does not.
        // Nor one the objects argument writes or hands on, which runs before the constructor reads it.
        ExprNode::Var { name, .. }
            if !read_after.contains(name.as_str())
                && !writes_local(objects, name.as_str())
                && !{
                    let mut escaped = HashSet::default();
                    escaped_locals(objects, &mut escaped);
                    escaped.contains(name.as_str())
                } =>
        {
            serializer = locals.get(name.as_str()).cloned();
            &[]
        }
        _ => return,
    };
    for (k, v) in entries {
        let ExprNode::Lit { value: Literal::Sym { value } } = &*k.node else { return };
        if value.as_str() == "each_serializer" {
            let ExprNode::Const { path } = &*v.node else { return };
            serializer = Some(path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::"));
        }
    }
    let Some(serializer) = serializer.filter(|s| done.contains(s)) else { return };
    if lazy && !lazy_ok.contains(&serializer) {
        return;
    }
    let path: Vec<Symbol> = std::iter::once(Symbol::from("")).chain(serializer.split("::").map(Symbol::from)).collect();
    let mut options = options.clone();
    if let ExprNode::Hash { kwargs, .. } = &mut *options.node {
        *kwargs = false;
    }
    let objects = objects.clone();
    *expr = if lazy {
        let class = Expr::new(Span::synthetic(), ExprNode::Const { path: path.into_iter().chain(std::iter::once(Symbol::from("AmsArray"))).collect() });
        Expr::new(span, ExprNode::Send { recv: Some(class), method: Symbol::from("new"), args: vec![objects, options], block: None, parenthesized: true })
    } else {
        let recv = Expr::new(Span::synthetic(), ExprNode::Const { path });
        Expr::new(span, ExprNode::Send { recv: Some(recv), method: Symbol::from("ams_array_as_json"), args: vec![objects, options], block: None, parenthesized: true })
    };
}
