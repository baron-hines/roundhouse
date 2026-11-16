//! A concern prepended to a model: its `prepended` block runs on the
//! model, its `class_methods` land there, and its instance methods sit
//! ahead of the model's own, reaching them through `super`.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/concerns/trimmed.rb", r##"module Trimmed
  extend ActiveSupport::Concern

  prepended do
    before_save :trim_label
  end

  class_methods do
    def trimmed_labels
      order(:label).pluck(:label)
    end
  end

  def display_label
    "[#{super}]"
  end

  private

  def trim_label
    self.label = label.strip
  end
end
"##)
        .write("app/models/badge.rb", r##"class Badge < ApplicationRecord
  prepend Trimmed

  def display_label
    label.upcase
  end
end
"##)
        .write("app/models/concerns/alpha.rb", r##"module Alpha
  extend ActiveSupport::Concern

  prepended do
    before_save :mark_alpha
  end

  def who
    "A>#{super}"
  end

  private

  def mark_alpha
    self.name = "#{name}A"
  end
end
"##)
        .write("app/models/concerns/beta.rb", r##"module Beta
  extend ActiveSupport::Concern

  prepended do
    before_save :mark_beta
  end

  def who
    "B>#{super}"
  end

  private

  def mark_beta
    self.name = "#{name}B"
  end
end
"##)
        .write("app/models/combo.rb", r##"class Combo < ApplicationRecord
  prepend Alpha, Beta

  def who
    "K"
  end
end
"##)
        .write("app/models/split.rb", r##"class Split < ApplicationRecord
  prepend Alpha
  prepend Beta

  def who
    "K"
  end
end
"##)
        .write("app/models/concerns/wrapping.rb", r##"module Wrapping
  extend ActiveSupport::Concern

  def wrapped_label
    "[#{super}]"
  end
end
"##)
        .write("app/models/gadget.rb", r##"class Gadget < ApplicationRecord
  prepend Wrapping

  def wrapped_label
    "gadget"
  end
end
"##)
        .write("app/models/concerns/left.rb", r##"module Left
  extend ActiveSupport::Concern

  prepended do
    before_save :mark_left
  end

  class_methods do
    def tag
      "Left"
    end
  end

  def who
    "L>#{super}"
  end

  private

  def mark_left
    self.name = "#{name}L"
  end
end
"##)
        .write("app/models/concerns/right.rb", r##"module Right
  extend ActiveSupport::Concern

  prepended do
    before_save :mark_right
  end

  class_methods do
    def tag
      "Right"
    end
  end

  def who
    "R>#{super}"
  end

  private

  def mark_right
    self.name = "#{name}R"
  end
end
"##)
        .write("app/models/repeat.rb", r##"class Repeat < ApplicationRecord
  prepend Left
  prepend Right
  prepend Left

  def who
    "K"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [["blue", "red"], "[RED]"]
  def self.prepended_concern
    Badge.create!(label: "  red ")
    Badge.create!(label: "blue  ")
    [Badge.trimmed_labels, Badge.order(:label).last.display_label]
  end

  # Rails: ["A>B>K", "KBA"] — `prepend A, B` puts A ahead of B in the
  # ancestry, and runs B's `prepended do` block before A's, so B's
  # `before_save` is registered first.
  def self.multi_argument_prepend
    combo = Combo.new(name: "K")
    combo.save!
    [combo.who, combo.name]
  end

  # Rails: ["B>A>K", "KAB"] — separate statements put the LATER module
  # ahead, and run their blocks in source order.
  def self.separate_prepends
    split = Split.new(name: "K")
    split.save!
    [split.who, split.name]
  end

  # Rails: ["R>L>K", "KLR", "Right"] — `prepend Left; prepend Right;
  # prepend Left` leaves the ancestry [Right, Left, K]: the second
  # `prepend Left` is a no-op (Left is already there), so Right keeps its
  # place ahead of it, its `prepended do` block does not run again, and
  # Right's `class_methods` stay ahead of Left's.
  def self.repeated_prepend
    repeat = Repeat.new(name: "K")
    repeat.save!
    [repeat.who, repeat.name, Repeat.tag]
  end

  # Rails: "[gadget]" — a Concern with no `prepended` block and no
  # `class_methods` is still prepended, so its method reaches the
  # model's through `super`.
  def self.method_only_prepended_concern
    Gadget.new.wrapped_label
  end

  def self.run
    Badge.delete_all
    check("prepended_concern", prepended_concern == [["blue", "red"], "[RED]"])
    check("multi_argument_prepend", multi_argument_prepend == ["A>B>K", "KBA"])
    check("separate_prepends", separate_prepends == ["B>A>K", "KAB"])
    check("repeated_prepend", repeated_prepend == ["R>L>K", "KLR", "Right"])
    check("method_only_prepended_concern", method_only_prepended_concern == "[gadget]")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "badges", force: :cascade do |t|
    t.string "label"
  end
  create_table "combos", force: :cascade do |t|
    t.string "name"
  end
  create_table "splits", force: :cascade do |t|
    t.string "name"
  end
  create_table "repeats", force: :cascade do |t|
    t.string "name"
  end
  create_table "gadgets", force: :cascade do |t|
    t.string "name"
  end
end
"##)
}

/// The same method-only Concern alone in its app: no other concern
/// contributes a block, which is the shape the concern splice used to
/// skip, leaving `prepend Wrapping` unwritten.
fn method_only_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/concerns/wrapping.rb", r##"module Wrapping
  extend ActiveSupport::Concern

  def wrapped_label
    "[#{super}]"
  end
end
"##)
        .write("app/models/gadget.rb", r##"class Gadget < ApplicationRecord
  prepend Wrapping

  def wrapped_label
    "gadget"
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "gadgets", force: :cascade do |t|
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";
const METHOD_ONLY_SCRIPT: &str = "raise \"method_only_prepended_concern differs from Rails\" unless Gadget.new.wrapped_label == \"[gadget]\"\nputs \"probe passed\"\n";

#[test]
fn a_prepended_concern_runs_its_block_and_wraps_the_model() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_prepended_concern_runs_its_block_and_wraps_the_model_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
fn a_method_only_prepended_concern_still_reaches_its_prepender() {
    let run = method_only_app().run_ruby(METHOD_ONLY_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_method_only_prepended_concern_still_reaches_its_prepender_on_spinel() {
    let run = method_only_app().run_spinel_with_db(METHOD_ONLY_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
