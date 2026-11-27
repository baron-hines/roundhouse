//! `lower::defined_ivar_memo` must see every writer of `@x`, not just
//! the guarding class's. A module extended into a class can memoise a
//! reader behind `defined?(@resolver)` while the value is set by the
//! class's inherited `resolver=` writer. A per-class rewrite gave the
//! module a flag the superclass setter never set, so the reader replaced
//! every declared value with nil.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const NESTED: &str = r#"module NestedResolver
  def resolver
    return @resolver if defined?(@resolver)

    @resolver = nested_resolver
  end

  private

  def nested_resolver
    nil
  end
end
"#;

const BASE: &str = r#"class BaseResolver
  class << self
    attr_reader :resolver

    def resolver=(val)
      @resolver = val
    end
  end
end
"#;

const DEFINITION: &str = r#"class Definition < BaseResolver
  extend NestedResolver

  def self.resolver(value = nil)
    self.resolver = value if value

    super()
  end
end
"#;

const DECLARED: &str = r#"class Declared < Definition
end
"#;

const NESTING: &str = r#"class Nesting < Definition
  def self.nested_resolver
    :inner
  end
end
"#;

#[test]
fn a_module_guard_sees_the_hosts_inherited_writer() {
    let script = "Declared.resolver(:declared)\np Declared.resolver\np Nesting.resolver\np Definition.resolver\n";
    let all = format!("{NESTED}\n{BASE}\n{DEFINITION}\n{DECLARED}\n{NESTING}\n{script}");
    let native = emit_and_run::ruby().args(["-W:no-deprecated", "-e", &all]).output().expect("CRuby control");
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), ":declared\n:inner\nnil\n");

    let (tree, errors) = emit_and_run::empty_app()
        .write("app/models/nested_resolver.rb", NESTED)
        .write("app/models/base_resolver.rb", BASE)
        .write("app/models/definition.rb", DEFINITION)
        .write("app/models/declared.rb", DECLARED)
        .write("app/models/nesting.rb", NESTING)
        .emit(BuildTarget::Ruby);
    let module = tree.join("app/models/nested_resolver.rb");
    assert!(module.exists(), "no {}: {errors:#?}", module.display());

    let run = std::process::Command::new("ruby")
        .current_dir(&tree)
        .env("BLOG_DB", ":memory:")
        .arg("-e")
        .arg(format!("require File.expand_path(\"main\", Dir.pwd)\nMain.configure_default_adapter!\n{script}"))
        .output()
        .expect("spawn ruby");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&native.stdout));
}
