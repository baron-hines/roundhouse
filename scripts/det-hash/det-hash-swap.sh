#!/bin/bash
# Mechanical part of "switch the tree to det::HashMap/HashSet". Run from the repository root on a
# clean tree; see det-hash-swap.pl for the rewrites. Then det-hash-hand.sh, then det-hash-tests.sh.
set -euo pipefail
here=$(dirname "$(readlink -f "$0")")
# Left on std: det.rs itself, the separate bin crates, and modules whose only HashMap is a
# cfg(test) FileTree builder (hand.sh / tip already pin those to FileTree — they are still
# excluded so a bare swap does not rewrite comments that mention std HashMap in prose).
# record_partial is NOT excluded: production RecordSites must land on det.
files=$(git ls-files 'src/*.rs' | grep -v -e '^src/det.rs$' -e '^src/bin/' \
  -e '^src/ingest/action_cable_config.rs$' -e '^src/ingest/public_file_server_config.rs$' \
  -e '^src/lower/string_core_ext.rs$' \
  -e '^src/emit/rust/expr/send/dispatch.rs$')
perl "$here/det-hash-swap.pl" crate $files
