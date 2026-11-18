//! `record.errors.messages` read from code the compiler does not lower
//! (an RSpec spec run against the emitted app, a gem's helper) answers
//! Rails' Hash of each attribute's messages, with `[]` for an attribute
//! without errors. The CRuby accumulator answered only `[]`, so the read
//! raised NoMethodError. The script below runs as written, unlowered;
//! the expected values are ActiveRecord 8.1.4's answers.
//!
//! CRuby only: the accumulator's reader is the Ruby target's overlay, and
//! a Spinel program has no unlowered code to read it from (a lowered
//! `errors.messages` is `ActiveSupport.errors_messages` on every target).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/memo.rb", r##"class Memo < ApplicationRecord
  validates :body, presence: true, length: { maximum: 5 }
  validate :heading_or_body

  def heading_or_body
    errors.add(:base, "needs a heading") if heading.to_s.empty?
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "memos", force: :cascade do |t|
    t.string "heading"
    t.string "body"
  end
end
"##)
}

const SCRIPT: &str = "def check(label, ok)\n  raise \"#{label} differs from Rails\" unless ok\nend\nblank = Memo.new(body: \"\")\nblank.valid?\nlong = Memo.new(heading: \"h\", body: \"toolong\")\nlong.valid?\n# Rails: {body: [\"can't be blank\"], base: [\"needs a heading\"]}, [\"is too long (maximum is 5 characters)\"], [], true\ncheck(\"messages\", blank.errors.messages == { body: [\"can't be blank\"], base: [\"needs a heading\"] })\ncheck(\"messages dig\", long.errors.messages.dig(:body) == [\"is too long (maximum is 5 characters)\"])\ncheck(\"messages of a clean attribute\", long.errors.messages[:heading] == [])\ncheck(\"messages frozen\", long.errors.messages.frozen?)\nputs \"probe passed\"\n";

#[test]
fn errors_messages_read_from_unlowered_code_answers_rails_hash() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
