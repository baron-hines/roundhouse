# The Rails side of tests/support/jbuilder_dynamic_partial.rs: the same
# schema, rows and templates rendered by Rails 8.1.4 + jbuilder 2.15.1.
# Run: ruby tests/support/jbuilder_dynamic_partial/oracle.rb
require_relative "../rails_oracle"
exit 77 unless RailsOracle.available?("railties", "actionpack", "activerecord", jbuilder: "2.15.1", sqlite3: ">= 2.9")
RailsOracle.activate("railties", "actionpack", "actionview", "activerecord", jbuilder: "2.15.1", sqlite3: ">= 2.9")
require "rails"
require "active_record"
require "action_controller/railtie"
require "jbuilder"
require "rack/mock"
require "tmpdir"

root = Dir.mktmpdir("jbuilder-dynamic-partial")
at_exit { FileUtils.rm_rf(root) }
views = File.join(root, "app/views")
{
  "widgets/_widget.json.jbuilder" => "json.id widget.id\njson.name widget.name\n",
  "widgets/_gadget.json.jbuilder" => "json.gadget widget.name\njson.size widget.size\n",
  "widgets/show.json.jbuilder" => "json.partial! \"widgets/\#{@widget.kind}\", widget: @widget\n",
  "widgets/wrapped.json.jbuilder" => "json.id @widget.id\njson.item do\n  json.partial!(\"widgets/\#{@widget.kind}\", widget: @widget)\nend\n",
}.each do |path, source|
  FileUtils.mkdir_p(File.dirname(File.join(views, path)))
  File.write(File.join(views, path), source)
end

class DynamicPartialOracle < Rails::Application
  config.eager_load = false
  config.secret_key_base = "jbuilder-dynamic-partial-oracle"
  config.hosts.clear
  config.logger = Logger.new(File::NULL)
end
DynamicPartialOracle.config.root = root
DynamicPartialOracle.initialize!
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define do
  create_table "widgets", force: :cascade do |t|
    t.string "name"
    t.integer "size"
    t.string "kind"
  end
end
class Widget < ActiveRecord::Base; end
class WidgetsController < ActionController::Base
  prepend_view_path File.join(Rails.root, "app/views")
  def show = (@widget = Widget.find(params[:id]))
  def wrapped = (@widget = Widget.find(params[:id]))
end
Widget.create!(name: "alpha", size: 3, kind: "widget")
Widget.create!(name: "beta", size: 5, kind: "gadget")
Widget.create!(name: "gamma", size: 7, kind: "sprocket")

%w[show wrapped].each do |action|
  (1..3).each do |id|
    env = Rack::MockRequest.env_for("/widgets/#{id}/#{action}.json")
    env["action_dispatch.request.path_parameters"] = {controller: "widgets", action: action, id: id.to_s, format: "json"}
    begin
      _, _, body = WidgetsController.action(action).call(env)
      out = +""
      body.each { |part| out << part }
      puts "#{action} #{id}: #{out}"
    rescue => e
      cause = e.cause ? " (cause #{e.cause.class})" : ""
      puts "#{action} #{id}: #{e.class}#{cause}: #{e.message.lines.first.strip}"
    end
  end
end
