//! A builder subclass's body reading a class method its FormBuilder does not
//! have (`default: field_names - […]`, beside maybe's real `field_helpers`)
//! raises `NameError` as the emitted class loads, where `check` only warned
//! of an unresolved type. A receiverless call in a class body that no class here
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
    let errors = errors("class ProbeBuilder < ActionView::Helpers::FormBuilder\n  class_attribute :listed, default: field_names - [:label]\nend\n");
    assert!(errors.iter().any(|e| e.contains("no known method `field_names` on ProbeBuilder")), "{errors:#?}");
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
        // a lambda's default, which runs when it is called
        "class ProbeLambda < ActionView::Helpers::FormBuilder\n  HANDLER = ->(value = unresolved_default) { value }\nend\n",
        // a rescue, which catches the NameError
        "class ProbeRescue < ActionView::Helpers::FormBuilder\n  LIST = optional_probe_hook rescue []\nend\n",
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

/// Rails' `FormBuilder.field_helpers` is the builder's own, which the Ruby family's overlay answers.
#[test]
fn field_helpers_in_a_builder_body_is_known() {
    let errors = errors("class ProbeBuilder < ActionView::Helpers::FormBuilder\n  class_attribute :listed, default: field_helpers - [:label]\nend\n");
    assert!(!errors.iter().any(|e| e.contains("field_helpers")), "{errors:#?}");
}

/// A rescue of another class (`rescue ArgumentError`) does not catch the `NameError`, so the class still fails to load.
#[test]
fn a_rescue_that_does_not_catch_name_error_keeps_the_error() {
    let errors = errors("class ProbeBuilder < ActionView::Helpers::FormBuilder\n  LIST = begin\n    absent_probe_hook\n  rescue ArgumentError\n    []\n  end\nend\n");
    assert!(errors.iter().any(|e| e.contains("no known method `absent_probe_hook`")), "{errors:#?}");
}
