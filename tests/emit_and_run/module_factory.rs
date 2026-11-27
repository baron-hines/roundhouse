use super::emit_and_run;

/// A module factory included with literal arguments (lobsters'
/// `include FullTextSearch[:comment]`, whose `self.[]` answers a
/// `Module.new` concern reading its arguments) is expanded into a named
/// module per argument list, with the arguments written in. The concern's
/// `after_create do |record| … end` reads the record as `self`, as Rails
/// runs a one-parameter callback block. Both were dropped before: the
/// factory refused, and the block form was "not lowered".
pub const FACTORY: &str = r##"module CopyLog
  def self.[](*cols)
    Module.new do
      extend ActiveSupport::Concern

      included do
        after_create do |record|
          table_name = "#{record.class.table_name}_log"
          column_list = cols.join(", ")
          value_list = (["?"] * cols.length).join(", ")
          values = cols.map { |c| record.public_send(c) }
          ActiveRecord::Base.connection.exec_insert("INSERT INTO #{table_name} (rowid, #{column_list}) values (?, #{value_list})", nil, [record.id] + values)
        end
      end
    end
  end
end
"##;

pub fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/models/copy_log.rb", FACTORY)
        .edit(
            "db/schema.rb",
            "  create_table \"comments\"",
            "  create_table \"articles_log\", id: false, force: :cascade do |t|\n    t.integer \"rowid\"\n    t.string \"title\"\n    t.text \"body\"\n  end\n\n  create_table \"comments\"",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  include CopyLog[:title, :body]\n\n  def self.logged\n    ActiveRecord::Base.connection.select_all(\"SELECT rowid, title, body FROM articles_log ORDER BY rowid\").to_a.map { |r| [r[\"title\"], r[\"body\"]] }\n  end\n",
        )
}

pub const ASSERTIONS: &str = r#"Article.delete_all
Article.create!(title: "Hello", body: "A sufficiently long body.")
raise "logged: #{Article.logged.inspect}" unless Article.logged == [["Hello", "A sufficiently long body."]]
puts "module factory passed"
"#;

#[test]
fn an_included_module_factory_runs_its_callbacks() {
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let warnings = app().ingest_warnings();
    assert!(!warnings.iter().any(|w| w.contains("after_create")), "{warnings:#?}");
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("module factory passed"));
}

/// Argument lists that camelize alike (`[:title_body]`, `[:title, :body]`)
/// each get a module of their own, and a factory that rebinds its
/// parameter (`cols ||= …`) is not expanded: past the write, the
/// argument list is no longer what the body reads.
#[test]
fn specialized_names_stay_distinct_and_a_rebound_parameter_declines() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .write("app/models/copy_log.rb", FACTORY)
        .write(
            "app/models/tagged.rb",
            "module Tagged\n  def self.[](*cols)\n    Module.new do\n      extend ActiveSupport::Concern\n\n      included do\n        cols ||= [:none]\n        define_method(:tag_cols) { cols }\n      end\n    end\n  end\nend\n",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  include CopyLog[:title, :body]\n  include Tagged[:title]\n",
        )
        .edit(
            "app/models/comment.rb",
            "class Comment < ApplicationRecord\n",
            "class Comment < ApplicationRecord\n  include CopyLog[:title_body]\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let article = std::fs::read_to_string(emitted.join("app/models/article.rb")).expect("article.rb");
    let comment = std::fs::read_to_string(emitted.join("app/models/comment.rb")).expect("comment.rb");
    let included = |source: &str| {
        source.lines().find_map(|l| l.trim().strip_prefix("include CopyLog").map(|rest| format!("CopyLog{rest}")))
    };
    let (a, c) = (included(&article), included(&comment));
    assert!(a.is_some() && c.is_some() && a != c, "{article}\n{comment}");
    assert!(!article.contains("TaggedTitle"), "{article}");
}
