//! `h()` beside a local `h`: Ruby calls the method, and the emitted call keeps its parentheses so it still does.

use super::emit_and_run;

#[test]
fn a_parenthesized_call_named_like_a_local_calls_the_method() {
    let lib = "class Shadow\n  def self.h\n    \"method\"\n  end\n\n  def self.both\n    h = \"local\"\n    [h, h()].join(\",\")\n  end\nend\n";
    emit_and_run::real_blog()
        .write("app/models/shadow.rb", lib)
        .run_ruby("raise Shadow.both unless Shadow.both == \"local,method\"\nputs \"paren passed\"\n")
        .assert_passes();
}
