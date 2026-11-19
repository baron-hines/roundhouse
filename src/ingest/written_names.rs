//! Each superclass and `include` target (and a model's `prepend`
//! target) the source writes, and the
//! class a job's `X.queue_adapter =` names, resolved once from
//! Rubydex's answers and stored on the IR in place.
//!
//! Ruby looks a constant up in the lexical scopes around the reference
//! (`Module.nesting`), then in the ancestors of the innermost one, then
//! at the top level. The written name alone cannot say which: in
//!
//! ```ruby
//! module Billing
//!   class Invoice < Record; end      # Billing::Record
//! end
//! class Billing::Receipt < Record; end # ::Record
//! ```
//!
//! both classes write `Record`, and only the second opens `Billing`
//! without making it a lexical scope. Rubydex has already resolved
//! every written reference against the whole app, so ingest asks it
//! once here, and analysis, lowering, emit and the IDE read the stored
//! name instead of each re-deriving it from the class's own name.
//!
//! A name keeps its written spelling when it has no source position
//! (a class a lowering synthesized) or Rubydex has no namespace answer
//! for it (a gem's class, an alias, a value). Ruby's own lookup found
//! no source declaration there, so no lexical guess could do better.

use crate::App;
use crate::analyze::ConstResolver;
use crate::dialect::{ControllerBodyItem, ModelBodyItem};
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol};
use crate::span::{SourceFile, Span};

/// Rewrite every source-written superclass and mixin in `app` to the
/// declaration Rubydex resolved it to.
pub(super) fn resolve(app: &mut App, sources: &[SourceFile], resolver: &ConstResolver) {
    let names = Names { sources, resolver };
    let declared: crate::det::HashSet<String> =
        app.library_classes.iter().map(|c| c.name.0.as_str().to_string()).chain(app.models.iter().map(|m| m.name.0.as_str().to_string())).collect();
    // The superclasses first, so the mixin fallback below follows each as Rubydex resolved it (`N::Base`, not `Base`).
    for class in &mut app.library_classes {
        if let Some(parent) = &mut class.parent {
            names.rewrite_parent(&class.name, parent, class.parent_span);
        }
    }
    for model in &mut app.models {
        if let Some(parent) = &mut model.parent {
            names.rewrite_parent(&model.name, parent, model.parent_span);
        }
    }
    let parents: crate::det::HashMap<String, String> = app
        .library_classes
        .iter()
        .filter_map(|c| c.parent.as_ref().map(|p| (c.name.0.as_str().to_string(), p.0.as_str().to_string())))
        .chain(app.models.iter().filter_map(|m| m.parent.as_ref().map(|p| (m.name.0.as_str().to_string(), p.0.as_str().to_string()))))
        .collect();
    // Rubydex's misses, for the ancestor fallback once every include it answers is rewritten.
    let mut missed: Vec<(usize, usize, String)> = Vec::new();
    for (ci, class) in app.library_classes.iter_mut().enumerate() {
        // A `Data.define` block's includes are absolute already, and its
        // spans locate the factory call, not a written name.
        let data = class.data_define().is_some();
        if data {
            continue;
        }
        for at in 0..class.includes.len() {
            let written = class.includes[at].0.as_str().to_string();
            if let Some(span) = names.argument_span(class.include_spans[at], &written) {
                match names.resolved(span, &written) {
                    Some(resolved) => class.includes[at] = resolved,
                    None => missed.push((ci, at, written)),
                }
            }
        }
        // `extend`/`prepend` a library class's own body does not model
        // structurally (unlike top-level `include`, above) stay in
        // `unknown_calls` as a raw `Send`; resolve those the same way,
        // so the Ruby/Spinel emitter's own lookup for which ones to
        // replay (`app_extension_calls` in `src/emit/ruby/library.rs`)
        // reads Rubydex's answer instead of guessing a second one.
        for call in &mut class.unknown_calls {
            names.rewrite_mixin_call(call, &["extend", "prepend"]);
            // `lower::job_payload::job_adapter` exempts a job whose own
            // body sets it an inline adapter, and reads this receiver to
            // tell. A bare `NotifyJob` there is the job itself inside
            // `module Admin; class NotifyJob`, and the top-level one
            // inside `class Admin::NotifyJob`.
            names.rewrite_receiver(call, "queue_adapter=");
        }
    }
    // A concern's `included do` statements, which the concern splices
    // copy onto each includer: an `include Guard` there names the module
    // as seen from inside the concern, not from the includer.
    for items in app.concern_model_items.values_mut() {
        for item in items {
            if let ModelBodyItem::Unknown { expr, .. } = item {
                names.rewrite_mixin_call(expr, &["include"]);
            }
        }
    }
    for model in &mut app.models {
        if let Some(parent) = &mut model.parent {
            names.rewrite_parent(&model.name, parent, model.parent_span);
        }
        for item in &mut model.body {
            if let ModelBodyItem::Unknown { expr, .. } = item {
                names.rewrite_mixin_call(expr, &["include", "prepend"]);
            }
        }
    }
    // Not the written spellings: a superclass's bare include reads the module Rubydex resolved it to.
    let mut unresolved: crate::det::HashSet<(usize, usize)> = missed.iter().map(|(ci, at, _)| (*ci, *at)).collect();
    let mut mixins: crate::det::HashMap<String, Vec<Vec<String>>> = app
        .library_classes
        .iter()
        .enumerate()
        .map(|(ci, c)| (c.name.0.as_str().to_string(), include_calls(&c.includes, &c.include_spans, &|at| unresolved.contains(&(ci, at)))))
        .chain(app.models.iter().map(|m| (m.name.0.as_str().to_string(), model_includes(&m.body))))
        .collect();
    // One at a time, in source order: an answer is visible to the includes resolved after it.
    for (ci, at, written) in &missed {
        let class = &app.library_classes[*ci];
        // Only the calls before this one: Ruby evaluates a call's arguments before the include changes the ancestry.
        let this_call = class.include_spans[*at];
        let first = class.include_spans.iter().position(|s| *s == this_call).unwrap_or(*at);
        let earlier = include_calls(&class.includes[..first], &class.include_spans[..first], &|at| unresolved.contains(&(*ci, at)));
        let lookup = Lookup { parents: &parents, mixins: &mixins, declared: &declared };
        let Some(hit) = ancestor_constant(class.name.0.as_str(), written, &earlier, &lookup) else { continue };
        unresolved.remove(&(*ci, *at));
        let class = &mut app.library_classes[*ci];
        class.includes[*at] = ClassId(Symbol::from(hit.as_str()));
        let calls = include_calls(&class.includes, &class.include_spans, &|at| unresolved.contains(&(*ci, at)));
        mixins.insert(class.name.0.as_str().to_string(), calls);
    }
    for controller in &mut app.controllers {
        if let Some(parent) = &mut controller.parent {
            names.rewrite_parent(&controller.name, parent, controller.parent_span);
        }
        for sibling in &mut controller.sibling_classes {
            let mut parent = ClassId(sibling.parent.clone());
            names.rewrite_parent(&ClassId(sibling.name.clone()), &mut parent, sibling.parent_span);
            sibling.parent = parent.0;
        }
        for item in &mut controller.body {
            if let ControllerBodyItem::Unknown { expr, .. } = item {
                names.rewrite_mixin_call(expr, &["include"]);
            }
        }
    }
}

struct Names<'a> {
    sources: &'a [SourceFile],
    resolver: &'a ConstResolver,
}

impl Names<'_> {
    fn source(&self, span: Span) -> Option<&str> {
        if span.is_synthetic() {
            return None;
        }
        let index = span.file.0.checked_sub(1)? as usize;
        Some(self.sources.get(index)?.text.as_str())
    }

    /// The namespace `written`, ending at `span.end`, resolves to.
    /// `None` unless the source spells that name there, so a span that
    /// a pass moved or a name it rewrote is never answered for a
    /// different reference.
    fn resolved(&self, span: Span, written: &str) -> Option<ClassId> {
        let text = self.source(span)?;
        let path: Vec<Symbol> = written.split("::").filter(|s| !s.is_empty()).map(Symbol::from).collect();
        let last = path.last()?.as_str();
        let start = (span.end as usize).checked_sub(last.len())?;
        if text.get(start..span.end as usize) != Some(last) {
            return None;
        }
        let Some(class) = self.resolver.namespace(span, &path) else {
            return self.resolved_prefix(span, &path, text);
        };
        let name = class.0.as_str();
        // A scoped `A::B` never names a top-level constant: Ruby does not
        // search `Object` for a scoped lookup (since 2.5 it is a
        // NameError). Rubydex does when the app reopens `A` without
        // declaring `B` (a gem's `A::B`), and its answer `Object` would
        // replace the real, undeclared `A::B`.
        if path.len() > 1 && !name.contains("::") {
            return None;
        }
        (name.rsplit("::").next() == Some(last)).then(|| class.clone())
    }

    /// A name Rubydex has no namespace for (a gem's module) whose leading
    /// segments it does resolve: Ruby binds the first segment lexically
    /// and then looks the rest up inside it, so `Testing::Assertions`
    /// inside `module ActiveSupport` is `ActiveSupport::Testing::
    /// Assertions` once the app declares `ActiveSupport::Testing`. Stored
    /// as written, the name reads as the top-level `Testing::Assertions`
    /// to every consumer that is not Ruby's own lexical lookup.
    /// The longest resolved prefix, with the rest appended.
    ///
    /// Not for a name led by a framework root (`Rails::Railtie` inside
    /// `class Billing`): the framework defines it before any app file
    /// loads, while an app namesake (`Billing::Rails`, from a file that
    /// may load after this one) is only there once its file has run.
    /// Binding the prefix statically read `Billing::Rails::Railtie`, a
    /// NameError at boot. The written name stays.
    ///
    /// Nor for a name Ruby's core or standard library defines whole
    /// (`Logger::Formatter` inside `module Tessera`, once a later file
    /// opens `Tessera::Logger`): until that file loads, Ruby's lexical
    /// lookup finds the top-level `Logger`, and after it the app's
    /// namespace, which declares no `Formatter`, is a NameError. The
    /// written name is the only answer that loads.
    fn resolved_prefix(&self, span: Span, path: &[Symbol], text: &str) -> Option<ClassId> {
        if path.first().is_some_and(|head| super::app::FRAMEWORK_ROOTS.contains(&head.as_str())) {
            return None;
        }
        let written = path.iter().map(Symbol::as_str).collect::<Vec<_>>().join("::");
        if crate::emit::audit::CORE_CONSTANTS.binary_search(&written.as_str()).is_ok() {
            return None;
        }
        let written_len: usize = path.iter().map(|s| s.as_str().len()).sum::<usize>() + 2 * (path.len() - 1);
        let start = (span.end as usize).checked_sub(written_len)?;
        (1..path.len()).rev().find_map(|keep| {
            let prefix = &path[..keep];
            let prefix_len = prefix.iter().map(|s| s.as_str().len()).sum::<usize>() + 2 * (keep - 1);
            let end = start + prefix_len;
            let last = prefix.last()?.as_str();
            if text.get(end - last.len()..end) != Some(last) {
                return None;
            }
            let class = self.resolver.namespace(Span { file: span.file, start: start as u32, end: end as u32 }, prefix)?;
            let name = class.0.as_str();
            if name.rsplit("::").next() != Some(last) {
                return None;
            }
            let rest = path[keep..].iter().map(Symbol::as_str).collect::<Vec<_>>().join("::");
            Some(ClassId(Symbol::from(format!("{name}::{rest}"))))
        })
    }

    fn rewrite_id(&self, id: &mut ClassId, span: Span) {
        if let Some(resolved) = self.resolved(span, id.0.as_str()) {
            *id = resolved;
        }
    }

    /// A superclass of the class `owner`. Ruby evaluates the superclass
    /// before it creates the class, so `module Admin; class
    /// NotesController < NotesController` names the top-level one;
    /// Rubydex answers with the class being declared. A class is never
    /// its own superclass, so that answer leaves the name as written.
    fn rewrite_parent(&self, owner: &ClassId, parent: &mut ClassId, span: Span) {
        if let Some(resolved) = self.resolved(span, parent.0.as_str())
            && resolved != *owner
        {
            *parent = resolved;
        }
    }

    /// The span of `written` as an argument of the `include` call at
    /// `call`: the one place inside the call that spells it whole.
    fn argument_span(&self, call: Span, written: &str) -> Option<Span> {
        let text = self.source(call)?.get(call.start as usize..call.end as usize)?;
        let identifier = |c: char| c.is_alphanumeric() || c == '_' || c == ':';
        let mut found = None;
        for (offset, _) in text.match_indices(written) {
            let before = text[..offset].chars().next_back();
            let after = text[offset + written.len()..].chars().next();
            if before.is_some_and(identifier) || after.is_some_and(identifier) {
                continue;
            }
            if found.is_some() {
                // `include A, A` or a name repeated in a comment; ask
                // about neither rather than guess.
                return None;
            }
            let end = call.start + (offset + written.len()) as u32;
            found = Some(Span { file: call.file, start: call.start + offset as u32, end });
        }
        found
    }

    /// `include X` in a model or controller body, a model's `prepend X`, or (passing
    /// `&["extend", "prepend"]`) a library class's own `extend`/
    /// `prepend` captured verbatim in its `unknown_calls`. A rooted
    /// `::X` is absolute as written.
    fn rewrite_mixin_call(&self, expr: &mut Expr, methods: &[&str]) {
        let ExprNode::Send { recv: None, method, args, .. } = &mut *expr.node else { return };
        if !methods.contains(&method.as_str()) {
            return;
        }
        for arg in args.iter_mut() {
            let span = arg.span;
            let ExprNode::Const { path } = &mut *arg.node else { continue };
            if path.first().is_none_or(|head| head.as_str().is_empty()) {
                continue;
            }
            let written = path.iter().map(Symbol::as_str).collect::<Vec<_>>().join("::");
            if let Some(resolved) = self.resolved(span, &written) {
                *path = resolved.0.as_str().split("::").map(Symbol::from).collect();
            }
            arg.decisions |= crate::expr::RESOLVED_MIXIN;
        }
    }

    /// The class receiver of a `X.method …` call in a class body. A
    /// rooted `::X` is absolute as written.
    fn rewrite_receiver(&self, expr: &mut Expr, method: &str) {
        let ExprNode::Send { recv: Some(recv), method: called, .. } = &mut *expr.node else { return };
        if called.as_str() != method {
            return;
        }
        let span = recv.span;
        let ExprNode::Const { path } = &mut *recv.node else { return };
        if path.first().is_none_or(|head| head.as_str().is_empty()) {
            return;
        }
        let written = path.iter().map(Symbol::as_str).collect::<Vec<_>>().join("::");
        if let Some(resolved) = self.resolved(span, &written) {
            *path = resolved.0.as_str().split("::").map(Symbol::from).collect();
        }
    }
}

struct Lookup<'a> {
    parents: &'a crate::det::HashMap<String, String>,
    /// Each class's and module's include calls in order, each call's arguments in order.
    mixins: &'a crate::det::HashMap<String, Vec<Vec<String>>>,
    declared: &'a crate::det::HashSet<String>,
}

/// A class's includes grouped by the call that wrote them (`include A, B` shares one span), a name Rubydex did not
/// answer marked `?` (its bare spelling may be any declaration of it).
fn include_calls(includes: &[ClassId], spans: &[Span], unresolved: &dyn Fn(usize) -> bool) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut last: Option<Span> = None;
    for (at, (id, span)) in includes.iter().zip(spans).enumerate() {
        let plain = id.0.as_str().trim_start_matches("::");
        let name = if unresolved(at) { format!("?{plain}") } else { plain.to_string() };
        match out.last_mut() {
            Some(call) if last == Some(*span) => call.push(name),
            _ => out.push(vec![name]),
        }
        last = Some(*span);
    }
    out
}

fn model_includes(body: &[ModelBodyItem]) -> Vec<Vec<String>> {
    body.iter()
        .filter_map(|item| match item {
            ModelBodyItem::Unknown { expr, .. } => match &*expr.node {
                ExprNode::Send { recv: None, method, args, .. } if method.as_str() == "include" => Some(args),
                _ => None,
            },
            _ => None,
        })
        .map(|args| {
            args.iter()
                .filter_map(|a| match &*a.node {
                    // Not known resolved: a bare single segment may be any declaration of it.
                    ExprNode::Const { path } => {
                        let rooted = path.first().is_some_and(|s| s.as_str().is_empty());
                        let name = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::");
                        Some(if !rooted && !name.contains("::") { format!("?{name}") } else { name })
                    }
                    _ => None,
                })
                .collect()
        })
        .collect()
}

/// A module's own ancestors below it, in Ruby's order, each module's expansion computed once.
fn expansion(module: &str, lookup: &Lookup, cache: &mut crate::det::HashMap<String, Vec<String>>, active: &mut Vec<String>) -> Vec<String> {
    if let Some(done) = cache.get(module) {
        return done.clone();
    }
    if active.iter().any(|m| m == module) {
        return Vec::new();
    }
    active.push(module.to_string());
    let calls = lookup.mixins.get(module).cloned().unwrap_or_default();
    let out = apply_calls(&calls, &[], lookup, cache, active);
    active.pop();
    cache.insert(module.to_string(), out.clone());
    out
}

/// A class's modules, highest first, after its include calls, as Ruby's `include` inserts them: each call's arguments
/// appended last first (`include A, B` puts `A` above `B`), each module with its own ancestors below it, a module the
/// class already has moving the insertion point past it, and one a superclass has (`inherited`) left where it is. An
/// `UNSURE` marker sits where an unresolved spelling leaves the rest unknown, so what sits above it still answers.
fn apply_calls(
    calls: &[Vec<String>],
    inherited: &[String],
    lookup: &Lookup,
    cache: &mut crate::det::HashMap<String, Vec<String>>,
    active: &mut Vec<String>,
) -> Vec<String> {
    let mut list: Vec<String> = Vec::new();
    for call in calls {
        for module in call.iter().rev() {
            let mut chain = vec![module.clone()];
            match module.strip_prefix('?') {
                // Not a declaration's ancestors as definite: an unresolved name may be any declaration of it, and one
                // with includes of its own leaves what follows unknown.
                Some(bare) => {
                    let nested = format!("::{bare}");
                    if lookup.mixins.iter().any(|(name, calls)| (name == bare || name.ends_with(&nested)) && !calls.is_empty()) {
                        chain.push(UNSURE.to_string());
                    }
                }
                None => chain.extend(expansion(module, lookup, cache, active)),
            }
            let mut at = 0;
            for m in chain {
                if m == UNSURE {
                    list.insert(at, m);
                    at += 1;
                    continue;
                }
                let bare = m.trim_start_matches('?');
                let found = list.iter().position(|l| l != UNSURE && l.trim_start_matches('?') == bare);
                let inherited_hit = inherited.iter().find(|l| l.as_str() != UNSURE && l.trim_start_matches('?') == bare);
                match (found, inherited_hit) {
                    // Not backwards past an earlier match: Ruby only moves the insertion point forward.
                    (Some(i), _) => {
                        // A resolved and an unresolved spelling of one name may be two modules, the other one
                        // inserted where this one would be.
                        let mut i = i;
                        if list[i].starts_with('?') != m.starts_with('?') {
                            list.insert(at, UNSURE.to_string());
                            if i >= at {
                                i += 1;
                            }
                            at += 1;
                        }
                        at = at.max(i + 1);
                    }
                    (None, Some(l)) => {
                        if l.starts_with('?') != m.starts_with('?') {
                            list.insert(at, UNSURE.to_string());
                            at += 1;
                        }
                    }
                    (None, None) => {
                        list.insert(at, m);
                        at += 1;
                    }
                }
            }
        }
    }
    list
}

/// Marks a class's module list whose order a resolved and an unresolved spelling of one name left unknown.
const UNSURE: &str = "?";

enum Search {
    Found(String),
    /// A module Rubydex did not resolve may be any declaration of its bare name: one holding the constant leaves the
    /// answer unknown.
    Unknown,
    Missing,
}

fn in_modules(modules: &[String], written: &str, lookup: &Lookup) -> Search {
    for module in modules {
        if module == UNSURE {
            return Search::Unknown;
        }
        let (bare, unresolved) = match module.strip_prefix('?') {
            Some(b) => (b, true),
            None => (module.as_str(), false),
        };
        let nested = format!("::{bare}::{written}");
        if unresolved && !bare.contains("::") && lookup.declared.iter().any(|d| d.ends_with(&nested)) {
            return Search::Unknown;
        }
        let candidate = format!("{bare}::{written}");
        if lookup.declared.contains(&candidate) {
            return Search::Found(candidate);
        }
    }
    Search::Missing
}

/// Rubydex answers nothing for a bare mixin of a class that includes more than one (`include RateConcept` beside
/// `include PriceConcept` in `class Provider::Synth < Provider`, where one alone resolves): the name as Ruby finds it in
/// the class's own constants and mixins, then each superclass's, then the top level's, among the app's declarations. Not the
/// enclosing namespaces, which a compact `class A::B` does not open. Removable once Shopify/rubydex#1106 is fixed.
fn ancestor_constant(owner: &str, written: &str, earlier: &[Vec<String>], lookup: &Lookup) -> Option<String> {
    if written.contains("::") {
        return None;
    }

    let mut chain: Vec<String> = Vec::new();
    let mut current = Some(owner.to_string());
    while let Some(class) = current {
        if chain.contains(&class) {
            break;
        }
        current = lookup.parents.get(&class).cloned();
        chain.push(class);
    }
    // Each class's own modules, from the top of the chain down, so a module a superclass has is not inserted again;
    // for the class itself only those included by the calls above the one being resolved.
    let mut cache = crate::det::HashMap::default();
    let mut inherited: Vec<String> = Vec::new();
    let mut own: Vec<Vec<String>> = vec![Vec::new(); chain.len()];
    for (i, class) in chain.iter().enumerate().rev() {
        let calls = if i == 0 { earlier } else { lookup.mixins.get(class).map(Vec::as_slice).unwrap_or(&[]) };
        own[i] = apply_calls(calls, &inherited, lookup, &mut cache, &mut Vec::new());
        inherited.extend(own[i].iter().cloned());
    }
    let mut found = None;
    for (class, modules) in chain.iter().zip(&own) {
        let candidate = format!("{class}::{written}");
        if lookup.declared.contains(&candidate) {
            found = Some(candidate);
            break;
        }
        match in_modules(modules, written, lookup) {
            Search::Found(hit) => {
                found = Some(hit);
                break;
            }
            Search::Unknown => return None,
            Search::Missing => {}
        }
    }
    // Not where an enclosing namespace declares another of the name: `module N; class C` reads `N::M` first and a
    // compact `class N::C` does not, which the class's name alone cannot tell.
    let found = found?;
    // The class's own constant is its innermost lexical scope, read first whether the class is nested or compact.
    if found == format!("{owner}::{written}") {
        return Some(found);
    }
    let mut prefix = owner;
    while let Some((outer, _)) = prefix.rsplit_once("::") {
        let lexical = format!("{outer}::{written}");
        if lookup.declared.contains(&lexical) && lexical != found {
            return None;
        }
        prefix = outer;
    }
    Some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(
        parents: &'a crate::det::HashMap<String, String>,
        mixins: &'a crate::det::HashMap<String, Vec<Vec<String>>>,
        declared: &'a crate::det::HashSet<String>,
    ) -> Lookup<'a> {
        Lookup { parents, mixins, declared }
    }

    #[test]
    fn the_current_calls_own_arguments_are_not_ancestors_yet() {
        // `class Child < Root; include Pack::A, Kind`: `Kind` is read before `Pack::A` joins the ancestry.
        let parents = [("Child".to_string(), "Root".to_string())].into_iter().collect();
        let mixins = crate::det::HashMap::default();
        let declared = ["Root", "Root::Kind", "Pack::A", "Pack::A::Kind"].into_iter().map(String::from).collect();
        let earlier = include_calls(&[], &[], &|_| false);
        assert_eq!(ancestor_constant("Child", "Kind", &earlier, &lookup(&parents, &mixins, &declared)).as_deref(), Some("Root::Kind"));
        // A completed call before it does count.
        let earlier = vec![vec!["Pack::A".to_string()]];
        assert_eq!(ancestor_constant("Child", "Kind", &earlier, &lookup(&parents, &mixins, &declared)).as_deref(), Some("Pack::A::Kind"));
    }

    #[test]
    fn calls_are_read_last_first_and_a_calls_arguments_in_order() {
        let parents = crate::det::HashMap::default();
        let mixins = crate::det::HashMap::default();
        let declared = ["A", "A::Kind", "B", "B::Kind"].into_iter().map(String::from).collect();
        let one_call = vec![vec!["A".to_string(), "B".to_string()]];
        assert_eq!(ancestor_constant("C", "Kind", &one_call, &lookup(&parents, &mixins, &declared)).as_deref(), Some("A::Kind"));
        let two_calls = vec![vec!["A".to_string()], vec!["B".to_string()]];
        assert_eq!(ancestor_constant("C", "Kind", &two_calls, &lookup(&parents, &mixins, &declared)).as_deref(), Some("B::Kind"));
    }

    #[test]
    fn a_shared_ancestor_sits_below_both_includers() {
        // `include Pack::A, Pack::B` with both including `Pack::Common`: Ruby's order is A, B, Common.
        let parents = crate::det::HashMap::default();
        let mixins: crate::det::HashMap<String, Vec<Vec<String>>> =
            [("Pack::A", "Pack::Common"), ("Pack::B", "Pack::Common")].into_iter().map(|(m, c)| (m.to_string(), vec![vec![c.to_string()]])).collect();
        let declared = ["Pack::A", "Pack::B", "Pack::B::Hue", "Pack::Common", "Pack::Common::Hue"].into_iter().map(String::from).collect();
        let calls = vec![vec!["Pack::A".to_string(), "Pack::B".to_string()]];
        assert_eq!(ancestor_constant("C", "Hue", &calls, &lookup(&parents, &mixins, &declared)).as_deref(), Some("Pack::B::Hue"));
    }

    #[test]
    fn only_an_unresolved_bare_name_is_ambiguous() {
        // A resolved top-level `A` is that module, whatever `N::A` also declares; an unresolved `A` may be either.
        let parents = crate::det::HashMap::default();
        let mixins = crate::det::HashMap::default();
        let declared = ["A", "A::Hue", "N::A", "N::A::Hue"].into_iter().map(String::from).collect();
        let resolved = vec![vec!["A".to_string()]];
        assert_eq!(ancestor_constant("C", "Hue", &resolved, &lookup(&parents, &mixins, &declared)).as_deref(), Some("A::Hue"));
        let unresolved = vec![vec!["?A".to_string()]];
        assert_eq!(ancestor_constant("C", "Hue", &unresolved, &lookup(&parents, &mixins, &declared)), None);
    }

    fn calls(groups: &[&[&str]]) -> Vec<Vec<String>> {
        groups.iter().map(|g| g.iter().map(|m| m.to_string()).collect()).collect()
    }

    #[test]
    fn a_shared_ancestor_keeps_its_place_and_moves_the_insertion_point() {
        // `include First; include Second`, First including Common, Second including Common, Tail: Ruby's order is
        // Second, First, Common, Tail.
        let parents = crate::det::HashMap::default();
        let mixins: crate::det::HashMap<String, Vec<Vec<String>>> =
            [("First".to_string(), calls(&[&["Common"]])), ("Second".to_string(), calls(&[&["Common", "Tail"]]))].into_iter().collect();
        let declared = ["First", "First::Hue", "Second", "Common", "Tail", "Tail::Hue"].into_iter().map(String::from).collect();
        let own = calls(&[&["First"], &["Second"]]);
        assert_eq!(ancestor_constant("C", "Hue", &own, &lookup(&parents, &mixins, &declared)).as_deref(), Some("First::Hue"));
    }

    #[test]
    fn a_module_a_superclass_has_stays_below_it() {
        // `Mid < Base`, Base including Common, Mid including Outer (which includes Common): Common stays below Base.
        let parents = [("Mid".to_string(), "Base".to_string())].into_iter().collect();
        let mixins: crate::det::HashMap<String, Vec<Vec<String>>> =
            [("Base".to_string(), calls(&[&["Common"]])), ("Outer".to_string(), calls(&[&["Common"]]))].into_iter().collect();
        let declared = ["Base", "Base::Hue", "Common", "Common::Hue", "Outer"].into_iter().map(String::from).collect();
        let own = calls(&[&["Outer"]]);
        assert_eq!(ancestor_constant("Mid", "Hue", &own, &lookup(&parents, &mixins, &declared)).as_deref(), Some("Base::Hue"));
    }

    #[test]
    fn the_insertion_point_never_moves_back() {
        // `include A; include B; include Outer`, Outer including A, B, Tail: Ruby's order is Outer, B, A, Tail.
        let parents = crate::det::HashMap::default();
        let mixins: crate::det::HashMap<String, Vec<Vec<String>>> = [("Outer".to_string(), calls(&[&["A", "B", "Tail"]]))].into_iter().collect();
        let declared = ["A", "A::Hue", "B", "Outer", "Tail", "Tail::Hue"].into_iter().map(String::from).collect();
        let own = calls(&[&["A"], &["B"], &["Outer"]]);
        assert_eq!(ancestor_constant("C", "Hue", &own, &lookup(&parents, &mixins, &declared)).as_deref(), Some("A::Hue"));
    }

    #[test]
    fn an_unresolved_inherited_spelling_is_not_the_resolved_module() {
        // `Child < Base`, Base including an unresolved `Common` (maybe `N::Common`), Child including resolved `Common`.
        let parents = [("Child".to_string(), "Base".to_string())].into_iter().collect();
        let mixins: crate::det::HashMap<String, Vec<Vec<String>>> = [("Base".to_string(), calls(&[&["?Common"]]))].into_iter().collect();
        let declared = ["Base", "Base::Hue", "Common", "Common::Hue", "N::Common"].into_iter().map(String::from).collect();
        let own = calls(&[&["Common"]]);
        assert_eq!(ancestor_constant("Child", "Hue", &own, &lookup(&parents, &mixins, &declared)), None);
    }

    #[test]
    fn an_unresolved_name_does_not_lend_one_declarations_ancestors() {
        // `include ?A`, where top-level `A` includes Tail and `N::A` includes Other: which one supplies `Hue` is unknown.
        let parents = crate::det::HashMap::default();
        let mixins: crate::det::HashMap<String, Vec<Vec<String>>> =
            [("A".to_string(), calls(&[&["Tail"]])), ("N::A".to_string(), calls(&[&["Other"]]))].into_iter().collect();
        let declared = ["A", "N::A", "Tail", "Tail::Hue", "Other", "Other::Hue"].into_iter().map(String::from).collect();
        let own = calls(&[&["?A"]]);
        assert_eq!(ancestor_constant("C", "Hue", &own, &lookup(&parents, &mixins, &declared)), None);
    }

    #[test]
    fn an_expansions_uncertainty_survives_an_inherited_one() {
        // Base: A, ?A (unsure); Child < Base includes Outer: B, X, ?B (unsure on its own).
        let parents = [("Child".to_string(), "Base".to_string())].into_iter().collect();
        let mixins: crate::det::HashMap<String, Vec<Vec<String>>> = [
            ("Base".to_string(), calls(&[&["A"], &["?A"]])),
            ("Outer".to_string(), calls(&[&["B"], &["X"], &["?B"]])),
        ]
        .into_iter()
        .collect();
        let declared = ["Base", "A", "B", "X", "X::Hue", "N::B", "N::B::Hue", "Outer"].into_iter().map(String::from).collect();
        let own = calls(&[&["Outer"]]);
        assert_eq!(ancestor_constant("Child", "Hue", &own, &lookup(&parents, &mixins, &declared)), None);
    }

    #[test]
    fn uncertainty_below_a_definite_match_does_not_hide_it() {
        // `include ?A; include B`, B holding `Hue` and above `?A`: B answers whatever `?A` denotes.
        let parents = crate::det::HashMap::default();
        let mixins: crate::det::HashMap<String, Vec<Vec<String>>> =
            [("A".to_string(), calls(&[&["Tail"]])), ("N::A".to_string(), calls(&[&["Other"]]))].into_iter().collect();
        let declared = ["A", "N::A", "B", "B::Hue", "Tail", "Tail::Hue", "Other"].into_iter().map(String::from).collect();
        let own = calls(&[&["?A"], &["B"]]);
        assert_eq!(ancestor_constant("C", "Hue", &own, &lookup(&parents, &mixins, &declared)).as_deref(), Some("B::Hue"));
    }
}
