//! A routing method a gem mixes into the mapper, as doorkeeper's `use_doorkeeper` is: its body draws
//! the routes, here from a block that configures it, and they answer where the call stands.

use super::emit_and_run;

const PING_ROUTES: &str = r##"module PingRoutes
  module Helper
    def use_pings(options = {}, &block)
      PingRoutes::Drawer.new(self, &block).draw(options[:path] || "pings")
    end
  end

  class Drawer
    def initialize(routes, &block)
      @routes = routes
      @controller = "pings"
      instance_eval(&block) if block
    end

    def controller(name)
      @controller = name
    end

    def draw(path)
      @routes.scope path, as: "pings" do
        @routes.get "/", controller: @controller, action: :show, as: :show
      end
    end
  end

  def self.install!
    ActionDispatch::Routing::Mapper.include PingRoutes::Helper
  end
end
"##;

#[test]
fn a_gems_routing_method_draws_routes_that_answer() {
    emit_and_run::real_blog()
        .write("lib/ping_routes.rb", PING_ROUTES)
        .write(
            "app/controllers/pongs_controller.rb",
            "class PongsController < ApplicationController\n  def show\n    render plain: \"pong #{pings_show_path}\"\n  end\nend\n",
        )
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  use_pings path: \"ping\" do\n    controller \"pongs\"\n  end\n",
        )
        .write(
            "test/controllers/pongs_controller_test.rb",
            "require \"test_helper\"\n\nclass PongsControllerTest < ActionDispatch::IntegrationTest\n  test \"the drawn route answers\" do\n    get \"/ping\"\n    assert_response :success\n    assert_equal \"pong /ping\", response.body\n  end\nend\n",
        )
        .run_test("test/controllers/pongs_controller_test.rb")
        .assert_passes();
}

const BELL_ROUTES: &str = r##"module Bells
  mattr_accessor :prefix
  @@prefix = "bells"

  def self.setup
    yield self
  end

  module Helper
    def use_bells(*names)
      options = names.extract_options!
      names.map! { |name| name.to_sym }
      base = { as: "bells" }.merge(options.slice(:as))
      with_options controller: "bells" do |routes|
        names.each do |name|
          routes.get "#{Bells.prefix}/#{name}", action: name, as: "#{base[:as]}_#{name}"
        end
      end
    end
  end

  def self.install!
    ActionDispatch::Routing::Mapper.include Bells::Helper
  end
end
"##;

/// The method takes its options as devise's `devise_for` does
/// (`extract_options!`, `map!`, a merged and sliced Hash), draws through
/// `with_options`, and reads a setting an initializer gave through the
/// gem's `setup { |config| … }`.
#[test]
fn a_gems_routing_method_reads_its_options_and_its_setup_block() {
    emit_and_run::real_blog()
        .write("lib/bells.rb", BELL_ROUTES)
        .write("config/initializers/bells.rb", "Bells.setup do |config|\n  config.prefix = \"chimes\"\nend\n")
        .write(
            "app/controllers/bells_controller.rb",
            "class BellsController < ApplicationController\n  def ding\n    render plain: \"ding #{bell_ding_path} #{bell_dong_path}\"\n  end\n\n  def dong\n    render plain: \"dong\"\n  end\nend\n",
        )
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  use_bells \"ding\", :dong, as: \"bell\"\n")
        .write(
            "test/controllers/bells_controller_test.rb",
            "require \"test_helper\"\n\nclass BellsControllerTest < ActionDispatch::IntegrationTest\n  test \"the drawn routes answer\" do\n    get \"/chimes/ding\"\n    assert_response :success\n    assert_equal \"ding /chimes/ding /chimes/dong\", response.body\n  end\nend\n",
        )
        .run_test("test/controllers/bells_controller_test.rb")
        .assert_passes();
}

const CHIMES: &str = r##"module Chimes
  ROUTES = {}

  def self.add_route(name, options = {})
    key = case options[:route]
          when Symbol then options[:route]
          when true then name
          end
    ROUTES[name] = key
  end
end

Chimes.add_route :ring, route: :ringer
Chimes.add_route :quiet

module ActionDispatch::Routing
  class Mapper
    def chimes_for(resource, options = {})
      names = Hash.new { |h, k| h[k] = k.to_s }
      names.merge!(options[:path_names]) if options[:path_names]
      routes = Chimes::ROUTES.values.compact
      prefix = @scope[:path]
      as = [@scope[:as], resource].compact.join("_")
      with_chimes_scope("#{prefix}/#{resource}", as) do
        routes.each { |route| send("chimes_#{route}", names) }
      end
    end

    protected

    def chimes_ringer(names)
      resource :ringer, only: [:show, :new], path: names[:ringer], controller: "chimes", path_names: { new: names[:start] }
    end

    def with_chimes_scope(path, as)
      current = @scope.dup
      exclusive = { as: as, path: path, module: nil }
      if @scope.respond_to?(:new)
        @scope = @scope.new(exclusive)
      else
        exclusive.each_pair { |key, value| @scope[key] = value }
      end
      yield
    ensure
      @scope = current
    end
  end
end
"##;

/// devise's shape, in a gem of its own: the gem's files register what the
/// routing method reads (`Chimes.add_route`, a `case` over the option),
/// the method reopens the mapper, keeps a Hash with a default block, calls
/// the mapper's other methods by a built name, and sets `@scope` for its
/// routes as `with_devise_exclusive_scope` does.
#[test]
fn a_gems_routing_method_that_sets_its_own_scope_draws_routes_that_answer() {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "GEM\n", "PATH\n  remote: vendor/gems/chimes\n  specs:\n    chimes (1.0.0)\n\nGEM\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  chimes!\n")
        .write("vendor/gems/chimes/lib/chimes.rb", CHIMES)
        .write(
            "app/controllers/chimes_controller.rb",
            "class ChimesController < ApplicationController\n  def show\n    render plain: \"show #{bells_ringer_path}\"\n  end\n\n  def new\n    render plain: \"new #{new_bells_ringer_path}\"\n  end\nend\n",
        )
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  chimes_for :bells, path_names: { ringer: \"ring\", start: \"begin\" }\n")
        .write(
            "test/controllers/chimes_controller_test.rb",
            "require \"test_helper\"\n\nclass ChimesControllerTest < ActionDispatch::IntegrationTest\n  test \"the routes the gem draws answer\" do\n    get \"/bells/ring\"\n    assert_equal \"show /bells/ring\", response.body\n    get \"/bells/ring/begin\"\n    assert_equal \"new /bells/ring/begin\", response.body\n  end\nend\n",
        )
        .run_test("test/controllers/chimes_controller_test.rb")
        .assert_passes();
}

/// The same method inside a `scope`: as devise's, it reads the enclosing scope's path and name off
/// `@scope` and draws its routes under a scope of its own, which takes neither again.
#[test]
fn a_gems_routing_method_inside_a_scope_draws_under_its_own() {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "GEM\n", "PATH\n  remote: vendor/gems/chimes\n  specs:\n    chimes (1.0.0)\n\nGEM\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  chimes!\n")
        .write("vendor/gems/chimes/lib/chimes.rb", CHIMES)
        .write(
            "app/controllers/chimes_controller.rb",
            "class ChimesController < ApplicationController\n  def show\n    render plain: \"show #{kiosk_bells_ringer_path}\"\n  end\n\n  def new\n    render plain: \"new #{new_kiosk_bells_ringer_path}\"\n  end\nend\n",
        )
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  scope \"kiosk\", as: \"kiosk\" do\n    chimes_for :bells, path_names: { ringer: \"ring\" }\n  end\n")
        .write(
            "test/controllers/chimes_controller_test.rb",
            "require \"test_helper\"\n\nclass ChimesControllerTest < ActionDispatch::IntegrationTest\n  test \"the routes stand under the gem's own scope\" do\n    get \"/kiosk/bells/ring\"\n    assert_equal \"show /kiosk/bells/ring\", response.body\n    get \"/kiosk/bells/ring/start\"\n    assert_equal \"new /kiosk/bells/ring/start\", response.body\n  end\nend\n",
        )
        .run_test("test/controllers/chimes_controller_test.rb")
        .assert_passes();
}

const DEVISE_SHAPED: &str = r##"module Devise
  def self.providers
    ENV.fetch("SIGN_IN_PROVIDERS", "").split(",")
  end
end

module ActionDispatch::Routing
  class Mapper
    def sign_in_routes_for(resource)
      scope "/#{resource}", as: resource do
        devise_session(resource)
        devise_omniauth_callback(resource)
      end
    end

    protected

    def devise_session(resource)
      get "sign_in", to: "articles#index", as: "sign_in"
    end

    def devise_omniauth_callback(resource)
      Devise.providers.each do |provider|
        get "auth/#{provider}", to: "articles#index"
      end
    end
  end
end
"##;

fn devise_shaped() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "GEM\n", "PATH\n  remote: vendor/gems/devise_shaped\n  specs:\n    devise_shaped (1.0.0)\n\nGEM\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  devise_shaped!\n")
        .write("vendor/gems/devise_shaped/lib/devise_shaped.rb", DEVISE_SHAPED)
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  sign_in_routes_for :users\n")
}

/// devise's OmniAuth callbacks are one route per provider the app sets up,
/// often from ENV at boot: not known here, they are left out and reported
/// at the call, and the rest of what the call draws stands.
#[test]
fn a_devise_omniauth_callback_whose_providers_are_not_known_is_reported_and_the_rest_drawn() {
    let errors = devise_shaped().check_errors();
    assert!(errors.iter().any(|e| e.contains("OmniAuth callback routes are not drawn")), "{errors:#?}");
    // Not a request: the left-out routes are an error, which keeps the tree's tests from being written.
    let (tree, _) = devise_shaped().emit(roundhouse::build_target::BuildTarget::Ruby);
    let routes = std::fs::read_to_string(tree.join("config/routes.rb")).expect("routes.rb");
    assert!(routes.contains("\"/users/sign_in\""), "{routes}");
    assert!(!routes.contains("/users/auth/"), "{routes}");
}

const DEVISE_SETUP: &str = r##"module Devise
  mattr_accessor :prefix
  @@prefix = "bells"
  mattr_accessor :loud
  @@loud = false

  def self.setup
    yield self
  end
end

module ActionDispatch::Routing
  class Mapper
    def ring_routes_for(resource)
      get "#{Devise.prefix}/#{resource}", to: "articles#index", as: "ring"
    end
  end
end
"##;

fn devise_setup(initializer: &str) -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "GEM\n", "PATH\n  remote: vendor/gems/devise_setup\n  specs:\n    devise_setup (1.0.0)\n\nGEM\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  devise_setup!\n")
        .write("vendor/gems/devise_setup/lib/devise_setup.rb", DEVISE_SETUP)
        .write("config/initializers/devise.rb", initializer)
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  ring_routes_for :users\n")
}

/// A setting `Devise.setup` sets under a condition not known here leaves that setting unsure, and
/// only that one: a route that reads another stands, one that reads it is refused.
#[test]
fn a_devise_setup_statement_passed_over_leaves_only_its_settings_unsure() {
    let (tree, _) = devise_setup("Devise.setup do |config|\n  config.prefix = \"chimes\"\n  if ENV[\"LOUD\"] == \"1\"\n    config.loud = true\n  end\nend\n")
        .emit(roundhouse::build_target::BuildTarget::Ruby);
    let routes = std::fs::read_to_string(tree.join("config/routes.rb")).expect("routes.rb");
    assert!(routes.contains("\"/chimes/users\""), "{routes}");

    let errors = devise_setup("Devise.setup do |config|\n  if ENV[\"CHIMES\"] == \"1\"\n    config.prefix = \"chimes\"\n  end\nend\n").check_errors();
    assert!(errors.iter().any(|e| e.contains("ring_routes_for")), "{errors:#?}");

    // Not only a setting the statement names: one it reaches through the app's own code, or in a block a call runs, is unsure too.
    for initializer in [
        "Devise.setup do |config|\n  if ENV[\"CHIMES\"] == \"1\"\n    Chimes.apply(config)\n  end\nend\n",
        "Devise.setup do |config|\n  if ENV[\"CHIMES\"] == \"1\"\n    [\"chimes\"].each { |prefix| config.prefix = prefix }\n  end\nend\n",
        "Devise.setup do |config|\n  if ENV[\"CHIMES\"] == \"1\"\n    target = Chimes\n    target.ring(\"chimes\")\n  end\nend\n",
        "Devise.setup do |config|\n  if ENV[\"CHIMES\"] == \"1\"\n    [\"chimes\"].each(&Chimes.method(:ring))\n  end\nend\n",
    ] {
        let errors = devise_setup(initializer)
            .write("app/models/chimes.rb", "module Chimes\n  def self.apply(config)\n    config.prefix = \"chimes\"\n  end\n\n  def self.ring(prefix)\n    Devise.prefix = prefix\n  end\nend\n")
            .check_errors();
        assert!(errors.iter().any(|e| e.contains("ring_routes_for")), "{initializer}\n{errors:#?}");
    }
}

const TOKEN_SHAPED: &str = r##"module Bells
  mattr_accessor :prefix
  @@prefix = "bells"
  LIST = []

  def self.setup
    yield self
  end
end

if defined?(Bells::LIST)
  Bells::LIST << :tokens
end

module DeviseTokenAuth
  mattr_accessor :omniauth_prefix
  self.omniauth_prefix = "/omniauth"
end

module ActionDispatch::Routing
  class Mapper
    def bells_for(name)
      get "#{Bells.prefix}/#{name}", to: "articles#index", as: "bells_#{name}"
    end

    def mount_bells_for(resource, opts)
      route = opts[:as] || resource.pluralize.underscore.gsub("/", "_")
      bells_for(route)
      get "#{DeviseTokenAuth.omniauth_prefix}/#{route}", to: "articles#index", as: "token_#{route}"
      if defined?(::OmniAuth)
        get "#{DeviseTokenAuth.omniauth_prefix}/:provider", to: redirect { |params, request| "/#{params[:provider]}" }
      end
    end
  end
end
"##;

/// devise_token_auth's shape: a routing method reaches the settings of a gem it names only through
/// another routing method, its own defaults are set through `mattr_accessor` writers, a gem's file
/// guards a write with `defined?`, and its OmniAuth routes redirect through a per-request block.
/// The rest is drawn, and those OmniAuth routes are left out and reported.
#[test]
fn a_routing_method_reaching_a_gem_through_another_draws_with_its_settings() {
    let (tree, errors) = emit_and_run::real_blog()
        .edit("Gemfile.lock", "GEM\n", "PATH\n  remote: vendor/gems/token_shaped\n  specs:\n    token_shaped (1.0.0)\n\nGEM\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  token_shaped!\n")
        .write("vendor/gems/token_shaped/lib/token_shaped.rb", TOKEN_SHAPED)
        .write("config/initializers/bells.rb", "Bells.setup do |config|\n  config.prefix = \"chimes\"\nend\n")
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  mount_bells_for \"Admin::User\", at: \"auth\"\n")
        .emit(roundhouse::build_target::BuildTarget::Ruby);
    let routes = std::fs::read_to_string(tree.join("config/routes.rb")).expect("routes.rb");
    assert!(routes.contains("\"/chimes/admin_users\""), "{routes}");
    assert!(routes.contains("\"/omniauth/admin_users\""), "{routes}");
    assert!(!routes.contains(":provider"), "{routes}");
    assert!(errors.iter().any(|e| e.contains("the OmniAuth routes are not drawn")), "{errors:#?}");
}

/// A routing method's block constraint is written with the locals it closes over as their values,
/// unless the block could read one other than that value: then the routes are refused, with why.
#[test]
fn a_block_constraint_that_could_read_a_captured_local_otherwise_is_refused() {
    for (body, why) in [
        ("allowed, unused = false, 0\n          allowed", "rebinds `allowed`"),
        ("case req.path\n          when allowed then true\n          end", "reads `allowed`"),
        ("allowed = false\n          allowed", "rebinds `allowed`"),
    ] {
        let gate = format!(
            "module Gates\n  module Helper\n    def gated\n      allowed = true\n      constraints(lambda do |req|\n          {body}\n        end) do\n        get \"/gated\", to: \"articles#index\"\n      end\n    end\n  end\n\n  def self.install!\n    ActionDispatch::Routing::Mapper.include Gates::Helper\n  end\nend\n"
        );
        let (_, errors) = emit_and_run::real_blog()
            .write("lib/gates.rb", &gate)
            .write("config/initializers/gates.rb", "Gates.install!\n")
            .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  gated\n")
            .emit(roundhouse::project::BuildTarget::Ruby);
        assert!(errors.iter().any(|e| e.contains("not drawn") && e.contains(why)), "{why}: {errors:#?}");
    }
}
