//! A model whose include defines `method_missing` still answers
//! ActiveRecord's own instance methods (`association`,
//! `has_changes_to_save?`, `previously_new_record?`): Ruby finds them in the
//! model's ancestors, so they never reach `method_missing`, and
//! `association(:x).loaded?` lowers as it does on any model.
//! Expected values are what ActiveRecord 8.1.4.1 answers for the same app.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/concerns/kind_predicates.rb", r##"module KindPredicates
  extend ActiveSupport::Concern

  def method_missing(name, *args)
    name.to_s.end_with?("?") ? false : super
  end

  def respond_to_missing?(name, include_private = false)
    name.to_s.end_with?("?") || super
  end
end
"##)
        .write("app/models/account.rb", "class Account < ApplicationRecord\n  has_many :authors\nend\n")
        .write("app/models/author.rb", "class Author < ApplicationRecord\n  include KindPredicates\n  belongs_to :account, optional: true\n  has_many :posts\nend\n")
        .write("app/models/post.rb", r##"class Post < ApplicationRecord
  belongs_to :author, optional: true

  def author_with_account
    return author if author&.association(:account)&.loaded?

    Author.includes(:account).find_by(id: author_id)
  end

  def author_flags
    a = author
    [a.association(:account).loaded?, a.has_changes_to_save?, a.previously_new_record?, a.new_record?, a.admin?]
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "accounts", force: :cascade do |t|
    t.string "name", null: false
  end
  create_table "authors", force: :cascade do |t|
    t.string "name", null: false
    t.bigint "account_id"
  end
  create_table "posts", force: :cascade do |t|
    t.string "title", null: false
    t.bigint "author_id"
  end
end
"##)
}

const SCRIPT: &str = r##"account = Account.create!(name: "a")
author = Author.create!(name: "w", account_id: account.id)
post = Post.create!(title: "t", author_id: author.id)
fresh = Post.find(post.id)
got = [fresh.author_flags, fresh.author_with_account.id]
expected = [[false, false, false, false, false], author.id]
raise "model active record methods: #{got.inspect}" unless got == expected
puts "probe passed"
"##;

#[test]
fn a_model_with_method_missing_answers_active_records_own_methods() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_model_with_method_missing_answers_active_records_own_methods_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
