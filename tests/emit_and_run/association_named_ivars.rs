//! In a model with `has_many :stamps`, the model's own `@stamps_cache` /
//! `@stamps_loaded` instance variables are its own memo, not the
//! association's storage: the memo holds what the method stored and the
//! association still reads its rows, as Ruby keeps each ivar.
//! Ignored as a fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/album.rb", r##"class Album < ApplicationRecord
  has_many :stamps

  def stamp_labels
    return @stamps_cache if @stamps_loaded
    @stamps_loaded = true
    @stamps_cache = stamps.map { |s| "no. #{s.code}" }
  end
end
"##)
        .write("app/models/stamp.rb", r##"class Stamp < ApplicationRecord
  belongs_to :album
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    a = Album.create!(title: "blue")
    a.stamps.create!(code: "s1")
    # Rails: ["no. s1"], ["no. s1"] (memoized), 2 records
    check("own memo", a.stamp_labels == ["no. s1"])
    a.stamps.create!(code: "s2")
    check("memo kept", a.stamp_labels == ["no. s1"])
    check("association untouched", a.stamps.map(&:code).sort == %w[s1 s2])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "albums", force: :cascade do |t|
    t.string "title"
  end
  create_table "stamps", force: :cascade do |t|
    t.integer "album_id"
    t.string "code"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn ivars_named_after_an_association_are_the_model_s_own() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn ivars_named_after_an_association_are_the_model_s_own_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
