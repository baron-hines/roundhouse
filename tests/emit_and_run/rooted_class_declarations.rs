//! A class or module whose declared name starts with `::` is defined at
//! the top level, wherever the declaration is written: `class ::Shelf::Box`
//! inside `module Adapter` defines `Shelf::Box`, and a class declared in the
//! body of `module ::Crate` is `Crate::<name>`. A rooted class or module
//! in a model's body is top-level too, and a rooted class's bare
//! superclass is still looked up in the module around it. The expected
//! values are Ruby 4.0.7's (ActiveRecord 8.1.4.1's for the models).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/shelf.rb", "module Shelf\nend\n")
        .write("app/models/adapter.rb", r##"module Adapter
  class ::Shelf::Box
    def self.kind
      "box"
    end
  end

  module ::Crate
    class Lid
      def self.kind
        "lid"
      end
    end
  end

  class Plug
    def self.kind
      "plug"
    end
  end
end
"##)
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
}

const SCRIPT: &str = r##"Adapter
answers = [
  defined?(Shelf::Box), defined?(Adapter::Shelf), Shelf::Box.kind,
  defined?(Crate::Lid), defined?(Adapter::Crate), Crate::Lid.kind,
  Adapter::Plug.kind,
]
expected = ["constant", nil, "box", "constant", nil, "lid", "plug"]
raise "rooted declarations: #{answers.inspect}" unless answers == expected
puts "probe passed"
"##;

#[test]
fn a_rooted_declaration_inside_a_module_is_defined_at_the_top_level() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_rooted_declaration_inside_a_module_is_defined_at_the_top_level_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

fn model_body_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/account.rb", r##"class Account < ApplicationRecord
  class ::Parser
    def self.kind
      "parser"
    end
  end

  module ::Formats
    def self.kind
      "formats"
    end
  end
end
"##)
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"accounts\", force: :cascade do |t|\n    t.string \"name\"\n  end\nend\n")
}

const MODEL_BODY: &str = r##"Account
got = [Parser.kind, Formats.kind, defined?(Account::Parser), defined?(Account::Formats)]
expected = ["parser", "formats", nil, nil]
raise "model body: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_rooted_declaration_in_a_model_body_is_defined_at_the_top_level() {
    let run = model_body_app().run_ruby(MODEL_BODY);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

fn superclass_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/ledger/record.rb", "module Ledger\n  class Record < ApplicationRecord\n    self.abstract_class = true\n  end\nend\n")
        .write("app/models/entry.rb", "module Ledger\n  class ::Entry < Record\n  end\nend\n")
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"entries\", force: :cascade do |t|\n    t.string \"name\"\n  end\nend\n")
}

const SUPERCLASS: &str = r##"Entry.create!(name: "e")
got = [Entry.superclass.name, Entry.table_name, Entry.count, Entry.first.name]
expected = ["Ledger::Record", "entries", 1, "e"]
raise "superclass: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_rooted_class_finds_its_superclass_in_the_module_around_it() {
    let run = superclass_app().run_ruby(SUPERCLASS);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
