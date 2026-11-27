//! A model's `thread_mattr_accessor` defines its per-thread class reader
//! and writer (with `default:` answered until the thread writes), and
//! instance accessors unless `instance_accessor: false`, as Rails does on
//! any class. Ignored as a fork spec until the compiler does; the
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/template.rb", r##"class Template < ApplicationRecord
  thread_mattr_accessor :cache_enabled, instance_accessor: false, default: true
  thread_mattr_accessor :render_mode

  def self.without_cache
    previous = cache_enabled
    self.cache_enabled = false
    yield
  ensure
    self.cache_enabled = previous
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    check("the default", Template.cache_enabled == true)
    inside = nil
    Template.without_cache { inside = Template.cache_enabled }
    check("a write inside", inside == false)
    check("restored after", Template.cache_enabled == true)
    t = Template.new
    t.render_mode = "fast"
    check("the instance writer writes the class's", Template.render_mode == "fast" && t.render_mode == "fast")
    check("no instance accessor when declined", !t.respond_to?(:cache_enabled))
    other = Thread.new { [Template.cache_enabled, Template.render_mode] }.value
    check("another thread reads the defaults", other == [true, nil])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "templates", force: :cascade do |t|
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn model_thread_mattr_accessor_defines_its_accessors() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn model_thread_mattr_accessor_defines_its_accessors_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
