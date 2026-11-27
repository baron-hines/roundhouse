//! A concern's instance method reading a `has_many` its includers declare,
//! when more than one model includes the concern: each includer's call is
//! its own association, as Rails runs it. A polymorphic association writes
//! and reads the includer's own type; a plain one, its own foreign key. An
//! includer's own def of the method still wins. `<has_many>.all` is the
//! association's relation, in a model's own body as in a concern's. The probe's expected values
//! are ActiveRecord 8.1.4.1's answers.

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
        .write("app/models/note.rb", r##"class Note < ApplicationRecord
  belongs_to :shop, optional: true
  belongs_to :stall, optional: true
end
"##)
        .write("app/models/concerns/token_holder.rb", r##"module TokenHolder
  extend ActiveSupport::Concern

  def issue_token!(name)
    tokens.create!(name: name)
  end

  def token_names_like(name)
    tokens.where(name: name).map(&:name)
  end

  def token_named(name)
    tokens.find_by(name: name)
  end

  def all_token_names
    tokens.all.map(&:name)
  end

  def revoke_tokens!
    tokens.destroy_all
  end

  def add_note!(body)
    notes.create!(body: body)
  end

  def note_bodies
    notes.all.map(&:body)
  end
end
"##)
        .write("app/models/shop.rb", r##"class Shop < ApplicationRecord
  include TokenHolder
  has_many :tokens, as: :bearer
  has_many :notes

  def own_token_names
    tokens.all.map(&:name)
  end
end
"##)
        .write("app/models/stall.rb", r##"class Stall < ApplicationRecord
  include TokenHolder
  has_many :tokens, as: :bearer
  has_many :notes
end
"##)
        .write("app/models/kiosk.rb", r##"class Kiosk < ApplicationRecord
  include TokenHolder
  has_many :tokens, as: :bearer
  has_many :notes

  def issue_token!(name)
    tokens.create!(name: "kiosk #{name}")
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.reset
    [Token, Note, Shop, Stall, Kiosk].each(&:delete_all)
  end

  # Rails: ["Shop", true, "Stall", true]
  def self.create_through_each_includer
    shop = Shop.create!(name: "s"); stall = Stall.create!(id: shop.id, name: "t")
    a = shop.issue_token!("first"); b = stall.issue_token!("first")
    [a.bearer_type, a.bearer_id == shop.id, b.bearer_type, b.bearer_id == stall.id]
  end

  # Rails: [["first"], "Stall", nil, ["first"], ["first", "second"]]
  def self.queries_through_each_includer
    shop = Shop.create!(name: "s"); stall = Stall.create!(id: shop.id, name: "t")
    shop.issue_token!("first"); stall.issue_token!("first"); stall.issue_token!("second")
    [shop.token_names_like("first"), stall.token_named("second").bearer_type, shop.token_named("second"),
     shop.all_token_names, stall.all_token_names.sort]
  end

  # Rails: [2, ["first"], [], 1]
  def self.destroy_all_through_one_includer
    shop = Shop.create!(name: "s"); stall = Stall.create!(id: shop.id, name: "t")
    shop.issue_token!("first"); stall.issue_token!("first"); stall.issue_token!("second")
    [stall.revoke_tokens!.size, shop.all_token_names, stall.all_token_names, Token.count]
  end

  # Rails: [["a"], ["b"], true, true]
  def self.plain_has_many_through_each_includer
    shop = Shop.create!(name: "s"); stall = Stall.create!(id: shop.id, name: "t")
    a = shop.add_note!("a"); b = stall.add_note!("b")
    [shop.note_bodies, stall.note_bodies, a.shop_id == shop.id && a.stall_id.nil?, b.stall_id == stall.id && b.shop_id.nil?]
  end

  # Rails: ["first"]: `all` in the model's own body, too.
  def self.all_in_a_models_own_method
    shop = Shop.create!(name: "s"); stall = Stall.create!(id: shop.id, name: "t")
    shop.issue_token!("first"); stall.issue_token!("second")
    shop.own_token_names
  end

  # Rails: ["kiosk first", "Kiosk"]
  def self.an_includers_own_def_wins
    t = Kiosk.create!(name: "k").issue_token!("first")
    [t.name, t.bearer_type]
  end

  def self.run
    reset
    check("create_through_each_includer", create_through_each_includer == ["Shop", true, "Stall", true])
    reset
    check("queries_through_each_includer", queries_through_each_includer == [["first"], "Stall", nil, ["first"], ["first", "second"]])
    reset
    check("destroy_all_through_one_includer", destroy_all_through_one_includer == [2, ["first"], [], 1])
    reset
    check("plain_has_many_through_each_includer", plain_has_many_through_each_includer == [["a"], ["b"], true, true])
    reset
    check("all_in_a_models_own_method", all_in_a_models_own_method == ["first"])
    reset
    check("an_includers_own_def_wins", an_includers_own_def_wins == ["kiosk first", "Kiosk"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "shops", force: :cascade do |t|
    t.string "name"
  end
  create_table "stalls", force: :cascade do |t|
    t.string "name"
  end
  create_table "kiosks", force: :cascade do |t|
    t.string "name"
  end
  create_table "tokens", force: :cascade do |t|
    t.string "bearer_type"
    t.integer "bearer_id"
    t.string "name"
  end
  create_table "notes", force: :cascade do |t|
    t.integer "shop_id"
    t.integer "stall_id"
    t.string "body"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_concern_reads_each_includers_has_many_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_concern_reads_each_includers_has_many_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
