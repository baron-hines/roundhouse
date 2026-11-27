//! A module factory included with literal arguments, partially evaluated.
//!
//! ```ruby
//! module FullTextSearch
//!   def self.[](*cols)
//!     Module.new do
//!       extend ActiveSupport::Concern
//!       included do
//!         after_create { |r| … cols.join(", ") … }
//!       end
//!     end
//!   end
//! end
//!
//! class Comment < ApplicationRecord
//!   include FullTextSearch[:comment]
//! end
//! ```
//!
//! Each distinct argument list becomes a named module whose body is the
//! factory's block with every read of a factory parameter replaced by the
//! literal it was called with (`[:comment]` for a splat), and the include
//! names that module. The program is the one Rails runs, except that the
//! module has a name where `Module.new` makes an anonymous one; Spinel
//! cannot build a `Module.new` whose block captures the factory's
//! parameters, and the analysis cannot type one either.
//!
//! The rewrite is a [`Vfs`] layer, so every reader of the tree sees the
//! same files: the specialized modules beside the factory, the including
//! files with their includes renamed, and the factory itself hidden once
//! no other reference to it remains.

use crate::det::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ruby_prism::Node;

use crate::ingest::prism_api::PrismLocation;

use crate::vfs::Vfs;

fn name_of(id: &ruby_prism::ConstantId<'_>) -> String {
    String::from_utf8_lossy(id.as_slice()).into_owned()
}

/// `module M; def self.[](params) Module.new do BODY end end; end`, the only thing in M.
struct Factory {
    file: PathBuf,
    /// Required parameter names, then the splat's, in order.
    required: Vec<String>,
    rest: Option<String>,
    /// The block body's source.
    body: String,
    /// Offsets (relative to `body`) of each read of a parameter, with its name.
    reads: Vec<(usize, usize, String)>,
}

/// `include M[lit, …]` in some file: the `M[…]` call's span and its arguments' sources.
struct Site {
    file: PathBuf,
    factory: String,
    start: usize,
    end: usize,
    args: Vec<String>,
}

pub(super) struct FactoryVfs<'a, V: Vfs + ?Sized> {
    inner: &'a V,
    files: HashMap<PathBuf, Vec<u8>>,
    hidden: HashSet<PathBuf>,
}

pub(super) fn expand<'a, V: Vfs + ?Sized>(inner: &'a V, dir: &Path) -> FactoryVfs<'a, V> {
    let mut out = FactoryVfs { inner, files: HashMap::default(), hidden: HashSet::default() };
    let mut sources: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    collect_rb(inner, &dir.join("app"), &mut sources);
    let mut factories: HashMap<String, Factory> = HashMap::default();
    for (file, bytes) in &sources {
        let parsed = ruby_prism::parse(bytes);
        if let Some((name, factory)) = find_factory(&parsed.node(), bytes, file) {
            factories.insert(name, factory);
        }
    }
    if factories.is_empty() {
        return out;
    }
    let mut sites: Vec<Site> = Vec::new();
    let mut other_refs: HashSet<String> = HashSet::default();
    for (file, bytes) in &sources {
        let parsed = ruby_prism::parse(bytes);
        let mut finder = SiteFinder { factories: &factories, file, bytes, sites: Vec::new(), refs: Vec::new(), in_site: 0 };
        ruby_prism::Visit::visit(&mut finder, &parsed.node());
        sites.extend(finder.sites);
        // A factory's own file names it in `module M`, which is not a use.
        let own = factories.iter().find(|(_, f)| &f.file == file).map(|(n, _)| n.clone());
        other_refs.extend(finder.refs.into_iter().filter(|r| Some(r) != own.as_ref()));
    }
    if sites.is_empty() {
        return out;
    }
    let mut generated: HashMap<(String, Vec<String>), String> = HashMap::default();
    // Not the camelized arguments alone: `[:a_b]` and `[:a, :b]` camelize alike, and `M[]` is `M`.
    let mut used: HashSet<String> = factories.keys().cloned().collect();
    let mut by_file: HashMap<PathBuf, Vec<(usize, usize, String)>> = HashMap::default();
    for site in &sites {
        let factory = &factories[&site.factory];
        let key = (site.factory.clone(), site.args.clone());
        let module = match generated.get(&key) {
            Some(name) => name.clone(),
            None => {
                let base = specialized_name(&site.factory, &site.args);
                let mut name = base.clone();
                let mut n = 2;
                while used.contains(&name) {
                    name = format!("{base}{n}");
                    n += 1;
                }
                used.insert(name.clone());
                let Some(source) = specialize(&name, factory, &site.args) else { continue };
                let path = factory.file.with_file_name(format!("{}.rb", crate::naming::underscore(&name)));
                out.files.insert(path, source.into_bytes());
                generated.insert(key, name.clone());
                name
            }
        };
        by_file.entry(site.file.clone()).or_default().push((site.start, site.end, module));
    }
    for (file, mut edits) in by_file {
        let Some((_, bytes)) = sources.iter().find(|(p, _)| *p == file) else { continue };
        edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
        let mut text = bytes.clone();
        for (start, end, module) in edits {
            text.splice(start..end, module.into_bytes());
        }
        out.files.insert(file, text);
    }
    let expanded: HashSet<&String> = generated.keys().map(|(f, _)| f).collect();
    for (name, factory) in &factories {
        if expanded.contains(name) && !other_refs.contains(name) {
            out.hidden.insert(factory.file.clone());
        }
    }
    out
}

fn collect_rb<V: Vfs + ?Sized>(vfs: &V, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
    let Ok(children) = vfs.read_dir(dir) else { return };
    for child in children {
        if vfs.is_dir(&child) {
            collect_rb(vfs, &child, out);
        } else if child.extension().is_some_and(|e| e == "rb") {
            if let Ok(bytes) = vfs.read(&child) {
                out.push((child, bytes));
            }
        }
    }
}

fn find_factory(program: &Node<'_>, bytes: &[u8], file: &Path) -> Option<(String, Factory)> {
    let stmts: Vec<Node<'_>> = program.as_program_node()?.statements().body().iter().collect();
    let module = stmts.iter().find_map(|s| s.as_module_node())?;
    let name = module.constant_path().as_constant_read_node().map(|c| name_of(&c.name()))?;
    let body: Vec<Node<'_>> = module.body()?.as_statements_node()?.body().iter().collect();
    let [only] = body.as_slice() else { return None };
    let def = only.as_def_node()?;
    if name_of(&def.name()) != "[]" || def.receiver().is_none_or(|r| r.as_self_node().is_none()) {
        return None;
    }
    let mut required = Vec::new();
    let mut rest = None;
    if let Some(params) = def.parameters() {
        if params.optionals().iter().next().is_some()
            || params.posts().iter().next().is_some()
            || params.keywords().iter().next().is_some()
            || params.keyword_rest().is_some()
            || params.block().is_some()
        {
            return None;
        }
        for p in params.requireds().iter() {
            required.push(name_of(&p.as_required_parameter_node()?.name()));
        }
        if let Some(r) = params.rest() {
            rest = Some(name_of(&r.as_rest_parameter_node()?.name()?));
        }
    }
    let def_body: Vec<Node<'_>> = def.body()?.as_statements_node()?.body().iter().collect();
    let [call] = def_body.as_slice() else { return None };
    let call = call.as_call_node()?;
    let receiver = call.receiver()?.as_constant_read_node()?;
    if name_of(&receiver.name()) != "Module" || name_of(&call.name()) != "new" || call.arguments().is_some() {
        return None;
    }
    let block = call.block()?.as_block_node()?;
    if block.parameters().is_some() {
        return None;
    }
    let block_body = block.body()?;
    let (start, end) = (block_body.location().start_offset(), block_body.location().end_offset());
    let params: HashSet<String> = required.iter().cloned().chain(rest.clone()).collect();
    let mut reads = Reads { params: &params, base: start, reads: Vec::new(), rebound: false };
    ruby_prism::Visit::visit(&mut reads, &block_body);
    if reads.rebound {
        return None;
    }
    Some((
        name,
        Factory {
            file: file.to_path_buf(),
            required,
            rest,
            body: String::from_utf8_lossy(&bytes[start..end]).into_owned(),
            reads: reads.reads,
        },
    ))
}

struct Reads<'a> {
    params: &'a HashSet<String>,
    base: usize,
    reads: Vec<(usize, usize, String)>,
    /// A block parameter or a write names a factory parameter again; the substitution would read the wrong value.
    rebound: bool,
}

impl<'pr> ruby_prism::Visit<'pr> for Reads<'_> {
    fn visit_local_variable_read_node(&mut self, node: &ruby_prism::LocalVariableReadNode<'pr>) {
        let name = name_of(&node.name());
        if self.params.contains(&name) {
            let loc = node.location();
            self.reads.push((loc.start_offset() - self.base, loc.end_offset() - self.base, name));
        }
    }
    fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'pr>) {
        self.rebound |= self.params.contains(&name_of(&node.name()));
        ruby_prism::visit_local_variable_write_node(self, node);
    }
    fn visit_local_variable_target_node(&mut self, node: &ruby_prism::LocalVariableTargetNode<'pr>) {
        self.rebound |= self.params.contains(&name_of(&node.name()));
    }
    fn visit_required_parameter_node(&mut self, node: &ruby_prism::RequiredParameterNode<'pr>) {
        self.rebound |= self.params.contains(&name_of(&node.name()));
    }
    fn visit_optional_parameter_node(&mut self, node: &ruby_prism::OptionalParameterNode<'pr>) {
        self.rebound |= self.params.contains(&name_of(&node.name()));
        ruby_prism::visit_optional_parameter_node(self, node);
    }
    fn visit_block_local_variable_node(&mut self, node: &ruby_prism::BlockLocalVariableNode<'pr>) {
        self.rebound |= self.params.contains(&name_of(&node.name()));
    }
    fn visit_local_variable_operator_write_node(&mut self, node: &ruby_prism::LocalVariableOperatorWriteNode<'pr>) {
        self.rebound |= self.params.contains(&name_of(&node.name()));
        ruby_prism::visit_local_variable_operator_write_node(self, node);
    }
    fn visit_local_variable_or_write_node(&mut self, node: &ruby_prism::LocalVariableOrWriteNode<'pr>) {
        self.rebound |= self.params.contains(&name_of(&node.name()));
        ruby_prism::visit_local_variable_or_write_node(self, node);
    }
    fn visit_local_variable_and_write_node(&mut self, node: &ruby_prism::LocalVariableAndWriteNode<'pr>) {
        self.rebound |= self.params.contains(&name_of(&node.name()));
        ruby_prism::visit_local_variable_and_write_node(self, node);
    }
    fn visit_rest_parameter_node(&mut self, node: &ruby_prism::RestParameterNode<'pr>) {
        self.rebound |= node.name().is_some_and(|n| self.params.contains(&name_of(&n)));
    }
    fn visit_required_keyword_parameter_node(&mut self, node: &ruby_prism::RequiredKeywordParameterNode<'pr>) {
        self.rebound |= self.params.contains(&name_of(&node.name()));
    }
    fn visit_optional_keyword_parameter_node(&mut self, node: &ruby_prism::OptionalKeywordParameterNode<'pr>) {
        self.rebound |= self.params.contains(&name_of(&node.name()));
        ruby_prism::visit_optional_keyword_parameter_node(self, node);
    }
    fn visit_keyword_rest_parameter_node(&mut self, node: &ruby_prism::KeywordRestParameterNode<'pr>) {
        self.rebound |= node.name().is_some_and(|n| self.params.contains(&name_of(&n)));
    }
    fn visit_block_parameter_node(&mut self, node: &ruby_prism::BlockParameterNode<'pr>) {
        self.rebound |= node.name().is_some_and(|n| self.params.contains(&name_of(&n)));
    }
}

struct SiteFinder<'a> {
    factories: &'a HashMap<String, Factory>,
    file: &'a Path,
    bytes: &'a [u8],
    sites: Vec<Site>,
    refs: Vec<String>,
    in_site: usize,
}

impl<'pr> ruby_prism::Visit<'pr> for SiteFinder<'_> {
    fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
        if node.receiver().is_none() && name_of(&node.name()) == "include" {
            if let Some(args) = node.arguments() {
                let args: Vec<Node<'_>> = args.arguments().iter().collect();
                if let [only] = args.as_slice()
                    && let Some(site) = self.site(only)
                {
                    self.sites.push(site);
                    return;
                }
            }
        }
        ruby_prism::visit_call_node(self, node);
    }
    fn visit_constant_read_node(&mut self, node: &ruby_prism::ConstantReadNode<'pr>) {
        let name = name_of(&node.name());
        if self.factories.contains_key(&name) {
            self.refs.push(name);
        }
    }
}

impl SiteFinder<'_> {
    fn site(&self, arg: &Node<'_>) -> Option<Site> {
        let call = arg.as_call_node()?;
        if name_of(&call.name()) != "[]" || call.block().is_some() {
            return None;
        }
        let factory = name_of(&call.receiver()?.as_constant_read_node()?.name());
        let shape = self.factories.get(&factory)?;
        let args: Vec<Node<'_>> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
        if args.len() < shape.required.len() || (shape.rest.is_none() && args.len() != shape.required.len()) {
            return None;
        }
        let literal = |n: &Node<'_>| {
            n.as_symbol_node().is_some() || n.as_string_node().is_some() || n.as_integer_node().is_some()
        };
        if !args.iter().all(literal) {
            return None;
        }
        let loc = arg.location();
        Some(Site {
            file: self.file.to_path_buf(),
            factory,
            start: loc.start_offset(),
            end: loc.end_offset(),
            args: args
                .iter()
                .map(|a| String::from_utf8_lossy(&self.bytes[a.location().start_offset()..a.location().end_offset()]).into_owned())
                .collect(),
        })
    }
}

/// `FullTextSearch` + `[:title, :body]` → `FullTextSearchTitleBody`.
fn specialized_name(factory: &str, args: &[String]) -> String {
    let words: String = args
        .iter()
        .map(|a| {
            let bare: String = a.trim_start_matches(':').chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            crate::naming::camelize(&bare)
        })
        .collect();
    format!("{factory}{words}")
}

fn specialize(name: &str, factory: &Factory, args: &[String]) -> Option<String> {
    let mut value: HashMap<&str, String> = HashMap::default();
    for (param, arg) in factory.required.iter().zip(args) {
        value.insert(param, arg.clone());
    }
    if let Some(rest) = &factory.rest {
        value.insert(rest, format!("[{}]", args[factory.required.len()..].join(", ")));
    }
    let mut body = factory.body.clone();
    let mut reads = factory.reads.clone();
    reads.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    for (start, end, param) in reads {
        body.replace_range(start..end, value.get(param.as_str())?);
    }
    Some(format!("module {name}\n{body}\nend\n"))
}

impl<V: Vfs + ?Sized> Vfs for FactoryVfs<'_, V> {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        if self.hidden.contains(path) {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "module factory expanded at its include sites"));
        }
        match self.files.get(path) {
            Some(bytes) => Ok(bytes.clone()),
            None => self.inner.read(path),
        }
    }

    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        // Not a lossy decode of `read`: a failed decode is how ingest tells a binary asset apart.
        if self.hidden.contains(path) || self.files.contains_key(path) {
            return self.read(path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        }
        self.inner.read_to_string(path)
    }

    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
        let mut out: Vec<PathBuf> = self.inner.read_dir(path)?.into_iter().filter(|p| !self.hidden.contains(p)).collect();
        for added in self.files.keys() {
            if added.parent() == Some(path) && !out.contains(added) {
                out.push(added.clone());
            }
        }
        Ok(out)
    }

    fn exists(&self, path: &Path) -> bool {
        !self.hidden.contains(path) && (self.files.contains_key(path) || self.inner.exists(path))
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.inner.is_dir(path)
    }

    fn is_symlink(&self, path: &Path) -> bool {
        self.inner.is_symlink(path)
    }

    fn canonical(&self, path: &Path) -> Option<PathBuf> {
        self.inner.canonical(path)
    }
}
