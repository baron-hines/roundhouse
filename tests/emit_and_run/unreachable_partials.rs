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
    // A collection built with `+=` renders its records, and a nil spacer names nothing; neither names a partial.
    let errors = errors_with("", "<%= render partial: \"articles/article\", collection: [], spacer_template: nil %>");
    assert!(!errors.iter().any(|e| e.contains("_ghost") || e.contains("@ghost")), "{errors:#?}");

    let errors = errors_with("", "<% posts = [] %><% posts += Article.all.to_a %><%= render posts %>");
    assert!(!errors.iter().any(|e| e.contains("_ghost") || e.contains("@ghost")), "{errors:#?}");
    // Downgraded, not dropped: the partial is still emitted.
    let (emitted, _) = emit_and_run::real_blog()
        .write("app/views/articles/_ghost.html.erb", "<p><%= @ghost.missing_method %></p>\n")
        .emit(roundhouse::project::BuildTarget::Ruby);
    let mut found = false;
    fn walk(dir: &std::path::Path, found: &mut bool) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("_ghost")) {
                *found = true;
            }
        }
    }
    walk(&emitted, &mut found);
    assert!(found, "no _ghost partial in {}", emitted.display());
}

/// A bare name a local holds names any directory's partial, as a literal does.
#[test]
fn a_bare_render_through_a_local_keeps_its_errors() {
    let errors = errors_with("", "<% panel = \"ghost\" %><%= render partial: panel %>");
    assert!(errors.iter().any(|e| e.contains("@ghost")), "{errors:#?}");
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
    for reads in [
        "<% panel = params[:panel].to_s %><%= render partial: panel %>",
        "<% source = params[:panel].to_s %><% panel = source %><%= render partial: panel %>",
        "<%= render params[:panel].to_s %>",
        "<%= render session[:panel].to_s %>",
        "<%= render partial: \"#{params[:panel]}t\" %>",
        "<%= render partial: \"#{params[:a]}o#{params[:b]}t\" %>",
        "<% @panel ||= params[:panel].to_s %><%= render partial: @panel %>",
        "<% panel = \"articles/known\" %><% [params[:panel].to_s].each do |panel| %><%= render partial: panel %><% end %>",
        "<% panel = \"articles/\" + params[:panel].to_s %><%= render partial: panel %>",
        "<% panel = \"articles/gh\" %><% panel += \"ost\" %><%= render partial: panel %>",
        "<%= render partial: \"articles/card\", layout: params[:wrapper].to_s %>",
        "<% opts = { partial: params[:panel].to_s } %><%= render opts %>",
        "<%= render params[:panel].to_s.gsub(\"-\", \"_\") %>",
        "<% @panel = cookies.signed[:panel] %><%= render partial: @panel %>",
        "<% panel = \"articles/gh\" %><% panel.concat(\"ost\") %><%= render partial: panel %>",
        "<% panel = \"articles/gh\" %><% panel += \"ost\" %><%= render panel %>",
        "<%= render sprintf(\"%s\", params[:panel]) %>",
        "<% panel = \"articles/gh\" %><% alias_name = panel %><% alias_name.concat(\"ost\") %><%= render panel %>",
        "<% panel = +\"articles/gh\" %><% alias_name = panel %><% alias_name.concat(\"ost\") %><%= render partial: panel %>",
        "<%= render partial: \"articles/article\", collection: [], spacer_template: params[:spacer].to_s %>",
    ] {
        let errors = errors_with(reads, "");
        assert!(errors.iter().any(|e| e.contains("@ghost")), "{reads}: {errors:#?}");
    }
}

/// An ivar a request value reaches through a local, rendered from another view.
#[test]
fn a_partial_an_ivar_fed_through_a_local_may_name_keeps_its_errors() {
    let errors = errors_with("<% candidate = params[:panel].to_s %><% @panel = candidate %>", "<%= render partial: @panel %>");
    assert!(errors.iter().any(|e| e.contains("@ghost")), "{errors:#?}");
}

/// A bare render target in a shared partial resolves through the controller's prefixes, not its own directory.
#[test]
fn a_bare_render_in_another_directory_keeps_its_errors() {
    let errors = errors_with("", "<%= render \"ghost\" %>");
    assert!(errors.iter().any(|e| e.contains("@ghost")), "{errors:#?}");
}
