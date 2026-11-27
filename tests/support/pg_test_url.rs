//! `ROUNDHOUSE_PG_TEST_URL`: the PostgreSQL server a test runs against.
//!
//! A test that needs PostgreSQL asks `pg_test_db(name)` for it. With the
//! variable set, it gets the URL and a schema name of its own, unique to
//! the test and the process; the program under test creates that schema
//! and drops it when it finishes (the runtime under test is the only
//! PostgreSQL client a test has), and a run that dies part way leaves a
//! schema no later run will reuse. With the variable unset the test
//! prints why and passes without running, so `cargo test` needs no
//! database; with `ROUNDHOUSE_PG_TEST_REQUIRED` also set, as CI sets it
//! wherever it provides the server, an unset URL fails the test instead
//! of skipping it.
//!
//! CI provides the PostgreSQL 17 service of the `spinel-framework` job:
//!
//!   ROUNDHOUSE_PG_TEST_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres
//!
//! Locally, any server a URL can reach without TLS:
//!
//!   ROUNDHOUSE_PG_TEST_URL=postgres://postgres@127.0.0.1:5432/postgres \
//!   cargo test --test spinel_pg_db -- --ignored --nocapture
#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};

pub const URL_VAR: &str = "ROUNDHOUSE_PG_TEST_URL";
pub const REQUIRED_VAR: &str = "ROUNDHOUSE_PG_TEST_REQUIRED";

/// The server and the disposable schema one test runs in.
pub struct PgTestDb {
    pub url: String,
    pub schema: String,
}

/// The test's database, or `None` (after saying so on stderr) when
/// `ROUNDHOUSE_PG_TEST_URL` is unset and the run does not require it.
pub fn pg_test_db(test: &str) -> Option<PgTestDb> {
    let url = std::env::var(URL_VAR).unwrap_or_default();
    if url.is_empty() {
        let required = std::env::var(REQUIRED_VAR).is_ok_and(|v| !v.is_empty());
        assert!(
            !required,
            "{test}: {REQUIRED_VAR} is set but {URL_VAR} is not; \
             set {URL_VAR} to a PostgreSQL URL (postgres://user:password@host:port/database)"
        );
        eprintln!(
            "skipped {test}: {URL_VAR} is unset; set it to a PostgreSQL URL \
             (postgres://user:password@host:port/database) to run this test"
        );
        return None;
    }
    Some(PgTestDb {
        url,
        schema: schema_name(test),
    })
}

/// `rh_<test>_<pid>_<n>`: lowercase, unquoted-identifier safe, and
/// within PostgreSQL's 63-byte limit, with room for a `_own` suffix.
pub fn schema_name(test: &str) -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let suffix = format!("_{}_{n}", std::process::id());
    let mut stem: String = test
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    stem.truncate(63 - "rh_".len() - suffix.len() - "_own".len());
    format!("rh_{stem}{suffix}")
}
