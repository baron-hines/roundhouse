//! `json.merge!(record.attributes…)` followed by a pair over one of the
//! merged keys.
//!
//! Jbuilder's attributes are a Hash: `merge!` inserts the record's
//! columns in column order, and a later `json.price(…)` assigns the
//! existing key, which keeps its position. A template that merges a
//! record's columns and then replaces one (a computed price over the
//! stored one) was refused whole, since the merge's keys were not all
//! unique in the object. The merge now writes the later pair's value at
//! the merged key's position, and the later pair writes nothing. The
//! expected bodies are Rails 8.1.4.1 + jbuilder 2.15.1's.
//!
//! Values run in Jbuilder's order: the merge is read first, then each
//! pair through the last override, each rendered into its own local,
//! and only then are the keys written in Jbuilder's order. The pairs
//! between must be plain pairs, and the override must be the only later
//! statement that sets the key; anything else stays refused.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const SCHEMA: &str = r#"ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "widgets", force: :cascade do |t|
    t.string "name"
    t.integer "price"
    t.integer "kind", default: 0
    t.integer "hits", default: 0
    t.timestamps
  end
end
"#;

const MODEL: &str = "class Widget < ApplicationRecord\n  enum :kind, { basic: 0, deluxe: 1 }\n\n  def list_price\n    price * 2\n  end\n\n  def bump\n    self.hits = hits + 1\n    hits\n  end\n\n  def rename\n    self.name = \"z\"\n    5\n  end\nend\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/widget.rb", MODEL)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::API\nend\n")
        .write(
            "app/controllers/widgets_controller.rb",
            r#"class WidgetsController < ApplicationController
  def show
    @widget = Widget.find(params[:id])
  end

  def between
    @widget = Widget.find(params[:id])
  end

  def literal
  end

  def guarded
    @widget = Widget.find(params[:id])
  end

  def local
    @widget = Widget.find(params[:id])
  end

  def twice
    @widget = Widget.find(params[:id])
  end

  def order
    @widget = Widget.find(params[:id])
  end

  def snapshot
    @widget = Widget.find(params[:id])
  end

  def snapshot_literal
    @widget = Widget.find(params[:id])
  end

  def symbol
  end

  def effect
  end
end
"#,
        )
        .write("app/views/widgets/show.json.jbuilder", "json.partial!(@widget, partial: \"widgets/widget\", as: :widget)\n")
        .write(
            "app/views/widgets/_widget.json.jbuilder",
            "json.merge!(widget.attributes.except(\"created_at\", \"updated_at\", \"hits\"))\njson.price(widget.list_price)\njson.label(\"#{widget.name}!\")\n",
        )
        .write(
            "app/views/widgets/between.json.jbuilder",
            "json.merge!(@widget.attributes.except(\"created_at\", \"updated_at\", \"hits\"))\njson.label(\"#{@widget.name}!\")\njson.name(@widget.name.upcase)\njson.price(@widget.list_price)\n",
        )
        .write(
            "app/views/widgets/literal.json.jbuilder",
            "json.merge!({ kind: \"widget\", \"version\" => 2, size: 1 })\njson.done true\njson.version 3\n",
        )
        .write(
            "app/views/widgets/guarded.json.jbuilder",
            "json.merge!(@widget.attributes.except(\"created_at\", \"updated_at\", \"hits\"))\njson.price(@widget.list_price) if @widget.deluxe?\n",
        )
        .write(
            "app/views/widgets/local.json.jbuilder",
            "json.merge!(@widget.attributes.except(\"created_at\", \"updated_at\", \"hits\"))\nshout = @widget.name.upcase\njson.name(shout)\n",
        )
        .write(
            "app/views/widgets/twice.json.jbuilder",
            "json.merge!(@widget.attributes.except(\"created_at\", \"updated_at\", \"hits\"))\njson.price(1)\njson.price(2)\n",
        )
        .write(
            "app/views/widgets/order.json.jbuilder",
            "json.merge!(@widget.attributes.slice(\"name\", \"price\"))\njson.first(@widget.bump)\njson.price(@widget.bump)\njson.last(@widget.bump)\n",
        )
        .write(
            "app/views/widgets/snapshot.json.jbuilder",
            "json.merge!(@widget.attributes.slice(\"price\", \"name\"))\njson.price(@widget.rename)\n",
        )
        .write(
            "app/views/widgets/snapshot_literal.json.jbuilder",
            "json.merge!({ \"a\" => 1, \"b\" => @widget.name })\njson.a(@widget.rename)\n",
        )
        .write("app/views/widgets/symbol.json.jbuilder", "json.merge!({ size: 1 })\njson.size 3\n")
        .write("app/views/widgets/effect.json.jbuilder", "json.merge!({ \"size\" => Widget.count })\njson.size 3\n")
        .write(
            "config/routes.rb",
            "Rails.application.routes.draw do\n  resources :widgets, only: %i[show] do\n    member do\n      get :between\n      get :literal\n      get :guarded\n      get :local\n      get :twice\n      get :order\n      get :snapshot\n      get :snapshot_literal\n      get :symbol\n      get :effect\n    end\n  end\nend\n",
        )
}

#[test]
fn a_pair_over_a_merged_attribute_writes_at_the_merged_position() {
    app()
        .run_ruby(
            r##"
def probe(path)
  status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => "", "HTTP_ACCEPT" => "application/json", "rack.input" => StringIO.new(""))
  text = +""
  body.each { |part| text << part }
  [status, text]
end
def refusal(path)
  probe(path)
  nil
rescue RuntimeError => e
  e.message
end
w = Widget.create!(name: "b", price: 10, kind: :deluxe)
got = probe("/widgets/#{w.id}.json")
raise "show #{got.inspect}" unless got == [200, %({"id":#{w.id},"name":"b","price":20,"kind":"deluxe","label":"b!"})]
# Two overrides, one after a pair that is not merged: each keeps its
# merged position, and the label still comes last.
got = probe("/widgets/#{w.id}/between.json")
raise "between #{got.inspect}" unless got == [200, %({"id":#{w.id},"name":"B","price":20,"kind":"deluxe","label":"b!"})]
# A literal Hash merge: the same in-place assignment.
got = probe("/widgets/#{w.id}/literal.json")
raise "literal #{got.inspect}" unless got == [200, %({"kind":"widget","version":3,"size":1,"done":true})]
# Each value runs where Jbuilder runs it: `bump` counts the calls, and the
# pair before the override takes 1 although `price` is written first.
fresh = Widget.create!(name: "b", price: 10, kind: :deluxe)
got = probe("/widgets/#{fresh.id}/order.json")
raise "order #{got.inspect}" unless got == [200, %({"name":"b","price":2,"first":1,"last":3})]
# The merge is read before the pairs: `rename` changes the column after
# `attributes` answered it, so the merged `name` is the old one.
fresh = Widget.create!(name: "b", price: 10, kind: :deluxe)
got = probe("/widgets/#{fresh.id}/snapshot.json")
raise "snapshot #{got.inspect}" unless got == [200, %({"price":5,"name":"b"})]
fresh = Widget.create!(name: "b", price: 10, kind: :deluxe)
got = probe("/widgets/#{fresh.id}/snapshot_literal.json")
raise "snapshot_literal #{got.inspect}" unless got == [200, %({"a":5,"b":"b"})]
# Still refused: an override under a condition, an override that reads
# a local assigned after the merge, a key set twice after it, a Symbol
# key (Jbuilder keeps it beside the String one and raises when rendering)
# and a merged value the override would leave unevaluated.
%w[guarded local twice symbol effect].each do |name|
  refused = refusal("/widgets/#{w.id}/#{name}.json")
  raise "#{name} #{refused.inspect}" unless refused.to_s.include?("merge! not supported")
end
puts "ALL OK"
"##,
        )
        .assert_passes();
}
