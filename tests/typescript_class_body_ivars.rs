//! A class-side ivar the class body writes (`module M; @flag = false; end`)
//! exists from load, so a class method that only reads it needs a static
//! field even when no surviving method writes it, and that field starts at
//! the body's value. Before, only writes inside class methods declared one:
//! once the runtime shake dropped `ViewHelpers.keep_hidden_autocomplete`
//! (its only writer, unreachable from the blog), `hidden_autocomplete_attr`
//! read an undeclared `this.hidden_autocomplete` and tsc failed (TS2551).

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

use roundhouse::emit::typescript::emit_library_class;
use roundhouse::project::BuildTarget;
use roundhouse::runtime_src::parse_library_with_rbs;

const GADGET_RB: &str = "module Gadget
  @armed = true
  @label = \"idle\"
  @unused = 1

  def self.status
    @armed == true ? \"armed\" : @label
  end

  def self.disarm
    @label = \"off\"
  end
end
";

const GADGET_RBS: &str = "module Gadget
  def self.status: () -> String
  def self.disarm: () -> String
end
";

#[test]
fn a_class_body_ivar_a_class_method_reads_is_a_static_field_at_the_bodys_value() {
    let classes = parse_library_with_rbs(GADGET_RB.as_bytes(), GADGET_RBS, "gadget.rb").expect("parse");
    let gadget = classes.iter().find(|c| c.name.0.as_str() == "Gadget").expect("Gadget");
    let ts = emit_library_class(gadget).expect("emit");
    // Read only: declared from the body's write, at the body's value
    // (`true`, not a boolean default `false`).
    assert!(ts.contains("static armed: boolean = true;"), "{ts}");
    // Also written by a method: one field, starting at the body's value.
    assert!(ts.contains("static label: string = \"idle\";"), "{ts}");
    assert_eq!(ts.matches("static label").count(), 1, "{ts}");
    // A body write no class method touches declares nothing.
    assert!(!ts.contains("unused"), "{ts}");
}

#[test]
fn the_blogs_view_helpers_declare_the_hidden_autocomplete_flag() {
    let (emitted, errors) = emit_and_run::real_blog().emit(BuildTarget::Typescript);
    assert!(errors.is_empty(), "{errors:?}");
    let helpers = std::fs::read_to_string(emitted.join("src/view_helpers.ts")).expect("view_helpers.ts");
    // The shake leaves the reader without its writer; the field stays.
    assert!(helpers.contains("this.hidden_autocomplete === true"), "{helpers}");
    assert!(helpers.contains("static hidden_autocomplete: boolean = false;"), "{helpers}");
}
