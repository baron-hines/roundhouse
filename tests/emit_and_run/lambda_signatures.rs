//! A lambda keeps its optional, keyword and keyword-rest parameters, so
//! the emitted program answers what plain Ruby answers.

use super::lambda_signatures_contract as contract;

#[test]
fn lambdas_with_optional_and_keyword_parameters_run() {
    contract::overlay().run_ruby(&contract::assertions()).assert_passes();
}
