//! maybe's `class Provider::Synth < Provider; include ExchangeRateConcept,
//! SecurityConcept`: a class that mixes in more than one module of its
//! superclass's namespace. Rubydex 0.4 answers nothing for those mixins, or
//! for the constants read through them (one module alone resolves), so the
//! includes and `Rate.new(…)` were reported. Ruby's lookup finds each through
//! the superclass, and the methods read the modules' constants.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/models/shop.rb", "class Shop\n  def self.kind = \"shop\"\nend\n")
        .write(
            "app/models/shop/rate_concept.rb",
            "module Shop::RateConcept\n  extend ActiveSupport::Concern\n\n  Rate = Data.define(:date, :rate)\n  LIMIT = 3\n\n  def limit = LIMIT\nend\n",
        )
        .write("app/models/shop/price_concept.rb", "module Shop::PriceConcept\n  CAP = 9\nend\n")
        .write(
            "app/models/shop/synth.rb",
            "class Shop::Synth < Shop\n  include RateConcept, PriceConcept\n\n  def fetch = Rate.new(date: \"d\", rate: LIMIT + CAP)\nend\n",
        )
}

const ASSERTIONS: &str = r#"s = Shop::Synth.new
got = [s.fetch.rate, s.fetch.date, s.limit]
raise got.inspect unless got == [12, "d", 3]
puts "mixed in constants passed"
"#;

#[test]
fn constants_of_several_mixed_in_modules_resolve() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("mixed in constants passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn constants_of_several_mixed_in_modules_resolve_on_spinel() {
    let run = app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("mixed in constants passed"), "{}", run.stdout);
}
