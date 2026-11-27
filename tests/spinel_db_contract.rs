//! The Db contract suite on the Spinel SQLite shim (runtime/spinel/db.rb).
//!
//! db.rb's sqlite3 calls are Spinel `ffi_func`s with no CRuby spelling,
//! so the lane compiles the suite with Spinel. The suite and its lanes
//! are described in tests/support/db_contract.rs; tests/db_contract.rs
//! runs the CRuby lanes and tests/spinel_pg_db.rs the PostgreSQL one.
//!
//!   SPINEL=/path/to/spinel cargo test --test spinel_db_contract -- --ignored --nocapture

#[path = "support/db_contract.rs"]
mod db_contract;

use db_contract::{Backend, Runtime, lanes};

#[test]
#[ignore = "requires the Spinel toolchain"]
fn spinel_sqlite_shims_pass_the_contract() {
    let mut ran = 0;
    for lane in lanes(Runtime::Spinel, Backend::Sqlite) {
        db_contract::run_lane(lane);
        ran += 1;
    }
    assert!(ran > 0, "no Spinel SQLite lane");
}
