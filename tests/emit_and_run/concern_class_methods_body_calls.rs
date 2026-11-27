//! A plain class includes a Concern and calls the Concern's
//! `class_methods` in its body, as discourse's services call
//! `Service::Base`'s `model`, `step` and `params`. Ruby runs the calls as
//! the class loads (Rails 8.1). `Service::Base`'s come from a helper module
//! its `class_methods` block includes: on main that include was taken as
//! the Concern's own, the helper's methods were no class methods of the
//! includer, and the calls were dropped with no error.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/concerns/stepped.rb", r##"module Stepped
  extend ActiveSupport::Concern

  class_methods do
    def steps
      @steps ||= []
    end

    def step(name)
      steps << name
    end
  end
end
"##)
        .write("app/models/publish_service.rb", "class PublishService\n  include Stepped\n\n  step :validate\n  step :publish\nend\n")
}

#[test]
fn a_concern_class_method_called_in_the_includer_s_body_runs() {
    let errors = app().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    app()
        .run_ruby("got = PublishService.steps\nraise got.inspect unless got == [:validate, :publish]\nputs \"concern body calls passed\"\n")
        .assert_passes();
}

/// Service::Base's shape: the Concern's `class_methods` include a helper
/// module, whose methods are then the includer's class methods.
#[test]
fn a_helper_module_included_into_class_methods_answers_in_the_includer_s_body() {
    let overlay = || {
        emit_and_run::empty_app()
            .write("app/models/concerns/stepped.rb", "module Stepped\n  extend ActiveSupport::Concern\n\n  module StepsHelpers\n    def step(name)\n      steps << name\n    end\n  end\n\n  class_methods do\n    include StepsHelpers\n\n    def steps\n      @steps ||= []\n    end\n  end\nend\n")
            .write("app/models/publish_service.rb", "class PublishService\n  include Stepped\n\n  step :validate\n  step :publish\nend\n")
    };
    let errors = overlay().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    overlay()
        .run_ruby("got = PublishService.steps\nraise got.inspect unless got == [:validate, :publish]\nputs \"helper module passed\"\n")
        .assert_passes();
}
