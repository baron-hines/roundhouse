//! A Concern's `included do include Other end` runs on every includer,
//! a plain Ruby class as much as a model or a controller: the includer
//! gets `Other` and `Other`'s class-method module. A class that is not a
//! model or a controller lost both, so `self.class.scoped?` raised
//! NoMethodError on it. Shared by the Ruby lane and the Spinel lane.

use super::emit_and_run;

const SCOPED: &str = r#"module Scoped
  extend ActiveSupport::Concern

  class_methods do
    def scoped?
      false
    end
  end
end
"#;

const SENDING: &str = r#"module Sending
  extend ActiveSupport::Concern

  included do
    include Scoped
  end

  def paths
    self.class.scoped? ? ["scoped"] : ["plain"]
  end
end
"#;

pub fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
        .write("app/models/concerns/scoped.rb", SCOPED)
        .write("app/models/concerns/sending.rb", SENDING)
        .write("app/models/gadget.rb", "class Gadget\n  include Sending\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\n  get \"/gadget\", to: \"gadgets#show\"\nend\n")
        .write(
            "app/controllers/gadgets_controller.rb",
            "class GadgetsController < ApplicationController\n  def show\n    render plain: Gadget.new.paths.join(\",\")\n  end\nend\n",
        )
}

/// What Ruby answers, asked of the class directly (the native binary
/// does not load `main.rb`, so it has no `Main.run_rack`).
pub const ASSERTIONS: &str = r#"paths = Gadget.new.paths
raise "paths #{paths.inspect}" unless paths == ["plain"]
raise "scoped? #{Gadget.scoped?.inspect}" unless Gadget.scoped? == false
puts "library concern include OK"
"#;
