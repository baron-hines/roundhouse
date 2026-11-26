//! An `Arel.sql` literal bound to a where fragment's `?` is written into
//! the statement as it is, not quoted as a String, as Rails' bound SQL
//! literal visits it. Ignored as a fork spec until the compiler does; the
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/reading.rb", r##"class Reading < ApplicationRecord
  def self.above(expression)
    where("level > ?", Arel.sql(expression)).order(:level).pluck(:level)
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    [3, 8, 12].each { |l| Reading.create!(level: l, floor: 5) }
    # Rails: [8, 12], [12]
    check("literal number", Reading.above("4") == [8, 12])
    check("literal expression", Reading.above("floor * 2") == [12])
    # Rails: (level > floor * 2)
    check("written as it is", Reading.where("level > ?", Arel.sql("floor * 2")).to_sql.include?("(level > floor * 2)"))
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "readings", force: :cascade do |t|
    t.integer "level"
    t.integer "floor"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: record queries"]
fn an_sql_literal_bound_to_a_placeholder_is_written_as_it_is() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: record queries"]
fn an_sql_literal_bound_to_a_placeholder_is_written_as_it_is_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
