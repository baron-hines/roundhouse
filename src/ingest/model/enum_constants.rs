//! Order-independent static inputs for Rails enum declarations. Source
//! collection and Sorbet invalidation are separate from lexical resolution.
//! Invalid inputs still shadow outer names; members use canonical metadata.

mod context;
mod derived;
mod policy;
mod source;

use ruby_prism::Node;

use super::super::util::constant_id_str;
use crate::Symbol;
use crate::expr::Literal;

/// One entry per declared name. Unsupported values still shadow outer
/// namespaces; Sorbet members keep their names for write validation.
enum EnumConstant {
    Namespace,
    Mapping(Vec<(String, Literal)>),
    Sorbet {
        members: Vec<(Symbol, Literal)>,
        base: String,
        owners: Vec<String>,
    },
    Unsupported,
}

/// Syntax roles, not safety decisions. Only policy interprets these roles.
enum ValueContext {
    Discarded,
    Receiver,
    SerializedPair,
    Argument {
        receiver: Option<String>,
        method: String,
    },
    Stored,
    Returned,
    Chained,
    Unmodeled,
}

enum SurfaceUse {
    Reference(ValueContext),
    Call {
        method: String,
        result: ValueContext,
    },
    EnumBase,
    Inheritance,
    Override,
}

struct ScopedName {
    path: String,
    owners: Vec<String>,
}

enum SourceFact {
    Write(ScopedName),
    Use {
        subject: ScopedName,
        context: SurfaceUse,
    },
    Reopen {
        subject: ScopedName,
        collected: String,
    },
}

/// What the recorded constants' names add up to, read once per model class
/// ([`EnumConstants::names`]).
struct Names {
    /// Every `::` segment below the first (`EnumConstants::nested_names`).
    nested: std::rc::Rc<crate::det::HashSet<String>>,
    /// Every first segment (`EnumConstants::top_level_names`).
    top_level: std::rc::Rc<crate::det::HashSet<String>>,
    /// Every segment (`EnumConstants::defines_name`).
    segments: crate::det::HashSet<String>,
    /// Each segment below the first, by the qualified owner before it.
    by_owner: crate::det::HashMap<String, crate::det::HashSet<String>>,
    /// Every recorded name, by its last segment.
    by_last: crate::det::HashMap<String, Vec<String>>,
}

struct ResolvedAncestry {
    edges: crate::det::HashMap<String, Vec<String>>,
    unknown: crate::det::HashSet<String>,
}

#[derive(Default)]
pub(in crate::ingest) struct EnumConstants {
    values: crate::det::HashMap<String, EnumConstant>,
    /// [`Names`] of `values`' keys, with how many keys that was: names are
    /// only ever added, so the count says whether it is still current.
    names: std::cell::RefCell<Option<(usize, std::rc::Rc<Names>)>>,
    /// Each class's superclass and each owner's `include`/`extend`/`prepend`
    /// arguments (`Object`'s at a file's top level): (owner, target), each as
    /// its spellings ([`EnumConstants::resolve_spellings`]). Resolved when
    /// asked, after every source is recorded.
    ancestor_edges: Vec<(Vec<String>, Vec<String>)>,
    /// Owners, as their spellings, that include an argument this pass cannot name.
    unknown_ancestors: Vec<Vec<String>>,
    /// `ancestor_edges` and `unknown_ancestors` resolved, with the counts they were resolved at.
    resolved_ancestry: std::cell::RefCell<Option<((usize, usize, usize), std::rc::Rc<ResolvedAncestry>)>>,
    /// Lexical owners at each class/module open, keyed by (file, start).
    /// Concern `included do` resolution reads the same table models use.
    pub(in crate::ingest) nesting: crate::det::HashMap<(String, usize), Vec<String>>,
    /// Superclass identities are lookup barriers, never additional enum inputs.
    parents: crate::det::HashMap<String, Vec<ScopedName>>,
    facts: Vec<SourceFact>,
    /// Each class or module's folded constants, by qualified name, for a
    /// validation in another body that names them: every body that
    /// declares the owner, merged. `None`: a body not at a file's or
    /// body's top level, or a compact `class A::B` inside another
    /// namespace, whose owner is not certain.
    owner_consts: crate::det::HashMap<String, Option<super::ClassConsts>>,
    /// `NAME = …` writes per qualified name, at any depth and in every
    /// body: a name written twice (a reopened owner writing it again)
    /// has the value of whichever file loads last.
    write_counts: crate::det::HashMap<String, usize>,
    /// Constant names some input file writes outside one plain top-level
    /// `NAME = …`, mutates, or lets a variable hold ([`super::ConstTaint`]
    /// over the whole file). By name, whatever owner the write names.
    foreign_taint: crate::det::HashSet<String>,
    snapshot_taint: crate::det::HashSet<String>,
    snapshot_aliases: Vec<(String, String)>,
    snapshot_reflected: crate::det::HashSet<String>,
    /// `ALIAS = KINDS` edges the boot sources write, across all of them:
    /// closed over `foreign_taint` once they are all recorded, in
    /// `finish`, so an alias one initializer writes and a later one
    /// mutates through taints the original.
    boot_aliases: Vec<(String, String)>,
    /// Owner names a reflective call reaches (`Rule.const_set`,
    /// `Rule.send(:remove_const, …)`, `Kinds.const_get(:ALL) << x`,
    /// `"Kinds".constantize`, a receiverless `const_set` in the owner's
    /// body): any of their constants may be replaced or mutated. `Object`
    /// reaches them all.
    reflected: crate::det::HashSet<String>,
    /// Each owner's own `enum` declarations (bodies whose owner is
    /// certain), for another body reading `Owner.<plural>`.
    enum_declarations: crate::det::HashMap<String, Vec<derived::Declaration>>,
    /// `(owner's last segment, method)` for `def self.m` and `class <<
    /// self; def m` in an owner's body.
    singleton_methods: crate::det::HashSet<(String, String)>,
    /// Singleton method names whose owner is not certain: `def Const.m`,
    /// and an enum's class method declared in a body whose owner is not
    /// certain.
    uncertain_singleton_methods: crate::det::HashSet<String>,
    /// Owners (last segment) whose singleton class something opens:
    /// `singleton_class`, `define_singleton_method`, a non-`def` statement
    /// inside `class << self`.
    open_singletons: crate::det::HashSet<String>,
    /// A singleton class opened on a receiver this pass cannot name.
    singletons_unknown: bool,
}

impl EnumConstants {
    /// True when any lexical owner defines a `JSON` constant that would
    /// win bare `JSON` lookup inside that nest (e.g. `Admin::JSON` for
    /// `module Admin; class Article`).
    pub(in crate::ingest) fn shadows_bare_json(&self, lexical_owners: &[String]) -> bool {
        lexical_owners
            .iter()
            .any(|owner| self.values.contains_key(&format!("{owner}::JSON")))
    }

    /// Every name the app's recorded sources define BELOW the top level
    /// (`URI` for `Admin::URI` or `ApplicationRecord::URI`).
    pub(in crate::ingest) fn nested_names(&self) -> std::rc::Rc<crate::det::HashSet<String>> {
        self.names().nested.clone()
    }

    /// The names the recorded sources define directly in one of `class`'s
    /// possible ancestors (`URI` for `ApplicationRecord::URI` when `class`
    /// inherits `ApplicationRecord`, not for `Addressable::URI` unless an
    /// ancestor includes `Addressable`). An unrooted path starting with one
    /// of these could resolve to the app's own constant, so callers that
    /// fold a well-known top-level constant treat it as possibly shadowed.
    /// An ancestor that includes what this pass cannot name gives every
    /// nested name. A lexical owner is the caller's to rule out.
    pub(in crate::ingest) fn ancestor_names(&self, class: &str) -> std::rc::Rc<crate::det::HashSet<String>> {
        let names = self.names();
        let ancestry = self.resolved_ancestry(&names);
        let (edges, unknown) = (&ancestry.edges, &ancestry.unknown);
        // Object, Kernel and BasicObject are every class's ancestors without being named as one.
        let mut pending: Vec<String> = vec![class.to_string(), "Object".into(), "Kernel".into(), "BasicObject".into()];
        let mut seen = crate::det::HashSet::default();
        let mut found = crate::det::HashSet::default();
        while let Some(owner) = pending.pop() {
            if !seen.insert(owner.clone()) {
                continue;
            }
            if unknown.contains(&owner) {
                return names.nested.clone();
            }
            if let Some(defined) = names.by_owner.get(&owner) {
                found.extend(defined.iter().cloned());
            }
            pending.extend(edges.get(&owner).into_iter().flatten().cloned());
        }
        std::rc::Rc::new(found)
    }

    fn resolved_ancestry(&self, names: &Names) -> std::rc::Rc<ResolvedAncestry> {
        let state = (self.values.len(), self.ancestor_edges.len(), self.unknown_ancestors.len());
        if let Some((counted, resolved)) = &*self.resolved_ancestry.borrow() {
            if *counted == state {
                return resolved.clone();
            }
        }
        let mut edges: crate::det::HashMap<String, Vec<String>> = Default::default();
        for (owner, target) in &self.ancestor_edges {
            let targets = self.resolve_spellings(target, names);
            for owner in self.resolve_spellings(owner, names) {
                edges.entry(owner).or_default().extend(targets.iter().cloned());
            }
        }
        let unknown = self.unknown_ancestors.iter().flat_map(|o| self.resolve_spellings(o, names)).collect();
        let resolved = std::rc::Rc::new(ResolvedAncestry { edges, unknown });
        *self.resolved_ancestry.borrow_mut() = Some((state, resolved.clone()));
        resolved
    }

    /// The recorded names a path written in a body may name. `spellings` is
    /// the path under each enclosing namespace, innermost first, then as
    /// written; a rooted path is the one spelling `::X`. The innermost
    /// spelling a source defines is Ruby's lexical answer. Without one, the
    /// path may come through an ancestor's constants (`include Contactable`
    /// finding `ApplicationRecord::Contactable`), so every recorded name it
    /// ends is a candidate, as is the top-level one.
    fn resolve_spellings(&self, spellings: &[String], names: &Names) -> Vec<String> {
        let Some(written) = spellings.last() else { return Vec::new() };
        if let Some(rooted) = written.strip_prefix("::") {
            return vec![rooted.to_string()];
        }
        if let Some(lexical) = spellings[..spellings.len() - 1].iter().find(|s| self.values.contains_key(*s)) {
            return vec![lexical.clone()];
        }
        let last = written.rsplit("::").next().unwrap_or(written);
        let suffix = format!("::{written}");
        let mut out: Vec<String> = names.by_last.get(last).into_iter().flatten().filter(|k| k.ends_with(&suffix)).cloned().collect();
        out.push(written.clone());
        out
    }

    /// The first segment of every name the app's recorded sources define
    /// or reopen (`URI` for `URI = …`, `module URI`, or
    /// `URI::MailTo::EMAIL_REGEXP = …`). A well-known top-level constant
    /// under one of these may have been replaced by the app, so callers
    /// must not fold it from a built-in table, whichever way it is spelled.
    pub(in crate::ingest) fn top_level_names(&self) -> std::rc::Rc<crate::det::HashSet<String>> {
        self.names().top_level.clone()
    }

    /// The names of the constants recorded so far. Every model class asks,
    /// and a tree has tens of thousands of constants, so they are summed
    /// once and again only when more were recorded.
    fn names(&self) -> std::rc::Rc<Names> {
        if let Some((count, names)) = &*self.names.borrow() {
            if *count == self.values.len() {
                return names.clone();
            }
        }
        let mut nested = crate::det::HashSet::default();
        let mut top_level = crate::det::HashSet::default();
        let mut segments = crate::det::HashSet::default();
        let mut by_owner: crate::det::HashMap<String, crate::det::HashSet<String>> = Default::default();
        let mut by_last: crate::det::HashMap<String, Vec<String>> = Default::default();
        for key in self.values.keys() {
            let parts: Vec<&str> = key.split("::").collect();
            by_last.entry(parts[parts.len() - 1].to_string()).or_default().push(key.clone());
            top_level.insert(parts[0].to_string());
            segments.insert(parts[0].to_string());
            for i in 1..parts.len() {
                nested.insert(parts[i].to_string());
                segments.insert(parts[i].to_string());
                by_owner.entry(parts[..i].join("::")).or_default().insert(parts[i].to_string());
            }
        }
        let names = std::rc::Rc::new(Names { nested: nested.into(), top_level: top_level.into(), segments, by_owner, by_last });
        *self.names.borrow_mut() = Some((self.values.len(), names.clone()));
        names
    }

    /// Only already-consumed original Ruby can add mutation facts here.
    /// Declaration admission remains the prepass's input roots. Revalidate
    /// enum declarations and derived validation lists: no model replacement
    /// or unrelated diagnostics.
    pub(in crate::ingest) fn validate_consumed_sources(
        &mut self,
        app: &crate::App,
        sources: &[crate::span::SourceFile],
        input_files: &crate::det::HashSet<std::path::PathBuf>,
    ) -> super::super::IngestResult<()> {
        let validation_lists: Vec<_> = app.models.iter().filter_map(|model| {
            let source = model.span.file.0.checked_sub(1).and_then(|index| sources.get(index as usize))?;
            let lists = super::folded_enum_validation_lists(model, source, self);
            (!lists.is_empty()).then_some((model, source, lists))
        }).collect();
        for source in sources {
            let path = std::path::Path::new(&source.path);
            if path.extension().is_some_and(|extension| extension == "rb")
                && !input_files.contains(path)
            {
                self.record_mutations(source.text.as_bytes(), &source.path);
            }
        }
        self.finish();
        for (model, source, before) in validation_lists {
            let after = super::folded_enum_validation_lists(model, source, self);
            if before.iter().any(|(span, values)| after.get(span) != Some(values)) {
                super::super::survey::unwrap_or_record::<()>(Err(super::super::IngestError::Unsupported {
                    file: source.path.clone(),
                    message: "enum-derived inclusion list changed after loading source; validation cannot be statically lowered".into(),
                }))?;
            }
        }
        for model in app.models.iter().filter(|model| !model.enums.is_empty()) {
            if let Some(source) = model.span.file.0.checked_sub(1)
                .and_then(|index| sources.get(index as usize))
            {
                super::validate_sorbet_enum_mappings(model, source, self)?;
            }
        }
        Ok(())
    }

    fn qualify(owner: &str, name: &str) -> String {
        if let Some(rooted) = name.strip_prefix("::") {
            rooted.to_string()
        } else if owner.is_empty() {
            name.to_string()
        } else {
            format!("{owner}::{name}")
        }
    }

    fn resolve_name(&self, path: &str, owners: &[String]) -> Option<String> {
        self.resolve_name_in(path, owners, &mut crate::det::HashSet::default())
    }

    fn resolve_name_in(
        &self,
        path: &str,
        owners: &[String],
        visited: &mut crate::det::HashSet<String>,
    ) -> Option<String> {
        if let Some(rooted) = path.strip_prefix("::") {
            return Some(rooted.to_string());
        }
        let first = path.split("::").next()?;
        for owner in owners {
            if self.values.contains_key(&Self::qualify(owner, first)) {
                return Some(Self::qualify(owner, path));
            }
        }
        // Ruby searches lexical owners, then the innermost class's ancestry,
        // then Object. Do not fold inherited values, or fall past their shadow.
        if owners.first().is_some_and(|owner| self.inherited_shadow(owner, first, visited)) {
            return None;
        }
        if self.values.contains_key(first) {
            return Some(path.to_string());
        }
        // The one external namespace assumed here is Sorbet's gem base.
        // Its qualified writes/overrides still invalidate dependent enums.
        matches!(path, "T" | "T::Enum").then(|| path.to_string())
    }

    fn inherited_shadow(
        &self,
        owner: &str,
        first: &str,
        visited: &mut crate::det::HashSet<String>,
    ) -> bool {
        if !visited.insert(owner.to_string()) {
            return true; // Cyclic ancestry is not a provable root lookup.
        }
        let shadow = self.parents.get(owner).is_some_and(|parents| parents.iter().any(|parent| {
            self.resolve_name_in(&parent.path, &parent.owners, visited).is_some_and(|base| {
                self.values.contains_key(&Self::qualify(&base, first))
                    || self.inherited_shadow(&base, first, visited)
            })
        }));
        visited.remove(owner);
        shadow
    }

    fn namespace_chain_valid(&self, name: &str) -> bool {
        let mut namespace = name;
        while let Some((parent, _)) = namespace.rsplit_once("::") {
            if !matches!(self.values.get(parent), Some(EnumConstant::Namespace)) {
                return false;
            }
            namespace = parent;
        }
        true
    }

    /// The owner, member and owner table for a constant a body reads as
    /// `key` (`Rule::TEXT_SIZE_LIMIT`, `::Fasp::DATA_CATEGORIES`, or a bare
    /// `DATA_CATEGORIES` inside `module Fasp`), resolved the way Ruby does
    /// from the reference's lexical `owners` (innermost first). `None`
    /// when Ruby may read anything other than the owner's folded value:
    /// an unknown or shadowed owner, an owner declared in several bodies
    /// or reached by reflection, or a name tainted in any input file.
    pub(in crate::ingest) fn owner_constant(
        &self,
        key: &str,
        owners: &[String],
    ) -> Option<(String, String, &super::ClassConsts)> {
        let name = self.resolve_name(key, owners)?;
        if !self.namespace_chain_valid(&name) {
            return None;
        }
        let (owner, member) = name.rsplit_once("::")?;
        // An app body that reopens a core or standard-library namespace
        // (`module URI; module MailTo`) is not the whole owner: the
        // library writes it too, and the app file may never load (nothing
        // autoloads a reopen of a constant already defined).
        let root = owner.split("::").next().unwrap_or(owner);
        if crate::emit::audit::CORE_CONSTANTS.contains(&root) {
            return None;
        }
        if self.foreign_taint.contains(member)
            || self.reflected.contains("Object")
            || owner.split("::").any(|segment| self.reflected.contains(segment))
            || self.write_counts.get(&name) != Some(&1)
        {
            return None;
        }
        let table = self.owner_consts.get(owner)?.as_ref()?;
        Some((owner.to_string(), member.to_string(), table))
    }

    pub(in crate::ingest) fn snapshot_constant_is_stable(&self, key: &str, owners: &[String]) -> bool {
        let Some(name) = self.resolve_name(key, owners) else { return false };
        let Some((owner, member)) = name.rsplit_once("::") else { return false };
        let mut tainted = self.snapshot_taint.clone();
        super::ConstTaint::close_aliases(&mut tainted, &self.snapshot_aliases);
        self.namespace_chain_valid(&name)
            && self.write_counts.get(&name) == Some(&1)
            && self.owner_consts.get(owner).is_some_and(Option::is_some)
            && !tainted.contains(member)
            && !self.reflected.contains("Object")
            && !owner.split("::").any(|segment| self.reflected.contains(segment))
            && !self.snapshot_reflected.contains("Object")
            && !owner.split("::").any(|segment| self.snapshot_reflected.contains(segment))
    }

    /// Whether any recorded source defines a constant, class or module
    /// named `name` at any depth (`Regexp`, `Admin::Regexp`).
    pub(in crate::ingest) fn defines_name(&self, name: &str) -> bool {
        self.names().segments.contains(name)
    }

    /// A mapping named by a constant (or a Sorbet enum's `values`), or
    /// derived from another owner's ([`derived`]: `Gadget.statuses`,
    /// `Owner::HASH`, `.keys`, `.merge`).
    pub(in crate::ingest) fn resolve(
        &self,
        node: &Node<'_>,
        owners: &[String],
    ) -> Option<Vec<(String, Literal)>> {
        self.resolve_constant(node, owners).or_else(|| self.derived(node, owners))
    }

    fn resolve_constant(
        &self,
        node: &Node<'_>,
        owners: &[String],
    ) -> Option<Vec<(String, Literal)>> {
        let values_receiver = node.as_call_node().and_then(|call| {
            (constant_id_str(&call.name()) == "values"
                && call.arguments().is_none()
                && call.block().is_none())
            .then(|| call.receiver())
            .flatten()
        });
        let input = values_receiver.as_ref().unwrap_or(node);
        // Resolve the FIRST segment lexically, then read the rest strictly
        // through that owner. Never fall back past a shadowing namespace.
        let name = self.resolve_name(&Self::path(input)?, owners)?;
        if !self.namespace_chain_valid(&name) {
            return None;
        }
        match self.values.get(&name)? {
            EnumConstant::Mapping(mapping) if values_receiver.is_none() => Some(mapping.clone()),
            EnumConstant::Sorbet { members, .. } if values_receiver.is_some() => members
                .iter()
                .map(|(_, value)| match value {
                    Literal::Str { value: label } => Some((label.clone(), value.clone())),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod names_tests {
    use super::*;
    use crate::det::HashSet;

    fn record(constants: &mut EnumConstants, names: &[&str]) {
        for name in names {
            constants.values.insert(name.to_string(), EnumConstant::Unsupported);
        }
    }

    /// The scans the summed names replaced.
    fn scanned(constants: &EnumConstants, name: &str) -> (HashSet<String>, HashSet<String>, bool) {
        let keys = || constants.values.keys();
        (
            keys().flat_map(|key| key.split("::").skip(1).map(str::to_string)).collect(),
            keys().filter_map(|key| key.split("::").next().map(str::to_string)).collect(),
            keys().any(|key| key.split("::").any(|segment| segment == name)),
        )
    }

    #[test]
    fn summed_names_match_a_scan_and_follow_later_constants() {
        let mut constants = EnumConstants::default();
        record(&mut constants, &["Admin", "Admin::URI", "URI::MailTo::PATTERN", "Plan"]);
        for step in 0..2 {
            for probe in ["URI", "Admin", "Plan", "PATTERN", "Regexp", "MailTo"] {
                let (nested, top_level, defined) = scanned(&constants, probe);
                assert_eq!(*constants.nested_names(), nested, "step {step}");
                assert_eq!(*constants.top_level_names(), top_level, "step {step}");
                assert_eq!(constants.defines_name(probe), defined, "{probe} at step {step}");
            }
            // Recording more constants refreshes the sums.
            record(&mut constants, &["Billing::Regexp"]);
        }
        assert!(constants.defines_name("Regexp"));
    }

    #[test]
    fn a_name_is_an_ancestors_only_under_an_owner_the_class_inherits_or_includes() {
        let mut constants = EnumConstants::default();
        record(&mut constants, &[
            "Addressable::URI", "ROTP::OTP", "ROTP::TOTP", "ROTP::OTP::URI", "Kernel::Shade", "Concern", "Concern::Inner",
            "User", "ViewComponent::Base", "ViewComponent::Slotable", "ViewComponent::Slotable::Slot",
            "ApplicationRecord", "ApplicationRecord::Contactable", "ApplicationRecord::Contactable::Mark", "Person",
        ]);
        let edge = |c: &mut EnumConstants, owner: &[&str], target: &[&str]| {
            c.ancestor_edges.push((owner.iter().map(|s| s.to_string()).collect(), target.iter().map(|s| s.to_string()).collect()));
        };
        edge(&mut constants, &["ROTP::TOTP"], &["ROTP::TOTP::OTP", "ROTP::OTP", "OTP"]);
        edge(&mut constants, &["User"], &["User::ApplicationRecord", "ApplicationRecord"]);
        edge(&mut constants, &["User"], &["User::Concern", "Concern"]);
        edge(&mut constants, &["ApplicationRecord"], &["ApplicationRecord::ActiveRecord::Base", "ActiveRecord::Base"]);
        edge(&mut constants, &["ViewComponent::Base"], &["ViewComponent::Base::Slotable", "ViewComponent::Slotable", "Slotable"]);
        // `include Contactable` in Person finds ApplicationRecord's through the superclass.
        edge(&mut constants, &["Person"], &["Person::ApplicationRecord", "ApplicationRecord"]);
        edge(&mut constants, &["Person"], &["Person::Contactable", "Contactable"]);
        let user = constants.ancestor_names("User");
        assert!(!user.contains("URI") && !user.contains("Slot"));
        assert!(user.contains("Shade") && user.contains("Inner"));
        assert!(constants.ancestor_names("ROTP::TOTP").contains("URI"));
        assert!(constants.ancestor_names("ViewComponent::Base").contains("Slot"));
        assert!(constants.ancestor_names("Person").contains("Mark"));
        constants.unknown_ancestors.push(vec!["Concern".into()]);
        assert!(constants.ancestor_names("User").contains("URI"));
    }

    #[test]
    fn a_repeated_ask_shares_the_sum() {
        let mut constants = EnumConstants::default();
        record(&mut constants, &["A::B"]);
        assert!(std::rc::Rc::ptr_eq(&constants.nested_names(), &constants.nested_names()));
    }
}
