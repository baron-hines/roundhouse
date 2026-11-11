//! A controller that includes a Concern can call the Concern's class
//! methods as `self.class.name`, as devise's `DeviseController` does for
//! `Devise::Controllers::ScopedViews`. A call needs the method on the
//! emitted class. One that nothing calls stays off it.

use super::emit_and_run;
use emit_and_run::Overlay;
use roundhouse::project::BuildTarget;

const CONCERN: &str = r#"module Scoped
  extend ActiveSupport::Concern

  FALLBACK = "plain"

  class_methods do
    def scope_label
      fallback_label
    end

    def fallback_label
      FALLBACK
    end

    def never_called
      "unused"
    end
  end
end
"#;

const CONTROLLER: &str = r#"class PanelsController < ApplicationController
  include Scoped

  def label
    self.class.scope_label
  end
end
"#;

fn app(controller: &str) -> Overlay {
    emit_and_run::real_blog()
        .write("app/controllers/concerns/scoped.rb", CONCERN)
        .write("app/controllers/panels_controller.rb", controller)
}

fn emitted(app: Overlay) -> String {
    let (dir, _errors) = app.emit(BuildTarget::Spinel);
    std::fs::read_to_string(dir.join("app/controllers/panels_controller.rb")).expect("panels_controller.rb emitted")
}

#[test]
fn a_called_class_method_reaches_the_includer() {
    app(CONTROLLER)
        .run_ruby(
            r#"
require_relative "app/controllers/panels_controller"
raise "label #{PanelsController.scope_label.inspect}" unless PanelsController.scope_label == "plain"
puts "ALL OK"
"#,
        )
        .assert_passes();
}

#[test]
fn a_subclass_reaches_it_through_its_parent() {
    app(CONTROLLER)
        .write("app/controllers/wide_panels_controller.rb", "class WidePanelsController < PanelsController\nend\n")
        .run_ruby(
            r#"
require_relative "app/controllers/wide_panels_controller"
raise "label #{WidePanelsController.scope_label.inspect}" unless WidePanelsController.scope_label == "plain"
puts "ALL OK"
"#,
        )
        .assert_passes();
}

#[test]
fn only_what_is_called_is_copied() {
    let source = emitted(app(CONTROLLER));
    assert!(source.contains("def self.fallback_label"), "a method the copy calls is missing:\n{source}");
    assert!(!source.contains("def self.never_called"), "an uncalled method was copied:\n{source}");
}

#[test]
fn the_controllers_own_method_wins() {
    let own = "class PanelsController < ApplicationController\n  include Scoped\n\n  def self.scope_label\n    \"own\"\n  end\n\n  def label\n    self.class.scope_label\n  end\nend\n";
    app(own)
        .run_ruby(
            r#"
require_relative "app/controllers/panels_controller"
raise "label #{PanelsController.scope_label.inspect}" unless PanelsController.scope_label == "own"
puts "ALL OK"
"#,
        )
        .assert_passes();
}

#[test]
fn the_last_include_wins() {
    let two = "class PanelsController < ApplicationController\n  include Scoped\n  include Rescoped\n\n  def label\n    self.class.scope_label\n  end\nend\n";
    app(two)
        .write(
            "app/controllers/concerns/rescoped.rb",
            "module Rescoped\n  extend ActiveSupport::Concern\n\n  class_methods do\n    def scope_label\n      \"later\"\n    end\n  end\nend\n",
        )
        .run_ruby(
            r#"
require_relative "app/controllers/panels_controller"
raise "label #{PanelsController.scope_label.inspect}" unless PanelsController.scope_label == "later"
puts "ALL OK"
"#,
        )
        .assert_passes();
}

const ATTRIBUTE: &str = r#"module Scoped
  extend ActiveSupport::Concern

  class_methods do
    def scope_label
      defined?(@scope_label) ? @scope_label : "plain"
    end

    def scope_label=(value)
      @scope_label = value
    end
  end
end
"#;

// Devise's `scoped_views?`. Its writer has no caller, so the ivar has no type to read.
#[test]
fn an_attribute_nothing_writes_reads_its_fallback() {
    let (dir, errors) = app(CONTROLLER).write("app/controllers/concerns/scoped.rb", ATTRIBUTE).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let source = std::fs::read_to_string(dir.join("app/controllers/panels_controller.rb")).expect("panels_controller.rb emitted");
    assert!(source.contains("def self.scope_label"), "not copied:\n{source}");
    assert!(!source.contains("@scope_label"), "the unwritten ivar is still read:\n{source}");
}

// The class body's write doesn't reach the emitted class, so the reader would answer with its fallback.
#[test]
fn a_write_from_the_class_body_is_not_answered_with_the_fallback() {
    let writing = "class PanelsController < ApplicationController\n  include Scoped\n\n  self.scope_label = \"set\"\n\n  def label\n    self.class.scope_label\n  end\nend\n";
    let source = emitted(app(writing).write("app/controllers/concerns/scoped.rb", ATTRIBUTE));
    assert!(!source.contains("def self.scope_label"), "the reader was copied without the write:\n{source}");
}
