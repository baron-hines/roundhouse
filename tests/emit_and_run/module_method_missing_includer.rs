//! A module that defines `method_missing` reads its includer's methods by
//! name: a receiverless `name` in its own method is the includer's attribute
//! reader, and only a name no includer answers reaches `method_missing`.
//! Expected values are what ActiveRecord 8.1.4.1 answers for the same app.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/concerns/describable.rb", r##"module Describable
  extend ActiveSupport::Concern

  def describe
    { id: id, label: name, colour: colour }
  end

  def shiny_label
    shiny? ? "shiny #{name}" : "plain #{name}"
  end

  def method_missing(name, *args)
    name.to_s.end_with?("?") ? false : super
  end

  def respond_to_missing?(method_name, include_private = false)
    method_name.to_s.end_with?("?") || super
  end
end
"##)
        .write("app/models/widget.rb", "class Widget < ApplicationRecord\n  include Describable\nend\n")
        .write("app/models/gizmo.rb", r##"class Gizmo
  include Describable

  def id
    7
  end

  def name
    "gizmo"
  end

  def colour
    "blue"
  end
end
"##)
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"widgets\", force: :cascade do |t|\n    t.string \"name\"\n    t.string \"colour\"\n  end\nend\n")
}

const SCRIPT: &str = r##"widget = Widget.create!(name: "Sprocket", colour: "red")
got = [widget.describe, widget.shiny_label, Gizmo.new.describe, Gizmo.new.shiny_label]
expected = [{ id: widget.id, label: "Sprocket", colour: "red" }, "plain Sprocket", { id: 7, label: "gizmo", colour: "blue" }, "plain gizmo"]
raise "includer reads: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_method_missing_module_reads_its_includers_methods() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_method_missing_module_reads_its_includers_methods_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
