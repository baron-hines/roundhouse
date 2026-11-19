//! maybe's `StyledFormBuilder < ActionView::Helpers::FormBuilder` opens
//! with `class_attribute :text_field_helpers, default: field_helpers - […]`.
//! The runtime's FormBuilder has no `field_helpers`, so the emitted class
//! raises `NameError` as it loads, where `check` only warned of an
//! unresolved type. A receiverless call in a class body that no class here
//! answers, under a known superclass, is now the error it is. Left alone:
//! a block's call (its `self` may be another object), a module's, a
//! parentless class's, and a name some class defines.

use super::emit_and_run;

fn errors(body: &str) -> Vec<String> {
    emit_and_run::real_blog()
        .write("app/models/probe_builder.rb", body)
        .check_errors()
}

#[test]
fn an_unknown_class_body_call_under_a_known_superclass_is_an_error() {
    let errors = errors("class ProbeBuilder < ActionView::Helpers::FormBuilder\n  class_attribute :listed, default: field_helpers - [:label]\nend\n");
    assert!(errors.iter().any(|e| e.contains("no known method `field_helpers` on ProbeBuilder")), "{errors:#?}");
}

#[test]
fn other_class_body_calls_stay_unreported() {
    for body in [
        // a block, `instance_eval`ed by its receiver
        "class ProbeMenu < ApplicationRecord\n  self.abstract_class = true\n  ITEMS = [1].map { |x| scope_item(x) }\nend\n",
        // a module
        "module ProbeConcern\n  def self.mappings = {}\n  BY = mappings\nend\n",
        // a parentless class, whose `class << self` the lookup does not read
        "class ProbePlain\n  class << self\n    def defaults = {}\n  end\n  D = defaults\nend\n",
        // a name another class defines
        "class ProbeHelper < ActionView::Helpers::FormBuilder\n  class_attribute :x, default: shared_list\nend\n",
    ] {
        let errors = emit_and_run::real_blog()
            .write("app/models/probe_builder.rb", body)
            .write("app/models/probe_other.rb", "class ProbeOther\n  def self.shared_list = []\nend\n")
            .check_errors();
        assert!(!errors.iter().any(|e| e.contains("no known method") && e.contains("Probe")), "{body}: {errors:#?}");
    }
}
