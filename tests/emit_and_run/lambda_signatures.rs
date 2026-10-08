//! A lambda keeps its optional, keyword and keyword-rest parameters, so
//! the emitted program answers what plain Ruby answers.

use super::lambda_signatures_contract as contract;

#[test]
fn lambdas_with_optional_and_keyword_parameters_run() {
    contract::overlay().run_ruby(&contract::assertions()).assert_passes();
}

/// A form builder block with an optional parameter has no lowering; the
/// emit reports it instead of rendering the form as nothing.
#[test]
fn a_form_block_with_an_optional_parameter_is_reported_not_dropped() {
    let (_tree, errors) = super::emit_and_run::real_blog()
        .edit("app/views/articles/_form.html.erb", "do |form| %>", "do |form, extra = 1| %>")
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(
        errors.iter().any(|e| e.contains("builder block with optional or keyword parameters")),
        "{errors:#?}"
    );
}
