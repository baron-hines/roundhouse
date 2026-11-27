//! maybe's `AccountableResource#accountable_type` (`controller_name.classify.constantize`), a concern included in
//! several controllers: Rails names each controller's model, so the articles controller reads `Article` and the
//! comments controller `Comment`.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/controllers/concerns/model_named.rb",
            "module ModelNamed\n  extend ActiveSupport::Concern\n\n  def model_label\n    model_class.name + \":\" + model_class.count.to_s\n  end\n\n  private\n    def model_class\n      controller_name.classify.constantize\n    end\nend\n",
        )
        .edit("app/controllers/articles_controller.rb", "class ArticlesController < ApplicationController\n", "class ArticlesController < ApplicationController\n  include ModelNamed\n")
        .edit("app/controllers/comments_controller.rb", "class CommentsController < ApplicationController\n", "class CommentsController < ApplicationController\n  include ModelNamed\n")
}

const SCRIPT: &str = r#"
require_relative "app/controllers/articles_controller"
require_relative "app/controllers/comments_controller"
Article.create!(title: "Hello", body: "Body text here")
got = [ArticlesController.new.model_label, CommentsController.new.model_label]
raise "got #{got.inspect}" unless got == ["Article:1", "Comment:0"]
puts "constantize passed"
"#;

#[test]
fn a_concerns_controller_name_constantize_names_each_controllers_model() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("constantize passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_concerns_controller_name_constantize_names_each_controllers_model_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("constantize passed"), "{}", run.stdout);
}

/// A `controller_name` a parent defines is what the call reads, not the controller's own name: the call is kept, and
/// stays reported.
#[test]
fn an_inherited_controller_name_keeps_the_call() {
    let errors = app()
        .edit(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\n",
            "class ApplicationController < ActionController::Base\n  def controller_name\n    \"comments\"\n  end\n\n",
        )
        .check_errors();
    assert!(errors.iter().any(|e| e.contains("constantize")), "{errors:#?}");
}
