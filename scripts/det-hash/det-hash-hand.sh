#!/bin/bash
# Hand-edited sites that det-hash-swap.sh cannot know about (each is a regex, not a line number).
# Run after det-hash-swap.sh, from the repository root.
#
# Policy: library maps are crate::det::{HashMap,HashSet}. The in-memory path→bytes tree
# handed to MapVfs / ingest_app_from_tree is crate::vfs::FileTree (std). CachedVfs::facts
# stays on det — do NOT strip HashMap from the vfs det import.
set -euo pipefail
sub(){ perl -pi -e "$2" "$1"; }

# `with_capacity` only exists for RandomState.
sub src/analyze/mod.rs 's/HashMap::with_capacity\(self\.classes\.len\(\)\)/HashMap::with_capacity_and_hasher(self.classes.len(), Default::default())/'

# `Alias::new()` where `type Alias = HashMap<..>`: `new` only exists for RandomState.
sub src/ingest/app.rs                    's/(TablePrefixes|HashMap::<crate::ident::Symbol, usize>)::new\(\)/$1::default()/'
sub src/ingest/class_macros.rs           's/\bEnv::new\(\)/Env::default()/'
sub src/ingest/delegate_declarations.rs  's/\bCallsWithArguments::new\(\)/CallsWithArguments::default()/'
sub src/ingest/model/table_prefix.rs     's/\bTablePrefixes::new\(\)/TablePrefixes::default()/'
sub src/ingest/model_macros.rs           's/(HashMap::<Symbol, usize>)::new\(\)/$1::default()/'
sub src/ingest/sorbet_sig.rs             's/\bMethodSides::new\(\)/MethodSides::default()/'
sub src/lower/controller_to_library/mod.rs 's/\bTextFormatActions::new\(\)/TextFormatActions::default()/'
sub src/lower/view_to_library/mod.rs     's/\bIvarLocals::new\(\)/IvarLocals::default()/'
sub src/rbs.rs                           's/\bAliasTable::new\(\)/AliasTable::default()/'
sub src/runtime_src.rs                   's/\b(ConstantTypes|OwnedConstantTypes)::new\(\)/$1::default()/'
sub src/lower/scope_chain.rs             's/\b(ScopeRegistry|UniqueKeys|AssocClassMethods)::new\b/$1::default/g'

# MapVfs / ingest_app_from_tree take FileTree (std). Keep det::{HashMap,HashSet} on vfs.rs
# so CachedVfs::facts stays deterministic.
sub src/vfs.rs 's/^(    files: |    pub fn new\(files: )HashMap<PathBuf, Vec<u8>>/$1FileTree/'
sub src/ingest/app.rs 's/^(pub fn ingest_app_from_tree\(tree: )HashMap<PathBuf, Vec<u8>>/$1crate::vfs::FileTree/'
# ...so test / helper code that feeds the tree pins FileTree.
sub src/ide.rs 's/(?:crate::det::)?HashMap<(?:std::path::)?PathBuf, Vec<u8>>/crate::vfs::FileTree/g'
sub src/analyze/fixpoint_check.rs 's/let tree = HashMap::from_iter\(/let tree = crate::vfs::FileTree::from_iter(/'
sub src/lower/model_to_library/mod.rs 's/\.collect::<(?:crate::det::)?HashMap<_, _>>\(\)/.collect::<crate::vfs::FileTree>()/'
sub src/lower/jbuilder_to_library/record_partial.rs 's/let files: HashMap<PathBuf, Vec<u8>>/let files: crate::vfs::FileTree/'

# lookup_tests compiles slices of library.rs with a bare rustc; give its stand-in crate a `det`
# module (std under that name). Construct with ::default so the surface matches real det.
sub src/emit/ruby/lookup_tests.rs 's/^(\s*)use std::collections::\{HashMap, HashSet, BTreeSet\};$/$1use std::collections::BTreeSet;\n$1mod det { pub use std::collections::{HashMap, HashSet}; }\n$1use det::{HashMap, HashSet};/'
sub src/emit/ruby/lookup_tests.rs 's/HashSet::new\(\)/HashSet::default()/g'
