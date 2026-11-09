#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/prerequisite.rs"]
mod prerequisite;

const SCHEMA: &str = r#"ActiveRecord::Schema.define do
  create_table "messages", force: :cascade do |t|
    t.string "kind", null: false
  end
end
"#;

const CONTROLLER: &str = r#"class PagesController < ApplicationController
  def index
    render json: { total: total }
  end

  def total(counts: Message.where(kind: "action").group(:kind).count)
    counts.values.sum
  end

  def positional(counts = Message.where(kind: "action").group(:kind).count)
    counts.values.sum
  end

  def scalar(count: Message.where(kind: "action").count)
    count
  end

  def label(value: "action")
    value
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/message.rb", "class Message < ApplicationRecord\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/pages_controller.rb", CONTROLLER)
        .write("config/routes.rb", "Rails.application.routes.draw do\n  get \"/pages\", to: \"pages#index\"\nend\n")
}

#[test]
fn controller_keyword_defaults_lower_like_positional_defaults() {
    let (dir, errors) = app().emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let source = std::fs::read_to_string(dir.join("app/controllers/pages_controller.rb")).unwrap();
    for name in ["total", "positional"] {
        let signature = source.lines().find(|l| l.contains(&format!("def {name}("))).unwrap();
        assert!(signature.contains("group_count"), "{signature}");
    }
    let scalar = source.lines().find(|l| l.contains("def scalar(")).unwrap();
    assert!(scalar.contains(".count") && !scalar.contains("group_count"), "{scalar}");
    assert!(source.contains("def label(value: \"action\")"), "{source}");
}

#[test]
fn controller_grouped_count_defaults_match_active_record() {
    let oracle = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/rails_oracle.rb");
    let probe = r#"
2.times { Message.create!(kind: "action") }
Message.create!(kind: "fyi")
pages = PagesController.new
puts [pages.total, pages.positional, pages.scalar, pages.label].inspect
puts pages.total(counts: { "override" => 7 })
"#;
    let script = format!(
        "require {oracle:?}\nexit 77 unless RailsOracle.available?(\"activerecord\", \"actionpack\", sqlite3: \">= 2.9\")\nRailsOracle.activate(\"activerecord\", \"actionpack\", sqlite3: \">= 2.9\")\nrequire \"active_record\"\nrequire \"action_controller\"\nActiveRecord::Base.establish_connection(adapter: \"sqlite3\", database: \":memory:\")\nActiveRecord::Schema.verbose = false\n{SCHEMA}\nclass Message < ActiveRecord::Base; end\nclass ApplicationController < ActionController::Base; end\n{CONTROLLER}\n{probe}"
    );
    let reference = std::process::Command::new("ruby").args(["-e", &script]).output().expect("ruby");
    if reference.status.code() == Some(77) {
        prerequisite::missing("Rails oracle with activerecord, actionpack and sqlite3");
        return;
    }
    assert!(reference.status.success(), "{}", String::from_utf8_lossy(&reference.stderr));
    let emitted = app().run_ruby(&format!("require_relative \"app/controllers/pages_controller\"\n{probe}"));
    emitted.assert_passes();
    assert_eq!(emitted.stdout.trim(), String::from_utf8_lossy(&reference.stdout).trim());
}
