//! `attr_reader(*FIELDS)` and `attr_accessor(*EXTRA)` over literal
//! constant lists declare a reader (and writer) for each name in the
//! list, as Ruby expands the splat. Ignored as a fork spec until the
//! compiler does; the expected values are Ruby's answers (run under
//! ActiveRecord 8.1.4).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/postcard.rb", r##"class Postcard
  FIELDS = %i[sender city].freeze
  EXTRA = [:stamp, :note].freeze

  attr_reader(*FIELDS)
  attr_accessor(*EXTRA)

  def initialize(sender, city)
    @sender = sender
    @city = city
  end

  def line
    "#{sender} from #{city}"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    card = Postcard.new("Ana", "Lima")
    check("each listed reader", card.sender == "Ana" && card.city == "Lima")
    check("read inside the class", card.line == "Ana from Lima")
    card.stamp = "bird"
    card.note = "hi"
    check("each listed accessor", card.stamp == "bird" && card.note == "hi")
    check("no writer for the readers", !card.respond_to?(:city=))
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn attr_reader_over_a_splatted_list_declares_each_name() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn attr_reader_over_a_splatted_list_declares_each_name_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
