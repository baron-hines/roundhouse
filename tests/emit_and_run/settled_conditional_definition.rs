//! A load-time `if defined?(K)` around method definitions, where `K` is a
//! class the app carries, is settled as the body loads: Ruby defines only
//! the first arm's methods, so the other arm's (et-orbi's tzinfo 1.x
//! fallbacks, naming classes tzinfo 2 does not have) is not the program's.

use super::emit_and_run;

const PROBE: &str = "module SettledProbe\n  class << self\n    if defined?(Article)\n      def kind\n        \"article\"\n      end\n    else\n      def kind\n        Missing::Thing.kind\n      end\n    end\n  end\nend\n";

#[test]
fn a_defined_test_of_a_carried_class_keeps_its_first_arm() {
    let run = emit_and_run::real_blog()
        .write("app/models/settled_probe.rb", PROBE)
        .run_ruby("raise SettledProbe.kind unless SettledProbe.kind == \"article\"\nputs \"settled conditional passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("settled conditional passed"), "{}", run.stdout);
}

const WRITER_PROBE: &str = "class SettledBase\n  class_attribute :status, default: :ok\nend\n\nclass SettledSub < SettledBase\n  if respond_to?(:status=)\n    self.status = :found\n  else\n    def self.status\n      :legacy\n    end\n  end\nend\n";

/// devise's `Responder`: the class answers `respond_to?(:status=)` for its superclass's
/// `class_attribute`, so the first arm's write runs as the body loads and the `else` arm's methods are not defined.
#[test]
fn a_class_bodys_own_respond_to_keeps_the_arm_whose_writes_then_run() {
    let run = emit_and_run::real_blog()
        .write("app/models/settled_base.rb", WRITER_PROBE)
        .run_ruby("raise SettledSub.status.inspect unless SettledSub.status == :found && SettledBase.status == :ok\nputs \"settled writer passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("settled writer passed"), "{}", run.stdout);
}

const DEPENDENCIES_PROBE: &str = "module DependenciesProbe\n  def self.get(name)\n    if ActiveSupport::Dependencies.respond_to?(:constantize)\n      ActiveSupport::Dependencies.constantize(name)\n    else\n      name.constantize\n    end\n  end\nend\n";

/// devise's `Getter#get`: Rails 7 removed the classic autoloader's `constantize`, so the probe takes the `else` arm.
#[test]
fn a_probe_of_the_classic_autoloader_takes_the_rails_7_arm() {
    let run = emit_and_run::real_blog()
        .write("app/models/dependencies_probe.rb", DEPENDENCIES_PROBE)
        .run_ruby("raise \"constantize\" unless DependenciesProbe.get(\"Article\") == Article\nputs \"dependencies probe passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("dependencies probe passed"), "{}", run.stdout);
}

const LATE_PROBE: &str = "class LateProbe\n  if respond_to?(:ready)\n    def self.kind\n      :early\n    end\n  else\n    def self.kind\n      :late\n    end\n  end\n\n  def self.ready\n    true\n  end\nend\n";

/// A class method the body defines further down is not there when the conditional runs, so the `else` arm is Ruby's.
#[test]
fn a_class_method_defined_below_the_conditional_does_not_answer_it() {
    let run = emit_and_run::real_blog()
        .write("app/models/late_probe.rb", LATE_PROBE)
        .run_ruby("raise LateProbe.kind.inspect unless LateProbe.kind == :late\nputs \"late probe passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("late probe passed"), "{}", run.stdout);
}
