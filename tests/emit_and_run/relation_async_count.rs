//! `relation.async_count(column = nil)` answers an `ActiveRecord::Promise`
//! of the count (a Hash for a grouped relation), as Rails' does. With no
//! async executor configured Rails runs the query at once, and so does
//! the runtime.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/comment.rb", r##"class Comment < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, actual, expected)
    raise "#{label}: #{actual.inspect}, Rails answers #{expected.inspect}" unless actual == expected
  end

  def self.run
    [[4, true], [4, true], [5, true], [6, false], [6, true], [6, true]].each { |post, approved| Comment.create!(post_id: post, approved: approved) }
    per_post = Comment.where(approved: true).group(:post_id).async_count
    check("grouped", per_post.value.sort, [[4, 2], [5, 1], [6, 2]])
    promise = Comment.where(post_id: 6).async_count(:all)
    check("plain", [promise.value, promise.pending?, promise.is_a?(ActiveRecord::Promise)], [3, false, true])
    check("then", Comment.where(approved: false).async_count(:id).then { |n| n + 100 }.value, 101)
    grouped = Comment.group(:post_id)
    check("grouped through a local", grouped.async_count.then { |counts| counts.keys.sort }.value, [4, 5, 6])
    unmatched = Comment.where(post_id: 0).group(:post_id)
    check("an empty grouped count through a local", unmatched.async_count.then { |counts| counts.blank? }.value, true)
    posts = Comment.select(:post_id).distinct
    check("distinct select", [posts.async_count.value, posts.async_count(:id).value], [3, 6])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "comments", force: :cascade do |t|
    t.integer "post_id"
    t.boolean "approved", default: false
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn async_count_answers_a_promise_of_the_count() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn async_count_answers_a_promise_of_the_count_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
