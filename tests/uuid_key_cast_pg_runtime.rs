//! PostgreSQL: a value bound to a `uuid` column is cast as Rails casts it
//! (OID::Uuid#cast_value), on the Ruby target against native Rails.
//!
//! Rails answers a String that matches `ACCEPTABLE_UUID` (with or without
//! hyphens, optionally in braces) in canonical form, and anything else as
//! nil, bound as NULL. Sent as written, PostgreSQL rejects the literal
//! (`invalid input syntax for type uuid`), so a blank or malformed id
//! raised StatementInvalid instead of finding nothing (#2166). A probe
//! needs ROUNDHOUSE_PG_TEST_URL and the pg gem (tests/support/pg_probe.rs).
#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/pg_probe.rs"]
mod pg_probe;

const STRUCTURE: &str = r#"CREATE TABLE widgets (
    id uuid DEFAULT gen_random_uuid() NOT NULL PRIMARY KEY,
    name character varying,
    owner_id uuid
);
"#;

const DATABASE_YML: &str = r#"default: &default
  adapter: postgresql
  encoding: unicode

test:
  <<: *default
  database: rh_spec_uuid_key_cast
"#;

const APPLICATION_RECORD: &str = r#"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"#;

const WIDGET: &str = "class Widget < ApplicationRecord\nend\n";

const PROBE: &str = r##"class UuidProbe
  REAL = "11111111-1111-4111-8111-111111111111"
  OWNER = "22222222-2222-4222-8222-222222222222"

  def self.try
    yield.inspect
  rescue StandardError => e
    e.class.to_s
  end

  def self.lines
    Widget.create!(id: REAL, name: "sprocket", owner_id: OWNER)
    Widget.create!(id: "33333333-3333-4333-8333-333333333333", name: "cog")
    [
      "find_by blank: #{try { Widget.find_by(id: "") }}",
      "find_by junk: #{try { Widget.find_by(id: "not-a-uuid") }}",
      "where blank count: #{try { Widget.where(id: "").count }}",
      "exists blank: #{try { Widget.exists?(id: "") }}",
      "find blank: #{try { Widget.find("") }}",
      "find_by real: #{try { Widget.find_by(id: REAL)&.name }}",
      "find junk message: #{begin; Widget.find("not-a-uuid"); rescue ActiveRecord::RecordNotFound => e; e.message; end}",
      "exists key blank: #{try { Widget.exists?("") }}",
      "where not blank count: #{try { Widget.where.not(id: "").count }}",
      "find braces: #{try { Widget.find("{11111111-1111-4111-8111-111111111111}").name }}",
      "find bare hex: #{try { Widget.find("11111111111141118111111111111111").name }}",
      "find_by upper: #{try { Widget.find_by(id: REAL.upcase)&.name }}",
      "find_by unpaired brace: #{try { Widget.find_by(id: "{" + REAL) }}",
      "find_by integer: #{try { Widget.find_by(id: 5) }}",
      "where list blank and real: #{try { Widget.where(id: ["", REAL]).pluck(:name) }}",
      "where list one blank: #{try { Widget.where(id: [""]).count }}",
      "where list all junk: #{try { Widget.where(id: ["", "x"]).count }}",
      "where list junk and nil: #{try { Widget.where(id: ["", nil]).count }}",
      "where not list all junk: #{try { Widget.where.not(id: ["", "x"]).count }}",
      "where list braces: #{try { Widget.where(id: ["{11111111-1111-4111-8111-111111111111}", "x"]).pluck(:name) }}",
      "where other uuid column blank: #{try { Widget.where(owner_id: "").count }}",
      "where other uuid column bare hex: #{try { Widget.where(owner_id: "22222222222242228222222222222222").pluck(:name) }}",
      "relation find_by blank: #{try { Widget.where(name: "sprocket").find_by(id: "") }}",
      "relation exists blank: #{try { Widget.where(name: "sprocket").exists?(id: "") }}",
      "relation find blank: #{try { Widget.where(name: "sprocket").find("") }}",
      "relation find junk: #{try { Widget.all.find("not-a-uuid") }}",
      "relation find bare hex: #{try { Widget.where(name: "sprocket").find("11111111111141118111111111111111").name }}",
    ]
  end
end
"##;

/// Native Rails 8.1.4's answers on PostgreSQL 18.
const EXPECTED: [&str; 27] = [
    "find_by blank: nil",
    "find_by junk: nil",
    "where blank count: 0",
    "exists blank: false",
    "find blank: ActiveRecord::RecordNotFound",
    r#"find_by real: "sprocket""#,
    r#"find junk message: Couldn't find Widget with 'id'="not-a-uuid""#,
    "exists key blank: false",
    "where not blank count: 0",
    r#"find braces: "sprocket""#,
    r#"find bare hex: "sprocket""#,
    r#"find_by upper: "sprocket""#,
    "find_by unpaired brace: nil",
    "find_by integer: nil",
    r#"where list blank and real: ["sprocket"]"#,
    "where list one blank: 0",
    "where list all junk: 0",
    "where list junk and nil: 0",
    "where not list all junk: 0",
    r#"where list braces: ["sprocket"]"#,
    "where other uuid column blank: 0",
    r#"where other uuid column bare hex: ["sprocket"]"#,
    "relation find_by blank: nil",
    "relation exists blank: false",
    "relation find blank: ActiveRecord::RecordNotFound",
    "relation find junk: ActiveRecord::RecordNotFound",
    r#"relation find bare hex: "sprocket""#,
];

const LOADS: [&str; 3] = ["app/models/application_record", "app/models/widget", "app/models/uuid_probe"];

const FILES: [(&str, &str); 7] = [
    ("roundhouse.yml", ""),
    ("config/database.yml", DATABASE_YML),
    ("config/routes.rb", "Rails.application.routes.draw do\nend\n"),
    ("db/structure.sql", STRUCTURE),
    ("app/models/application_record.rb", APPLICATION_RECORD),
    ("app/models/widget.rb", WIDGET),
    ("app/models/uuid_probe.rb", PROBE),
];

fn overlay() -> emit_and_run::Overlay {
    FILES.iter().fold(emit_and_run::empty_app(), |app, (path, text)| app.write(path, text))
}

/// `check`'s errors other than the PostgreSQL adapter notice every
/// PostgreSQL probe app carries.
fn real_errors(errors: &[String]) -> Vec<&String> {
    errors.iter().filter(|e| !e.contains("database_adapter not supported")).collect()
}

/// Key lookups and hash conditions on a uuid column, natively and
/// emitted, same PostgreSQL.
#[test]
fn a_uuid_value_is_cast_like_native_rails() {
    let (emitted, errors) = overlay().emit(roundhouse::project::BuildTarget::Ruby);
    let Some((native, emitted_lines)) = pg_probe::native_and_emitted(
        "uuid-key-cast",
        &FILES,
        &emitted,
        "rh_spec_uuid_key_cast",
        "UuidProbe",
        &LOADS,
    ) else { return };
    // Not a vacuous agreement: Rails' own answers.
    assert_eq!(native, EXPECTED, "native");
    assert!(real_errors(&errors).is_empty(), "{}", errors.join("\n"));
    assert_eq!(emitted_lines, native, "emitted");
}

/// Without a database: the emitted model names its uuid columns for the
/// Relation's cast, and its key lookups cast before the adapter.
#[test]
fn the_emitted_model_casts_its_uuid_columns() {
    let (emitted, errors) = overlay().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(real_errors(&errors).is_empty(), "{}", errors.join("\n"));
    let model = std::fs::read_to_string(emitted.join("app/models/widget.rb")).expect("emitted model");
    assert!(model.contains("def self.schema_uuid_query_columns"), "the uuid columns are listed:\n{model}");
    assert!(model.contains("[:id, :owner_id]"), "both uuid columns:\n{model}");
    assert_eq!(
        model.matches("ActiveRecord::UuidCast.cast_text(id)").count(),
        2,
        "find and exists? cast the key:\n{model}"
    );
}
