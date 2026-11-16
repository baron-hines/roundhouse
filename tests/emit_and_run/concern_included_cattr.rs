//! A concern's `included` block that declares `cattr_accessor` /
//! `mattr_reader` with defaults: every includer gets the class and
//! instance readers, and a class-side write is seen by its instances.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/concerns/capped.rb", r##"module Capped
  extend ActiveSupport::Concern

  included do
    cattr_accessor :cap, default: 3
    mattr_reader :unit, default: "cm"
  end

  def over_cap?
    length > cap
  end

  def with_unit
    "#{length}#{unit}"
  end
end
"##)
        .write("app/models/rope.rb", r##"class Rope < ApplicationRecord
  include Capped
end
"##)
        .write("app/models/concerns/seeded.rb", r##"module Seeded
  extend ActiveSupport::Concern

  included do
    cattr_accessor :quantity, default: 3
    cattr_reader :unit_size, default: 3
    cattr_writer :unit_size
    cattr_accessor :girth, default: 3
  end
end
"##)
        .write("app/models/concerns/widened.rb", r##"module Widened
  extend ActiveSupport::Concern

  included do
    cattr_accessor :girth, default: 5
  end
end
"##)
        .write("app/models/crate.rb", r##"class Crate < ApplicationRecord
  cattr_accessor :quantity, default: 1
  include Seeded
  include Widened
  cattr_accessor :girth, default: 1
end
"##)
        .write("app/models/concerns/first.rb", r##"module First
  extend ActiveSupport::Concern

  included do
    cattr_accessor :span, default: 1
  end
end
"##)
        .write("app/models/concerns/second.rb", r##"module Second
  extend ActiveSupport::Concern

  included do
    cattr_accessor :span, default: 2
  end
end
"##)
        .write("app/models/concerns/blank.rb", r##"module Blank
  extend ActiveSupport::Concern

  included do
    cattr_reader :width, default: nil
  end
end
"##)
        .write("app/models/ribbon.rb", r##"class Ribbon < ApplicationRecord
  include First, Second
end
"##)
        .write("app/models/tape.rb", r##"class Tape < ApplicationRecord
  cattr_accessor :width, default: 1
  include Blank
end
"##)
        .write("app/models/concerns/bounded.rb", r##"module Bounded
  extend ActiveSupport::Concern

  BOUND = 7

  included do
    cattr_accessor :bound, default: Bounded::BOUND
  end
end
"##)
        .write("app/models/spool.rb", r##"class Spool < ApplicationRecord
  cattr_accessor :bound
  include Bounded
end
"##)
        .write("app/models/concerns/layered.rb", r##"module Layered
  extend ActiveSupport::Concern

  included do
    cattr_accessor :ply, default: 0
  end
end
"##)
        .write("app/models/concerns/single_ply.rb", r##"module SinglePly
  extend ActiveSupport::Concern
  include Layered
end
"##)
        .write("app/models/concerns/double_ply.rb", r##"module DoublePly
  extend ActiveSupport::Concern
  include Layered

  included do
    cattr_accessor :ply, default: 2
  end
end
"##)
        .write("app/models/cord.rb", r##"class Cord < ApplicationRecord
  include SinglePly, DoublePly
end
"##)
        .write("app/models/concerns/limited.rb", r##"module Limited
  extend ActiveSupport::Concern

  LIMIT = 3

  included do
    cattr_accessor :ceiling, default: LIMIT
  end
end
"##)
        .write("app/models/reel.rb", r##"class Reel < ApplicationRecord
  LIMIT = 9
  include Limited
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [3, "cm", true, false, "4cm"]
  def self.class_attributes_from_the_block
    rope = Rope.create!(length: 4)
    before = [Rope.cap, Rope.unit, rope.over_cap?]
    Rope.cap = 5
    before + [rope.over_cap?, rope.with_unit]
  end

  # Rails: 3 — the class body runs top to bottom, so the concern's seed
  # (executed at the `include`) replaces the model's earlier one.
  def self.concern_seed_after_the_models_own
    Crate.quantity
  end

  # Rails: 3 — a `cattr_writer` with no default does not replace the
  # seed `cattr_reader` set before it.
  def self.seed_without_a_default_keeps_the_earlier
    Crate.unit_size
  end

  # Rails: 1 — the model's own declaration below both includes runs
  # last, after each concern's seed.
  def self.models_seed_after_two_concerns
    Crate.girth
  end

  # Rails: 1 — one `include First, Second` runs Ruby's hooks in reverse
  # argument order (Second's, then First's), so First's seed lands last.
  def self.one_statement_two_concerns
    Ribbon.span
  end

  # Rails: 1 — `default: nil` sets the class variable only where nothing
  # has, so the concern's nil never replaces the model's seed above it.
  def self.nil_default_keeps_the_models_seed
    Tape.width
  end

  # Rails: 7 — a constant default replaces the nil the model's own
  # no-default declaration left.
  def self.constant_default_over_a_nil_seed
    Spool.bound
  end

  # Rails: 2 — a dependency two arguments share runs once, before
  # DoublePly's block (the last argument runs first); SinglePly finds it
  # already an ancestor.
  def self.shared_dependency_runs_once
    Cord.ply
  end

  # Rails: 3 — the block reads the concern's own LIMIT, not the
  # includer's.
  def self.seed_reads_the_concerns_constant
    Reel.ceiling
  end

  def self.run
    Rope.delete_all
    check("class_attributes_from_the_block", class_attributes_from_the_block == [3, "cm", true, false, "4cm"])
    check("concern_seed_after_the_models_own", concern_seed_after_the_models_own == 3)
    check("seed_without_a_default_keeps_the_earlier", seed_without_a_default_keeps_the_earlier == 3)
    check("models_seed_after_two_concerns", models_seed_after_two_concerns == 1)
    check("one_statement_two_concerns", one_statement_two_concerns == 1)
    check("nil_default_keeps_the_models_seed", nil_default_keeps_the_models_seed == 1)
    check("constant_default_over_a_nil_seed", constant_default_over_a_nil_seed == 7)
    check("shared_dependency_runs_once", shared_dependency_runs_once == 2)
    check("seed_reads_the_concerns_constant", seed_reads_the_concerns_constant == 3)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "ropes", force: :cascade do |t|
    t.integer "length"
  end
  create_table "crates", force: :cascade do |t|
    t.string "name"
  end
  create_table "ribbons", force: :cascade do |t|
    t.string "name"
  end
  create_table "tapes", force: :cascade do |t|
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_concerns_included_cattr_accessor_reaches_its_includer() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_concerns_included_cattr_accessor_reaches_its_includer_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
