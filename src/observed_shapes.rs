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
pub const APP_CACHE: &str = ".roundhouse/observed_app_shapes";

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
    /// Each gem file the boot loaded, as `<gem dir>/<path in the gem>`.
    #[serde(default)]
    pub loaded: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cached {
    pub key: String,
    pub observation: Observation,
    /// The Ruby and gems the boot ran with, which a later build must share to reuse it.
    #[serde(default)]
    pub env: Option<String>,
}

/// The Ruby and gem paths a boot runs with, read in the app's directory: a failed boot is reused only under the same.
pub fn environment(dir: &Path) -> String {
    let mut probe = boot_env(std::process::Command::new("ruby"));
    probe
        .args(["-e", "print RUBY_DESCRIPTION, \"\\n\", Gem.path.join(\":\"), \"\\n\", Gem::Specification.map(&:full_name).sort.join(\" \")"])
        .current_dir(dir);
    match output_within(probe, PROBE_SECS) {
        Ok(out) => {
            String::from_utf8_lossy(&out.stdout).into_owned()
                + &std::env::var("GEM_PATH").unwrap_or_default()
                + &std::env::var("GEM_HOME").unwrap_or_default()
                + &std::env::var(BOOT_BUNDLE_PATH).unwrap_or_default()
                // Not one observation for every environment: Rails boots what `RAILS_ENV` names.
                + &std::env::var("RAILS_ENV").unwrap_or_else(|_| "development".into())
        }
        Err(_) => "no ruby".into(),
    }
}

const PROBE_SECS: u64 = 60;

/// A subprocess's output, or an error once it runs past `secs`: a Ruby that stalls must not hold the check.
fn output_within(mut command: std::process::Command, secs: u64) -> Result<std::process::Output, String> {
    let mut child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return child.wait_with_output().map_err(|e| e.to_string());
        }
        if started.elapsed() > std::time::Duration::from_secs(secs) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("it did not finish in {secs} s"));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// The bundle only the app's own boot reads (`ROUNDHOUSE_BOOT_BUNDLE_PATH`): the app's whole bundle, which the compiler
/// must not read as installed gems it could compile from source.
pub const BOOT_BUNDLE_PATH: &str = "ROUNDHOUSE_BOOT_BUNDLE_PATH";

fn boot_env(mut command: std::process::Command) -> std::process::Command {
    if std::env::var_os(BOOT_BUNDLE_PATH).is_some() {
        // Not the caller's pinned Ruby: the app's `.ruby-version` names the one its bundle was installed for.
        command.env_remove("RBENV_VERSION");
    }
    if let Some(bundle) = std::env::var_os(BOOT_BUNDLE_PATH).filter(|p| !p.is_empty()) {
        command.env("BUNDLE_PATH", bundle);
        // Not the app's `.bundle/config` as well: Bundler ranks its `BUNDLE_PATH` above the environment's.
        let config = std::env::temp_dir().join(format!("roundhouse-boot-bundle-config-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&config);
        command.env("BUNDLE_APP_CONFIG", config);
    }
    command
}

/// What the boot runs: the emitted CRuby tree a closure writes (the Spinel build's), or the app itself under its bundle.
#[derive(Clone, Copy)]
pub enum Boot<'a> {
    Tree(&'a dyn Fn(&Path) -> Result<(), String>),
    App,
}

/// The source edits an observation makes: statements written after an app file's offset, and gem statements that read `nil`.
#[derive(Debug, Default)]
pub struct Patches {
    inserts: BTreeMap<PathBuf, Vec<(usize, String)>>,
    /// `<gem dir>/<path in the gem>`, then each range with the text it must hold.
    gem_edits: BTreeMap<String, Vec<(usize, usize, String)>>,
    /// `<gem dir>`, then the features under its `lib/` the boot loaded.
    loaded: BTreeMap<String, Vec<String>>,
}

impl Patches {
    pub fn is_empty(&self) -> bool {
        self.inserts.is_empty() && self.gem_edits.is_empty() && self.loaded.is_empty()
    }

    /// The features (`aasm/persistence/active_record_persistence`) of `gem_dir`'s `lib/` the boot loaded.
    pub fn loaded_features(&self, gem_dir: &str) -> &[String] {
        self.loaded.get(gem_dir).map_or(&[], Vec::as_slice)
    }
}

/// The patches an observation of the app at `dir` makes, or why it cannot be written as source.
pub fn patches(observation: &Observation, dir: &Path) -> Result<Patches, String> {
    if let Some(why) = observation.refusals.first() {
        return Err(why.clone());
    }
    let mut out = Patches::default();
    for file in &observation.loaded {
        if let Some((gem, feature)) = file.split_once("/lib/") {
            out.loaded.entry(gem.to_string()).or_default().push(feature.trim_end_matches(".rb").to_string());
        }
    }
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

/// What `gems` do to the app's classes as the emitted tree boots: the cached observation under `key`, or a `boot`,
/// cached for the next build.
pub fn observe(input: &Path, gems: &[String], key: &str, boot: Boot<'_>) -> Result<Patches, String> {
    let observation = observed(input, gems, key, boot, &input.join(CACHE));
    patches(&observation, &input.canonicalize().map_err(|e| e.to_string())?)
}

/// The app's own boot, once per gem: what each gem does to the app's classes, from the gems whose doings it could
/// write as source; each other gem with the first reason it could not, whose calls stay as they were.
pub fn observe_app(input: &Path, gems: &[String], key: &str) -> (Result<Patches, String>, Vec<(String, String)>) {
    let mut merged = Observation::default();
    let mut refused = Vec::new();
    let dir = match input.canonicalize() {
        Ok(dir) => dir,
        Err(e) => return (Err(e.to_string()), refused),
    };
    // The class-body statements whose effects a gem already merged: each gem's boot is separate, so the order of two
    // gems' effects behind one statement is not known.
    let mut triggers: std::collections::BTreeSet<Trigger> = std::collections::BTreeSet::new();
    for gem in gems {
        // Not one boot for all: a gem whose doing cannot be written (view_component's generated slot module) would
        // keep every other gem's from being read.
        let observation = observed(input, std::slice::from_ref(gem), key, Boot::App, &input.join(APP_CACHE).join(format!("{gem}.json")));
        if let Some(why) = observation.refusals.first() {
            refused.push((gem.clone(), why.clone()));
            continue;
        }
        // Not merged before its own patches apply: a gem whose trigger reads two ways would drop every other's.
        if let Err(why) = patches(&observation, &dir) {
            refused.push((gem.clone(), why));
            continue;
        }
        let own: std::collections::BTreeSet<Trigger> = observation.effects.iter().filter_map(|e| e.trigger.clone()).collect();
        if let Some(shared) = own.iter().find(|t| triggers.contains(*t)) {
            refused.push((gem.clone(), format!("{}: another gem's effects come from `{}` too, in an order the separate boots do not keep", shared.file, shared.text)));
            continue;
        }
        triggers.extend(own);
        merged.effects.extend(observation.effects);
        merged.loaded.extend(observation.loaded);
    }
    (patches(&merged, &dir), refused)
}

fn observed(input: &Path, gems: &[String], key: &str, boot: Boot<'_>, cache: &Path) -> Observation {
    use self as obs;
    let cached = std::fs::read(cache)
        .ok()
        .and_then(|b| serde_json::from_slice::<obs::Cached>(&b).ok())
        .filter(|c| c.key == key && c.env.as_ref().is_none_or(|env| *env == obs::environment(input)));
    if let Some(c) = cached {
        return c.observation;
    }
    // Not reused once the Ruby or its gems differ: a failed boot may be the machine's, and a gem may shape a class by the Ruby it runs on.
    let before = obs::environment(input);
    let booted = boot_observer(input, gems, boot);
    let env = Some(before).filter(|before| *before == obs::environment(input));
    // Not the app's failed boot kept: a database or service that was down may be up on the next check, which asked for it.
    let keep = booted.is_ok() || !matches!(boot, Boot::App);
    let observation = booted.unwrap_or_else(|why| obs::Observation { effects: Vec::new(), refusals: vec![why], loaded: Vec::new() });
    // Not only a boot that answered: one that failed would otherwise be tried again on every build.
    // Nor one whose Ruby or gems changed while it ran: which of them it saw is not known.
    if env.is_some() && keep {
        if let Some(dir) = cache.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(
            cache,
            serde_json::to_vec_pretty(&obs::Cached { key: key.to_string(), observation: observation.clone(), env }).unwrap_or_default(),
        );
    }
    observation
}

fn boot_observer(input: &Path, gems: &[String], boot: Boot<'_>) -> Result<Observation, String> {
    // Not a name another user can guess and create first: the boot runs the Ruby files in it.
    let tmp = (0..16)
        .map(|attempt| {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
            std::env::temp_dir().join(format!("roundhouse-observe-{}-{nanos:x}-{attempt}", std::process::id()))
        })
        .find(|dir| {
            let mut builder = std::fs::DirBuilder::new();
            // Not readable by other users: the boot's log holds what the app prints as it loads.
            #[cfg(unix)]
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            builder.create(dir).is_ok()
        })
        .ok_or("no fresh temporary directory for the observed boot")?;
    let observed = boot_observer_in(input, gems, &tmp, boot);
    if std::env::var_os("ROUNDHOUSE_OBSERVE_KEEP").is_none() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    observed
}

fn boot_observer_in(input: &Path, gems: &[String], tmp: &Path, boot: Boot<'_>) -> Result<Observation, String> {
    use self as obs;
    let tmp = tmp.to_path_buf();
    let ruby = || {
        let mut command = boot_env(std::process::Command::new(if matches!(boot, Boot::App) { "bundle" } else { "ruby" }));
        if matches!(boot, Boot::App) {
            command.args(["exec", "ruby"]);
        }
        command
    };
    {
        {
            if let Boot::Tree(emit_ruby) = boot {
                emit_ruby(&tmp)?;
            }
            let mut roots = Vec::new();
            for gem in gems {
                let (name, version) = gem
                    .rsplit_once('-')
                    .ok_or_else(|| format!("`{gem}` names no version"))?;
                // In the app's directory: its `.ruby-version` names the Ruby whose gems the app runs on.
                let mut lookup = ruby();
                lookup
                    .args([
                        "-e",
                        &format!(
                            "print Gem::Specification.find_by_name({name:?}, {version:?}).gem_dir"
                        ),
                    ])
                    .current_dir(input);
                let found = output_within(lookup, PROBE_SECS).map_err(|e| format!("looking up {gem}: {e}"))?;
                if !found.status.success() {
                    let stderr = String::from_utf8_lossy(&found.stderr);
                    // Not always a missing gem: under `bundle exec` the bundle itself may not set up.
                    let first = stderr.lines().next().unwrap_or("").chars().take(300).collect::<String>();
                    return Err(if stderr.contains("Gem::MissingSpecError") {
                        format!("CRuby does not have {gem} installed")
                    } else {
                        format!("looking up {gem}: {first}")
                    });
                }
                roots.push(String::from_utf8_lossy(&found.stdout).into_owned());
            }
            let observer = tmp.join(".roundhouse-shape-observer.rb");
            let out = tmp.join(".roundhouse-shapes.json");
            std::fs::write(&observer, obs::OBSERVER).map_err(|e| e.to_string())?;
            // The app's own boot loads every class as production does (`eager_load!`), where the tree's boot file loads its own.
            let script = match boot {
                Boot::Tree(_) => format!("require {:?}", tmp.join("boot").display().to_string()),
                Boot::App => "require File.expand_path(\"config/environment\"); Rails.application.eager_load!".to_string(),
            };
            let app_root = if matches!(boot, Boot::App) { input.to_path_buf() } else { tmp.clone() };
            let mut child = ruby()
                .arg(format!("-r{}", observer.display()))
                .args(["-e", &script])
                .current_dir(input)
                .env("RH_SHAPE_OUT", &out)
                .env("RH_SHAPE_APP", &app_root)
                .env("RH_SHAPE_GEMS", roots.join(":"))
                .stdout(std::process::Stdio::null())
                .stderr(std::fs::File::create(tmp.join(".roundhouse-boot.log")).map_err(|e| e.to_string())?)
                .spawn()
                .map_err(|e| format!("ruby: {e}"))?;
            let started = std::time::Instant::now();
            // Not left to run: a boot that waits on a server or a socket would hold the build.
            let status = loop {
                if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                    break status;
                }
                if started.elapsed() > std::time::Duration::from_secs(120) {
                    let _ = child.kill();
                    return Err("the boot did not finish in 120 s".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            };
            if !status.success() {
                let stderr = std::fs::read_to_string(tmp.join(".roundhouse-boot.log")).unwrap_or_default();
                return Err(format!(
                    "the boot failed: {}",
                    stderr.lines().next().unwrap_or("")
                ));
            }
            let observation: obs::Observation =
                serde_json::from_slice(&std::fs::read(&out).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            Ok(observation)
        }
    }
}

