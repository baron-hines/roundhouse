//! Devise's `devise_for` calls `Devise.add_mapping(resource, options)` as Rails draws the routes, at boot,
//! and its controllers read `Devise.mappings` on every request. A routing method's call on its own gem's
//! class that changes state runs in the boot program, with the arguments the drawing gave it.

use super::emit_and_run;

const REGISTRY: &str = r#"module RouteRegistry
  @@names = []

  def self.add(name, options)
    @@names << [name, options[:path]]
  end

  def self.names
    @@names
  end

end

module ActionDispatch::Routing
  class Mapper
    def register_route(name, options = {})
      RouteRegistry.add(name, options)
      get options[:path], to: "articles#index"
    end
  end
end
"#;

const BOOTED: &str = "raise RouteRegistry.names.inspect unless RouteRegistry.names == [[:feed, \"/feed\"]]\n";

const SCRIPT: &str = r#"env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => "/feed", "QUERY_STRING" => "", "REMOTE_ADDR" => "127.0.0.1",
        "HTTP_HOST" => "localhost", "rack.input" => StringIO.new("") }
status, = Main.run_rack(env)
raise "status #{status}" unless status == 200
puts "boot state passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("lib/route_registry.rb", REGISTRY)
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  register_route :feed, path: \"/feed\"\n")
}

#[test]
fn a_routing_methods_state_change_runs_at_boot() {
    let run = app().run_ruby(&format!("{BOOTED}{SCRIPT}"));
    run.assert_passes();
    assert!(run.stdout.contains("boot state passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_routing_methods_state_change_runs_at_boot_natively() {
    let run = app().run_spinel(&format!("{BOOTED}puts \"boot state passed\"\n"));
    run.assert_passes();
    assert!(run.stdout.contains("boot state passed"), "{}", run.stdout);
}
