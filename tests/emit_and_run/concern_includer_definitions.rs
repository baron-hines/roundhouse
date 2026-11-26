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

/// The second review's cases: two modules of one `include` call that
/// include the same module, a method a module defines twice, and
/// definitions behind a load-time `if`.
fn second_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)        .write("app/models/token.rb", r##"class Token < ApplicationRecord
  belongs_to :bearer, polymorphic: true
end
"##)        .write("app/models/concerns/shared.rb", r##"module Shared
  extend ActiveSupport::Concern

  def badge_token!(name)
    tokens.create!(name: "shared #{name}")
  end
end
"##)        .write("app/models/concerns/plain.rb", r##"module Plain
  extend ActiveSupport::Concern
  include Shared
end
"##)        .write("app/models/concerns/badged.rb", r##"module Badged
  extend ActiveSupport::Concern
  include Shared

  def badge_token!(name)
    tokens.create!(name: "badged #{name}")
  end
end
"##)        .write("app/models/concerns/redefined.rb", r##"module Redefined
  extend ActiveSupport::Concern

  def spare_token!(name)
    tokens.create!(name: name)
  end

  def spare_token!(name)
    nil
  end
end
"##)        .write("app/models/concerns/versioned.rb", r##"module Versioned
  extend ActiveSupport::Concern

  if RUBY_VERSION >= "3.0"
    def modern_token!(name)
      tokens.create!(name: "modern #{name}")
    end
  end

  if RUBY_VERSION < "3.0"
    def legacy_token!(name)
      tokens.create!(name: "legacy #{name}")
    end
  end
end
"##)        .write("app/models/shop.rb", r##"class Shop < ApplicationRecord
  has_many :tokens, as: :bearer
  include Plain, Badged
  include Redefined
  include Versioned
end
"##)        .write("app/models/stall.rb", r##"class Stall < ApplicationRecord
  has_many :tokens, as: :bearer
  include Plain, Badged
  include Redefined
  include Versioned
end
"##)        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "shops", force: :cascade do |t|
    t.string "name"
  end
  create_table "stalls", force: :cascade do |t|
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

const SHARED_NESTED_INCLUDE: &str = r##"got = [Shop.create!.badge_token!("a").name, Stall.create!.badge_token!("b").name]
expected = ["badged a", "badged b"]
raise "shared_nested_include: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn two_modules_of_one_include_keep_their_shared_module_last() {
    let run = second_app().run_ruby(SHARED_NESTED_INCLUDE);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

const LAST_DEFINITION: &str = r##"shop = Shop.create!
got = [shop.spare_token!("a"), Token.count]
expected = [nil, 0]
raise "last_definition: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_module_s_last_definition_of_a_method_is_the_one_copied() {
    let run = second_app().run_ruby(LAST_DEFINITION);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

const CONDITIONAL_DEFINITION: &str = r##"shop = Shop.create!
got = [shop.modern_token!("a").name, shop.respond_to?(:legacy_token!), Stall.new.respond_to?(:legacy_token!)]
expected = ["modern a", false, false]
raise "conditional_definition: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_definition_behind_a_load_time_if_keeps_its_condition() {
    let run = second_app().run_ruby(CONDITIONAL_DEFINITION);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_concerns_includers_run_their_winning_definitions_on_spinel_second_review() {
    for script in [SHARED_NESTED_INCLUDE, LAST_DEFINITION, CONDITIONAL_DEFINITION] {
        let run = second_app().run_spinel_with_db(script);
        run.assert_passes();
        assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
    }
}

/// Review 3: a module already in the chain as the insertion point, for
/// plain modules and for concerns; a name defined twice in one branch; a
/// query helper given the association; a guard Ruby read at load time.
fn third_app() -> emit_and_run::Overlay {
    let module = |name: &str, concern: bool, body: &str| {
        let extend = if concern { "  extend ActiveSupport::Concern\n\n" } else { "" };
        format!("module {name}\n{extend}{body}end\n")
    };
    let includer = |name: &str| {
        format!("class {name} < ApplicationRecord\n  has_many :tokens, as: :bearer\n  include Shared\n  include Wrapper\n  include PlainShared\n  include PlainWrapper\n  include Branched\n  include Fallback\n  include Guarded\nend\n")
    };
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/token.rb", "class Token < ApplicationRecord\n  belongs_to :bearer, polymorphic: true\nend\n")
        .write("app/models/picker.rb", "class Picker\n  def self.pick(rows)\n    rows.order(:name).first\n  end\nend\n")
        .write("app/models/concerns/shared.rb", &module("Shared", true, "  def issue_token!(name)\n    nil\n  end\n"))
        .write("app/models/concerns/tail.rb", &module("Tail", true, "  def issue_token!(name)\n    tokens.create!(name: \"tail #{name}\")\n  end\n"))
        .write("app/models/concerns/wrapper.rb", &module("Wrapper", true, "  include Tail\n  include Shared\n"))
        .write("app/models/concerns/plain_shared.rb", &module("PlainShared", false, "  def plain_token!(name)\n    nil\n  end\n"))
        .write("app/models/concerns/plain_tail.rb", &module("PlainTail", false, "  def plain_token!(name)\n    tokens.create!(name: \"plain tail #{name}\")\n  end\n"))
        // A module of nothing but includes is not ingested; one method keeps it.
        .write(
            "app/models/concerns/plain_wrapper.rb",
            &module("PlainWrapper", false, "  include PlainTail\n  include PlainShared\n\n  def wrapper_tag\n    \"plain wrapper\"\n  end\n"),
        )
        .write(
            "app/models/concerns/branched.rb",
            &module(
                "Branched",
                true,
                "  if RUBY_VERSION >= \"3.0\"\n    def branch_token!(name)\n      tokens.create!(name: name)\n    end\n\n    def branch_token!(name)\n      nil\n    end\n  end\n\n  def first_token_name\n    Picker.pick(tokens)&.name\n  end\n",
            ),
        )
        .write("app/models/concerns/fallback.rb", &module("Fallback", true, "  def guarded_token!(name)\n    nil\n  end\n"))
        .write(
            "app/models/concerns/guarded.rb",
            &module("Guarded", true, "  if method_defined?(:guarded_token!)\n    def guarded_token!(name)\n      tokens.create!(name: \"guarded #{name}\")\n    end\n  end\n"),
        )
        .write("app/models/shop.rb", &includer("Shop"))
        .write("app/models/stall.rb", &includer("Stall"))
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "shops", force: :cascade do |t|
    t.string "name"
  end
  create_table "stalls", force: :cascade do |t|
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

const INSERTION_POINT: &str = r##"shop = Shop.create!
got = [shop.plain_token!("a"), Token.count, shop.issue_token!("b").name, Token.count]
expected = [nil, 0, "tail b", 1]
raise "insertion_point: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_module_already_included_is_where_the_next_ones_go() {
    let run = third_app().run_ruby(INSERTION_POINT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

const SAME_BRANCH: &str = r##"got = [Shop.create!.branch_token!("a"), Stall.create!.branch_token!("b"), Token.count]
expected = [nil, nil, 0]
raise "same_branch: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_branch_runs_its_own_last_definition_of_a_name() {
    let run = third_app().run_ruby(SAME_BRANCH);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

const QUERY_ARGUMENT: &str = r##"shop = Shop.create!
stall = Stall.create!
Token.create!(bearer_type: "Shop", bearer_id: shop.id, name: "b")
Token.create!(bearer_type: "Shop", bearer_id: shop.id, name: "a")
Token.create!(bearer_type: "Stall", bearer_id: stall.id, name: "c")
got = [shop.first_token_name, stall.first_token_name]
expected = ["a", "c"]
raise "query_argument: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_query_helper_gets_each_includers_association() {
    let run = third_app().run_ruby(QUERY_ARGUMENT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

const GUARD_AT_LOAD: &str = r##"got = [Shop.create!.guarded_token!("a"), Stall.create!.guarded_token!("b"), Token.count]
expected = [nil, nil, 0]
raise "guard_at_load: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_guard_ruby_read_at_load_time_is_not_read_again_in_the_model() {
    let run = third_app().run_ruby(GUARD_AT_LOAD);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_concerns_includers_run_their_winning_definitions_on_spinel_third_review() {
    for script in [INSERTION_POINT, SAME_BRANCH, QUERY_ARGUMENT, GUARD_AT_LOAD] {
        let run = third_app().run_spinel_with_db(script);
        run.assert_passes();
        assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
    }
}

/// Review 4: a shared concern's literal reflective calls on a model, a
/// scope (`Token.public_send(:named, name)`) and an association
/// (`Shop.first.public_send(:tokens)`), which the module's own body
/// reads as the direct calls.
fn fourth_app() -> emit_and_run::Overlay {
    let includer = |name: &str| format!("class {name} < ApplicationRecord\n  has_many :tokens, as: :bearer\n  include TokenHolder\nend\n");
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/token.rb", "class Token < ApplicationRecord\n  belongs_to :bearer, polymorphic: true\n  scope :named, ->(name) { where(name: name) }\nend\n")
        .write("app/models/concerns/token_holder.rb", r##"module TokenHolder
  extend ActiveSupport::Concern

  def issue_and_count!(name)
    tokens.create!(name: name)
    Token.public_send(:named, name).count
  end

  def issue_with_first_shop!(name)
    tokens.create!(name: name)
    Shop.first.public_send(:tokens).create!(name: "first #{name}")
  end
end
"##)
        .write("app/models/shop.rb", &includer("Shop"))
        .write("app/models/stall.rb", &includer("Stall"))
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "shops", force: :cascade do |t|
    t.string "name"
  end
  create_table "stalls", force: :cascade do |t|
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

const REFLECTIVE: &str = r##"shop = Shop.create!
stall = Stall.create!
first = stall.issue_with_first_shop!("a")
got = [first.name, first.bearer_type, stall.issue_and_count!("b"), shop.issue_and_count!("b"), Token.where(bearer_type: "Stall").count]
expected = ["first a", "Shop", 1, 2, 2]
raise "reflective: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_copy_reads_a_literal_reflective_call_as_the_module_does() {
    let run = fourth_app().run_ruby(REFLECTIVE);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_copy_reads_a_literal_reflective_call_as_the_module_does_on_spinel() {
    let run = fourth_app().run_spinel_with_db(REFLECTIVE);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
