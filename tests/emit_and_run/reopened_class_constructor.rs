//! A class opened again in another file keeps its constructor lookup
//! verified when the reopening replaces nothing. rotp's `class OTP` is
//! reopened in otp/uri.rb only to nest `class URI`, and lobsters'
//! `ROTP::TOTP.new(secret, issuer: …)` was refused as "unmodeled
//! constructor lookup" and raised on the Spinel tree. A reopening that adds
//! `self.new` is found as Ruby finds it.

use super::emit_and_run;

const ASSERTIONS: &str = r#"
t = Otp::Timed.new("secret", issuer: "lobsters", interval: 60)
raise "timed: #{[t.issuer, t.s, t.interval].inspect}" unless [t.issuer, t.s, t.interval] == ["lobsters", "secret", 60]
raise "custom new: #{ReopenedConstructor.build.inspect}" unless ReopenedConstructor.build == "custom:z"
puts "reopened constructors passed"
"#;

#[test]
fn a_reopening_that_replaces_nothing_keeps_keyword_constructors() {
    let run = emit_and_run::real_blog()
        .write(
            "app/models/otp/base.rb",
            "module Otp\n  class Base\n    attr_reader :issuer, :s\n\n    def initialize(s, options = {})\n      @s = s\n      @issuer = options[:issuer]\n    end\n  end\nend\n",
        )
        .write(
            "app/models/otp/uri.rb",
            "module Otp\n  class Base\n    class Uri\n      def initialize(otp)\n        @otp = otp\n      end\n    end\n  end\nend\n",
        )
        .write(
            "app/models/otp/timed.rb",
            "module Otp\n  class Timed < Otp::Base\n    attr_reader :interval\n\n    def initialize(s, options = {})\n      @interval = options[:interval] || 30\n      super\n    end\n  end\nend\n",
        )
        .write(
            "app/models/reopened_constructor.rb",
            "class ReopenedConstructor\n  def initialize(label: \"default\")\n    @label = label\n  end\n\n  def self.build\n    ReopenedConstructor.new(label: \"z\")\n  end\nend\n",
        )
        .write(
            "app/models/reopened_constructor_new.rb",
            "class ReopenedConstructor\n  def self.new(label:)\n    \"custom:#{label}\"\n  end\nend\n",
        )
        .run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("reopened constructors passed"), "{}", run.stdout);
}
