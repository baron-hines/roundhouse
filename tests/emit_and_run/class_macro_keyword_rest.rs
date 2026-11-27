//! maybe's `Monetizable.monetize(*fields)`: `define_method("#{field}_money") do |**args| … self.send(field, **args)`,
//! a class macro whose defined methods take and pass on a keyword rest. Expanded at each call, the methods read the
//! keywords they are given as Ruby's do.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/models/concerns/doubled.rb",
            "module Doubled\n  extend ActiveSupport::Concern\n\n  class_methods do\n    def doubled(*fields)\n      fields.each do |field|\n        define_method(\"#{field}_doubled\") do |**args|\n          value = self.send(field, **args)\n\n          return nil if value.nil?\n\n          value * 2\n        end\n      end\n    end\n  end\nend\n",
        )
        .write(
            "app/models/scoreboard.rb",
            "class Scoreboard\n  include Doubled\n\n  doubled :total\n\n  def initialize(base)\n    @base = base\n  end\n\n  def total(extra: 0)\n    @base + extra\n  end\n\n  def label\n    total_doubled.to_s + \"/\" + total_doubled(extra: 1).to_s\n  end\nend\n",
        )
}

const SCRIPT: &str = r#"
board = Scoreboard.new(5)
got = [board.total_doubled, board.total_doubled(extra: 1), board.label]
raise "got #{got.inspect}" unless got == [10, 12, "10/12"]
puts "keyword rest passed"
"#;

#[test]
fn a_class_macros_keyword_rest_method_passes_its_keywords() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("keyword rest passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_class_macros_keyword_rest_method_passes_its_keywords_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("keyword rest passed"), "{}", run.stdout);
}
