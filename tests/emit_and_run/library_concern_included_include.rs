//! The Ruby half of `support/library_concern_included_include.rs`.

use super::emit_and_run;
use super::library_concern_included_include_contract as shared;
use roundhouse::project::BuildTarget;

// Through a routed action, as Ruby answers it: `Gadget.include?(Scoped)`
// and `Gadget.scoped? == false`.
const ROUTED: &str = r#"
status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/gadget", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
raise "status #{status}: #{body.join}" unless status == 200
raise "paths #{body.join.inspect}" unless body.join == "plain"
raise "Scoped not included" unless Gadget.include?(Scoped)
"#;

#[test]
fn a_library_class_takes_the_include_its_concern_runs() {
    let run = shared::app().run_ruby(&format!("{ROUTED}{}", shared::ASSERTIONS));
    run.assert_passes();
    assert!(run.stdout.contains("library concern include OK"), "{}", run.stdout);
}

#[test]
fn the_spinel_lane_takes_it_cleanly() {
    let (dir, errors) = shared::app().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let gadget = std::fs::read_to_string(dir.join("app/models/gadget.rb")).expect("gadget.rb emitted");
    assert!(gadget.contains("def self.scoped?"), "the class-method module was not copied:\n{gadget}");
}
