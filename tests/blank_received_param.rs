//! A parameter's type is the join of the argument types the analyzer typed
//! at its call sites, and an argument it could not type adds nothing to it.
//! A helper reads `raw.present?`; one caller passes `Array(...)` and another
//! passes `response.body["errors"]`, so the parameter was typed `Array` and
//! the lowering folded `present?` to `!raw.empty?`, which raised
//! `undefined method 'empty?' for nil` for a response without errors.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

#[test]
fn present_on_a_parameter_typed_from_its_typed_callers_is_false_for_nil() {
    emit_and_run::empty_app()
        .write(
            "app/models/errs.rb",
            "module Errs\n  def self.messages(raw)\n    return [] unless raw.present?\n\n    if raw.is_a?(String)\n      raw = [raw]\n    end\n\n    raw.map { |e| e.to_s }\n  end\nend\n",
        )
        .write(
            "app/models/typed_caller.rb",
            "class TypedCaller\n  #: (untyped) -> Array[String]\n  def self.go(response)\n    errors = Array(response)\n    Errs.messages(errors)\n  end\nend\n",
        )
        .write(
            "app/models/untyped_caller.rb",
            "class UntypedCaller\n  #: (untyped) -> untyped\n  def self.go(body)\n    Errs.messages(body[\"errors\"])\n  end\nend\n",
        )
        .run_ruby(
            "raise 'nil' unless UntypedCaller.go({}) == []\n\
             raise 'one' unless UntypedCaller.go({ 'errors' => 'boom' }) == ['boom']\n\
             raise 'empty' unless TypedCaller.go(nil) == []\n\
             raise 'two' unless TypedCaller.go(['a', 'b']) == ['a', 'b']\n\
             puts 'ok'\n",
        )
        .assert_passes();
}
