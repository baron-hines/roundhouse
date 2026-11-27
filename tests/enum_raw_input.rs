//! Rails answers `<enum>_before_type_cast` with what was assigned until
//! the record saves or loads, and the stored value after (activerecord
//! 8.1.4). The ruby family keeps the assigned value; strict targets refuse.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

fn raw_state_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "create_table \"articles\", force: :cascade do |t|",
            "create_table \"articles\", force: :cascade do |t|\n    t.integer \"state\"\n    t.string \"tone\"\n    t.integer \"level\", default: 0, null: false",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  enum :state, { draft: 0, published: 1 }\n  enum :tone, { quiet: \"q\", loud: \"l\" }\n  enum :level, { low: 0, high: 1 }\n\n  def raw_state\n    state_before_type_cast\n  end\n\n  def raw_tone\n    tone_before_type_cast\n  end\n\n  def raw_level\n    level_before_type_cast\n  end\n",
        )
}

const ANSWERS_TEST: &str = r#"require "test_helper"

class ArticleRawStateTest < ActiveSupport::TestCase
  ATTRS = { title: "Raw", body: "A body long enough to validate." }

  test "an unsaved record answers what was assigned" do
    assert_nil Article.new.raw_state
    assert_equal :published, Article.new(state: :published).raw_state
    assert_equal "published", Article.new(state: "published").raw_state
    assert_equal 1, Article.new(state: 1).raw_state
    assert_nil Article.new(state: nil).raw_state
    assert_equal :loud, Article.new(tone: :loud).raw_tone
    assert_equal "l", Article.new(tone: "l").raw_tone
    article = Article.new
    article.state = :published
    assert_equal :published, article.raw_state
    article.state = :draft
    assert_equal :draft, article.raw_state
  end

  test "a schema default reads as its text" do
    assert_equal "0", Article.new.raw_level
  end

  test "a saved or loaded record answers the stored value" do
    article = Article.create!(ATTRS.merge(state: :published, tone: :loud))
    assert_equal 1, article.raw_state
    assert_equal "l", article.raw_tone
    assert_equal 1, article.reload.raw_state
    found = Article.find(article.id)
    assert_equal 1, found.raw_state
    assert_equal "l", found.raw_tone
    assert_equal 0, found.raw_level
    found.state = :draft
    assert_equal :draft, found.raw_state
    found.save!
    assert_equal 0, found.raw_state
    found.state = :draft
    found.save!
    assert_equal 0, found.raw_state
  end

  test "a bang writer saves" do
    article = Article.new(ATTRS)
    article.published!
    assert_equal 1, article.raw_state
  end
end
"#;

#[test]
fn enum_before_type_cast_answers_as_rails_does() {
    raw_state_app()
        .write("test/models/article_raw_state_test.rb", ANSWERS_TEST)
        .run_test("test/models/article_raw_state_test.rb")
        .assert_passes();
}

/// A label written on a `Model.new` in a controller stays as assigned when
/// only that controller reads the raw value (`:published` in Rails 8.1.4).
#[test]
fn a_controller_write_keeps_the_assigned_label() {
    emit_and_run::real_blog()
        .edit("db/schema.rb", "create_table \"articles\", force: :cascade do |t|",
            "create_table \"articles\", force: :cascade do |t|\n    t.integer \"state\"")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  enum :state, { draft: 0, published: 1 }\n")
        .edit("config/routes.rb", "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/raw_probe\", to: \"raw_probes#show\"\n")
        .write("app/controllers/raw_probes_controller.rb", r#"class RawProbesController < ApplicationController
  def show
    article = Article.new(title: "t", body: "a body long enough", state: :published)
    render plain: article.state_before_type_cast.inspect
  end
end
"#)
        .write("test/controllers/raw_probes_controller_test.rb", r#"require "test_helper"

class RawProbesControllerTest < ActionDispatch::IntegrationTest
  test "the raw reader answers the assigned label" do
    get "/raw_probe"
    assert_equal ":published", response.body
  end
end
"#)
        .run_test("test/controllers/raw_probes_controller_test.rb")
        .assert_passes();
}

/// An enum's `default:` reads as its label, as Rails 8.1.4 does.
#[test]
fn an_enum_default_reads_as_its_label() {
    emit_and_run::real_blog()
        .edit("db/schema.rb", "create_table \"articles\", force: :cascade do |t|",
            "create_table \"articles\", force: :cascade do |t|\n    t.integer \"state\"")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  enum :state, { draft: 0, published: 1 }, default: :published\n  def raw_state\n    state_before_type_cast\n  end\n")
        .run_ruby("raise Article.new.raw_state.inspect unless Article.new.raw_state == :published")
        .assert_passes();
}

fn state_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("db/schema.rb", "create_table \"articles\", force: :cascade do |t|",
            "create_table \"articles\", force: :cascade do |t|\n    t.integer \"state\"")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  enum :state, { draft: 0, published: 1 }\n")
}

const READ: &str = "Article.new(state: :published).state_before_type_cast";

/// Apps whose only read of the generated reader is in one kind of body.
fn apps_reading_it_in_one_body() -> Vec<(&'static str, emit_and_run::Overlay)> {
    let test = |body: String| format!("require \"test_helper\"\n\nclass ArticleProbeTest < ActiveSupport::TestCase\n{body}end\n");
    vec![
        ("test helper", state_app().write("test/models/article_probe_test.rb",
            &test(format!("  def raw\n    {READ}\n  end\n\n  test \"reads\" do\n    assert_equal :published, raw\n  end\n")))),
        ("test setup", state_app().write("test/models/article_probe_test.rb",
            &test(format!("  setup do\n    @raw = {READ}\n  end\n\n  test \"reads\" do\n    assert_equal :published, @raw\n  end\n")))),
        ("fixture preamble", state_app().write("test/fixtures/articles.yml",
            &format!("<% RAW_STATE = {READ} %>\none:\n  title: t\n  body: a body long enough\n"))),
        ("direct helper", state_app().edit("config/routes.rb", "  root \"articles#index\"\n",
            &format!("  root \"articles#index\"\n  direct(:raw_state) {{ \"/\" + {READ}.to_s }}\n"))),
    ]
}

/// However the reader is reached, the ruby target has it.
#[test]
fn a_read_in_any_body_gets_the_reader() {
    for (body, app) in apps_reading_it_in_one_body() {
        let (emitted, _) = app.emit(roundhouse::project::BuildTarget::Ruby);
        let model = std::fs::read_to_string(emitted.join("app/models/article.rb")).expect("the emitted model");
        assert!(model.contains("def state_before_type_cast"), "{body}: no reader in\n{model}");
    }
}

#[test]
fn a_read_in_any_body_is_refused_on_strict_targets() {
    for (body, app) in apps_reading_it_in_one_body() {
        let (_, errors) = app.emit(roundhouse::project::BuildTarget::Go);
        assert!(errors.iter().any(|e| e.contains("enum_before_type_cast not supported (go)")), "{body}: {errors:?}");
    }
}

/// One model defines the reader and another's is generated: Article and
/// Comment share an enum column.
fn two_models_app() -> emit_and_run::Overlay {
    state_app()
        .edit("db/schema.rb", "create_table \"comments\", force: :cascade do |t|",
            "create_table \"comments\", force: :cascade do |t|\n    t.integer \"state\"")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def raw_state\n    state_before_type_cast\n  end\n\n")
        .edit("app/models/comment.rb", "class Comment < ApplicationRecord\n",
            "class Comment < ApplicationRecord\n  enum :state, { draft: 0, published: 1 }\n\n  def state_before_type_cast\n    \"custom\"\n  end\n\n  def raw_state\n    state_before_type_cast\n  end\n\n")
}

#[test]
fn another_models_generated_reader_leaves_a_source_reader_alone() {
    two_models_app()
        .run_ruby("raise Comment.new(state: :published).raw_state.inspect unless Comment.new(state: :published).raw_state == 'custom'\nraise 'state' unless Comment.new(state: :published).state == 'published'\nraise Article.new(state: :published).raw_state.inspect unless Article.new(state: :published).raw_state == :published")
        .assert_passes();
}

#[test]
fn strict_targets_refuse_only_the_generated_reader() {
    let (_, errors) = two_models_app().emit(roundhouse::project::BuildTarget::Go);
    let refused: Vec<&String> = errors.iter().filter(|e| e.contains("enum_before_type_cast")).collect();
    // One call in each model; only Article's reader is generated.
    assert_eq!(refused.len(), 1, "{errors:?}");
}

#[test]
fn roda_refuses_the_generated_reader() {
    let (_, errors) = raw_state_app().emit(roundhouse::project::BuildTarget::Roda);
    assert!(errors.iter().any(|e| e.contains("enum_before_type_cast not supported (roda)")), "{errors:?}");
}

/// Roda's emit carries no tests or library classes, so a read only there
/// has nothing to refuse.
#[test]
fn roda_ignores_a_read_it_does_not_emit() {
    let in_test = state_app().write("test/models/article_probe_test.rb", &format!(
        "require \"test_helper\"\n\nclass ArticleProbeTest < ActiveSupport::TestCase\n  def raw\n    {READ}\n  end\nend\n"));
    let in_library = state_app().write("app/models/raw_reader.rb", &format!("class RawReader\n  def raw\n    {READ}\n  end\nend\n"));
    for app in [in_test, in_library] {
        let (_, errors) = app.emit(roundhouse::project::BuildTarget::Roda);
        assert!(!errors.iter().any(|e| e.contains("enum_before_type_cast")), "{errors:?}");
    }
}

/// A query in a writer's receiver or argument compares against the column,
/// so a String label still folds to the stored value (activerecord 8.1.4
/// finds the row by its label).
#[test]
fn a_query_around_a_writer_folds_its_string_label() {
    state_app()
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def raw_state\n    state_before_type_cast\n  end\n\n  def self.retitle_draft(text)\n    where.not(state: \"published\").first.update(title: text)\n  end\n\n  def tally_drafts\n    update(title: self.class.where(state: \"draft\").count.to_s)\n  end\n\n")
        .edit("app/models/comment.rb", "class Comment < ApplicationRecord\n",
            "class Comment < ApplicationRecord\n  def self.retitle_draft(text)\n    Article.where(state: \"draft\").where(title: \"old\").first.update(title: text)\n  end\n\n  def self.copy_draft\n    Article.create!(title: Article.where(state: \"draft\").where(title: \"new\").first.title, body: \"a body long enough\", state: :published)\n  end\n\n")
        .write("test/models/article_query_test.rb", r#"require "test_helper"

class ArticleQueryTest < ActiveSupport::TestCase
  ATTRS = { body: "A body long enough to validate." }

  test "a query in a writer's receiver finds the row" do
    Article.create!(ATTRS.merge(title: "pub", state: :published))
    draft = Article.create!(ATTRS.merge(title: "old", state: :draft))
    Article.retitle_draft("new")
    assert_equal "new", draft.reload.title
  end

  test "a query in a writer's argument finds the row" do
    published = Article.create!(ATTRS.merge(title: "pub", state: :published))
    Article.create!(ATTRS.merge(title: "old", state: :draft))
    published.tally_drafts
    assert_equal "1", published.reload.title
  end

  test "another model's query around a writer finds the row" do
    Article.create!(ATTRS.merge(title: "pub", state: :published))
    draft = Article.create!(ATTRS.merge(title: "old", state: :draft))
    Comment.retitle_draft("new")
    assert_equal "new", draft.reload.title
    copy = Comment.copy_draft
    assert_equal "new", copy.title
  end
end
"#)
        .run_test("test/models/article_query_test.rb")
        .assert_passes();
}

/// A read in a body Roda does convert is still refused.
#[test]
fn roda_refuses_a_read_in_a_controller_or_a_view() {
    let route = |app: emit_and_run::Overlay| {
        app.edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"/probe\", to: \"probes#show\"\n")
    };
    let controller = route(state_app()).write("app/controllers/probes_controller.rb",
        &format!("class ProbesController < ApplicationController\n  def show\n    render plain: {READ}.to_s\n  end\nend\n"));
    let view = route(state_app())
        .write("app/controllers/probes_controller.rb", "class ProbesController < ApplicationController\n  def show\n  end\nend\n")
        .write("app/views/probes/show.html.erb", &format!("<%= {READ} %>\n"));
    for app in [controller, view] {
        let (_, errors) = app.emit(roundhouse::project::BuildTarget::Roda);
        assert!(errors.iter().any(|e| e.contains("enum_before_type_cast not supported (roda)")), "{errors:?}");
    }
}

/// The reader answers what was assigned, not the normalized value, and
/// remembering it doesn't call the normalizer (activerecord 8.1.4).
#[test]
fn a_normalized_enum_remembers_the_assigned_value() {
    let app = || state_app().edit("app/models/article.rb", "class Article < ApplicationRecord\n",
        "class Article < ApplicationRecord\n  normalizes :state, with: ->(v) { v.to_s == \"draft\" ? \"published\" : v }\n  def raw_state\n    state_before_type_cast\n  end\n\n");
    app()
        .run_ruby("a = Article.new\na.state = \"draft\"\nraise a.raw_state.inspect unless a.raw_state == \"draft\"\nb = Article.new\nb.state = :draft\nraise b.raw_state.inspect unless b.raw_state == :draft\nc = Article.new\nc.state = \"published\"\nraise c.raw_state.inspect unless c.raw_state == \"published\"")
        .assert_passes();
    let (emitted, _) = app().emit(roundhouse::project::BuildTarget::Ruby);
    let model = std::fs::read_to_string(emitted.join("app/models/article.rb")).expect("the emitted model");
    assert!(!model.contains("_before_type_cast = Article._normalize_state"), "{model}");
}

#[test]
fn roda_keeps_a_source_reader() {
    let (_, errors) = emit_and_run::real_blog()
        .edit("db/schema.rb", "create_table \"articles\", force: :cascade do |t|",
            "create_table \"articles\", force: :cascade do |t|\n    t.integer \"state\"")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  enum :state, { draft: 0, published: 1 }\n\n  def state_before_type_cast\n    \"custom\"\n  end\n\n  def raw_state\n    state_before_type_cast\n  end\n\n")
        .emit(roundhouse::project::BuildTarget::Roda);
    assert!(!errors.iter().any(|e| e.contains("enum_before_type_cast")), "{errors:?}");
}

#[test]
fn strict_targets_refuse_enum_before_type_cast() {
    let (_, errors) = raw_state_app().emit(roundhouse::project::BuildTarget::Go);
    assert!(
        errors.iter().any(|e| e.contains("enum_before_type_cast not supported (go)")),
        "the value as assigned must not be claimed on Go: {errors:?}",
    );
}

#[test]
fn source_defined_before_type_cast_methods_keep_their_behavior() {
    emit_and_run::real_blog()
        .edit("db/schema.rb", "create_table \"articles\", force: :cascade do |t|",
            "create_table \"articles\", force: :cascade do |t|\n    t.integer \"state\"")
        .write("app/models/concerns/raw_state.rb", "module RawState\n  def state_before_type_cast\n    \"custom\"\n  end\nend\n")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  enum :state, { draft: 0, published: 1 }\n  include RawState\n  def raw_state\n    state_before_type_cast\n  end\n")
        .run_ruby("raise 'custom raw state' unless Article.new.raw_state == 'custom'")
        .assert_passes();
}
