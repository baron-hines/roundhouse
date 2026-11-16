//! `lock!` / `with_lock` take a row lock on PostgreSQL, as Rails does.
//!
//! Rails' `lock!` reloads with `SELECT … FOR UPDATE` (or the clause it
//! is given), so a second `with_lock` on the same row waits for the
//! first transaction to commit and then reads its write. SQLite has no
//! row locks and its single writer already serializes the transaction
//! (tests/pessimistic_locking.rs); on PostgreSQL the lock is what makes
//! a read-check-write inside `with_lock` safe. These run on the Ruby
//! target; the Spinel tree ships the same runtime (connection.rb's
//! reopen), checked in tests/postgresql_spinel_target.rs.
//!
//! The SQLite case needs nothing. The PostgreSQL probes compare the
//! emitted program with native Rails on the server ROUNDHOUSE_PG_TEST_URL
//! names, and pass without running when it is unset
//! (tests/support/pg_probe.rs).
#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/pg_probe.rs"]
mod pg_probe;

/// On SQLite `lock!` is the reload it always was: one SELECT, no lock
/// clause, whatever clause it is given.
#[test]
fn lock_adds_no_statement_or_clause_on_sqlite() {
    let app = emit_and_run::empty_app()
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        )
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 1) do\n  create_table \"widgets\", force: :cascade do |t|\n    t.string \"name\"\n    t.integer \"revision\", default: 1, null: false\n  end\nend\n",
        )
        .write("app/models/widget.rb", "class Widget < ApplicationRecord\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n");
    let run = app.run_ruby(
        r#"
widget = Widget.create!(name: "a")
plain = Db.capture_sql { widget.lock! }
shared = Db.capture_sql { widget.lock!("FOR SHARE") }
block = Db.capture_sql { widget.with_lock { widget.revision } }
locking = (plain + shared + block).select { |sql| sql.include?(" FOR ") }
raise "SQLite took a row lock: #{locking.inspect}" unless locking.empty?
raise "lock! should be one SELECT on SQLite: #{plain.inspect}" unless plain.length == 1
raise "lock!(String) should be one SELECT on SQLite: #{shared.inspect}" unless shared.length == 1
puts "sqlite lock! takes no row lock"
"#,
    );
    run.assert_passes();
    assert!(run.stdout.contains("sqlite lock! takes no row lock"), "{}", run.stdout);
}

const STRUCTURE: &str = r#"CREATE TABLE articles (
    id bigserial NOT NULL,
    title character varying,
    revision integer DEFAULT 1 NOT NULL,
    CONSTRAINT articles_pkey PRIMARY KEY (id)
);
"#;

const DATABASE_YML: &str = r#"default: &default
  adapter: postgresql
  encoding: unicode

test:
  <<: *default
  database: rh_spec_row_locking
"#;

const APPLICATION_RECORD: &str = r#"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"#;

const ARTICLE: &str = r#"class Article < ApplicationRecord
end
"#;

/// The lock clause each form sends, counted off `sql.active_record`.
const SQL_PROBE: &str = r##"class LockSqlProbe
  def self.clauses
    sql = []
    sub = ActiveSupport::Notifications.subscribe("sql.active_record") { |*, payload| sql << payload[:sql] }
    yield
    ActiveSupport::Notifications.unsubscribe(sub)
    "update=#{sql.count { |s| s.include?("FOR UPDATE") }} share=#{sql.count { |s| s.include?("FOR SHARE") }}"
  end

  def self.lines
    out = []
    article = Article.create!(title: "a", revision: 1)
    Article.where(id: article.id).update_all(title: "b")
    out << "lock!: #{clauses { Article.transaction { article.lock! } }} title=#{article.title}"
    out << "with_lock: #{clauses { article.with_lock { article.revision } }}"
    out << "with_lock share: #{clauses { article.with_lock("FOR SHARE") { article.revision } }}"
    out << "lock! nowait: #{clauses { Article.transaction { article.lock!("FOR UPDATE NOWAIT") } }}"
    out << "lock! false: #{clauses { Article.transaction { article.lock!(false) } }}"
    out << "new record: #{clauses { Article.new(title: "c").lock! }}"
    out
  end
end
"##;

const SQL_EXPECTED: [&str; 6] = [
    "lock!: update=1 share=0 title=b",
    "with_lock: update=1 share=0",
    "with_lock share: update=0 share=1",
    "lock! nowait: update=1 share=0",
    "lock! false: update=0 share=0",
    "new record: update=0 share=0",
];

/// Two clients edit the same article from the revision they both read
/// (1). The first holds `with_lock` while it decides; the second's
/// `with_lock` waits for it, then sees revision 2 and refuses, the 409
/// a Rails app answers. Without the row lock both read revision 1 and
/// both write.
const RACE_PROBE: &str = r##"class LockRaceProbe
  def self.edit(id, seen, title, events, locked, pause)
    Db.with_connection do
      article = Article.find(id)
      article.with_lock do
        events << "#{title} reads revision #{article.revision}"
        locked << true
        sleep(pause)
        if article.revision == seen
          article.update!(title: title, revision: article.revision + 1)
          "updated"
        else
          "conflict"
        end
      end
    end
  end

  def self.lines
    article = Article.create!(title: "draft", revision: 1)
    events = Queue.new
    locked = Queue.new
    first = Thread.new { edit(article.id, 1, "first", events, locked, 0.5) }
    locked.pop
    second = Thread.new { edit(article.id, 1, "second", events, locked, 0) }
    results = [first.value, second.value]
    order = []
    order << events.pop until events.empty?
    article.reload
    ["results: #{results.join(" ")}", "order: #{order.join(", ")}", "row: #{article.title} r#{article.revision}"]
  end
end
"##;

const RACE_EXPECTED: [&str; 3] = [
    "results: updated conflict",
    "order: first reads revision 1, second reads revision 2",
    "row: first r2",
];

/// Native Rails has no `Db`: the emitted runtime's lease is Rails'
/// `with_connection`, which a thread gets on its own anyway.
const NATIVE_PRELUDE: &str = r#"module Db
  def self.with_connection
    ActiveRecord::Base.connection_pool.with_connection { yield }
  end
end
"#;

fn files(probe_path: &'static str, probe: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![
        ("roundhouse.yml", ""),
        ("config/database.yml", DATABASE_YML),
        ("config/routes.rb", "Rails.application.routes.draw do\nend\n"),
        ("db/structure.sql", STRUCTURE),
        ("app/models/application_record.rb", APPLICATION_RECORD),
        ("app/models/article.rb", ARTICLE),
        (probe_path, probe),
    ]
}

fn probe(name: &str, files: &[(&str, &str)], database: &str, class: &str, probe_load: &str) -> Option<(Vec<String>, Vec<String>)> {
    let app = files.iter().fold(emit_and_run::empty_app(), |app, (path, text)| app.write(path, text));
    let (emitted, errors) = app.emit(roundhouse::project::BuildTarget::Ruby);
    let real: Vec<_> = errors.iter().filter(|e| !e.contains("database_adapter not supported")).collect();
    assert!(real.is_empty(), "{real:?}");
    pg_probe::native_and_emitted(
        name,
        files,
        &emitted,
        database,
        class,
        &["app/models/application_record", "app/models/article", probe_load],
    )
}

#[test]
fn lock_sends_rails_lock_clause_on_postgresql() {
    let files = files("app/models/lock_sql_probe.rb", SQL_PROBE);
    let Some((native, emitted)) = probe("row-lock-sql", &files, "rh_spec_row_lock_sql", "LockSqlProbe", "app/models/lock_sql_probe")
    else {
        return;
    };
    assert_eq!(native, SQL_EXPECTED, "native");
    assert_eq!(emitted, native, "emitted");
}

#[test]
fn concurrent_with_lock_waits_for_the_holder_on_postgresql() {
    let mut files = files("app/models/lock_race_probe.rb", RACE_PROBE);
    files.push(("native_prelude.rb", NATIVE_PRELUDE));
    let Some((native, emitted)) =
        probe("row-lock-race", &files, "rh_spec_row_lock_race", "LockRaceProbe", "app/models/lock_race_probe")
    else {
        return;
    };
    assert_eq!(native, RACE_EXPECTED, "native");
    assert_eq!(emitted, native, "emitted");
}
