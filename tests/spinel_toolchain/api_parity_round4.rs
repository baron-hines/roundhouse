//! The Spinel side of the Rails API parity round 4 fixes this file's
//! siblings test on CRuby, in one app and one `spin build`:
//!
//! * an enum label mapped through the relation's own model
//!   (`tests/enum_model_column_mapping.rs`);
//! * `permit(...).reverse_merge(...)` in an action
//!   (`tests/params_reverse_merge_in_action.rs`);
//! * `request.filtered_parameters` (`tests/request_filtered_parameters.rs`),
//!   whose params and path parameters the server's dispatcher fills;
//! * `json.merge!` of a record's attributes (`tests/jbuilder_merge_attributes.rs`);
//! * safe navigation through rewritten chains (`tests/safe_nav_rewritten_chains.rs`).
//!
//! The dispatcher is the server's (`runtime/spinel/scaffold/main.rb`),
//! so it is a full `spin build`, not a `SPINEL_TESTS` entry.

use super::{emit_and_run, native_http};

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("config/application.rb", r##"require_relative "boot"
require "rails/all"

module Shop
  class Application < Rails::Application
    config.load_defaults 8.1
    config.api_only = true
  end
end
"##)
        .write("config/initializers/filter_parameter_logging.rb", "Rails.application.config.filter_parameters += [:passw, :name, /secret/]\n")
        .write("app/controllers/application_controller.rb", r##"class ApplicationController < ActionController::API
  rescue_from ActiveRecord::RecordInvalid, with: :unprocessable

  private

  def unprocessable(e)
    message = e.record&.errors&.full_messages&.first || e.message
    render json: { error: message }, status: :unprocessable_entity
  end
end
"##)
        .write("app/controllers/widgets_controller.rb", r##"class WidgetsController < ApplicationController
  ROLES = %i[admin owner].freeze

  def seed
    alpha = Widget.create!(name: "alpha", role: :admin, price: 3)
    Widget.create!(name: "beta", role: :member, price: 9)
    Widget.create!(name: "gamma", role: :owner, price: 1)
    Gadget.create!(widget_id: alpha.id, label: "g1", role: :guest)
    Gadget.create!(widget_id: alpha.id, label: "g2", role: :owner)
    render plain: Widget.count.to_s
  end

  def admins
    render json: {
      listed: Widget.where(role: ROLES).order(:name).pluck(:name),
      others: Widget.where(price: 0..).where.not(role: :admin).order(:name).pluck(:name),
      gadgets: Gadget.order(:label).where(role: :owner).pluck(:label)
    }
  end

  def staged
    attrs = params.require(:widget).permit(:name, :price).reverse_merge(price: 7, role: :owner)
    widget = Widget.create!(attrs)
    render json: { name: widget.name, price: widget.price, role: widget.role }
  end

  def filtered
    render json: { logged: request.filtered_parameters }
  end

  def config_filtered
    filtered = ActiveSupport::ParameterFilter.new(Rails.application.config.filter_parameters).filter(params)
    render plain: [filtered["password"], filtered["my_secret"], filtered["city"]].join("|")
  end

  def plural_kind
    value = params[:kind]
    permitted = params.permit(:kind).to_h
    from_hash = permitted[:kind]
    render plain: value.pluralize.capitalize + "|" + from_hash.pluralize.capitalize + "|" + value.pluralize(1) + "|" + value.pluralize
  end

  def plural_items
    rows = params.require(:items).map { it.permit(:category).to_h }
    rows.each { it[:category] = it["category"].pluralize.capitalize }
    raw = params[:items].map { it["category"].pluralize }
    render json: { items: rows, raw: raw }
  end

  def invalid
    Widget.create!(name: nil)
    head :ok
  end

  def labels
    labels = Widget.find_by(name: params[:name])&.gadgets&.where(role: :owner)&.pluck(:label)
    render json: { labels: labels, loaded: Gadget.first.owner_loaded? }
  end

  def show
    @widget = Widget.find(params[:id])
  end
end
"##)
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  primary_abstract_class
end
"##)
        .write("app/models/widget.rb", r##"class Widget < ApplicationRecord
  enum :role, %i[member admin owner]
  has_many :gadgets
  validates :name, presence: true
end
"##)
        .write("app/models/gadget.rb", r##"class Gadget < ApplicationRecord
  enum :role, %i[guest owner]
  belongs_to :widget

  def owner_loaded?
    widget&.association(:gadgets)&.loaded?
  end
end
"##)
        .write("app/views/widgets/show.json.jbuilder", "json.merge!(@widget.attributes.except(\"id\"))\njson.done true\n")
        .write("config/routes.rb", r##"Rails.application.routes.draw do
  get "/widgets/seed", to: "widgets#seed"
  get "/widgets/admins", to: "widgets#admins"
  post "/widgets/staged", to: "widgets#staged"
  post "/widgets/filtered", to: "widgets#filtered"
  get "/widgets/config_filtered", to: "widgets#config_filtered"
  get "/widgets/plural_kind", to: "widgets#plural_kind"
  post "/widgets/plural_items", to: "widgets#plural_items"
  post "/widgets/invalid", to: "widgets#invalid"
  get "/widgets/labels", to: "widgets#labels"
  resources :widgets, only: %i[show]
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "widgets", force: :cascade do |t|
    t.string "name"
    t.integer "role", default: 0, null: false
    t.integer "price", default: 0
  end

  create_table "gadgets", force: :cascade do |t|
    t.integer "widget_id", null: false
    t.string "label"
    t.integer "role", default: 0, null: false
  end
end
"##)
}

#[test]
#[ignore = "requires Spinel; run in its CI lane"]
fn api_parity_round4_fixes_run_on_spinel() {
    let tree = app().native_http_tree();
    let server = native_http::Server::start(&tree);
    let json = "application/json";

    let seed = server.get("/widgets/seed");
    assert_eq!((seed.status, seed.body.as_str()), (200, "3"), "seed:\n{}", server.log());

    let admins = server.get("/widgets/admins");
    assert_eq!(
        admins.body,
        r#"{"listed":["alpha","gamma"],"others":["beta","gamma"],"gadgets":["g2"]}"#,
        "enum by model:\n{}",
        server.log()
    );

    let staged = server.post("/widgets/staged", json, r#"{"widget":{"name":"delta"}}"#);
    assert_eq!(
        (staged.status, staged.body.as_str()),
        (200, r#"{"name":"delta","price":7,"role":"owner"}"#),
        "reverse_merge:\n{}",
        server.log()
    );

    let filtered = server.post("/widgets/filtered", json, r#"{"widget":{"name":"delta","password":"x","size":"3"}}"#);
    assert_eq!(
        filtered.body,
        r#"{"logged":{"widget":{"name":"[FILTERED]","password":"[FILTERED]","size":"3"},"controller":"widgets","action":"filtered"}}"#,
        "filtered_parameters:\n{}",
        server.log()
    );

    let invalid = server.post("/widgets/invalid", json, "{}");
    assert_eq!(
        (invalid.status, invalid.body.as_str()),
        (422, r#"{"error":"Name can't be blank"}"#),
        "errors&.full_messages:\n{}",
        server.log()
    );

    let labels = server.get("/widgets/labels?name=alpha");
    assert_eq!(labels.body, r#"{"labels":["g2"],"loaded":false}"#, "safe-nav chains:\n{}", server.log());
    let none = server.get("/widgets/labels?name=none");
    assert_eq!(none.body, r#"{"labels":null,"loaded":false}"#, "safe-nav nil:\n{}", server.log());

    // `config.filter_parameters` read in an action (Rails 8.1.4).
    let config_filtered = server.get("/widgets/config_filtered?password=a&my_secret=b&city=c");
    assert_eq!(config_filtered.body, "[FILTERED]|[FILTERED]|c", "config.filter_parameters:\n{}", server.log());

    // `String#pluralize` on a request value (Rails 8.1.4).
    for (kind, expected) in [
        ("status", "Statuses|Statuses|status|statuses"),
        // Rails' uncountables match at a word boundary.
        ("big%20fish", "Big fish|Big fish|big fish|big fish"),
        // Rails' rules keep what they capture and take the rest as written.
        ("CHILD", "Children|Children|CHILD|Children"),
        ("BUS", "Buses|Buses|BUS|BUses"),
    ] {
        let plural = server.get(&format!("/widgets/plural_kind?kind={kind}"));
        assert_eq!(plural.body, expected, "pluralize {kind}:\n{}", server.log());
    }

    // `String#pluralize` on a value read in a block over the request's list (Rails 8.1.4).
    let plural_items = server.post(
        "/widgets/plural_items",
        json,
        r#"{"items":[{"category":"status"},{"category":"big fish"},{"category":"CHILD"}]}"#,
    );
    assert_eq!(
        (plural_items.status, plural_items.body.as_str()),
        (200, r#"{"items":[{"category":"Statuses"},{"category":"Big fish"},{"category":"Children"}],"raw":["statuses","big fish","Children"]}"#),
        "pluralize in a block:\n{}",
        server.log()
    );

    // A numeric value has no `pluralize` (Rails 8.1.4 answers 500).
    let numeric = server.post("/widgets/plural_items", json, r#"{"items":[{"category":5}]}"#);
    assert_eq!(numeric.status, 500, "pluralize on a number: {}\n{}", numeric.body, server.log());

    let show = server.get("/widgets/1.json");
    assert_eq!(
        show.body,
        r#"{"name":"alpha","role":"admin","price":3,"done":true}"#,
        "attributes merge:\n{}",
        server.log()
    );
}
