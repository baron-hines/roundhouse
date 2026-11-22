//! `x.respond_to?(:m)` over a core class whose own method `m` is: the other branch never runs, so it is not typed.

use super::emit_and_run;

#[test]
fn respond_to_on_an_array_takes_the_branch_it_answers() {
    // The untaken branch's Object would be a `MultiJson.dump` refusal, typed.
    let lib = "class Sizer\n  def self.dump(xs)\n    MultiJson.dump(xs.respond_to?(:to_ary) ? xs.to_ary : Object.new)\n  end\n\n  def self.go\n    dump([1, 2])\n  end\nend\n";
    let run = emit_and_run::real_blog()
        .edit("Gemfile.lock", "  specs:\n", "  specs:\n    multi_json (1.21.1)\n    oj (3.17.6)\n")
        .write("config/initializers/100-oj.rb", "MultiJson.use(:oj)\n")
        .write("lib/sizer.rb", lib)
        .run_ruby("raise Sizer.go.inspect unless Sizer.go == \"[1,2]\"\nputs \"fold passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("fold passed"));
}

/// Not where the program reopens the class with its own `respond_to?`, which may answer otherwise.
#[test]
fn respond_to_on_a_reopened_array_is_left_to_run() {
    let lib = "class Array\n  def respond_to?(name, all = false)\n    name == :to_ary ? false : super\n  end\nend\n\nclass Sizer\n  def self.size_of(xs)\n    xs.respond_to?(:to_ary) ? xs.size : 0\n  end\n\n  def self.go\n    size_of([1, 2])\n  end\nend\n";
    let run = emit_and_run::real_blog().write("lib/sizer.rb", lib).run_ruby("raise Sizer.go.inspect unless Sizer.go == 0\nputs \"kept passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("kept passed"));
}

/// Nor where the program reopens the class with its own definition of the asked method, which may be private.
#[test]
fn respond_to_of_a_method_the_program_redefines_is_left_to_run() {
    let lib = "class Array\n  private\n\n  def to_ary\n    self\n  end\nend\n\nclass Sizer\n  def self.size_of(xs)\n    xs.respond_to?(:to_ary) ? xs.size : 0\n  end\n\n  def self.go\n    size_of([1, 2])\n  end\nend\n";
    let run = emit_and_run::real_blog().write("lib/sizer.rb", lib).run_ruby("raise Sizer.go.inspect unless Sizer.go == 0\nputs \"private kept passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("private kept passed"));
}
