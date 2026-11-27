use super::emit_and_run;

const PROBE: &str = r#"class DateErrorProbe
  def self.parse(text)
    Date.parse(text).iso8601
  rescue Date::Error => error
    error.message
  end

  def self.iso(text)
    Date.iso8601(text).iso8601
  rescue ::Date::Error => error
    error.message
  end
end
"#;

const ASSERTIONS: &str = r#"raise "wrong superclass" unless Date::Error.superclass == ArgumentError
raise "valid parse changed" unless DateErrorProbe.parse("2024-02-29") == "2024-02-29"
raise "valid iso8601 changed" unless DateErrorProbe.iso("2024-02-29") == "2024-02-29"
raise "parse rescue lost" unless DateErrorProbe.parse("invalid") == "invalid date"
raise "iso8601 rescue lost" unless DateErrorProbe.iso("invalid") == "invalid date"
puts "date error passed"
"#;

fn app(source: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("app/models/date_error_probe.rb", source)
}

#[test]
fn date_error_rescue_runs() {
    app(PROBE).run_ruby(ASSERTIONS).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn date_error_rescue_runs_on_spinel() {
    app(PROBE).run_spinel(ASSERTIONS).assert_passes();
}

const CLASS_ONLY: &str = r#"class DateErrorProbe
  def self.error_class
    Date::Error
  end
end
"#;

const CLASS_ASSERTIONS: &str = r#"raise "wrong class" unless DateErrorProbe.error_class == Date::Error
raise "wrong superclass" unless DateErrorProbe.error_class.superclass == ArgumentError
puts "date error class passed"
"#;

#[test]
fn date_error_class_runs() {
    app(CLASS_ONLY).run_ruby(CLASS_ASSERTIONS).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn date_error_class_loads_package_on_spinel() {
    app(CLASS_ONLY).run_spinel(CLASS_ASSERTIONS).assert_passes();
}
