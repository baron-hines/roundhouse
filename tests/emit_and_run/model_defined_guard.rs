use super::emit_and_run;

/// #2282: a model whose body includes a module only `if defined?` it.
/// Ruby runs the branch only when the module is defined; either way the
/// model loads and queries. The model was refused as a conditional
/// visibility declaration.
fn app(auditable: bool) -> emit_and_run::Overlay {
    let app = emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"posts\", force: :cascade do |t|\n    t.string \"title\"\n  end\nend\n",
        )
        .write(
            "app/models/post.rb",
            "class Post < ApplicationRecord\n  if defined?(Tracking::Auditable)\n    include Tracking::Auditable\n  end\nend\n",
        );
    if auditable {
        app.write("app/models/concerns/tracking/auditable.rb", "module Tracking\n  module Auditable\n    def audited?\n      true\n    end\n  end\nend\n")
    } else {
        app
    }
}

const SCRIPT: &str = r#"post = Post.new
post.title = "hello"
post.save!
got = [Post.count, Post.first.title, Post.new.respond_to?(:audited?)]
raise "got #{got.inspect}" unless got == [1, "hello", AUDITED]
puts "guarded include passed"
"#;

#[test]
fn a_model_guarding_an_absent_module_loads_and_queries() {
    let run = app(false).run_ruby(&SCRIPT.replace("AUDITED", "false"));
    run.assert_passes();
    assert!(run.stdout.contains("guarded include passed"), "{}", run.stdout);
}

#[test]
fn a_model_guarding_a_defined_module_includes_it() {
    let run = app(true).run_ruby(&SCRIPT.replace("AUDITED", "true"));
    run.assert_passes();
    assert!(run.stdout.contains("guarded include passed"), "{}", run.stdout);
}
