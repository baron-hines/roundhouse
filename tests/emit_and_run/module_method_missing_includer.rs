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

/// Review 1: the includer answers a name from a module it reaches at depth
/// (`Wrapper` includes `Middle`, which includes `Provider`, which defines `token`), directly or
/// through its parent class. Ruby 4.0.7 gives `["tok-host", "tok-child"]`.
fn nested_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/concerns/badged.rb", r##"module Badged
  extend ActiveSupport::Concern

  def badge
    "#{token}-#{label}"
  end

  def method_missing(name, *args)
    name.to_s.end_with?("?") ? false : super
  end

  def respond_to_missing?(name, include_private = false)
    name.to_s.end_with?("?") || super
  end
end
"##)
        .write("app/models/provider.rb", "module Provider\n  def token\n    \"tok\"\n  end\nend\n")
        .write("app/models/middle.rb", "module Middle\n  include Provider\n\n  def middle?\n    true\n  end\nend\n")
        .write("app/models/wrapper.rb", "module Wrapper\n  include Middle\n\n  def wrapped?\n    true\n  end\nend\n")
        .write("app/models/host.rb", "class Host\n  include Badged\n  include Wrapper\n\n  def label\n    \"host\"\n  end\nend\n")
        .write("app/models/base_host.rb", "class BaseHost\n  include Wrapper\nend\n")
        .write("app/models/child_host.rb", "class ChildHost < BaseHost\n  include Badged\n\n  def label\n    \"child\"\n  end\nend\n")
}

const NESTED: &str = r##"got = [Host.new.badge, ChildHost.new.badge]
raise "nested includes: #{got.inspect}" unless got == ["tok-host", "tok-child"]
puts "probe passed"
"##;

#[test]
fn a_method_missing_module_reads_a_method_its_includer_includes_at_depth() {
    let run = nested_app().run_ruby(NESTED);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_method_missing_module_reads_a_method_its_includer_includes_at_depth_on_spinel() {
    let run = nested_app().run_spinel(NESTED);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

/// Review 2: the module's includers found indirectly. `Courier` reaches
/// `Badged` only through `Carrier`, which no other class includes; `Agent`
/// inherits it from `BadgedBase`, which defines neither `token` nor `label`.
/// Ruby 4.0.7 with ActiveSupport 8.1.4.1 gives `"tok-courier"` and `"tok-agent"`.
fn badged_concern() -> &'static str {
    r##"module Badged
  extend ActiveSupport::Concern

  def badge
    "#{token}-#{label}"
  end

  def method_missing(name, *args)
    name.to_s.end_with?("?") ? false : super
  end

  def respond_to_missing?(name, include_private = false)
    name.to_s.end_with?("?") || super
  end
end
"##
}

fn indirect_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/concerns/badged.rb", badged_concern())
        .write("app/models/provider.rb", "module Provider\n  def token\n    \"tok\"\n  end\nend\n")
        .write("app/models/carrier.rb", "module Carrier\n  include Badged\n\n  def carried?\n    true\n  end\nend\n")
        .write("app/models/courier.rb", "class Courier\n  include Carrier\n  include Provider\n\n  def label\n    \"courier\"\n  end\nend\n")
}

const INDIRECT: &str = r##"got = Courier.new.badge
raise "indirect includer: #{got.inspect}" unless got == "tok-courier"
puts "probe passed"
"##;

#[test]
fn a_method_missing_module_reads_a_class_that_includes_it_through_another_module() {
    let run = indirect_app().run_ruby(INDIRECT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_method_missing_module_reads_a_class_that_includes_it_through_another_module_on_spinel() {
    let run = indirect_app().run_spinel(INDIRECT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

fn inherited_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/concerns/badged.rb", badged_concern())
        .write("app/models/provider.rb", "module Provider\n  def token\n    \"tok\"\n  end\nend\n")
        .write("app/models/badged_base.rb", "class BadgedBase\n  include Badged\nend\n")
        .write("app/models/agent.rb", "class Agent < BadgedBase\n  include Provider\n\n  def label\n    \"agent\"\n  end\nend\n")
}

const INHERITED: &str = r##"got = Agent.new.badge
raise "inherited includer: #{got.inspect}" unless got == "tok-agent"
puts "probe passed"
"##;

#[test]
fn a_method_missing_module_reads_a_subclass_of_its_includer() {
    let run = inherited_app().run_ruby(INHERITED);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_method_missing_module_reads_a_subclass_of_its_includer_on_spinel() {
    let run = inherited_app().run_spinel(INHERITED);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
