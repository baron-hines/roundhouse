//! A partial nothing renders (maybe's `accounts/show/_activity`, which a
//! component replaced) reports its errors as warnings, as an action template
//! no request reaches already does. Named anywhere it stays: a String
//! literal of its path, a bare name in a view of its own directory, or a
//! model's `to_partial_path`. A bare name elsewhere (`tab: "activity"`) names
//! no partial of another directory.

use super::emit_and_run;

fn errors_with(show_reads: &str, elsewhere: &str) -> Vec<String> {
    let (_emitted, errors) = emit_and_run::real_blog()
        .write("app/views/articles/_ghost.html.erb", "<p><%= @ghost.missing_method %></p>\n")
        .write("app/views/pages/_tabs.html.erb", &format!("{elsewhere}\n"))
        .edit("app/views/articles/show.html.erb", "    <p class=\"text-gray-700\">", &format!("{show_reads}\n    <p class=\"text-gray-700\">"))
        .emit(roundhouse::project::BuildTarget::Ruby);
    errors
}

#[test]
fn a_partial_nothing_renders_warns() {
    let errors = errors_with("", "<%= link_to \"x\", \"/x\", data: { tab: \"ghost\" } %>");
    assert!(!errors.iter().any(|e| e.contains("_ghost") || e.contains("@ghost")), "{errors:#?}");
}

#[test]
fn a_rendered_partial_keeps_its_errors() {
    let errors = errors_with("<%= render \"ghost\" %>", "");
    assert!(errors.iter().any(|e| e.contains("@ghost")), "{errors:#?}");
}

#[test]
fn a_partial_an_interpolation_may_name_keeps_its_errors() {
    let errors = errors_with("", "<%= render \"articles/gh#{1}\" rescue nil %>");
    assert!(errors.iter().any(|e| e.contains("@ghost")), "{errors:#?}");
}

#[test]
fn a_partial_a_request_may_name_keeps_its_errors() {
    let errors = errors_with("<%= render partial: params[:panel].to_s %>", "");
    assert!(errors.iter().any(|e| e.contains("@ghost")), "{errors:#?}");
}

#[test]
fn a_partial_a_variable_or_a_request_string_may_name_keeps_its_errors() {
    for reads in ["<% panel = params[:panel].to_s %><%= render partial: panel %>", "<%= render params[:panel].to_s %>", "<%= render partial: \"#{params[:panel]}t\" %>"] {
        let errors = errors_with(reads, "");
        assert!(errors.iter().any(|e| e.contains("@ghost")), "{reads}: {errors:#?}");
    }
}
