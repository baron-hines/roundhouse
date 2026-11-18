//! An app whose connection class routes its models by role and shard
//! across SQLite files, as `connects_to shards:` and `connects_to
//! database:` declare it, and the reports it prints.
//!
//! Each `*_EXPECTED` is what Rails 8.1.4 prints for the same statements
//! (Ruby 4.0.7, sqlite3 2.9.4, `load_defaults 8.1`, the default `:thread`
//! isolation level, the roles `writing` and `reading`), each app
//! initialized with the `config/database.yml` below. Where Rails' line
//! carries a statement, the emitted SQL writes its values in where Rails
//! binds them, so the lines are compared up to the statement's first
//! word ([`comparable`]).
//!
//! Both files hold a `notes` table with the same ids, so a pool, a
//! statement cache or a query cache shared between them would show.
#![allow(dead_code)]

use super::emit_and_run;

const SCHEMA: &str = "ActiveRecord::Schema[8.1].define(version: 1) do\n  create_table :notes, force: true do |t|\n    t.string :tag, null: false\n  end\nend\n";

pub const SHARDS: &str = "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\n  connects_to shards: {\n    left: { writing: :left, reading: :left_reader },\n    right: { writing: :right, reading: :right_reader }\n  }\nend\n";
/// `ROLES` with a reader that is not a replica, so only the scope's guard refuses a write.
pub fn roles_plain() -> String {
    ROLES.replace("reading: :left_reader", "reading: :left_plain")
}
pub const ROLES: &str = "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\n  connects_to database: { writing: :left, reading: :left_reader }\nend\n";

/// `config/database.yml`: a primary no model uses, two writable files
/// that name the app's schema, and a replica configuration on each.
pub fn database_yml() -> String {
    let mut yml = String::new();
    for env in ["development", "test"] {
        yml.push_str(&format!(
            "{env}:\n  primary:\n    adapter: sqlite3\n    database: storage/primary.sqlite3\n  left:\n    adapter: sqlite3\n    database: storage/left.sqlite3\n    schema_dump: schema.rb\n  right:\n    adapter: sqlite3\n    database: storage/right.sqlite3\n    schema_dump: schema.rb\n  left_reader:\n    adapter: sqlite3\n    database: storage/left.sqlite3\n    replica: true\n  left_plain:\n    adapter: sqlite3\n    database: storage/left.sqlite3\n  right_reader:\n    adapter: sqlite3\n    database: storage/right.sqlite3\n    replica: true\n"
        ));
    }
    yml
}

/// The app with `record` as its `ApplicationRecord` and `report` as
/// `app/models/route_report.rb`.
pub fn app(record: &str, report: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", record)
        .write("app/models/note.rb", "class Note < ApplicationRecord\nend\n")
        .write("app/models/route_report.rb", report)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("config/database.yml", &database_yml())
        .write("db/schema.rb", SCHEMA)
}

/// What a contract runs before the report: the boot every emitted
/// program runs, on a primary of its own. `Db.configure` opens the
/// routed files and creates their tables.
pub const BOOT: &str = "Dir.mkdir(\"storage\") unless Dir.exist?(\"storage\")\nDb.configure(\"storage/primary.sqlite3\")\nSchema.statements.each { |sql| Db.exec(sql) }\nActiveRecord.adapter = SqliteAdapter\nActiveRecord::Base.connection.execute(\"CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY, tag TEXT NOT NULL)\")\n";

const STATE: &str = r##"  def self.state(label)
    puts "#{label}=#{ApplicationRecord.current_role}/#{ApplicationRecord.current_shard}/#{ApplicationRecord.current_preventing_writes}"
  end

  def self.refused(label)
    yield
    puts "#{label}=NO ERROR"
  rescue ActiveRecord::ReadOnlyError => e
    puts "#{label}=ActiveRecord::ReadOnlyError: #{e.message}"
  rescue ActiveRecord::ConnectionNotDefined => e
    puts "#{label}=ActiveRecord::ConnectionNotDefined: #{e.message}"
  rescue ArgumentError => e
    puts "#{label}=ArgumentError: #{e.message}"
  end
"##;

/// The statements of the Rails reductions `two_files`, `shards` and
/// `errors`, then the cases a shared pool or a lost restoration would
/// fail.
pub fn shards_report() -> String {
    format!(
        r##"class RouteReport
{STATE}
  def self.two_files
    ActiveRecord::Base.connected_to(shard: :left) {{ Note.create!(tag: "left") }}
    ActiveRecord::Base.connected_to(shard: :right) {{ Note.create!(tag: "right") }}
    ActiveRecord::Base.connected_to(shard: :left) {{ puts "left=#{{Note.count}}:#{{Note.first.tag}}" }}
    ActiveRecord::Base.connected_to(shard: :right) {{ puts "right=#{{Note.count}}:#{{Note.first.tag}}" }}
    ActiveRecord::Base.connected_to(shard: :left) {{ puts "left-again=#{{Note.count}}:#{{Note.first.tag}}" }}
  end

  def self.shards
    state("default")
    ActiveRecord::Base.connected_to(shard: :left) do
      puts "left=#{{Note.pluck(:tag).join(',')}}"
      ActiveRecord::Base.connected_to(shard: :right) {{ puts "right=#{{Note.pluck(:tag).join(',')}}" }}
      puts "left-again=#{{Note.count}}:#{{Note.first.tag}}"
      begin
        ActiveRecord::Base.connected_to(role: :reading, shard: :right) do
          state("nested")
          raise "scope exit"
        end
      rescue RuntimeError
        state("after-raise")
      end
      ActiveRecord::Base.connected_to(role: :reading) do
        state("reading-inherits-shard")
        puts "read=#{{Note.count}}"
        refused("write") {{ Note.create!(tag: "must-not-exist") }}
      end
      puts "after-write=#{{Note.count}}"
    end
    state("restored")
  end

  def self.errors
    puts "defaults=#{{ActiveRecord::Base.default_shard}}/#{{ApplicationRecord.default_shard}}"
    puts "implicit-match=#{{ApplicationRecord.connected_to?(role: :writing)}}"
    puts "explicit-match=#{{ApplicationRecord.connected_to?(role: :writing, shard: :left)}}"
    puts "string-match=#{{ApplicationRecord.connected_to?(role: "writing", shard: "left")}}"
    refused("no-selection") {{ ApplicationRecord.connected_to(prevent_writes: true) {{ 1 }} }}
    ApplicationRecord.connected_to(shard: :missing) {{ state("missing-lazy") }}
    refused("missing-shard") {{ ApplicationRecord.connected_to(shard: :missing) {{ Note.count }} }}
    state("restored")
  end

  def self.first_tag(shard)
    ActiveRecord::Base.connected_to(shard: shard) do
      return Note.find(1).tag
    end
    "not reached"
  end

  def self.exits
    puts "returned=#{{first_tag(:right)}}"
    state("after-return")
    [:right, :left].each do |shard|
      ApplicationRecord.connected_to(shard: shard) do
        break if shard == :right
      end
    end
    state("after-break")
    ActiveRecord::Base.connected_to(role: :reading) do
      ActiveRecord::Base.connected_to(role: :writing) do
        state("writing-inside-reading")
        Note.create!(tag: "inner")
      end
      state("reading-again")
    end
    puts "left-rows=#{{Note.order(:id).pluck(:tag).join(',')}}"
  end

  def self.guards
    ApplicationRecord.connected_to(role: :reading, shard: :right) do
      refused("update-all") {{ Note.update_all(tag: "blocked") }}
      refused("delete-all") {{ Note.delete_all }}
      refused("insert-all") {{ Note.insert_all([{{ tag: "blocked" }}]) }}
      refused("update") {{ Note.find(1).update!(tag: "blocked") }}
      refused("destroy") {{ Note.find(1).destroy }}
      puts "read-on-replica=#{{Note.find(1).tag}}"
    end
    # A shard-only block inherits the reading role without its guard; the
    # replica configuration still refuses.
    ApplicationRecord.connected_to(role: :reading) do
      ApplicationRecord.connected_to(shard: :right) do
        state("shard-only")
        refused("replica") {{ Note.create!(tag: "blocked") }}
      end
    end
    [:left, :right].each do |shard|
      ActiveRecord::Base.connected_to(shard: shard) {{ puts "#{{shard}}-unchanged=#{{Note.order(:id).pluck(:tag).join(',')}}" }}
    end
  end

  OWNER = ApplicationRecord
  # Taken when the class loads, before the databases are opened.
  STORED = ApplicationRecord.connection

  def self.record_owner
    ApplicationRecord
  end

  def initialize
    @owner = ApplicationRecord
  end

  def held
    @owner.connection
  end

  # A connection handle stays on the pool it was taken on, however the
  # class reached it: a method's return, an instance variable, a constant.
  def self.handles
    one = "SELECT tag FROM notes WHERE id = 1"
    by_method = record_owner.connection
    by_ivar = new.held
    by_constant = OWNER.lease_connection
    ApplicationRecord.connected_to(shard: :right) do
      puts "outside-used-inside=#{{by_method.select_value(one)}},#{{by_ivar.select_value(one)}},#{{by_constant.select_value(one)}}"
      by_method.execute("INSERT INTO notes(tag) VALUES ('captured-write')")
      puts "taken-inside=#{{record_owner.connection.select_value(one)}},#{{new.held.select_value(one)}}"
      puts "model-inside=#{{Note.find(1).tag}}"
    end
    inside = ApplicationRecord.connected_to(shard: :right) {{ record_owner.connection }}
    puts "inside-used-outside=#{{inside.select_value(one)}}"
    puts "with-connection=#{{Note.with_connection {{ |c| c.select_value(one) }}}}"
    ApplicationRecord.connected_to(role: :reading) do
      refused("handle-write") {{ by_method.execute("INSERT INTO notes(tag) VALUES ('guarded')") }}
    end
    ApplicationRecord.connected_to(shard: :right) do
      puts "taken-at-load=#{{STORED.select_value(one)}}"
      STORED.execute("INSERT INTO notes(tag) VALUES ('stored-write')")
    end
    # Base's own handle is on the primary, under Base's write guard: a
    # reading block on the connection class leaves it writable, one on
    # Base does not.
    base = ActiveRecord::Base.connection
    base.execute("DELETE FROM notes")
    ApplicationRecord.connected_to(role: :reading, shard: :right) do
      base.execute("INSERT INTO notes(tag) VALUES ('base-write')")
      ActiveRecord::Base.connection.execute("INSERT INTO notes(tag) VALUES ('base-write-2')")
    end
    ActiveRecord::Base.connected_to(role: :reading) do
      refused("base-guarded") {{ base.execute("INSERT INTO notes(tag) VALUES ('guarded')") }}
    end
    puts "base-rows=#{{base.select_value("SELECT COUNT(*) FROM notes")}}"
    # Its own transaction is the model's, not Base's.
    found = Note.create_or_find_by(tag: "found")
    puts "create-or-find=#{{found.tag}}"
    right_tags = ActiveRecord::Base.connected_to(shard: :right) {{ Note.order(:id).pluck(:tag).join(',') }}
    puts "tags=#{{Note.order(:id).pluck(:tag).join(',')}}/#{{right_tags}}"
  end

  def self.isolation
    ActiveRecord::Base.connected_to(shard: :right) {{ Note.create!(tag: "right-2") }}
    right_ids = ActiveRecord::Base.connected_to(shard: :right) {{ Note.order(:id).pluck(:id).join(',') }}
    puts "ids=#{{Note.order(:id).pluck(:id).join(',')}}/#{{right_ids}}"
    lock = Mutex.new
    inside = []
    seen = []
    threads = [:left, :right].map do |shard|
      Thread.new do
        ActiveRecord::Base.connected_to(shard: shard) do
          lock.synchronize {{ inside << shard.to_s }}
          waited = 0
          while lock.synchronize {{ inside.length }} < 2 && waited < 500
            sleep(0.01)
            waited += 1
          end
          tag = Note.find(1).tag
          lock.synchronize {{ seen << "#{{shard}}:#{{tag}}:#{{ApplicationRecord.current_shard}}" }}
        end
      end
    end
    threads.each {{ |thread| thread.join }}
    puts "threads=#{{seen.sort.join(',')}}"
    ActiveRecord::Base.connected_to(shard: :right) do
      thread = Thread.new {{ puts "new-thread=#{{ApplicationRecord.current_shard}}:#{{Note.find(1).tag}}" }}
      thread.join
    end
    state("after-threads")
  end
end
"##
    )
}

pub const SHARDS_SCRIPT: &str = r##"RouteReport.two_files
puts "--"
RouteReport.shards
puts "--"
RouteReport.errors
puts "--"
RouteReport.exits
puts "--"
RouteReport.guards
puts "--"
RouteReport.isolation
puts "--"
RouteReport.handles
puts "--"
# One lease (a request's) across both files, with its query cache on:
# the same statement, the same id, each file's own row.
Db.with_connection do
  seen = [Note.find(1).tag]
  ActiveRecord::Base.connected_to(shard: :right) { seen << Note.find(1).tag << Note.find(1).tag }
  seen << Note.find(1).tag
  puts "one-lease=" + seen.join(",")
end
# Rails raises for a role no pool has at the first query, by name.
begin
  ApplicationRecord.connected_to(role: :archive) { Note.count }
  puts "missing-role=NO ERROR"
rescue ActiveRecord::ConnectionNotDefined => e
  puts "missing-role=ActiveRecord::ConnectionNotDefined: " + e.message
end
# One handle read by two threads at once, neither holding a lease: each
# call leases a connection of the handle's pool, so they share none.
shared = Note.connection
reads = [:left, :right].map do |shard|
  Thread.new do
    count = 0
    ApplicationRecord.connected_to(shard: shard) do
      300.times { count += 1 if shared.select_value("SELECT tag FROM notes WHERE id = 1").to_s == "left" }
    end
    count
  end
end
puts "handle-threads=" + reads.map { |thread| thread.value.to_s }.join(",")
# The same handles under a request's lease: taken back on the slot set
# aside, or leased from the handle's pool for the statement.
Db.with_connection do
  left = Note.connection
  right = ActiveRecord::Base.connected_to(shard: :right) { Note.connection }
  ActiveRecord::Base.connected_to(shard: :right) do
    puts "leased-handles=" + left.select_value("SELECT tag FROM notes WHERE id = 1").to_s + "," + right.select_value("SELECT tag FROM notes WHERE id = 1").to_s
  end
  puts "leased-outside=" + right.select_value("SELECT tag FROM notes WHERE id = 1").to_s
end
# Base's own transaction is on the primary, not on the models' database: refused by name when it is called.
begin
  base = ActiveRecord::Base
  base.transaction { Note.create!(tag: "never") }
  puts "base-transaction=NO ERROR"
rescue NotImplementedError => e
  puts "base-transaction=NotImplementedError"
end
# A transaction across two files is not carried: refused by name, and the
# scope outside is as it was.
begin
  ApplicationRecord.transaction do
    ActiveRecord::Base.connected_to(shard: :right) { Note.create!(tag: "never") }
  end
  puts "across=NO ERROR"
rescue NotImplementedError => e
  puts "across=NotImplementedError: " + e.message
end
RouteReport.state("after-across")
# A block whose transaction SQLite has already ended still leaves its scope:
# the rollback at its end fails, and the selection outside is restored.
begin
  ActiveRecord::Base.connected_to(shard: :right) do
    Db.exec("BEGIN")
    SQL.sqlite3_exec(Db.current_conn.dbh, "ROLLBACK", nil, nil, nil)
  end
  puts "ended=left"
rescue StandardError => e
  puts "ended=raised"
end
RouteReport.state("after-ended")
# A relation returned from a block is refused by name, not loaded on another database later.
begin
  ApplicationRecord.connected_to(shard: :right) { Note.where(id: 1) }
  puts "relation=NO ERROR"
rescue NotImplementedError => e
  puts "relation=NotImplementedError: " + e.message
end
RouteReport.state("after-relation")
# A write in a writing block inside a reading one clears the reader's
# cached reads on the same file too, as Rails clears every pool's query
# cache on the thread.
Db.with_connection do
  ApplicationRecord.connected_to(role: :reading) do
    before = Note.count
    ApplicationRecord.connected_to(role: :writing) { Note.create!(tag: "cached") }
    puts "reader-cache=" + before.to_s + "," + Note.count.to_s
  end
end
# `with_connection` holds one connection for its block, as Rails does: a
# raw transaction begun through its handle carries to the next statement.
begin
  Note.with_connection do |c|
    c.execute("BEGIN")
    c.execute("INSERT INTO notes(tag) VALUES ('rolled-back')")
    c.execute("ROLLBACK")
  end
  puts "with-connection-rollback=" + Note.where(tag: "rolled-back").count.to_s
rescue StandardError => e
  puts "with-connection-rollback=" + e.class.name
end
puts "left-final=" + Note.count.to_s + " right-final=" + ActiveRecord::Base.connected_to(shard: :right) { Note.count }.to_s
"##;

/// Rails' `two_files`, `shards` (without its file-name line) and the
/// `errors` lines an emitted program reaches: the three it leaves out
/// are refused when the app is compiled (`database_routes` tests).
pub const TWO_FILES_EXPECTED: &str = "left=1:left\nright=1:right\nleft-again=1:left\n";
pub const SHARDS_EXPECTED: &str = "default=writing/left/false\nleft=left\nright=right\nleft-again=1:left\nnested=reading/right/true\nafter-raise=writing/left/false\nreading-inherits-shard=reading/left/true\nread=1\nwrite=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: INSERT INTO \"notes\" (\"tag\") VALUES (?) RETURNING \"id\"\nafter-write=1\nrestored=writing/left/false\n";
pub const ERRORS_EXPECTED: &str = "defaults=default/left\nimplicit-match=false\nexplicit-match=true\nstring-match=true\nno-selection=ArgumentError: must provide a `shard` and/or `role`.\nmissing-lazy=writing/missing/false\nmissing-shard=ActiveRecord::ConnectionNotDefined: No database connection defined for 'missing' shard.\nrestored=writing/left/false\n";
pub const EXITS_EXPECTED: &str = "returned=right\nafter-return=writing/left/false\nafter-break=writing/left/false\nwriting-inside-reading=writing/left/false\nreading-again=reading/left/true\nleft-rows=left,inner\n";
pub const GUARDS_EXPECTED: &str = "update-all=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: UPDATE \"notes\" SET \"tag\" = ?\ndelete-all=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: DELETE FROM \"notes\"\ninsert-all=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: INSERT INTO \"notes\" (\"tag\") VALUES ('blocked') ON CONFLICT  DO NOTHING RETURNING \"id\"\nupdate=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: UPDATE \"notes\" SET \"tag\" = ? WHERE \"notes\".\"id\" = ?\ndestroy=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: DELETE FROM \"notes\" WHERE \"notes\".\"id\" = ?\nread-on-replica=right\nshard-only=reading/right/false\nreplica=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: INSERT INTO \"notes\" (\"tag\") VALUES (?) RETURNING \"id\"\nleft-unchanged=left,inner\nright-unchanged=right\n";
pub const ISOLATION_EXPECTED: &str = "ids=1,2/1,2\nthreads=left:left:left,right:right:right\nnew-thread=left:left\nafter-threads=writing/left/false\n";
pub const HANDLES_EXPECTED: &str = "outside-used-inside=left,left,left\ntaken-inside=right,right\nmodel-inside=right\ninside-used-outside=right\nwith-connection=left\nhandle-write=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: INSERT INTO notes(tag) VALUES ('guarded')\ntaken-at-load=left\nbase-guarded=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: INSERT INTO notes(tag) VALUES ('guarded')\nbase-rows=2\ncreate-or-find=found\ntags=left,inner,captured-write,stored-write,found/right,right-2\n";
pub const LEASE_EXPECTED: &str = "one-lease=left,right,right,left\nmissing-role=ActiveRecord::ConnectionNotDefined: No database connection defined for 'left' shard and 'archive' role.\nhandle-threads=300,300\nleased-handles=left,right\nleased-outside=right\nbase-transaction=NotImplementedError\nacross=NotImplementedError: connected_to selects another database inside an open transaction; a transaction across databases is not supported yet\nafter-across=writing/left/false\nended=left\nafter-ended=writing/left/false\nrelation=NotImplementedError: connected_to returns a relation, which is not carried yet; load it in the block (to_a)\nafter-relation=writing/left/false\nreader-cache=5,6\nwith-connection-rollback=0\nleft-final=6 right-final=2\n";

/// The statements of the Rails reduction `roles`.
pub fn roles_report() -> String {
    format!(
        r##"class RouteReport
{STATE}
  def self.roles
    Note.create!(tag: "writer")
    state("default")
    puts "default-match=#{{ApplicationRecord.connected_to?(role: :writing)}}"
    ApplicationRecord.while_preventing_writes do
      state("prevent")
      refused("prevent-update") {{ Note.update_all(tag: "blocked") }}
      ApplicationRecord.while_preventing_writes(false) do
        state("nested-false")
        Note.create!(tag: "allowed")
      end
      ApplicationRecord.connected_to(role: :writing) {{ state("nested-writing") }}
      state("prevent-restored")
    end
    ApplicationRecord.connected_to(role: :reading, prevent_writes: false) do
      state("reading-forces-prevent")
      ApplicationRecord.while_preventing_writes(false) {{ state("reading-cannot-disable") }}
      puts "reading-match=#{{ApplicationRecord.connected_to?(role: :reading)}}"
      refused("reading-delete") {{ Note.delete_all }}
    end
    ActiveRecord::Base.connected_to(role: :reading) do
      ActiveRecord::Base.connected_to(role: :writing) do
        state("global-writing-inside-reading")
        Note.create!(tag: "inner-writer")
      end
      state("global-reading-restored")
    end
    begin
      ApplicationRecord.while_preventing_writes {{ raise "restore the write flag" }}
    rescue RuntimeError
      state("prevent-after-raise")
    end
    state("restored")
    puts "rows=#{{Note.order(:id).pluck(:tag).join(',')}}"
    # A shard-only block inherits the reading role and not its guard
    # (Rails records `prevent_writes: false` for it), so on a reader that
    # is not a replica the write goes through, as in Rails.
    ApplicationRecord.connected_to(role: :reading) do
      ApplicationRecord.connected_to(shard: :default) do
        state("shard-only")
        refused("shard-only-write") {{ Note.create!(tag: "shard-only") }}
      end
      state("reading-after")
    end
    puts "rows-after=#{{Note.order(:id).pluck(:tag).join(',')}}"
  end
end
"##
    )
}

pub const ROLES_EXPECTED: &str = "default=writing/default/false\ndefault-match=true\nprevent=writing/default/true\nprevent-update=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: UPDATE \"notes\" SET \"tag\" = ?\nnested-false=writing/default/false\nnested-writing=writing/default/false\nprevent-restored=writing/default/true\nreading-forces-prevent=reading/default/true\nreading-cannot-disable=reading/default/true\nreading-match=true\nreading-delete=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: DELETE FROM \"notes\"\nglobal-writing-inside-reading=writing/default/false\nglobal-reading-restored=reading/default/true\nprevent-after-raise=writing/default/false\nrestored=writing/default/false\nrows=writer,allowed,inner-writer\nshard-only=reading/default/false\nshard-only-write=NO ERROR\nreading-after=reading/default/true\nrows-after=writer,allowed,inner-writer,shard-only\n";

/// After the `roles` report: one connection per pool, a request's lease
/// on the writer, a reading block, and a writing block inside it. The
/// inner block takes back the lease set aside, where a second lease on
/// the writer's only connection would wait on this thread forever.
pub const ONE_CONNECTION_SCRIPT: &str = r##"Db.close
Db.configure("storage/primary.sqlite3", pool_size: 1)
Db.with_connection do
  ApplicationRecord.connected_to(role: :reading) do
    back = ApplicationRecord.connected_to(role: :writing) { Note.count }
    puts "back=" + back.to_s + ":" + Note.count.to_s
  end
  puts "lease=" + Note.count.to_s
end
# Two threads, each holding the only connection of the pool the other
# selects next: one raises after the checkout timeout, where both waited
# on each other forever.
Db.checkout_timeout = 0.3
lock = Mutex.new
holding = []
ended = []
crossed = [:writing, :reading].map do |held|
  Thread.new do
    ApplicationRecord.connected_to(role: held) do
      Db.with_connection do
        lock.synchronize { holding << held.to_s }
        waited = 0
        while lock.synchronize { holding.length } < 2 && waited < 500
          sleep(0.01)
          waited += 1
        end
        begin
          ApplicationRecord.connected_to(role: held == :writing ? :reading : :writing) { Note.count }
          lock.synchronize { ended << "through" }
        rescue ActiveRecord::ConnectionTimeoutError
          lock.synchronize { ended << "timeout" }
        end
      end
    end
  end
end
crossed.each { |thread| thread.join }
# The first to time out frees its connection, so the other may get through.
puts "crossed=" + ended.length.to_s + ":" + (ended.include?("timeout") ? "timeout" : "none")
puts "after-crossed=" + Note.count.to_s
# Every file gets one checkpoint loop: the reader shares its writer's, and
# a file configured after the loops were started gets one at the next call.
module Db
  def self.checkpointed_files
    files = []
    @slots.each { |s| files << s.path if s.permit.checkpointing && !files.include?(s.path) }
    files
  end
end
Db.checkpoint_in_background!
Db.checkpoint_in_background!
puts "loops=" + Db.checkpointed_files.join(",")
Db.close
Db.configure("storage/later.sqlite3", pool_size: 1)
puts "before-call=" + Db.checkpointed_files.join(",")
Db.checkpoint_in_background!
puts "later=" + Db.checkpointed_files.join(",")
"##;
pub const ONE_CONNECTION_EXPECTED: &str = "back=4:4\nlease=4\ncrossed=2:timeout\nafter-crossed=4\nloops=storage/primary.sqlite3,storage/left.sqlite3\nbefore-call=storage/left.sqlite3\nlater=storage/later.sqlite3,storage/left.sqlite3\n";

pub const HELD_ROUTE_SCRIPT: &str = r##"ApplicationRecord.connected_to(shard: :right) do
  ActiveRecord::Base.with_connection do |c|
    Note.create!(tag: "model")
    c.execute("INSERT INTO notes(tag) VALUES ('handle')")
  end
  puts "held-right=" + Note.count.to_s
  ApplicationRecord.while_preventing_writes do
    ActiveRecord::Base.with_connection do |c|
      RouteReport.refused("held-guard") { Note.create!(tag: "guarded") }
      c.execute("INSERT INTO notes(tag) VALUES ('base-writable')")
    end
  end
end
puts "held-primary=" + ActiveRecord::Base.connection.select_value("SELECT COUNT(*) FROM notes").to_s
begin
  ApplicationRecord.connected_to(shard: :right) do
    ActiveRecord::Base.with_connection { raise "restore the route" }
  end
rescue RuntimeError
  puts "held-restored=" + ApplicationRecord.current_shard.to_s
end
"##;
pub const HELD_ROUTE_EXPECTED: &str = "held-right=1\nheld-guard=ActiveRecord::ReadOnlyError: Write query attempted while in readonly mode: INSERT\nheld-primary=2\nheld-restored=left\n";

pub const RAW_SWITCH_SCRIPT: &str = r##"Note.with_connection do |c|
  c.execute("BEGIN")
  c.execute("INSERT INTO notes(tag) VALUES ('raw')")
  puts "raw-same=" + c.select_value("SELECT COUNT(*) FROM notes").to_s
  begin
    ApplicationRecord.connected_to(shard: :right) { Note.create!(tag: "escaped") }
    puts "raw-switch=allowed"
  rescue NotImplementedError
    puts "raw-switch=refused"
  end
  begin
    ActiveRecord::Base.connection.select_value("SELECT COUNT(*) FROM notes")
    puts "raw-handle=allowed"
  rescue NotImplementedError
    puts "raw-handle=refused"
  end
  c.execute("ROLLBACK")
end
puts "raw-left=" + Note.count.to_s
ApplicationRecord.connected_to(shard: :right) { puts "raw-right=" + Note.count.to_s }
refused = 0
["BEGIN", "begin immediate", "/* synthetic */ BEGIN TRANSACTION", "SAVEPOINT synthetic"].each do |sql|
  begin
    ActiveRecord::Base.with_connection { |c| c.execute(sql) }
  rescue NotImplementedError
    refused += 1
  end
end
puts "raw-base-begins=" + refused.to_s
ActiveRecord::Base.with_connection do |c|
  ActiveRecord::Base.connected_to(shard: :default) do
    c.execute("BEGIN")
    c.execute("INSERT INTO notes(tag) VALUES ('scope-raw')")
  end
  puts "raw-scope=" + c.select_value("SELECT COUNT(*) FROM notes WHERE tag = 'scope-raw'").to_s
end
Db.exec("BEGIN")
Note.with_connection { Db.exec("COMMIT") }
puts "borrowed-ended=" + Db.in_lease?.to_s
Db.exec("BEGIN")
Note.with_connection { Note.count }
puts "borrowed-open=" + Db.in_lease?.to_s
Db.exec("ROLLBACK")
pool = Db.pool_for_thread
ActiveRecord::Base.with_connection { Note.with_connection { Db.close } }
puts "closed-lease=" + Db.in_lease?.to_s
index = pool.lease(0.01)
raise "closed pool lost its checkout" unless index == 0
pool.release(index)
Db.configure("storage/primary.sqlite3")
puts "reopened-count=" + Note.count.to_s
"##;
pub const RAW_SWITCH_EXPECTED: &str = "raw-same=1\nraw-switch=refused\nraw-handle=refused\nraw-left=0\nraw-right=0\nraw-base-begins=4\nraw-scope=0\nborrowed-ended=false\nborrowed-open=true\nclosed-lease=false\nreopened-count=0\n";

pub const SNAPSHOT_SCRIPT: &str = r##"Db.with_connection do
  ApplicationRecord.connected_to(role: :reading) do
    Db.read_snapshot_begin
    before = Note.count
    ApplicationRecord.connected_to(role: :writing) { Note.create!(tag: "fresh") }
    puts "snapshot=" + before.to_s + "," + Note.count.to_s
    ApplicationRecord.connected_to(role: :writing) do
      rows = Note.connection.execute("INSERT INTO notes(tag) VALUES ('executed') RETURNING id").rows
      puts "execute-returning=" + rows[0][0].to_s
    end
    puts "execute-uncached=" + Note.where(tag: "executed").count.to_s
    puts "execute-cached=" + Note.count.to_s
    Db.read_snapshot_end
  end
  ApplicationRecord.connected_to(shard: :right, role: :reading) do
    Db.read_snapshot_begin
    Note.count
    ApplicationRecord.connected_to(shard: :left, role: :writing) { Note.create!(tag: "other-file") }
    puts "unrelated-snapshot=" + Db.current_conn.snap.to_s
    Db.read_snapshot_end
  end
end
"##;
pub const SNAPSHOT_EXPECTED: &str = "snapshot=0,1\nexecute-returning=2\nexecute-uncached=1\nexecute-cached=2\nunrelated-snapshot=2\n";

pub const DIRECTORY_RACE_SCRIPT: &str = r##"path = "storage/shared-parent"
lock = Mutex.new
ready = []
results = []
workers = [1, 2].map do |_|
  Thread.new do
    absent = !Dir.exist?(path)
    raise "parent already exists before the barrier" unless absent
    lock.synchronize { ready << true }
    sleep(0.001) while lock.synchronize { ready.length } < 2
    begin
      Db.mkdir_parent(path)
      lock.synchronize { results << "ok" }
    rescue Errno::EEXIST
      lock.synchronize { results << "exists" }
    end
  end
end
workers.each { |worker| worker.join }
raise "parent was not created" unless Dir.exist?(path)
puts "directory-creators=" + results.sort.join(",")
File.write("storage/not-directory", "synthetic")
begin
  Db.mkdir_parent("storage/not-directory")
  puts "file-collision=allowed"
rescue Errno::EEXIST
  puts "file-collision=refused"
end
"##;
pub const DIRECTORY_RACE_EXPECTED: &str = "directory-creators=ok,ok\nfile-collision=refused\n";

/// `text` with each refused statement cut after its first word: Rails'
/// lines carry its SQL with bind marks, an emitted program's the values.
pub fn comparable(text: &str) -> String {
    const MARK: &str = "readonly mode: ";
    text.lines()
        .map(|line| match line.find(MARK) {
            Some(at) => {
                let sql = &line[at + MARK.len()..];
                format!("{}{}\n", &line[..at + MARK.len()], sql.split(' ').next().unwrap_or(""))
            }
            None => format!("{line}\n"),
        })
        .collect()
}
