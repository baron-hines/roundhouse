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

fn ledger_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/ledger/record.rb", "module Ledger\n  class Record < ApplicationRecord\n    self.abstract_class = true\n\n    def ledger_tag\n      \"ledger\"\n    end\n  end\nend\n")
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"entries\", force: :cascade do |t|\n    t.string \"name\"\n  end\n\n  create_table \"things\", force: :cascade do |t|\n    t.string \"name\"\n  end\nend\n")
}

const UNROOTED_IN_ROOTED: &str = r##"entry = Export::Entry.create!(name: "e")
got = [Export::Entry.superclass.name, Export::Entry.table_name, entry.ledger_tag, Export::Entry.count, defined?(Ledger::Export)]
expected = ["Ledger::Record", "entries", "ledger", 1, nil]
raise "unrooted in rooted: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_class_in_a_rooted_module_finds_its_superclass_where_it_is_written() {
    let app = ledger_app().write("app/models/export/entry.rb", "module Ledger\n  module ::Export\n    class Entry < Record\n    end\n  end\nend\n");
    let run = app.run_ruby(UNROOTED_IN_ROOTED);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

const ROOTED_ABSTRACT_BASE: &str = r##"thing = Thing.create!(name: "t")
got = [Thing.superclass.name, RootBase.superclass.name, Thing.table_name, thing.ledger_tag, Thing.count]
expected = ["RootBase", "Ledger::Record", "things", "ledger", 1]
raise "rooted abstract base: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_rooted_abstract_base_reaches_the_base_in_the_module_around_it() {
    let app = ledger_app()
        .write("app/models/root_base.rb", "module Ledger\n  class ::RootBase < Record\n    self.abstract_class = true\n  end\nend\n")
        .write("app/models/thing.rb", "class Thing < RootBase\nend\n");
    let run = app.run_ruby(ROOTED_ABSTRACT_BASE);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

fn controller_concern_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/rules_controller.rb", r##"class RulesController < ApplicationController
  module ::AccessRules
    extend ActiveSupport::Concern

    included do
      before_action :guard
    end

    private

    def guard
      @trace = "#{@trace}guard "
    end
  end

  include AccessRules

  def show
    render plain: "trace=#{@trace}"
  end
end
"##)
        .write("config/routes.rb", "Rails.application.routes.draw do\n  get \"rules\", to: \"rules#show\"\nend\n")
}

const CONTROLLER_CONCERN: &str = r##"status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/rules", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
text = +""
body.each { |part| text << part }
got = [status, text, defined?(RulesController::AccessRules)]
expected = [200, "trace=guard ", nil]
raise "controller concern: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_rooted_concern_in_a_controller_body_runs_its_included_filters() {
    let run = controller_concern_app().run_ruby(CONTROLLER_CONCERN);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

fn prefixed_ledger_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/record.rb", "class Record < ApplicationRecord\n  self.abstract_class = true\n\n  def tag\n    \"top\"\n  end\nend\n")
        .write("app/models/ledger/record.rb", "module Ledger\n  class Record < ApplicationRecord\n    self.abstract_class = true\n    self.table_name_prefix = \"ledger_\"\n\n    def tag\n      \"ledger\"\n    end\n  end\nend\n")
        .write("app/models/export/record.rb", "module Export\n  class Record < ApplicationRecord\n    self.abstract_class = true\n\n    def tag\n      \"export\"\n    end\n  end\nend\n")
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"things\", force: :cascade do |t|\n    t.string \"name\"\n  end\n\n  create_table \"ledger_entries\", force: :cascade do |t|\n    t.string \"name\"\n  end\nend\n")
}

const ROOTED_SUPERCLASS: &str = r##"thing = Thing.create!(name: "t")
got = [RootBase.superclass.name, Thing.table_name, thing.tag, Thing.count]
expected = ["Record", "things", "top", 1]
raise "rooted superclass: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_rooted_superclass_is_not_looked_up_in_the_module_around_it() {
    let app = prefixed_ledger_app()
        .write("app/models/root_base.rb", "module Ledger\n  class ::RootBase < ::Record\n    self.abstract_class = true\n  end\nend\n")
        .write("app/models/thing.rb", "class Thing < RootBase\nend\n");
    let run = app.run_ruby(ROOTED_SUPERCLASS);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

const COMPACT_ROOTED_MODULE: &str = r##"entry = Export::Inner::Entry.create!(name: "e")
got = [Export::Inner::Entry.superclass.name, Export::Inner::Entry.table_name, entry.tag, Export::Inner::Entry.count]
expected = ["Ledger::Record", "ledger_entries", "ledger", 1]
raise "compact rooted module: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_class_in_a_compact_rooted_module_looks_through_its_nesting_only() {
    let app = prefixed_ledger_app()
        .write("app/models/export/inner/entry.rb", "module Ledger\n  module ::Export::Inner\n    class Entry < Record\n    end\n  end\nend\n");
    let run = app.run_ruby(COMPACT_ROOTED_MODULE);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
