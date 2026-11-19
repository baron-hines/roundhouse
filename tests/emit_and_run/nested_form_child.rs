//! `form_with model: [@article, @article.comments.build]`: Rails evaluates the child once, so the association holds
//! one built comment, and posts to the nested collection while it is new.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit(
        "app/views/articles/show.html.erb",
        "<%= form_with model: [@article, Comment.new], class: \"space-y-4\" do |form| %>",
        "<p id=\"held-before\"><%= @article.comments.size %></p>\n<%= form_with model: [@article, @article.comments.build], class: \"space-y-4\" do |form| %>",
    )
    .edit(
        "app/views/articles/show.html.erb",
        "rounded\" %>\n<% end %>",
        "rounded\" %>\n<% end %>\n<p id=\"held\"><%= @article.comments.size %></p>",
    )
}

const SCRIPT: &str = r#"
require "stringio"
hello = Article.create!(title: "Hello", body: "Body text here")
env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => "/articles/#{hello.id}", "QUERY_STRING" => "", "HTTP_ACCEPT" => "text/html" }
r = Main.dispatch_core(env, StringIO.new(""))
raise "status #{r[0]}" unless r[0] == 200
body = r[1].is_a?(Array) ? r[1].join : r[1].to_s
raise body[0, 3000] unless body.include?("<form action=\"/articles/#{hello.id}/comments\"")
raise body[0, 3000] unless body.include?("<p id=\"held-before\">0</p>") && body.include?("<p id=\"held\">1</p>")
puts "nested child passed"
"#;

#[test]
fn a_built_nested_child_is_built_once() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("nested child passed"), "{}", run.stdout);
}
