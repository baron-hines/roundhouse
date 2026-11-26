// Not every unknown constant read made a NameError: only where the code itself rescues or tests for the constant's absence.
use crate::app::App;
use crate::expr::{Expr, ExprNode};
use crate::ident::Symbol;

/// A method by its class, name and side: `(class, name, class-side)`.
type MethodKey = (String, String, bool);

fn method_key(class: &str, m: &crate::dialect::MethodDef) -> MethodKey {
    (class.to_string(), m.name.as_str().to_string(), matches!(m.receiver, crate::dialect::MethodReceiver::Class))
}

/// What a source gem wrote, read while each method still resolves its
/// file: the classes it wrote code in, and its methods. A class the app
/// reopens holds the app's methods too.
pub(super) struct SourceGemCode {
    /// What reads a constant no locked gem could define there is a gem
    /// written for apps without it.
    pub(super) classes: crate::det::HashSet<String>,
    gem_methods: crate::det::HashSet<MethodKey>,
    app_methods: crate::det::HashSet<MethodKey>,
    /// The gems compiled from source, by name: every constant they define is in the program.
    gems: crate::det::HashSet<String>,
}

impl SourceGemCode {
    /// Whether the gem alone wrote this method: one the app also defines
    /// under that name is left as the app's.
    fn wrote(&self, class: &str, m: &crate::dialect::MethodDef) -> bool {
        self.wrote_key(&method_key(class, m))
    }

    fn wrote_key(&self, key: &MethodKey) -> bool {
        self.gem_methods.contains(key) && !self.app_methods.contains(key)
    }
}

pub(super) fn source_gem_code(app: &App) -> SourceGemCode {
    let mut code = SourceGemCode { classes: Default::default(), gem_methods: Default::default(), app_methods: Default::default(), gems: Default::default() };
    for lc in &app.library_classes {
        let class = lc.name.0.as_str();
        for m in &lc.methods {
            if let Some(path) = super::sources::path_of(m.name_span.file).filter(|p| is_source_gem_path(p)) {
                if let Some(gem) = source_gem_name(&path) {
                    code.gems.insert(gem);
                }
                code.classes.insert(class.to_string());
                code.gem_methods.insert(method_key(class, m));
            } else {
                code.app_methods.insert(method_key(class, m));
            }
        }
    }
    code
}

/// The names a gem's `const_set` may make: literals, and the literal prefix of a built one (aasm's
/// `"STATE_#{name.upcase}"`); a name of neither shape may be any, which the empty prefix stands for.
#[derive(Default)]
struct ConstSetNames {
    exact: crate::det::HashSet<String>,
    prefixes: crate::det::HashSet<String>,
}

impl ConstSetNames {
    fn may_make(&self, segment: &str) -> bool {
        self.exact.contains(segment) || self.prefixes.iter().any(|p| segment.starts_with(p.as_str()))
    }
}

/// What a name argument may be: `Some(true, s)` the literal `s`, `Some(false, p)` a name starting with `p`.
fn name_shapes(node: &ruby_prism::Node<'_>, locals: &crate::det::HashMap<String, Vec<(bool, String)>>) -> Vec<(bool, String)> {
    if let Some(name) = super::util::symbol_or_string_value(node) {
        return vec![(true, name)];
    }
    if let Some(interp) = node.as_interpolated_string_node().map(|n| n.parts()).or_else(|| node.as_interpolated_symbol_node().map(|n| n.parts())) {
        let head = interp.iter().next().and_then(|p| p.as_string_node().map(|s| String::from_utf8_lossy(s.unescaped()).into_owned()));
        return vec![(false, head.unwrap_or_default())];
    }
    if let Some(branch) = node.as_if_node() {
        let then = branch.statements().and_then(|s| s.body().iter().last());
        let other = branch.subsequent().and_then(|e| e.as_else_node()).and_then(|e| e.statements()).and_then(|s| s.body().iter().last());
        if let (Some(then), Some(other)) = (then, other) {
            let mut out = name_shapes(&then, locals);
            out.extend(name_shapes(&other, locals));
            return out;
        }
    }
    if let Some(read) = node.as_local_variable_read_node()
        && let Some(shapes) = locals.get(super::util::constant_id_str(&read.name()).as_ref() as &str)
    {
        return shapes.clone();
    }
    vec![(false, String::new())]
}

/// Every constant a source gem's file assigns, by full name (`A::B = …` at any nesting, `B = …` in a class or module
/// body), and the names it hands `const_set`.
fn gem_source_constants(texts: &[String]) -> (crate::det::HashSet<String>, ConstSetNames) {
    struct Collect {
        nesting: Vec<String>,
        written: crate::det::HashSet<String>,
        const_set: ConstSetNames,
        /// The shapes each local of the method being read is assigned anywhere in it.
        locals: crate::det::HashMap<String, Vec<(bool, String)>>,
        /// Reading a method for its locals' writes, before its `const_set`s.
        gathering: bool,
    }
    impl Collect {
        fn under(&self, name: &str) -> String {
            match self.nesting.last() {
                Some(outer) => format!("{outer}::{name}"),
                None => name.to_string(),
            }
        }
        fn path(node: &ruby_prism::Node<'_>) -> Option<(bool, Vec<String>)> {
            if let Some(read) = node.as_constant_read_node() {
                return Some((false, vec![super::util::constant_id_str(&read.name()).to_string()]));
            }
            let path = node.as_constant_path_node()?;
            let name = super::util::constant_id_str(&path.name()?).to_string();
            match path.parent() {
                None => Some((true, vec![name])),
                Some(parent) => {
                    let (rooted, mut head) = Self::path(&parent)?;
                    head.push(name);
                    Some((rooted, head))
                }
            }
        }
        fn full(&self, node: &ruby_prism::Node<'_>) -> Option<String> {
            let (rooted, parts) = Self::path(node)?;
            Some(if rooted { parts.join("::") } else { self.under(&parts.join("::")) })
        }
        fn binding(&mut self, node: &ruby_prism::Node<'_>) {
            let bare = node.as_constant_or_write_node().map(|n| n.name())
                .or_else(|| node.as_constant_and_write_node().map(|n| n.name()))
                .or_else(|| node.as_constant_operator_write_node().map(|n| n.name()))
                .or_else(|| node.as_constant_target_node().map(|n| n.name()));
            if let Some(name) = bare {
                let full = self.under(&super::util::constant_id_str(&name));
                self.written.insert(full);
                return;
            }
            let target = node.as_constant_path_or_write_node().map(|n| n.target())
                .or_else(|| node.as_constant_path_and_write_node().map(|n| n.target()))
                .or_else(|| node.as_constant_path_operator_write_node().map(|n| n.target()));
            if let Some(full) = target.and_then(|t| self.full(&t.as_node())) {
                self.written.insert(full);
                return;
            }
            if let Some(target) = node.as_constant_path_target_node()
                && let Some(name) = target.name()
            {
                let name = super::util::constant_id_str(&name).to_string();
                let full = match target.parent() {
                    None => Some(name),
                    Some(parent) => self.full(&parent).map(|owner| format!("{owner}::{name}")),
                };
                self.written.extend(full);
            }
        }
    }
    impl<'pr> ruby_prism::Visit<'pr> for Collect {
        fn visit_class_node(&mut self, node: &ruby_prism::ClassNode<'pr>) {
            let name = self.full(&node.constant_path()).unwrap_or_default();
            self.nesting.push(name);
            ruby_prism::visit_class_node(self, node);
            self.nesting.pop();
        }
        fn visit_module_node(&mut self, node: &ruby_prism::ModuleNode<'pr>) {
            let name = self.full(&node.constant_path()).unwrap_or_default();
            self.nesting.push(name);
            ruby_prism::visit_module_node(self, node);
            self.nesting.pop();
        }
        fn visit_constant_write_node(&mut self, node: &ruby_prism::ConstantWriteNode<'pr>) {
            let full = self.under(&super::util::constant_id_str(&node.name()));
            self.written.insert(full);
            ruby_prism::visit_constant_write_node(self, node);
        }
        fn visit_constant_path_write_node(&mut self, node: &ruby_prism::ConstantPathWriteNode<'pr>) {
            if let Some(full) = self.full(&node.target().as_node()) {
                self.written.insert(full);
            }
            ruby_prism::visit_constant_path_write_node(self, node);
        }
        // `X ||= …`, `A::X &&= …`, `A::X += …` and a multiple assignment's `A::X, Y = …` define the constant as well.
        fn visit_branch_node_enter(&mut self, node: ruby_prism::Node<'pr>) {
            self.binding(&node);
        }
        fn visit_leaf_node_enter(&mut self, node: ruby_prism::Node<'pr>) {
            self.binding(&node);
        }
        fn visit_def_node(&mut self, node: &ruby_prism::DefNode<'pr>) {
            // Twice: a write after the `const_set` in a loop reaches it too.
            let (outer, was) = (std::mem::take(&mut self.locals), self.gathering);
            self.gathering = true;
            ruby_prism::visit_def_node(self, node);
            self.gathering = was;
            ruby_prism::visit_def_node(self, node);
            self.locals = outer;
        }
        fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'pr>) {
            let shapes = name_shapes(&node.value(), &self.locals);
            self.locals.entry(super::util::constant_id_str(&node.name()).to_string()).or_default().extend(shapes);
            ruby_prism::visit_local_variable_write_node(self, node);
        }
        fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
            if !self.gathering
                && super::util::constant_id_str(&node.name()) == "const_set"
                && let Some(first) = node.arguments().and_then(|a| a.arguments().iter().next())
            {
                for (exact, name) in name_shapes(&first, &self.locals) {
                    if exact { self.const_set.exact.insert(name) } else { self.const_set.prefixes.insert(name) };
                }
            }
            ruby_prism::visit_call_node(self, node);
        }
    }
    let mut collect = Collect { nesting: Vec::new(), written: Default::default(), const_set: Default::default(), locals: Default::default(), gathering: false };
    for text in texts {
        let parsed = super::prism::parse_silent(text.as_bytes());
        collect.nesting.clear();
        ruby_prism::Visit::visit(&mut collect, &parsed.node());
    }
    (collect.written, collect.const_set)
}

/// `…/.roundhouse/source_gems/redis-client-0.30.1/lib/…`: `redis-client`.
fn source_gem_name(path: &str) -> Option<String> {
    let rest = path.split(".roundhouse/source_gems/").nth(1)?;
    let dir = rest.split('/').next()?;
    let (name, version) = dir.rsplit_once('-')?;
    version.starts_with(|c: char| c.is_ascii_digit()).then(|| name.to_string())
}

pub(crate) fn is_source_gem_path(p: &str) -> bool {
    p.contains("/.roundhouse/source_gems/") || p.starts_with(".roundhouse/source_gems/")
}

/// `shipped`: whether an installed locked gem (or a path gem) has the
/// feature's file under its `lib`.
/// `gem_texts`: each source gem's files as installed, read for what they assign.
pub(super) fn mark(app: &mut App, source_gem: &SourceGemCode, shipped: &dyn Fn(&str) -> bool, gem_texts: &[String]) {
    let source_gem_classes = &source_gem.classes;
    let Some(lock) = app.gem_lock.clone() else { return };
    let index = crate::gems::LockGemIndex::new(&lock);
    let absent_namespaces = crate::gems::AbsentGemNamespaces::new(&lock);
    let own: Vec<String> = app
        .models
        .iter()
        .map(|m| m.name.0.as_str().to_string())
        .chain(app.library_classes.iter().map(|c| c.name.0.as_str().to_string()))
        .chain(app.controllers.iter().map(|c| c.name.0.as_str().to_string()))
        .collect();
    // A root the app's own classes answer to: a whole name, the first
    // segment of a nested one, or the last segment of a nested one.
    let own_roots: crate::det::HashSet<&str> = own
        .iter()
        .flat_map(|o| {
            let nested = o.contains("::");
            let first = o.split("::").next().filter(|_| nested);
            let last = o.rsplit("::").next().filter(|_| nested);
            std::iter::once(o.as_str()).chain(first).chain(last)
        })
        .collect();
    // Not a name some class of the app declares as a constant: a relative read of it (Sponge's `MAX_DNS_TIME`) finds it.
    let mut declared: crate::det::HashSet<String> = app
        .library_classes
        .iter()
        .flat_map(|c| c.constants.iter().map(|(n, _)| n.as_str().to_string()))
        .chain(app.models.iter().flat_map(|m| m.body.iter().filter_map(|item| match item {
            crate::dialect::ModelBodyItem::Unknown { expr, .. } => match &*expr.node {
                ExprNode::Assign { target: crate::expr::LValue::Const { path }, .. } => path.last().map(|n| n.as_str().to_string()),
                _ => None,
            },
            _ => None,
        })))
        .collect();
    // Nor one an initializer assigns, under a condition or not: a gem's `defined?(APP_GATE)` sees it at run time.
    fn assigned_constants(e: &Expr, out: &mut crate::det::HashSet<String>) {
        if let ExprNode::Assign { target: crate::expr::LValue::Const { path }, .. } = &*e.node
            && let Some(root) = path.iter().find(|p| !p.as_str().is_empty())
        {
            out.insert(root.as_str().to_string());
        }
        e.node.for_each_child(&mut |c| assigned_constants(c, out));
    }
    let mut boot_assigned: crate::det::HashSet<String> = crate::det::HashSet::default();
    for stmt in &app.boot_program {
        assigned_constants(stmt, &mut boot_assigned);
    }
    declared.extend(boot_assigned.iter().cloned());
    // Not a constant some gem of the app, Ruby's standard library or the app itself may define: only one nothing could.
    let absent = |written: &[&Symbol]| {
        let written = rooted_tail(written).1;
        let Some(root) = written.first().map(|r| r.as_str()) else { return false };
        let segments: Vec<&str> = written.iter().map(|p| p.as_str()).collect();
        let full = segments.join("::");
        index.owners(&full).is_empty()
            && !crate::gems::is_stdlib_namespace(root)
            && !crate::gems::is_core_constant(&segments)
            && !own_roots.contains(root)
            && !declared.contains(root)
    };
    // Every class and constant by its full name: a gem's relative read names one only from where it is looked up.
    // What the gems' own source assigns, wherever IR keeps it or not (message_bus's top-level `MessageBus::BACKENDS = {}`),
    // and the names a `const_set` may make, which no reading of a gem's code calls absent.
    let (gem_written, const_set_names) = gem_source_constants(gem_texts);
    // Each namespace a class name passes through is a module Ruby has (`Greetings` of `Greetings::Codec::Base`), with no
    // methods of its own to record it.
    let full_names: crate::det::HashSet<String> = own
        .iter()
        .flat_map(|name| {
            let parts: Vec<&str> = name.split("::").filter(|s| !s.is_empty()).collect();
            (1..=parts.len()).map(move |k| parts[..k].join("::"))
        })
        .chain(app.library_classes.iter().flat_map(|c| c.constants.iter().map(move |(n, _)| format!("{}::{}", c.name.0.as_str(), n.as_str()))))
        .chain(boot_assigned.iter().cloned())
        .chain(gem_written.iter().cloned())
        .chain(app.models.iter().flat_map(|m| m.body.iter().filter_map(move |item| match item {
            crate::dialect::ModelBodyItem::Unknown { expr, .. } => match &*expr.node {
                ExprNode::Assign { target: crate::expr::LValue::Const { path }, .. } => path.last().map(|n| format!("{}::{}", m.name.0.as_str(), n.as_str())),
                _ => None,
            },
            _ => None,
        })))
        .collect();
    // Of every entry by the name: an opening (`X.prepend(M)` read as one) has no parent of its own.
    let mut ancestry: crate::det::HashMap<String, Vec<String>> = crate::det::HashMap::default();
    for c in &app.library_classes {
        let up = c.parent.iter().chain(c.includes.iter()).map(|p| p.0.as_str().trim_start_matches("::").to_string());
        ancestry.entry(c.name.0.as_str().to_string()).or_default().extend(up);
    }
    // In a gem's own code, a bare name the class's lexical scopes, Object, and its ancestors and mixins all lack: the
    // last segment of a class nested elsewhere (message_bus's `Backends::Base::ConcreteClassMustImplementError`, read
    // bare from `Codec::Base`) is no definition Ruby finds there.
    // Not a gem compiled from source: every constant it defines is in the program, so its namespace answers nothing more.
    let unknown_owners = |path: &str| index.owners(path).into_iter().filter(|g| !source_gem.gems.contains(*g)).count();
    let absent_from = |owner: &str, written: &[&Symbol]| {
        // `::X` is looked up at the top level alone, whatever namespace the read sits in.
        let (rooted, written) = rooted_tail(written);
        let owner = if rooted { "" } else { owner };
        let root = written.first().map(|r| r.as_str()).unwrap_or_default();
        let segments: Vec<&str> = written.iter().map(|p| p.as_str()).collect();
        if segments.iter().any(|s| const_set_names.may_make(s)) {
            return false;
        }
        if unknown_owners(&segments.join("::")) > 0 || crate::gems::is_stdlib_namespace(root) || crate::gems::is_core_constant(&segments) {
            return false;
        }
        // `A::B::C`: where `A` resolves, each later segment is looked up in what the earlier ones named; one missing
        // there is Ruby's NameError (redis's `::RedisClient::Cluster::InitialSetupError` without redis-clustering).
        if let [_, rest @ ..] = segments.as_slice()
            && !rest.is_empty()
        {
            // Not where the root names more than one class of the chain: a `::Root` written rooted, which this probe
            // no longer sees, would be the top-level one.
            let found: Vec<String> = owner_chain(owner)
                .into_iter()
                .map(|scope| if scope.is_empty() { root.to_string() } else { format!("{scope}::{root}") })
                .filter(|full| full_names.contains(full))
                .collect();
            let only = match found.as_slice() {
                [only] => only,
                // Not a root the chain defines: as anywhere else, unless the class sits in a Rails framework's namespace,
                // where the framework may define it (responders' `Renderers::RENDERERS` in `ActionController::Responder`).
                [] => {
                    let framework = owner.split("::").next().is_some_and(|r| super::app::FRAMEWORK_ROOTS.contains(&r));
                    return !framework && absent(written);
                }
                _ => return false,
            };
            let mut at = only.clone();
            for segment in rest {
                let next = format!("{at}::{segment}");
                if !full_names.contains(&next) {
                    return unknown_owners(&next) == 0;
                }
                at = next;
            }
            return false;
        }
        let mut seen: crate::det::HashSet<String> = crate::det::HashSet::default();
        let mut pending = vec![owner.to_string()];
        while let Some(scope) = pending.pop() {
            if !seen.insert(scope.clone()) || seen.len() > 64 {
                continue;
            }
            let parts: Vec<&str> = scope.split("::").filter(|s| !s.is_empty()).collect();
            for k in (0..=parts.len()).rev() {
                let full = parts[..k].iter().copied().chain(std::iter::once(root)).collect::<Vec<_>>().join("::");
                if full_names.contains(&full) {
                    return false;
                }
            }
            pending.extend(ancestry.get(&scope).into_iter().flatten().cloned());
        }
        // Not in a Rails framework's namespace: the framework, which the program does not hold, may define it there.
        !owner.split("::").next().is_some_and(|r| super::app::FRAMEWORK_ROOTS.contains(&r))
    };
    // A relative read is looked up from each namespace the class sits in, innermost first.
    fn owner_chain(owner: &str) -> Vec<String> {
        let parts: Vec<&str> = owner.split("::").filter(|s| !s.is_empty()).collect();
        (0..=parts.len()).rev().map(|k| parts[..k].join("::")).collect()
    }
    let gone_in = |owner: &str, written: &[&Symbol]| -> Option<String> {
        let tail: Vec<&str> = written.iter().map(|p| p.as_str()).collect();
        let scopes: Vec<&str> = owner.split("::").filter(|s| !s.is_empty()).collect();
        (0..=scopes.len()).rev().find_map(|k| {
            let full: Vec<&str> = scopes[..k].iter().copied().chain(tail.iter().copied()).collect();
            let (_, depth) = absent_namespaces.owner(&full.join("::"))?;
            // Ruby names the first segment it could not find: the absent gem's namespace.
            Some(full[..depth].join("::"))
        })
    };
    // A required path is the absent gem's own when its namespace is (tzinfo-data's `tzinfo/data`), unless a locked,
    // installed gem ships the file: Ruby's `require` then loads that one.
    let unloadable = |path: &str| {
        absent_namespaces.owner(&path.split('/').collect::<Vec<_>>().join("::")).is_some() && !shipped(feature_of(path))
    };
    // Not a feature any locked gem (by name, constant or file), Ruby (by exact path) or Rails provides: a gem's optional feature's library the app doesn't bundle
    // (`require "method_source"` in a loader that rescues LoadError). Ruby raises LoadError where the method runs; Spinel
    // resolves a require statically, and `spin build` refuses one it cannot satisfy.
    let provided_by_nothing = |path: &str| {
        // Looked up without its suffix: `io/wait.so` is Ruby's `io/wait` (the raise keeps the name as written).
        let feature = feature_of(path);
        let root = feature.split('/').next().unwrap_or(feature);
        let under_ruby_root = feature.contains('/') && RUBY_FEATURE_ROOTS.contains(&root);
        let as_constant: Vec<String> = feature.split('/').map(|s| s.split(['_', '-']).map(capitalized).collect::<String>()).collect();
        !root.is_empty()
            && !feature.starts_with('.')
            && !crate::gems::is_framework_feature(feature)
            && !ships_with_ruby(feature)
            && ![root.to_string(), root.replace('_', "-"), feature.replace('/', "-")].iter().any(|n| lock.version_of(n).is_some())
            // Under a standard-library root, the first segment (`Net`) is every `net-*` gem's namespace, so the
            // constant names no gem: the gem's name (net-http-persistent) or a file it ships decides.
            && (under_ruby_root || index.owners(&as_constant.join("::")).is_empty())
            && !shipped(feature)
    };
    unreached_gated_classes(app, source_gem_classes, &absent);
    for lc in app.library_classes.iter_mut() {
        let owner = lc.name.0.as_str().to_string();
        let gone = |written: &[&Symbol]| gone_in(&owner, written);
        // A gem's own read of a constant nothing locked defines is Ruby's NameError where it runs (rack-attack's `Redis::VERSION`).
        let everywhere: Vec<Vec<Symbol>> = if source_gem_classes.contains(&owner) { vec![Vec::new()] } else { Vec::new() };
        for m in lc.methods.iter_mut() {
            // Only in a source gem's own methods: its `require` runs when the method does. An app's own, in a
            // class it reopens or not, is the app's to bundle.
            let from_gem = source_gem.wrote(&owner, m);
            let unloadable = |path: &str| unloadable(path) || (from_gem && provided_by_nothing(path));
            let in_gem = |written: &[&Symbol]| absent_from(&owner, written);
            let probe: Probe<'_> = if from_gem { &in_gem } else { &absent };
            walk(&mut m.body, &everywhere, probe, &gone, &unloadable);
        }
    }
    let gone = |written: &[&Symbol]| gone_in("", written);
    // A mixin's method spliced into a controller is a copy of the module's: one a source gem alone wrote decides its
    // `require`s as the original does. The walk below then finds nothing left to rewrite in it.
    for controller in app.controllers.iter_mut() {
        let Some(origins) = app.concern_spliced_actions.get(&controller.name) else { continue };
        for item in controller.body.iter_mut() {
            let crate::dialect::ControllerBodyItem::Action { action, .. } = item else { continue };
            let Some(module) = origins.get(&action.name) else { continue };
            let key = (module.0.as_str().to_string(), action.name.as_str().to_string(), false);
            if source_gem.wrote_key(&key) {
                let unloadable = |path: &str| unloadable(path) || provided_by_nothing(path);
                walk(&mut action.body, &[], &absent, &gone, &unloadable);
            }
        }
    }
    crate::lower::for_each_hook_body(app, &mut |e| walk(e, &[], &absent, &gone, &unloadable));
}

/// Ruby ships this exact feature: a root such as `json`, or a path under one that Ruby's own library or a default gem
/// has (`net/http`, not net-http-persistent's `net/http/persistent`).
fn ships_with_ruby(feature: &str) -> bool {
    if feature.contains('/') {
        super::stdlib_features::STDLIB_SUBFEATURES.binary_search(&feature).is_ok()
            || super::app::APP_REQUIRABLE_STDLIB.contains(&feature)
    } else {
        RUBY_FEATURE_ROOTS.contains(&feature)
    }
}

/// A required path as a feature: without `.rb` or a native extension's suffix.
fn feature_of(path: &str) -> &str {
    [".rb", ".so", ".bundle", ".dll"].iter().find_map(|ext| path.strip_suffix(ext)).unwrap_or(path)
}

/// The first segment of a feature Ruby itself provides, as a default or bundled gem or part of the standard
/// library, beyond `APP_REQUIRABLE_STDLIB` (`net/smtp`, `rbconfig`, `prism`).
const RUBY_FEATURE_ROOTS: &[&str] = &[
    "abbrev", "base64", "benchmark", "bigdecimal", "bundler", "cgi", "continuation", "coverage", "csv", "date", "debug",
    "delegate", "did_you_mean", "digest", "drb", "English", "erb", "error_highlight", "etc", "expect", "fcntl", "fiber",
    "fiddle", "fileutils", "find", "forwardable", "getoptlong", "io", "ipaddr", "irb", "json", "logger", "matrix",
    "minitest", "mkmf", "monitor", "mutex_m", "net", "nkf", "objspace", "observer", "open-uri", "open3", "openssl",
    "optparse", "ostruct", "pathname", "power_assert", "pp", "prettyprint", "prime", "prism", "pstore", "psych", "pty",
    "racc", "rake", "rbconfig", "rbs", "rdoc", "readline", "reline", "repl_type_completor", "resolv", "rexml", "rinda",
    "ripper", "rss", "ruby2_keywords", "rubygems", "securerandom", "set", "shellwords", "singleton", "socket",
    "stringio", "strscan", "syntax_suggest", "syslog", "tempfile", "test", "time", "timeout", "tmpdir", "tsort",
    "typeprof", "un", "uri", "weakref", "win32ole", "yaml", "zlib",
];

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

type Probe<'p> = &'p dyn Fn(&[&Symbol]) -> bool;

/// Whether a probed path is written rooted (`::X`), and its segments past the root.
fn rooted_tail<'a, 'b>(written: &'a [&'b Symbol]) -> (bool, &'a [&'b Symbol]) {
    match written.split_first() {
        Some((first, rest)) if first.as_str().is_empty() => (true, rest),
        _ => (false, written),
    }
}
type Gone<'p> = &'p dyn Fn(&[&Symbol]) -> Option<String>;
type Unloadable<'p> = &'p dyn Fn(&str) -> bool;

fn walk(e: &mut Expr, guarded: &[Vec<Symbol>], absent: Probe<'_>, gone: Gone<'_>, unloadable: Unloadable<'_>) {
    // Not left as the require: Spinel ignores one it cannot satisfy (and `spin build` refuses it), so tzinfo's `rescue LoadError` probe would read the gem as present.
    if let ExprNode::Send { recv: None, method, args, block: None, .. } = &*e.node
        && method.as_str() == "require"
        && let [arg] = args.as_slice()
        && let ExprNode::Lit { value: crate::expr::Literal::Str { value: path } } = &*arg.node
        && unloadable(path)
    {
        let span = e.span;
        let mut load_error = Expr::new(span, ExprNode::Const { path: vec![Symbol::from("LoadError")] });
        load_error.decisions |= crate::expr::GENERATED_CONST_REF;
        let text = Expr::new(span, ExprNode::Lit { value: crate::expr::Literal::Str { value: format!("cannot load such file -- {path}") } });
        let error = Expr::new(
            span,
            ExprNode::Send { recv: Some(load_error), method: Symbol::from("new"), args: vec![text], block: None, parenthesized: true },
        );
        *e.node = ExprNode::Raise { value: error };
        return;
    }
    if let ExprNode::Const { path } = &*e.node {
        let written: Vec<&Symbol> = path.iter().filter(|p| !p.as_str().is_empty()).collect();
        // The probe tells `::X` from `X` by the empty root it keeps.
        let probed: Vec<&Symbol> = if path.first().is_some_and(|p| p.as_str().is_empty()) { path.iter().collect() } else { written.clone() };
        // Not only where guarded: a constant of a gem the lockfile lacks is missing wherever it is read.
        if let Some(missing) = gone(&written) {
            let span = e.span;
            let mut name_error = Expr::new(span, ExprNode::Const { path: vec![Symbol::from("NameError")] });
            name_error.decisions |= crate::expr::GENERATED_CONST_REF;
            let text = Expr::new(span, ExprNode::Lit { value: crate::expr::Literal::Str { value: format!("uninitialized constant {missing}") } });
            let error = Expr::new(
                span,
                ExprNode::Send { recv: Some(name_error), method: Symbol::from("new"), args: vec![text], block: None, parenthesized: true },
            );
            *e.node = ExprNode::Raise { value: error };
            e.decisions |= crate::expr::GUARDED_NAME_ERROR;
            return;
        }
        if guarded.iter().any(|g| g.as_slice() == [Symbol::from(DEAD)])
            || (guarded.iter().any(|g| g.is_empty() || (written.len() >= g.len() && g.iter().zip(&written).all(|(a, b)| a == *b)))
                && !written.first().is_some_and(|root| super::app::FRAMEWORK_ROOTS.contains(&root.as_str()))
                && absent(&probed))
        {
            e.decisions |= crate::expr::ABSENCE_GUARDED;
        }
        return;
    }
    match &mut *e.node {
        ExprNode::BoolOp { op: crate::expr::BoolOpKind::And, left, right, .. } => {
            walk(left, guarded, absent, gone, unloadable);
            let inner = with_tested(guarded, &tested_constants(left), absent);
            walk(right, &inner, absent, gone, unloadable);
        }
        ExprNode::If { cond, then_branch, else_branch } => {
            walk(cond, guarded, absent, gone, unloadable);
            let inner = with_tested(guarded, &tested_constants(cond), absent);
            walk(then_branch, &inner, absent, gone, unloadable);
            walk(else_branch, guarded, absent, gone, unloadable);
        }
        ExprNode::BeginRescue { body, rescues, .. } if rescues.iter().any(catches_name_error) => {
            let mut inner = guarded.to_vec();
            inner.push(Vec::new());
            walk(body, &inner, absent, gone, unloadable);
            for r in rescues.iter_mut() {
                walk(&mut r.body, guarded, absent, gone, unloadable);
            }
        }
        ExprNode::RescueModifier { expr, fallback } => {
            let mut inner = guarded.to_vec();
            inner.push(Vec::new());
            walk(expr, &inner, absent, gone, unloadable);
            walk(fallback, guarded, absent, gone, unloadable);
        }
        ExprNode::Seq { exprs } => {
            let mut scope = guarded.to_vec();
            for stmt in exprs.iter_mut() {
                walk(stmt, &scope, absent, gone, unloadable);
                if let Some(root) = absence_return_guard(stmt) {
                    scope = with_tested(&scope, std::slice::from_ref(&root), absent);
                }
            }
        }
        _ => e.node.for_each_child_mut(&mut |c| walk(c, guarded, absent, gone, unloadable)),
    }
}

fn catches_name_error(r: &crate::expr::RescueClause) -> bool {
    r.classes.is_empty()
        || r.classes.iter().any(|c| {
            matches!(&*c.node, ExprNode::Const { path }
                if matches!(path.iter().filter(|p| !p.as_str().is_empty()).map(|p| p.as_str()).collect::<Vec<_>>()[..],
                    ["StandardError"] | ["NameError"] | ["Exception"]))
        })
}

/// A source gem's class that declares itself inapplicable without a constant nothing defines
/// (rack-attack's `DalliProxy.handle?`, `return false unless defined?(::Dalli)`), and that nothing
/// outside it names, is only ever picked by asking that: its instance methods never run here.
fn unreached_gated_classes(app: &mut App, source_gem_classes: &crate::det::HashSet<String>, absent: Probe<'_>) {
    let gated: Vec<String> = app
        .library_classes
        .iter()
        .filter(|lc| source_gem_classes.contains(lc.name.0.as_str()))
        .filter(|lc| {
            lc.methods.iter().any(|m| {
                m.receiver == crate::dialect::MethodReceiver::Class && absent_gate(&m.body).is_some_and(|t| {
                    let refs: Vec<&Symbol> = t.iter().collect();
                    !t.first().is_some_and(|root| super::app::FRAMEWORK_ROOTS.contains(&root.as_str())) && absent(&refs)
                })
            })
        })
        .map(|lc| lc.name.0.as_str().to_string())
        .collect();
    if gated.is_empty() {
        return;
    }
    let leaf = |name: &str| name.rsplit("::").next().unwrap_or(name).to_string();
    let mut named: crate::det::HashSet<String> = crate::det::HashSet::default();
    fn reads(e: &Expr, out: &mut crate::det::HashSet<String>) {
        if let ExprNode::Const { path } = &*e.node
            && let Some(last) = path.last()
        {
            out.insert(last.as_str().to_string());
        }
        e.node.for_each_child(&mut |c| reads(c, out));
    }
    for lc in &app.library_classes {
        // Its own bodies do not count: they run only once it is picked.
        if gated.contains(&lc.name.0.as_str().to_string()) {
            continue;
        }
        for m in &lc.methods {
            reads(&m.body, &mut named);
        }
        for e in lc.unknown_calls.iter().chain(lc.class_ivar_initializers.iter()).chain(lc.constants.iter().map(|(_, v)| v)) {
            reads(e, &mut named);
        }
    }
    crate::lower::for_each_hook_body_ref(app, &mut |e| reads(e, &mut named));
    for lc in app.library_classes.iter_mut() {
        let name = lc.name.0.as_str().to_string();
        if !gated.contains(&name) || named.contains(&leaf(&name)) {
            continue;
        }
        for m in lc.methods.iter_mut().filter(|m| m.receiver == crate::dialect::MethodReceiver::Instance) {
            unreached(&mut m.body);
        }
    }
}

fn unreached(e: &mut Expr) {
    e.decisions |= crate::expr::UNCALLED_DEFINER;
    e.node.for_each_child_mut(&mut unreached);
}

/// The constant whose absence makes a predicate answer falsy before anything else runs:
/// `return false unless defined?(C)` first, or a body that is `defined?(C) && …`.
fn absent_gate(body: &Expr) -> Option<Vec<Symbol>> {
    let first = match &*body.node {
        ExprNode::Seq { exprs } => exprs.first()?,
        _ => body,
    };
    if let ExprNode::BoolOp { op: crate::expr::BoolOpKind::And, .. } | ExprNode::Defined { .. } = &*first.node {
        return tested_constants(first).into_iter().next();
    }
    let ExprNode::If { cond, then_branch, else_branch } = &*first.node else { return None };
    let falsy = |e: &Expr| match &*e.node {
        ExprNode::Return { value } => matches!(&*value.node, ExprNode::Lit { value: crate::expr::Literal::Nil | crate::expr::Literal::Bool { value: false } }),
        _ => false,
    };
    // `return false unless defined?(C)` is an If whose else (or a negated test's then) returns.
    match &*cond.node {
        ExprNode::Defined { .. } if falsy(else_branch) => tested_constants(cond).into_iter().next(),
        _ if falsy(then_branch) => match &*cond.node {
            ExprNode::Send { recv: Some(r), method, args, .. } if method.as_str() == "!" && args.is_empty() => tested_constants(r).into_iter().next(),
            _ => None,
        },
        _ => None,
    }
}

/// The marker of a region no run reaches: its guard tests a constant nothing defines.
const DEAD: &str = "<absent>";

/// What `guarded` becomes past a test for `tested`: each constant guards its own reads, and
/// one nothing could define makes the region dead, so every read in it is Ruby's NameError if ever reached.
fn with_tested(guarded: &[Vec<Symbol>], tested: &[Vec<Symbol>], absent: Probe<'_>) -> Vec<Vec<Symbol>> {
    let mut out = guarded.to_vec();
    for t in tested {
        let refs: Vec<&Symbol> = t.iter().collect();
        // Not a framework constant: one missing there is a gap of this runtime, which a read still reports.
        let framework = t.first().is_some_and(|root| super::app::FRAMEWORK_ROOTS.contains(&root.as_str()));
        if !t.is_empty() && !framework && absent(&refs) {
            out.push(vec![Symbol::from(DEAD)]);
        }
        out.push(t.clone());
    }
    out
}

/// The constants `defined?` tests in a condition that holds only where all of them are defined:
/// `defined?(C)` itself, or any conjunct of an `&&` chain.
fn tested_constants(cond: &Expr) -> Vec<Vec<Symbol>> {
    match &*cond.node {
        ExprNode::Defined { operand } => match &*operand.node {
            ExprNode::Const { path } => vec![path.iter().filter(|p| !p.as_str().is_empty()).cloned().collect()],
            _ => Vec::new(),
        },
        ExprNode::BoolOp { op: crate::expr::BoolOpKind::And, left, right, .. } => {
            let mut out = tested_constants(left);
            out.extend(tested_constants(right));
            out
        }
        _ => Vec::new(),
    }
}

/// `return … unless defined?(C)`: the statements after it run only where `C` is defined.
fn absence_return_guard(stmt: &Expr) -> Option<Vec<Symbol>> {
    let ExprNode::If { cond, then_branch, else_branch } = &*stmt.node else { return None };
    let (tested, exits) = match &*cond.node {
        ExprNode::Defined { operand } => (operand, else_branch),
        ExprNode::Send { recv, method, args, .. } if method.as_str() == "!" => {
            let negated = match (recv, args.as_slice()) {
                (Some(r), []) => r,
                (None, [a]) => a,
                _ => return None,
            };
            match &*negated.node {
                ExprNode::Defined { operand } => (operand, then_branch),
                _ => return None,
            }
        }
        _ => return None,
    };
    if !matches!(&*exits.node, ExprNode::Return { .. } | ExprNode::Raise { .. }) {
        return None;
    }
    let ExprNode::Const { path } = &*tested.node else { return None };
    Some(path.iter().filter(|p| !p.as_str().is_empty()).cloned().collect())
}

/// `fail ArgumentError("…")` in a source gem (et-orbi's `EoTime#_to_f`): a
/// call of a capitalized method nothing defines, which Ruby raises
/// NoMethodError for when it is reached. Kernel's conversions answer their
/// own names, and an app's own method of the name is a call as written.
// Not run with `mark`: by then a source gem's methods no longer resolve their file, which tells them from the app's own.
pub(super) fn raise_undefined_capitalized_calls(app: &mut App) {
    const KERNEL: &[&str] = &["Array", "Complex", "Float", "Hash", "Integer", "Rational", "String", "BigDecimal", "Pathname", "URI", "JSON"];
    let defined: crate::det::HashSet<String> = app
        .library_classes
        .iter()
        .flat_map(|c| c.methods.iter().map(|m| m.name.as_str().to_string()))
        .chain(app.models.iter().flat_map(|m| m.methods().map(|d| d.name.as_str().to_string())))
        .collect();
    fn visit(e: &mut Expr, owner: &str, class_side: Option<&str>, defined: &crate::det::HashSet<String>) {
        e.node.for_each_child_mut(&mut |c| visit(c, owner, class_side, defined));
        let ExprNode::Send { recv: None, method, args, block: None, .. } = &*e.node else { return };
        let name = method.as_str();
        if args.is_empty() || !name.starts_with(|c: char| c.is_ascii_uppercase()) || KERNEL.contains(&name) || defined.contains(name) {
            return;
        }
        let span = e.span;
        let mut cls = Expr::new(span, ExprNode::Const { path: vec![Symbol::from("NoMethodError")] });
        cls.decisions |= crate::expr::GENERATED_CONST_REF;
        let whom = match class_side {
            Some(kind) => format!("{kind} {owner}"),
            None => format!("an instance of {owner}"),
        };
        let text = Expr::new(span, ExprNode::Lit { value: crate::expr::Literal::Str { value: format!("undefined method '{name}' for {whom}") } });
        let error = Expr::new(span, ExprNode::Send { recv: Some(cls), method: Symbol::from("new"), args: vec![text], block: None, parenthesized: true });
        *e.node = ExprNode::Raise { value: error };
    }
    for lc in app.library_classes.iter_mut() {
        let owner = lc.name.0.as_str().to_string();
        let gem = lc.methods.iter().any(|m| {
            super::sources::path_of(m.name_span.file).is_some_and(|p| p.contains("/.roundhouse/source_gems/") || p.starts_with(".roundhouse/source_gems/"))
        });
        if !gem {
            continue;
        }
        let kind = if lc.is_module { "module" } else { "class" };
        for m in lc.methods.iter_mut() {
            let class_side = (m.receiver == crate::dialect::MethodReceiver::Class).then_some(kind);
            visit(&mut m.body, &owner, class_side, &defined);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: &str = "module Shelf\n  class Box\n    def self.pretty!\n      require \"fancy_printer\"\n    end\n\n    def self.local_tag\n      require \"local_formatter\"\n    end\n  end\nend\n";

    /// A gem class the app reopens holds the gem's methods and the app's. Only the gem's own `require` of a library
    /// nothing provides becomes the `LoadError` Ruby raises; the app's `require` of its own lib file is the app's.
    #[test]
    fn an_apps_own_method_on_a_reopened_gem_class_keeps_its_require() {
        let tree: crate::vfs::FileTree = [
            ("db/schema.rb", "ActiveRecord::Schema.define do\nend\n"),
            ("lib/shelf/box.rb", BOX),
            ("lib/local_formatter.rb", "class LocalFormatter\nend\n"),
        ]
        .into_iter()
        .map(|(path, source)| (std::path::PathBuf::from(path), source.as_bytes().to_vec()))
        .collect();
        let mut app = crate::ingest::ingest_app_from_tree(tree).expect("ingest");
        // The class as ingest sees it when the gem is compiled from source and an app file reopens it.
        let gem_file = super::super::sources::register(".roundhouse/source_gems/shelf-1.0.0/lib/shelf.rb", BOX);
        let app_file = super::super::sources::register("lib/shelf/box.rb", BOX);
        let class = app.library_classes.iter_mut().find(|c| c.name.0.as_str() == "Shelf::Box").expect("Shelf::Box");
        for m in &mut class.methods {
            m.name_span.file = if m.name.as_str() == "pretty!" { gem_file } else { app_file };
        }
        app.gem_lock = Some(crate::gems::Lockfile::default());
        let code = source_gem_code(&app);
        mark(&mut app, &code, &|_| false, &[]);
        let class = app.library_classes.iter().find(|c| c.name.0.as_str() == "Shelf::Box").expect("Shelf::Box");
        let body = |name: &str| format!("{:?}", class.methods.iter().find(|m| m.name.as_str() == name).expect(name).body);
        assert!(body("pretty!").contains("cannot load such file -- fancy_printer"), "{}", body("pretty!"));
        assert!(body("local_tag").contains("local_formatter") && !body("local_tag").contains("cannot load such file"), "{}", body("local_tag"));
    }
}
