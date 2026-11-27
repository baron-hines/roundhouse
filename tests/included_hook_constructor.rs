//! A module whose `included` hook registers a filter through an eval block
//! on its receiver, guarded so that only controllers run it, doesn't change
//! its includer's constructor. The keyword call to a plain includer's `new`
//! used to be refused because any eval block on the receiver counted as a
//! possible replacement of `new`. Rails 8.1.4:
//!
//!     ruby tests/included_hook_constructor/oracle.rb
//!
//! The analysis-level matrix of hooks that must stay refused is in
//! `tests/constructor_unrelated_mutations.rs`.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const TAGGING: &str = r#"module Tagging
  def self.included(base)
    base.instance_eval do
      before_action(:load_tag) if base.methods.include?(:before_action)
    end
  end

  def load_tag
    @tag = "loaded"
  end

  def tag_label
    "tag"
  end
end
"#;

const WIDGET_CLIENT: &str = r##"class WidgetClient
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
"##;

#[test]
fn a_plain_includer_keeps_its_keyword_new() {
    let run = emit_and_run::empty_app()
        .write("app/models/concerns/tagging.rb", TAGGING)
        .write("app/models/widget_client.rb", WIDGET_CLIENT)
        .run_ruby("puts WidgetReport.lines\n");
    run.assert_passes();
    assert!(run.stdout.contains("tag:w:3\ntag:x:9"), "{}", run.stdout);
}
