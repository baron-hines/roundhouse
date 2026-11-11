require_relative "../rails_oracle"
unless RailsOracle.available?("railties", "actionpack")
  warn "skipping #{RailsOracle.label}: railties and actionpack are required"
  exit 77
end
RailsOracle.activate("railties", "actionpack")
require "rails"
require "action_controller/railtie"
require "rack/mock"
require "json"
class DefaultsApplication < Rails::Application
  config.load_defaults 8.1
  config.eager_load = false
  config.secret_key_base = "route-defaults-synthetic-control"
  config.hosts.clear
  config.logger = Logger.new(File::NULL)
end
DefaultsApplication.initialize!
class ApplicationController < ActionController::API; end
require_relative "controller"
DefaultsApplication.routes.draw do
  instance_eval(File.read(File.join(__dir__, "routes.rb")))
end
expected = JSON.parse(File.read(File.join(__dir__, "expected.json")))
actual = expected.to_h do |path, _|
  response = Rack::MockRequest.new(DefaultsApplication).get(path)
  raise "#{path}: #{response.status}" unless response.status == 200
  [path, response.body]
end
raise "controller params: #{actual.inspect}" unless actual == expected
recognized = DefaultsApplication.routes.recognize_path("/literal")
{ text: ["fixed", String], symbol: [:staff, Symbol], number: [1, Integer], yes: [true, TrueClass], no: [false, FalseClass], absent: [nil, NilClass] }.each do |key, (value, type)|
  raise "recognize #{key}" unless recognized.key?(key) && recognized[key] == value && recognized[key].class == type
end
raise "required helper" unless DefaultsApplication.routes.url_helpers.number_path == "/number/1"
raise "optional helper" unless DefaultsApplication.routes.url_helpers.optional_path == "/optional"
post = Rack::MockRequest.new(DefaultsApplication).post("/matched?text=query")
raise "match via" unless post.body == '"fixed"|nil|nil|nil|nil|nil'
puts "Rails route defaults contract passed"
[nil, false].each do |value|
  routes = ActionDispatch::Routing::RouteSet.new
  routes.draw { get '/required/:number', to: 'defaults#show', as: :required, defaults: { number: value } }
  begin
    routes.url_helpers.required_path
    raise "#{value.inspect} supplied a required segment"
  rescue ActionController::UrlGenerationError
  end
end
