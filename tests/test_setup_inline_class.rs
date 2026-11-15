//! A class a test's `setup do` block declares (maybe's `class MockProcessor; include PlaidAccount::TypeMappable; end`),
//! whose body only declares: Ruby defines it in the test class's lexical scope as the block first runs, and each
//! later run reopens it to the same effect. Ingest refused it ("class/module defined inside a method or block"); it is
//! now hoisted with the test class's inline classes.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const CONCERN: &str = "module Shouty\n  def shout(word)\n    word.upcase\n  end\nend\n";

const TEST: &str = r#"require "test_helper"

class ShoutyTest < ActiveSupport::TestCase
  setup do
    class Speaker
      include Shouty
    end

    @speaker = Speaker.new
  end

  test "the hoisted class answers its concern's method" do
    assert_equal "HI", @speaker.shout("hi")
  end
end
"#;

#[test]
fn a_class_declared_in_setup_runs_on_ruby() {
    emit_and_run::real_blog()
        .write("app/models/concerns/shouty.rb", CONCERN)
        .write("test/models/shouty_test.rb", TEST)
        .run_test("test/models/shouty_test.rb")
        .assert_passes();
}

/// Ruby runs the class body before `setup`, so a class both define ends with
/// setup's methods; hoisted side by side the body's would win. Either order
/// stays refused.
#[test]
fn a_class_both_setup_and_the_body_define_stays_refused() {
    for test in [
        "require \"test_helper\"\n\nclass ShoutyTest < ActiveSupport::TestCase\n  setup do\n    class Speaker\n      def value = 2\n    end\n  end\n\n  class Speaker\n    def value = 1\n  end\nend\n",
        "require \"test_helper\"\n\nclass ShoutyTest < ActiveSupport::TestCase\n  class Speaker\n    def value = 1\n  end\n\n  setup do\n    class Speaker\n      def value = 2\n    end\n  end\nend\n",
    ] {
        let refused = match std::panic::catch_unwind(|| emit_and_run::real_blog().write("test/models/shouty_test.rb", test).ingest_warnings()) {
            Ok(warnings) => warnings.join("\n"),
            Err(panic) => panic.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(refused.contains("defined twice") || refused.contains("defined inside a method or block"), "{refused}");
    }
}
