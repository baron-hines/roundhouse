//! `Model.transaction` runs every statement of its block on one
//! connection, and a nested `transaction` joins the outer one, on the
//! spinel lane.
//!
//! Outside a `Db.with_connection` lease (scripts, runner, tests, boot)
//! `Db.current_conn` falls back to the pool's first connection, and the
//! BEGIN runs there. A lease taken inside that transaction — what
//! `Rails.application.executor.wrap`, a cable broadcast or a job does —
//! used to check out a DIFFERENT connection: its writes either waited
//! out the write permit and failed with "database is locked", or, on a
//! one-connection shard, ran on the transaction's own connection and
//! then rolled it back from the lease's cleanup, so the rest of the
//! block autocommitted and a later ROLLBACK undid nothing. A nested
//! `transaction` raised "cannot start a transaction within a
//! transaction" where Rails joins the outer one. `runtime/spinel/db_pg.rb`
//! already pins a bare transaction's connection (roundhouse#585); this
//! is the same rule for SQLite.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const PROBE: &str = r#"
def check(name, ok, detail)
  puts (ok ? "ok " : "FAIL ") + name + (ok ? "" : " (" + detail + ")")
end

# The block's update and create both roll back and its error propagates.
def rollback_case(name, mode)
  a = Article.create!(title: "before", body: "long enough body")
  n0 = Article.count
  msg = ""
  begin
    Article.transaction do
      if mode == "nested"
        Article.transaction { a.update!(title: "during") }
      else
        a.update!(title: "during")
      end
      if mode == "lease"
        Db.with_connection { Article.create!(title: "inner", body: "long enough body") }
      else
        Article.create!(title: "inner", body: "long enough body")
      end
      raise "boom"
    end
  rescue => e
    msg = e.message
  end
  title = Article.find(a.id).title
  delta = Article.count - n0
  check(name, msg == "boom" && title == "before" && delta == 0,
        "error=" + msg + " title=" + title + " delta=" + delta.to_s)
end

# Nested blocks that finish commit once, with the outer one.
def commit_case(name)
  a = Article.create!(title: "before", body: "long enough body")
  Article.transaction do
    Article.transaction { a.update!(title: "nested") }
    Db.with_connection { Article.create!(title: "leased", body: "long enough body") }
  end
  title = Article.find(a.id).title
  check(name, title == "nested" && Db.in_lease? == false, "title=" + title)
end
"#;

fn script(pool_size: usize) -> String {
    format!(
        "Db.configure(\"txn.sqlite3\", pool_size: {pool_size})\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\nDb.write_permit_timeout = 0.2\n{PROBE}\n\
         rollback_case(\"bare\", \"flat\")\n\
         Db.with_connection {{ rollback_case(\"in a request lease\", \"flat\") }}\n\
         rollback_case(\"lease inside a bare transaction\", \"lease\")\n\
         rollback_case(\"nested, bare\", \"nested\")\n\
         Db.with_connection {{ rollback_case(\"nested, in a request lease\", \"nested\") }}\n\
         commit_case(\"nested and leased blocks commit with the outer one\")\n\
         puts \"done\"\n"
    )
}

fn assert_all_ok(pool_size: usize) {
    let run = emit_and_run::real_blog().run_spinel(&script(pool_size));
    run.assert_passes();
    let out = &run.stdout;
    assert!(out.lines().any(|l| l == "done"), "driver did not finish\n{out}\n{}", run.stderr);
    let failed: Vec<&str> = out.lines().filter(|l| l.starts_with("FAIL")).collect();
    assert!(failed.is_empty(), "pool_size {pool_size}:\n{}\n=== stdout ===\n{out}", failed.join("\n"));
    assert_eq!(out.lines().filter(|l| l.starts_with("ok ")).count(), 6, "{out}");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_transaction_keeps_its_connection_on_spinel() {
    assert_all_ok(8);
}

/// A one-connection shard: the lease inside the transaction gets the
/// transaction's own connection back.
#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_transaction_keeps_its_connection_on_a_one_connection_pool() {
    assert_all_ok(1);
}

/// The nested join is the shared ruby-family `transaction`, so the CRuby
/// lane runs it too (its lease cases are the CRuby shim's own business).
#[test]
fn a_nested_transaction_joins_the_outer_one_on_cruby() {
    let script = format!(
        "{PROBE}\nrollback_case(\"bare\", \"flat\")\nrollback_case(\"nested, bare\", \"nested\")\n\
         a = Article.create!(title: \"before\", body: \"long enough body\")\n\
         Article.transaction {{ Article.transaction {{ a.update!(title: \"nested\") }} }}\n\
         check(\"nested commit\", Article.find(a.id).title == \"nested\", \"\")\nputs \"done\"\n"
    );
    let run = emit_and_run::real_blog().run_ruby(&script);
    run.assert_passes();
    let out = &run.stdout;
    assert!(out.lines().any(|l| l == "done"), "{out}");
    assert!(!out.contains("FAIL"), "{out}");
    assert_eq!(out.lines().filter(|l| l.starts_with("ok ")).count(), 3, "{out}");
}
