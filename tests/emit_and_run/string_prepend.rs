//! `String#prepend` answers the receiver, written to in place.

use super::emit_and_run;

#[test]
fn string_prepend_answers_the_string() {
    let overlay = || {
        emit_and_run::real_blog().edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n\n  def headed_title\n    text = title.dup\n    text.prepend(\"# \").upcase\n  end\n",
        )
    };
    let errors = overlay().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    overlay()
        .run_ruby("a = Article.new(title: \"robots\")\nraise a.headed_title unless a.headed_title == \"# ROBOTS\"\nputs \"prepend passed\"\n")
        .assert_passes();
}
