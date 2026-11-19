//! maybe's `Current`: `delegate :family, to: :user, allow_nil: true`, where
//! `user` is a method (`impersonated_user || session&.user`), not an
//! attribute. Rails' delegate calls the method, so `Current.family` follows
//! the session; reading an ivar named after it answered nil always.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        "app/models/current.rb",
        "class Current < ActiveSupport::CurrentAttributes\n  attribute :article_id\n\n  delegate :title, to: :article, allow_nil: true\n  delegate :body, to: :article\n  delegate :name, to: :class, prefix: true\n  delegate :to_s, to: :class, prefix: true, allow_nil: true\n  delegate :size, to: :KINDS, allow_nil: true\n  KINDS = %w[a b].freeze\n\n  def article\n    Article.find_by(id: article_id)\n  end\nend\n",
    )
}

const ASSERTIONS: &str = r##"
a = Article.create!(title: "Hello", body: "A body long enough for validation.")
Current.article_id = a.id
raise "title: #{Current.title.inspect}" unless Current.title == "Hello"
raise "body: #{Current.body.inspect}" unless Current.body == "A body long enough for validation."
raise "class_name: #{Current.class_name.inspect}" unless Current.class_name == "Current"
raise "class_to_s: #{Current.class_to_s.inspect}" unless Current.class_to_s == "Current"
raise "size: #{Current.size.inspect}" unless Current.size == 2
Current.reset
raise "reset: #{Current.title.inspect}" unless Current.title.nil?
puts "delegate passed"
"##;

#[test]
fn a_delegate_to_a_method_calls_it() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("delegate passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_delegate_to_a_method_calls_it_on_spinel() {
    let run = app().run_spinel_with_db(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("delegate passed"), "{}", run.stdout);
}
