//! `DateTime` on Spinel is its `date` package's (ruby/date), loaded when
//! the program names it. The tree used to carry a program-defined `Date`
//! with no `DateTime` (svg-graph's `TimeSeries`, behind lobsters'
//! `/stats`), which could not load beside the package either.

use super::emit_and_run;

const STAMP: &str = r##"class Stamp
  def self.at(text)
    DateTime.strptime(text, "%Y-%m-%d %H:%M")
  end

  def self.label(text)
    at(text).strftime("%b %d %H:%M")
  end
end
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/stamp.rb", STAMP)
}

const ASSERTIONS: &str = r##"at = Stamp.at("2024-01-02 03:04")
got = [at.year, at.month, at.day, at.hour, at.minute, Stamp.label("2024-12-31 23:59"), (at + 1).day]
raise got.inspect unless got == [2024, 1, 2, 3, 4, "Dec 31 23:59", 3]
puts "date time package passed"
"##;

#[test]
fn date_time_package_runs() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("date time package passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn date_time_package_runs_on_spinel() {
    let run = app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("date time package passed"), "{}", run.stdout);
}
