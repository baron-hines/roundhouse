//! A polymorphic `belongs_to` that no model names with an inverse `as:`
//! still reads and writes any model, as Rails' reader constantizes the
//! class name the row holds: the writer stores the record's base class
//! (an STI subclass's root) and its own key (a `Ticket` keyed by `code`),
//! `create!` takes the association, and the reader answers the stored
//! row. The association is named after one of
//! its classes (`:post`, which a `Photo` can also fill), so its preload
//! must go by the type column too rather than load every key from
//! `posts`. The expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/post.rb", r##"class Post < ApplicationRecord
end
"##)
        .write("app/models/photo.rb", r##"class Photo < ApplicationRecord
end
"##)
        .write("app/models/panorama.rb", r##"class Panorama < Photo
end
"##)
        .write("app/models/ticket.rb", r##"class Ticket < ApplicationRecord
  self.primary_key = "code"
end
"##)
        .write("app/models/comment.rb", r##"class Comment < ApplicationRecord
  belongs_to :post, polymorphic: true, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    essay = Post.create!(title: "essay")
    harbor = Photo.create!(title: "harbor")
    skyline = Panorama.create!(title: "skyline")
    Comment.create!(body: "a", post: essay)
    Comment.create!(body: "b", post: skyline)
    Comment.create!(body: "c", post_type: "Photo", post_id: harbor.id)
    Comment.create!(body: "d")
    triage = Ticket.create!(code: 40, title: "triage")
    Comment.create!(body: "e", post: triage)
    # Rails: [["Post", essay.id], ["Photo", skyline.id], ["Photo", harbor.id], [nil, nil], ["Ticket", 40]]
    check("stored", Comment.order(:body).map { |c| [c.post_type, c.post_id] } == [["Post", essay.id], ["Photo", skyline.id], ["Photo", harbor.id], [nil, nil], ["Ticket", 40]])
    # Rails: ["essay", "skyline", "harbor", nil, "triage"]
    check("reader", Comment.order(:body).map { |c| c.post && c.post.title } == ["essay", "skyline", "harbor", nil, "triage"])
    check("preload", Comment.order(:body).preload(:post).map { |c| c.post && c.post.title } == ["essay", "skyline", "harbor", nil, "triage"])
    moved = Comment.find_by(body: "a")
    moved.post = harbor
    moved.save!
    # Rails: ["Photo", "harbor"]
    check("reassigned", [Comment.find_by(body: "a").post_type, Comment.find_by(body: "a").post.title] == ["Photo", "harbor"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "posts", force: :cascade do |t|
    t.string "title"
  end
  create_table "photos", force: :cascade do |t|
    t.string "title"
    t.string "type"
  end
  create_table "tickets", primary_key: "code", force: :cascade do |t|
    t.string "title"
  end
  create_table "comments", force: :cascade do |t|
    t.string "body"
    t.string "post_type"
    t.integer "post_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_polymorphic_belongs_to_with_no_inverse_reads_any_model() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_polymorphic_belongs_to_with_no_inverse_reads_any_model_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
