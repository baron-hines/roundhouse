# rails 8.1.4: a hook whose `instance_eval` block only registers a filter,
# guarded so it runs for controllers alone, leaves `Klass.new` alone; the
# hooks that do change `new` still change it. Run it outside a Rails app's
# tree:
#
#     ruby tests/included_hook_constructor/oracle.rb
gem "rails", "8.1.4"
require "rails"
require "action_controller/railtie"
class OracleApp < Rails::Application
  config.eager_load = false
  config.secret_key_base = "x" * 64
  config.logger = Logger.new(nil)
  config.hosts.clear
  config.action_dispatch.show_exceptions = :rescuable
end
OracleApp.initialize!
OracleApp.routes.draw { get "/gadgets", to: "gadgets#index" }

module Tagging
  def self.included(base)
    base.instance_eval do
      before_action(:load_tag) if base.methods.include?(:before_action)
    end
  end

  def load_tag
    @tag = "loaded"
  end

  def tag_label
    "tag"
  end
end

class WidgetClient
  include Tagging

  attr_reader :name

  def initialize(args)
    @name = args[:name]
  end

  def label
    "#{tag_label}:#{@name}"
  end
end

class GadgetsController < ActionController::Base
  include Tagging

  def index
    render json: { tag: @tag, label: WidgetClient.new(name: "w").label }
  end
end

widget = WidgetClient.new(name: "plain")
puts "plain includer: new is Class#new=#{WidgetClient.method(:new).owner == Class} label=#{widget.label}"
puts "plain includer filters: #{WidgetClient.respond_to?(:_process_action_callbacks)}"

session = ActionDispatch::Integration::Session.new(OracleApp)
session.get "/gadgets"
puts "controller includer: status=#{session.response.status} body=#{session.response.body}"

# The registrations the compiler reads as leaving `new` alone.
macros = %i[before_action after_action around_action append_before_action append_after_action
            append_around_action prepend_before_action prepend_after_action prepend_around_action]
controller = Class.new(ActionController::Base)
before = controller.method(:new).unbind
macros.each { |macro| controller.public_send(macro, :load_tag) }
%i[skip_before_action skip_after_action skip_around_action].each do |macro|
  controller.public_send(macro, :load_tag, raise: false)
end
puts "filter macros keep new: #{controller.method(:new).unbind == before}"

# Each of these eval blocks really changes what `new` does.
{
  "define_singleton_method" => "base.instance_eval { define_singleton_method(:new) { |**o| \"replaced\" } }",
  "def in instance_eval" => "base.instance_eval { def new(**o) = \"replaced\" }",
  "def self in class_eval" => "base.class_eval { def self.new(**o) = \"replaced\" }",
  "eval string" => "base.instance_eval { eval(\"def self.new(**o) = 'replaced'\") }",
  "self through a local" => "base.instance_eval { target = self; target.define_singleton_method(:new) { |**o| \"replaced\" } }",
  "private_class_method" => "base.instance_eval { private_class_method :new }",
  "method object" => "base.instance_eval { method(:define_singleton_method).call(:new) { |**o| \"replaced\" } }",
  "base.send through singleton" => "base.instance_eval { singleton_class.send(:define_method, :new) { |**o| \"replaced\" } }",
}.each do |label, body|
  hook = Module.new
  hook.module_eval("def self.included(base)\n  #{body}\nend\n", __FILE__, __LINE__)
  klass = Class.new do
    include hook
    def initialize(args) = @args = args
  end
  result = begin
    klass.new(name: "x").then { |v| v == "replaced" ? v : "initialize ran" }
  rescue NoMethodError => e
    "NoMethodError: #{e.message[/private method .new./]}"
  end
  puts "#{label}: #{result}"
end
