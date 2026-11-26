//! An `after_initialize` callback that requires an autoloaded `lib/` file
//! defining a class (`config.after_initialize { require
//! Rails.root.join("lib/shelf_label.rb").to_s }`). The class is the one
//! the `lib/` walk already compiles, so the require loads nothing new,
//! and the callback's own statement after it still runs. Rails 8.1.4 on
//! CRuby 4.0.9 boots the same app with the class defined,
//! `BootLog.entries == ["shelf label"]`, and `Book#label == "B-12: Dune"`.

use super::emit_and_run;

const APPLICATION: &str = r#"require "rails"
require "active_record/railtie"
module Library
  class Application < Rails::Application
    config.load_defaults 8.1
    config.autoload_lib(ignore: %w[tasks])
    config.after_initialize do
      require Rails.root.join("lib/shelf_label.rb").to_s
      BootLog.record("shelf label")
    end
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("config/application.rb", APPLICATION)
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/models/application_record.rb", r#"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"#)
        .write("app/models/book.rb", r#"class Book < ApplicationRecord
  def label
    ShelfLabel.new(shelf).call(title)
  end
end
"#)
        .write("app/models/boot_log.rb", r#"class BootLog
  ENTRIES = []

  def self.record(entry)
    ENTRIES << entry
  end

  def self.entries
    ENTRIES
  end
end
"#)
        .write("lib/shelf_label.rb", r#"class ShelfLabel
  def initialize(shelf)
    @shelf = shelf
  end

  def call(title)
    "B-#{@shelf}: #{title}"
  end
end
"#)
        .write("db/schema.rb", r#"ActiveRecord::Schema.define do
  create_table "books", force: :cascade do |t|
    t.string "title"
    t.integer "shelf"
  end
end
"#)
}

/// `config.ru` as Puma loads it: it opens the database, then runs the
/// app's callbacks.
const SERVE: &str = "require \"rack\"\nRack::Builder.new.instance_eval(File.read(\"config.ru\"), \"config.ru\")\n";

/// A contract starts no server: it calls the runner as the server does.
const SPINEL_RUN: &str = "AppInitializers.run(true)\n";

const SCRIPT: &str = r#"
raise "boot log: #{BootLog.entries.inspect}" unless BootLog.entries == ["shelf label"]
book = Book.new(title: "Dune", shelf: 12)
raise "label: #{book.label.inspect}" unless book.label == "B-12: Dune"
puts "probe passed"
"#;

#[test]
fn a_callback_requiring_an_autoloaded_class_file_boots_as_rails_does() {
    let run = app().run_ruby(&format!("{SERVE}{SCRIPT}"));
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

fn ingest_error(files: &[(&str, &str)]) -> String {
    let mut all: Vec<(&str, &str)> = vec![("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")];
    all.extend_from_slice(files);
    let tree = all.iter().map(|(p, c)| (std::path::PathBuf::from(p), c.as_bytes().to_vec())).collect();
    match roundhouse::ingest::ingest_app_from_tree(tree) {
        Ok(_) => panic!("ingest accepted {files:?}"),
        Err(err) => err.to_string(),
    }
}

/// A required file is read only when it holds declarations alone that
/// the program already defines; anything else stays an ingest error: a
/// class in an `autoload_lib` ignored directory, a statement or constant
/// beside the class, a second class beside a model that only the model's
/// walk reads. So does a constant assignment left to run, which would
/// sit in a method, where Ruby refuses it.
#[test]
fn a_class_off_the_autoload_paths_and_a_constant_assignment_are_errors() {
    const REFUSED: &str = "it is read only when it holds nothing else and the compiled program defines each of them";
    let ignored = ingest_error(&[
        ("config/application.rb", &APPLICATION.replace("lib/shelf_label.rb", "lib/tasks/shelf_label.rb")),
        ("lib/tasks/shelf_label.rb", "class ShelfLabel\nend\n"),
    ]);
    assert!(ignored.contains(REFUSED), "{ignored}");
    for beside in ["\nSHELF_PREFIX = \"B\"\n", "\nShelfTag = Struct.new(:name)\n", "\nputs ShelfLabel.name\n"] {
        let mixed = ingest_error(&[
            ("config/application.rb", APPLICATION),
            ("lib/shelf_label.rb", &format!("class ShelfLabel\nend\n{beside}")),
        ]);
        assert!(mixed.contains(REFUSED), "{mixed}");
    }
    let beside_model = ingest_error(&[
        ("config/application.rb", &APPLICATION.replace("lib/shelf_label.rb", "lib/stack.rb")),
        ("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n"),
        ("lib/stack.rb", "class Stack < ApplicationRecord\nend\n\nclass Other\nend\n"),
    ]);
    assert!(beside_model.contains(REFUSED), "{beside_model}");
    // Files the walk did not compile: one reopening a class it did, and
    // modules holding constants, which then no walk registered.
    for (path, source) in [
        ("lib/tasks/shelf_patch.rb", "class ShelfLabel\n  def call(title) = title\nend\n"),
        ("lib/tasks/shelf_limits.rb", "module ShelfLimits\n  MAX = 3\nend\n"),
        ("lib/tasks/shelf_limits.rb", "module ShelfLimits\n  MAX = 3\n\n  class Gauge\n  end\nend\n"),
    ] {
        let unread = ingest_error(&[
            ("config/application.rb", &APPLICATION.replace("lib/shelf_label.rb", path)),
            ("app/models/shelf_label.rb", "class ShelfLabel\n  def call(title) = title.upcase\nend\n"),
            (path, source),
        ]);
        assert!(unread.contains(REFUSED), "{path}: {unread}");
    }
    for body in ["SHELF_LIMIT = 40", "SHELF_LIMIT, shelf = [40, 1]"] {
        let assigned = ingest_error(&[(
            "config/initializers/limit.rb",
            &format!("Rails.application.config.after_initialize do\n  {body}\nend\n"),
        )]);
        assert!(assigned.contains("a constant assignment inside an after_initialize callback is not modeled"), "{assigned}");
    }
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_callback_requiring_an_autoloaded_class_file_boots_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(&format!("{SPINEL_RUN}{SCRIPT}"));
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
