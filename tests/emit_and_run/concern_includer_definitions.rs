//! Which definition a model runs when several of its concerns define a
//! method that reads its `has_many`, and how a copied method's defaults
//! and constants read, as Rails runs them. A concern a concern includes
//! comes after it in the model's ancestors; `include First, Second` puts
//! `First` ahead; a later concern's override wins even when it reads no
//! association. A default is evaluated on the includer, and a bare
//! constant is the concern's own. The expected values are ActiveRecord
//! 8.1.4.1's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/token.rb", r##"class Token < ApplicationRecord
  belongs_to :bearer, polymorphic: true
end
"##)
        .write("app/models/concerns/inner.rb", r##"module Inner
  extend ActiveSupport::Concern

  def label_token!(name)
    tokens.create!(name: "inner #{name}")
  end
end
"##)
        .write("app/models/concerns/outer.rb", r##"module Outer
  extend ActiveSupport::Concern
  include Inner

  def label_token!(name)
    tokens.create!(name: "outer #{name}")
  end
end
"##)
        .write("app/models/concerns/first.rb", r##"module First
  extend ActiveSupport::Concern

  def stamp_token!(name)
    tokens.create!(name: "first #{name}")
  end
end
"##)
        .write("app/models/concerns/second.rb", r##"module Second
  extend ActiveSupport::Concern

  def stamp_token!(name)
    tokens.create!(name: "second #{name}")
  end
end
"##)
        .write("app/models/concerns/nested.rb", r##"module Nested
  extend ActiveSupport::Concern

  def mark_token!(name)
    tokens.create!(name: "nested #{name}")
  end
end
"##)
        .write("app/models/concerns/wrapper.rb", r##"module Wrapper
  extend ActiveSupport::Concern
  include Nested
end
"##)
        .write("app/models/concerns/later.rb", r##"module Later
  extend ActiveSupport::Concern

  def mark_token!(name)
    tokens.create!(name: "later #{name}")
  end
end
"##)
        .write("app/models/concerns/defaults.rb", r##"module Defaults
  extend ActiveSupport::Concern

  def find_or_issue!(name, prior = tokens.find_by(name: name))
    prior || tokens.create!(name: name)
  end

  def tally(count = tokens.count)
    count
  end
end
"##)
        .write("app/models/concerns/prefixed.rb", r##"module Prefixed
  extend ActiveSupport::Concern
  PREFIX = "concern"

  def prefixed_token!(name)
    tokens.create!(name: "#{PREFIX} #{name}")
  end
end
"##)
        .write("app/models/concerns/token_holder.rb", r##"module TokenHolder
  extend ActiveSupport::Concern

  def issue_token!(name)
    tokens.create!(name: name)
  end
end
"##)
        .write("app/models/concerns/disabled_tokens.rb", r##"module DisabledTokens
  extend ActiveSupport::Concern

  def issue_token!(name)
    nil
  end
end
"##)
        .write("app/models/shop.rb", r##"class Shop < ApplicationRecord
  PREFIX = "shop"
  has_many :tokens, as: :bearer
  include Outer
  include First, Second
  include Wrapper
  include Later
  include Defaults
  include Prefixed
end
"##)
        .write("app/models/stall.rb", r##"class Stall < ApplicationRecord
  has_many :tokens, as: :bearer
  include Outer
  include First, Second
  include Wrapper
  include Later
  include Defaults
  include Prefixed
end
"##)
        .write("app/models/booth.rb", r##"class Booth < ApplicationRecord
  has_many :tokens, as: :bearer
  include TokenHolder
  include DisabledTokens
end
"##)
        .write("app/models/cart.rb", r##"class Cart < ApplicationRecord
  has_many :tokens, as: :bearer
  include TokenHolder
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "shops", force: :cascade do |t|
    t.string "name"
  end
  create_table "stalls", force: :cascade do |t|
    t.string "name"
  end
  create_table "booths", force: :cascade do |t|
    t.string "name"
  end
  create_table "carts", force: :cascade do |t|
    t.string "name"
  end
  create_table "tokens", force: :cascade do |t|
    t.string "bearer_type"
    t.integer "bearer_id"
    t.string "name"
  end
end
"##)
}

const ANCESTOR_ORDER: &str = r##"shop = Shop.create!
got = [shop.label_token!("a").name, shop.stamp_token!("a").name, shop.mark_token!("a").name]
expected = ["outer a", "first a", "later a"]
raise "ancestor_order: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_model_runs_the_definition_first_in_its_ancestors() {
    let run = app().run_ruby(ANCESTOR_ORDER);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

const WINNING_DEFINITION: &str = r##"booth = Booth.create!
got = [booth.issue_token!("a"), booth.tokens.count, Cart.create!.issue_token!("a").name]
expected = [nil, 0, "a"]
raise "winning_definition: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_later_concerns_override_wins_when_it_reads_no_association() {
    let run = app().run_ruby(WINNING_DEFINITION);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

const PARAMETER_DEFAULTS: &str = r##"stall = Stall.create!
first = stall.find_or_issue!("d")
got = [first.id == stall.find_or_issue!("d").id, Token.where(name: "d").count, stall.tally, Shop.create!.tally]
expected = [true, 1, 1, 0]
raise "parameter_defaults: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_default_reads_the_includers_association() {
    let run = app().run_ruby(PARAMETER_DEFAULTS);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

const LEXICAL_CONSTANTS: &str = r##"got = [Shop.create!.prefixed_token!("x").name, Stall.create!.prefixed_token!("x").name]
expected = ["concern x", "concern x"]
raise "lexical_constants: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_bare_constant_is_the_concerns_own() {
    let run = app().run_ruby(LEXICAL_CONSTANTS);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_concerns_includers_run_their_winning_definitions_on_spinel() {
    for script in [ANCESTOR_ORDER, WINNING_DEFINITION, PARAMETER_DEFAULTS, LEXICAL_CONSTANTS] {
        let run = app().run_spinel_with_db(script);
        run.assert_passes();
        assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
    }
}
