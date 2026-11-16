//! What a gem's code does to an app class's shape as the app boots, observed for the Spinel tree.
//!
//! Spinel lays out every class when it compiles, so it refuses a gem that defines methods on, or
//! mixes modules into, a class it holds in a variable (aasm's `klass.send(:define_method, …)` and
//! `base.send(:include, …)`). Such a gem's doings depend on what runs as the app boots, which the
//! static passes cannot always follow. When they cannot, the Spinel build boots the app's CRuby tree
//! once with `runtime/observe/shape_observer.rb` loaded, which records each definition, mixin and
//! class macro a gem's statement made on an app class, and the app's own class-body statement that
//! set it off. They are written where that statement is, in the app's source, and the gem's
//! statement that made them reads `nil`: the Spinel tree has the same class from the start.
//!
//! Run only for the Spinel target, only when a reached gem statement still does such a thing on a
//! variable receiver, and cached by what it read. A boot that fails, or a doing the observer could
//! not write as source, leaves the app as it was, where the refusals still stand.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::App;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ingest::prism_api::PrismLocation;
use crate::vfs::Vfs;

pub const OBSERVER: &str = include_str!("../runtime/observe/shape_observer.rb");
pub const CACHE: &str = ".roundhouse/observed_shapes.json";

thread_local! {
    static INSTALLED: std::cell::RefCell<Option<std::rc::Rc<Patches>>> = const { std::cell::RefCell::new(None) };
}

/// The patches `ingest_app` writes into what it reads, until [`uninstall`].
pub fn install(patches: Patches) {
    INSTALLED.with(|c| *c.borrow_mut() = Some(std::rc::Rc::new(patches)));
}

pub fn uninstall() {
    INSTALLED.with(|c| *c.borrow_mut() = None);
}

pub fn installed() -> Option<std::rc::Rc<Patches>> {
    INSTALLED.with(|c| c.borrow().clone())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effect {
    pub owner: Option<String>,
    /// The app's own class-body statement that set it off: its file in the emitted tree, and its first line as written there.
    pub trigger: Option<Trigger>,
    /// The source to write after it; none for a gem statement that only hands a lambda to one that does.
    pub statement: Option<String>,
    pub gem_file: String,
    pub range: [usize; 2],
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Trigger {
    pub file: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Observation {
    pub effects: Vec<Effect>,
    pub refusals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cached {
    pub key: String,
    pub observation: Observation,
    /// For a boot that failed: the Ruby and gem paths it failed with, which a later build must share to reuse it.
    #[serde(default)]
    pub env: Option<String>,
}

/// The Ruby and gem paths a boot runs with, read in the app's directory: a failed boot is reused only under the same.
pub fn environment(dir: &Path) -> String {
    let out = std::process::Command::new("ruby")
        .args(["-e", "print RUBY_DESCRIPTION, \"\\n\", Gem.path.join(\":\")"])
        .current_dir(dir)
        .output();
    match out {
        Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned() + &std::env::var("GEM_PATH").unwrap_or_default() + &std::env::var("GEM_HOME").unwrap_or_default(),
        Err(_) => "no ruby".into(),
    }
}

/// The source edits an observation makes: statements written after an app file's offset, and gem statements that read `nil`.
#[derive(Debug, Default)]
pub struct Patches {
    inserts: BTreeMap<PathBuf, Vec<(usize, String)>>,
    /// `<gem dir>/<path in the gem>`, then each range with the text it must hold.
    gem_edits: BTreeMap<String, Vec<(usize, usize, String)>>,
}

impl Patches {
    pub fn is_empty(&self) -> bool {
        self.inserts.is_empty() && self.gem_edits.is_empty()
    }
}

/// The patches an observation of the app at `dir` makes, or why it cannot be written as source.
pub fn patches(observation: &Observation, dir: &Path) -> Result<Patches, String> {
    if let Some(why) = observation.refusals.first() {
        return Err(why.clone());
    }
    let mut out = Patches::default();
    let mut by_trigger: BTreeMap<Trigger, Vec<String>> = BTreeMap::new();
    for effect in &observation.effects {
        let edits = out.gem_edits.entry(effect.gem_file.clone()).or_default();
        if !edits
            .iter()
            .any(|(s, e, _)| *s == effect.range[0] && *e == effect.range[1])
        {
            edits.push((effect.range[0], effect.range[1], effect.text.clone()));
        }
        let (Some(trigger), Some(statement)) = (&effect.trigger, &effect.statement) else {
            continue;
        };
        // Not only the first of each: a later `const_set` of the same name wins, as it did as the app booted.
        by_trigger.entry(trigger.clone()).or_default().push(statement.clone());
    }
    for edits in out.gem_edits.values_mut() {
        edits.sort_by_key(|(s, e, _)| (*s, std::cmp::Reverse(*e)));
        // Not a statement inside one already read as `nil`: it goes with it.
        let mut kept: Vec<(usize, usize, String)> = Vec::new();
        for edit in edits.drain(..) {
            if kept.last().is_some_and(|(_, end, _)| edit.0 < *end) {
                if edit.1 > kept.last().map(|k| k.1).unwrap_or(0) {
                    return Err(format!(
                        "two observed gem statements overlap at byte {}",
                        edit.0
                    ));
                }
                continue;
            }
            kept.push(edit);
        }
        *edits = kept;
    }
    for (trigger, statements) in by_trigger {
        let path = dir.join(&trigger.file);
        let source = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (end, indent) = statement_end(&source, &trigger.text).ok_or_else(|| {
            format!(
                "{}: no one class-body statement reads `{}`",
                trigger.file, trigger.text
            )
        })?;
        let text: String = statements
            .iter()
            .map(|s| {
                s.lines()
                    .map(|l| {
                        if l.is_empty() {
                            "\n".to_string()
                        } else {
                            format!("{indent}{l}\n")
                        }
                    })
                    .collect::<String>()
            })
            .collect();
        out.inserts
            .entry(path.canonicalize().unwrap_or(path))
            .or_default()
            .push((end, format!("\n{text}")));
    }
    Ok(out)
}

/// The end of the one class-body statement whose first line reads `text`, and the indentation it is written at.
fn statement_end(source: &[u8], text: &str) -> Option<(usize, String)> {
    struct Finder<'a> {
        source: &'a [u8],
        text: &'a str,
        depth: usize,
        found: Vec<(usize, usize)>,
    }
    impl<'pr> ruby_prism::Visit<'pr> for Finder<'_> {
        fn visit_class_node(&mut self, node: &ruby_prism::ClassNode<'pr>) {
            self.depth += 1;
            if let Some(body) = node.body().and_then(|b| b.as_statements_node()) {
                for stmt in body.body().iter() {
                    let loc = stmt.location();
                    let (s, e) = (loc.start_offset(), loc.end_offset());
                    let first = self.source[s..e]
                        .split(|b| *b == b'\n')
                        .next()
                        .unwrap_or_default();
                    if String::from_utf8_lossy(first).trim() == self.text.trim() {
                        self.found.push((s, e));
                    }
                }
            }
            ruby_prism::visit_class_node(self, node);
            self.depth -= 1;
        }
    }
    let parsed = ruby_prism::parse(source);
    let mut finder = Finder {
        source,
        text,
        depth: 0,
        found: Vec::new(),
    };
    ruby_prism::Visit::visit(&mut finder, &parsed.node());
    let [(start, end)] = finder.found[..] else {
        return None;
    };
    let bol = source[..start]
        .iter()
        .rposition(|b| *b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let indent = String::from_utf8_lossy(&source[bol..start]).to_string();
    indent
        .chars()
        .all(|c| c == ' ' || c == '\t')
        .then_some((end, indent))
}

/// A [`Vfs`] that reads the app and its gems with an observation's patches written in.
pub struct ObservedVfs<'a, V: Vfs + ?Sized> {
    pub inner: &'a V,
    pub patches: &'a Patches,
}

impl<V: Vfs + ?Sized> ObservedVfs<'_, V> {
    fn patched(&self, path: &Path, bytes: Vec<u8>) -> io::Result<Vec<u8>> {
        let mut bytes = bytes;
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(inserts) = self.patches.inserts.get(&canonical) {
            let mut sorted = inserts.clone();
            sorted.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
            for (at, text) in sorted {
                bytes.splice(at..at, text.into_bytes());
            }
        }
        let written = path.to_string_lossy();
        for (gem_file, edits) in &self.patches.gem_edits {
            if !written.ends_with(&format!("/{gem_file}")) {
                continue;
            }
            for (start, end, text) in edits.iter().rev() {
                if bytes.get(*start..*end) != Some(text.as_bytes()) {
                    return Err(io::Error::other(format!(
                        "{written}: the observed statement at byte {start} is not there"
                    )));
                }
                bytes.splice(*start..*end, b"nil".iter().copied());
            }
        }
        Ok(bytes)
    }
}

impl<V: Vfs + ?Sized> Vfs for ObservedVfs<'_, V> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let bytes = self.inner.read(path)?;
        self.patched(path, bytes)
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        String::from_utf8(self.read(path)?).map_err(io::Error::other)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.inner.read_dir(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
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

/// The gems (`name-version` directories) whose reached code still changes a class it holds in a variable.
pub fn needed(app: &App) -> Vec<String> {
    let unreached = crate::source_gem_shake::UnreachedGemCode::of(app);
    let mut gems: Vec<String> = Vec::new();
    for lc in &app.library_classes {
        for m in &lc.methods {
            let Some(gem) = gem_dir_of(app, m.body.span) else {
                continue;
            };
            if gems.contains(&gem) || unreached.as_ref().is_some_and(|u| u.covers(&m.body.span)) {
                continue;
            }
            let mut found = false;
            visit(&m.body, &mut |e| found |= changes_held_class(e));
            if found {
                gems.push(gem);
            }
        }
    }
    gems.sort();
    gems
}

const SHAPE: &[&str] = &["define_method", "include", "extend", "prepend", "const_set"];

fn changes_held_class(e: &Expr) -> bool {
    let ExprNode::Send {
        recv: Some(recv),
        method,
        args,
        ..
    } = &*e.node
    else {
        return false;
    };
    if matches!(&*recv.node, ExprNode::SelfRef | ExprNode::Const { .. }) {
        return false;
    }
    let named = match (method.as_str(), args.first().map(|a| &*a.node)) {
        (
            "send" | "__send__" | "public_send",
            Some(ExprNode::Lit {
                value: Literal::Sym { value },
            }),
        ) => value.as_str().to_string(),
        (m, _) => m.to_string(),
    };
    SHAPE.contains(&named.as_str())
        || named.starts_with("after_")
        || named.starts_with("before_")
        || matches!(named.as_str(), "validate" | "scope")
}

/// The `name-version` directory of the source gem a span's file is in.
fn gem_dir_of(app: &App, span: crate::span::Span) -> Option<String> {
    let path = &crate::ide::source(app, span.file)?.path;
    let rest = path.split("/source_gems/").nth(1)?;
    rest.split('/').next().map(str::to_string)
}

/// What the observation read, so a rebuild that changes none of it reuses it: read before ingest, from the app's own files.
pub fn key(dir: &Path) -> String {
    use sha2::{Digest, Sha256};
    fn walk(dir: &Path, h: &mut Sha256) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, h);
            } else if let Ok(bytes) = std::fs::read(&path) {
                h.update(path.to_string_lossy().as_bytes());
                h.update(&bytes);
            }
        }
    }
    let mut h = Sha256::new();
    h.update(crate::version::describe().as_bytes());
    h.update(OBSERVER.as_bytes());
    // Not the gems' own files: an installed version does not change, and the lockfile names each.
    for file in ["Gemfile", "Gemfile.lock", ".ruby-version"] {
        h.update(std::fs::read(dir.join(file)).unwrap_or_default());
    }
    for sub in ["app", "config", "lib"] {
        walk(&dir.join(sub), &mut h);
    }
    format!("{:x}", h.finalize())
}

/// The cached observation's patches, when nothing it read has changed since.
pub fn cached(dir: &Path) -> Option<Result<Patches, String>> {
    let cached: Cached = serde_json::from_slice(&std::fs::read(dir.join(CACHE)).ok()?).ok()?;
    if cached.key != key(dir) || cached.env.as_ref().is_some_and(|env| *env != environment(dir)) {
        return None;
    }
    Some(patches(&cached.observation, dir))
}

fn visit(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    e.node.for_each_child(&mut |c| visit(c, f));
}
