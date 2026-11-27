//! A concern's `included do` filters register when its `include` runs.
//!
//! Rails runs the block in the including class at the `include`, so its
//! `before_action`s land between the class's own, in source order. Two
//! shapes lost that:
//!
//! - A concern nested in its controller (`module Versioning` inside
//!   `class CacheController`, or in a `cache_controller/versioning.rb`
//!   opening, as Discourse's `JsonApiKit` concerns are) had its filters
//!   read only when the file did not ingest as a controller. `check`
//!   passed and the action ran without them.
//! - Every concern's filters went to the head of the includer's chain, so
//!   a `before_action` written above the `include` ran after them.
//!
//! The expected traces are what Rails 8.1.4 answers for the same app.

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::dialect::FilterKind;
use roundhouse::ingest::ingest_app_from_tree;

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

/// `@trace` records each filter as it runs; the action renders it.
fn tracer(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("  def {n}\n    @trace = \"#{{@trace}}{n} \"\n  end\n\n"))
        .collect()
}

const SHOW: &str = "  def show\n    render plain: \"trace=#{@trace}\"\n  end\n";
const INDEX: &str = "  def index\n    render plain: \"trace=#{@trace}\"\n  end\n";

fn nested(name: &str, filters: &str, methods: &[&str]) -> String {
    let methods = tracer(methods).replace("\n  ", "\n    ");
    format!(
        "  module {name}\n    extend ActiveSupport::Concern\n\n    included do\n{filters}    end\n\n    private\n\n  {}  end\n",
        methods.trim_end_matches(' ')
    )
}

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write(
            "app/controllers/cache_controller.rb",
            &format!(
                "class CacheController < ApplicationController\n{}\n  include Versioning\n\n{SHOW}end\n",
                nested("Versioning", "      before_action :set_version\n", &["set_version"]),
            ),
        )
        .write(
            "app/controllers/order_controller.rb",
            &format!(
                "class OrderController < ApplicationController\n  before_action :first\n\n{}\n  include Stamping\n  before_action :last\n\n{SHOW}\n  private\n\n{}end\n",
                nested("Stamping", "      before_action :stamp\n", &["stamp"]),
                tracer(&["first", "last"]),
            ),
        )
        .write(
            "app/controllers/concerns/folder_stamping.rb",
            &format!(
                "module FolderStamping\n  extend ActiveSupport::Concern\n\n  included do\n    before_action :folder_stamp\n  end\n\n  private\n\n{}end\n",
                tracer(&["folder_stamp"]),
            ),
        )
        .write(
            "app/controllers/folder_controller.rb",
            &format!(
                "class FolderController < ApplicationController\n  before_action :first\n  include FolderStamping\n  before_action :last\n\n{SHOW}\n  private\n\n{}end\n",
                tracer(&["first", "last"]),
            ),
        )
        .write(
            "app/controllers/gates_controller.rb",
            &format!(
                "class GatesController < ApplicationController\n{}\n  before_action :own\n  include Gates\n\n{SHOW}\n{INDEX}\n  private\n\n{}end\n",
                nested(
                    "Gates",
                    "      before_action :only_show, only: :show\n      before_action :not_show, except: :show\n      prepend_before_action :prepended\n",
                    &["only_show", "not_show", "prepended"],
                ),
                tracer(&["own"]),
            ),
        )
        .write(
            "app/controllers/other_controller.rb",
            &format!("class OtherController < ApplicationController\n  include CacheController::Versioning\n\n{INDEX}end\n"),
        )
        .write(
            "app/controllers/layered_controller.rb",
            &format!(
                "class LayeredController < ApplicationController\n{}\n{}\n  include Outer\n\n{SHOW}end\n",
                nested("Inner", "      before_action :inner_filter\n", &["inner_filter"]),
                nested("Outer", "      before_action :outer_filter\n", &["outer_filter"])
                    .replace("    extend ActiveSupport::Concern\n", "    extend ActiveSupport::Concern\n    include Inner\n"),
            ),
        )
        .write(
            "config/routes.rb",
            "Rails.application.routes.draw do\n  get \"cache\", to: \"cache#show\"\n  get \"order\", to: \"order#show\"\n  get \"folder\", to: \"folder#show\"\n  get \"gates\", to: \"gates#show\"\n  get \"gates/all\", to: \"gates#index\"\n  get \"other\", to: \"other#index\"\n  get \"layered\", to: \"layered#show\"\nend\n",
        )
}

#[test]
fn included_filters_run_at_their_include_in_the_emitted_app() {
    app()
        .run_ruby(
            r##"
def probe(path)
  status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
  text = +""
  body.each { |part| text << part }
  [status, text.strip]
end
{
  "/cache" => [200, "trace=set_version"],
  "/order" => [200, "trace=first stamp last"],
  "/folder" => [200, "trace=first folder_stamp last"],
  "/gates" => [200, "trace=prepended own only_show"],
  "/gates/all" => [200, "trace=prepended own not_show"],
  "/other" => [200, "trace=set_version"],
  "/layered" => [200, "trace=inner_filter outer_filter"],
}.each do |path, want|
  got = probe(path)
  raise "#{path} #{got.inspect}, want #{want.inspect}" unless got == want
end
puts "ALL OK"
"##,
        )
        .assert_passes();
}

/// Discourse's layout: each concern opens its controller in a file of
/// its own under the controller's folder, and those files sort ahead of
/// the controller's own.
#[test]
fn a_concern_in_a_controller_opening_reaches_the_includer() {
    let concern = |name: &str, filters: &str| {
        format!(
            "module Kit\n  class BaseController\n    module {name}\n      extend ActiveSupport::Concern\n\n      included do\n{filters}      end\n    end\n  end\nend\n"
        )
    };
    let tree: HashMap<PathBuf, Vec<u8>> = [
        ("db/schema.rb", "ActiveRecord::Schema.define do\nend\n".to_string()),
        ("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n".to_string()),
        (
            "app/controllers/kit/base_controller.rb",
            "module Kit\n  class BaseController < ::ApplicationController\n    before_action :own\n    include Rendering\n    include Negotiating\n    include Versioning\n  end\nend\n".to_string(),
        ),
        ("app/controllers/kit/base_controller/rendering.rb", concern("Rendering", "        before_action :set_json_format\n")),
        (
            "app/controllers/kit/base_controller/negotiating.rb",
            concern("Negotiating", "        before_action :set_vary_header\n        before_action :negotiate\n"),
        ),
        ("app/controllers/kit/base_controller/versioning.rb", concern("Versioning", "        before_action :resolve_version\n")),
    ]
    .into_iter()
    .map(|(path, source)| (PathBuf::from(path), source.into_bytes()))
    .collect();
    let app = ingest_app_from_tree(tree).expect("ingest");
    let base = app
        .controllers
        .iter()
        .find(|c| c.name.0.as_str() == "Kit::BaseController" && c.parent.is_some())
        .expect("the defining declaration");
    let chain: Vec<(&str, Option<&str>)> = base
        .filters()
        .filter(|f| f.kind == FilterKind::Before)
        .map(|f| (f.target.as_str(), f.from_concern.as_ref().map(|m| m.0.as_str())))
        .collect();
    assert_eq!(
        chain,
        vec![
            ("own", None),
            ("set_json_format", Some("Kit::BaseController::Rendering")),
            ("set_vary_header", Some("Kit::BaseController::Negotiating")),
            ("negotiate", Some("Kit::BaseController::Negotiating")),
            ("resolve_version", Some("Kit::BaseController::Versioning")),
        ]
    );
}
