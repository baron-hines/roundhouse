//! `left_outer_joins(:post, :author)` joins both, as Rails' variadic
//! `joins` family does. lobsters' `Moderation.for_story` is
//! `left_outer_joins(:story, :comment)`; the runtime took one spec, so
//! every story page a moderator opened raised ArgumentError (500). With
//! no association at all each raises ArgumentError, as Rails does.

use super::emit_and_run;

const SCRIPT: &str = r#"
alice = Author.create!(name: "alice")
bob = Author.create!(name: "bob")
hello = Post.create!(title: "hello")
other = Post.create!(title: "other")
Comment.create!(post: hello, author: bob)
Comment.create!(post: other, author: alice)
Comment.create!(post: other, author: bob)
got = [
  Comment.on_post_or_by("hello", "alice").count,
  Comment.inner_both("other", "bob").count,
]
raise "joins: #{got.inspect}" unless got == [2, 1]
messages = %w[joins left_outer_joins left_joins].map do |m|
  begin
    Comment.all.public_send(m)
    "no error"
  rescue ArgumentError => e
    e.message
  end
end
want = ["The method .joins() must contain arguments.", "The method .left_outer_joins() must contain arguments.", "The method .left_joins() must contain arguments."]
raise "empty joins: #{messages.inspect}" unless messages == want
puts "joins passed"
"#;

#[test]
fn the_joins_family_takes_several_associations() {
    let run = emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/author.rb", "class Author < ApplicationRecord\nend\n")
        .write("app/models/post.rb", "class Post < ApplicationRecord\nend\n")
        .write(
            "app/models/comment.rb",
            r#"class Comment < ApplicationRecord
  belongs_to :post
  belongs_to :author

  scope :on_post_or_by, ->(title, name) {
    left_outer_joins(:post, :author).where("posts.title = ? OR authors.name = ?", title, name)
  }

  scope :inner_both, ->(title, name) {
    joins(:post, :author).where("posts.title = ? AND authors.name = ?", title, name)
  }
end
"#,
        )
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"authors\", force: :cascade do |t|\n    t.string \"name\"\n  end\n  create_table \"posts\", force: :cascade do |t|\n    t.string \"title\"\n  end\n  create_table \"comments\", force: :cascade do |t|\n    t.integer \"post_id\"\n    t.integer \"author_id\"\n  end\nend\n",
        )
        .run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("joins passed"), "{}", run.stdout);
}
