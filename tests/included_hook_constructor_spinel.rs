//! The Spinel side of `tests/included_hook_constructor.rs`. Needs a Spinel
//! compiler, so `#[ignore]`d like its sibling suites; CI's
//! `spinel-framework` job runs it with `--ignored`
//! (`scripts/ci-plan.py`'s `SPINEL_TESTS`).
//!
//! The hook here calls an accessor on its receiver rather than using the
//! eval-block form of the CRuby test: Spinel doesn't compile
//! `base.instance_eval do ... end` or `base.methods`, with or without this
//! analysis, so that form can't reach a binary yet. An app method that makes
//! no call is the part of the same check that Spinel can run.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_hook_reading_an_accessor_keeps_the_includers_keyword_new_on_spinel() {
    let run = emit_and_run::empty_app()
        .write(
            "app/models/tagging.rb",
            r#"module Tagging
  def self.included(base)
    base.registry_name
  end

  def tag_label
    "tag"
  end
end
"#,
        )
        .write(
            "app/models/widget_client.rb",
            r##"class WidgetClient
  def self.registry_name
    @registry_name
  end

  include Tagging

  def initialize(args)
    @name = args[:name]
    @size = args.fetch(:size, 3)
  end

  def label
    "#{tag_label}:#{@name}:#{@size}"
  end
end

class WidgetReport
  def self.lines
    [WidgetClient.new(name: "w").label, WidgetClient.new(name: "x", size: 9).label]
  end
end
"##,
        )
        .run_spinel("puts WidgetReport.lines\n");
    run.assert_passes();
    assert!(run.stdout.contains("tag:w:3\ntag:x:9"), "{}", run.stdout);
}
