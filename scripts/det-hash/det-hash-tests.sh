#!/bin/bash
# The integration tests whose types must match the analyzer's det maps (ClassInfo tables,
# ViewSeeds fields, engine route sources). Same rewrite as det-hash-swap.sh, with the
# library's path (`roundhouse::det`). Run from the repository root, after det-hash-swap.sh.
set -euo pipefail
files="tests/roundtrip.rs tests/inference_on_spinel_blog_runtime_with_rbs.rs tests/runtime_block_signatures.rs
tests/routes_draw.rs tests/ingest.rs tests/runtime_src_integration.rs tests/tableless_model_attr_surface.rs
tests/routes_literal_each.rs tests/routes_conditional_gap.rs tests/diagnostics_reporting.rs
tests/controller_helper_method.rs tests/container_narrowing.rs"
perl "$(dirname "$(readlink -f "$0")")/det-hash-swap.pl" roundhouse $files
# ...except the in-memory file tree handed to `ingest_app_from_tree`, which is FileTree (std).
perl -pi -e 's/(?<![:\w])(?:roundhouse::det::)?HashMap<(?:(?:std::path::)?PathBuf), Vec<u8>>/roundhouse::vfs::FileTree/g' $files
# per-function `use` lines the tree annotations made redundant
perl -ni -e 'print unless /^    use roundhouse::det::HashMap;$/' tests/ingest.rs
