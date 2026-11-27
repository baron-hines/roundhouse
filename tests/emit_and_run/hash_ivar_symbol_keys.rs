//! A Hash ivar seeded `{}` and written with Symbol keys keeps them
//! Symbols. The write widening typed it `Hash[String, _]` whatever the
//! key, and the Ruby emit then rewrote each `:runs` to `"runs"` on the
//! ivar — consistent inside the class, wrong to anyone who read the
//! Hash itself (`keys`, a lookup from outside).

use super::emit_and_run;

const TALLY: &str = r##"class Tally
  attr_reader :counts

  def initialize
    @counts = {}
  end

  def record
    @counts[:runs] = 1
    self
  end

  def runs
    @counts[:runs]
  end
end
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/tally.rb", TALLY)
}

const ASSERTIONS: &str = r##"tally = Tally.new.record
got = [tally.runs, tally.counts.keys, tally.counts[:runs]]
raise got.inspect unless got == [1, [:runs], 1]
puts "hash ivar symbol keys passed"
"##;

#[test]
fn hash_ivar_symbol_keys_run() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("hash ivar symbol keys passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn hash_ivar_symbol_keys_run_on_spinel() {
    let run = app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("hash ivar symbol keys passed"), "{}", run.stdout);
}
