//! Devise compiled from its installed source: `devise_for` draws what
//! `Devise::Mapping` and the model's modules choose, as in Rails.

use roundhouse::build_target::BuildTarget;

use super::emit_and_run;

const GEMS: [(&str, &str); 4] = [("devise", "4.9.4"), ("warden", "1.2.9"), ("orm_adapter", "0.5.0"), ("responders", "3.1.1")];

const SPECS: &str = "    bcrypt (3.1.20)
    devise (4.9.4)
      bcrypt (~> 3.0)
      orm_adapter (~> 0.1)
      railties (>= 4.1.0)
      responders
      warden (~> 1.2.3)
    orm_adapter (0.5.0)
    responders (3.1.1)
      actionpack (>= 5.2)
      railties (>= 5.2)
    warden (1.2.9)
      rack (>= 2.0.9)
";

const USERS: &str = r#"  create_table "users", force: :cascade do |t|
    t.string "email", default: "", null: false
    t.string "encrypted_password", default: "", null: false
    t.string "reset_password_token"
    t.datetime "reset_password_sent_at"
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
  end

"#;

fn installed(name: &str, version: &str) -> std::path::PathBuf {
    let out = std::process::Command::new("ruby")
        .args(["-e", &format!("print Gem::Specification.find_by_name('{name}', '{version}').gem_dir")])
        .output()
        .expect("spawn ruby");
    assert!(out.status.success(), "{name} {version} is not installed: {}", String::from_utf8_lossy(&out.stderr));
    std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn rb_files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rb_files(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "rb") {
            out.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

fn app(modules: &str, routes: &str) -> emit_and_run::Overlay {
    let fixture = roundhouse::fixtures::real_blog();
    let lock = std::fs::read_to_string(fixture.join("Gemfile.lock")).unwrap();
    // Not a bare "    rack (": a dependency line ("      rack (>= 2.2.4)") holds it too.
    let lock = lock.replacen("\n    rack (", &format!("\n{SPECS}    rack ("), 1).replacen("DEPENDENCIES\n", "DEPENDENCIES\n  devise\n", 1);
    let gemfile = std::fs::read_to_string(fixture.join("Gemfile")).unwrap() + "gem \"devise\"\n";
    let mut overlay = emit_and_run::real_blog()
        .write("Gemfile", &gemfile)
        .write("Gemfile.lock", &lock)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .edit("db/schema.rb", "  create_table \"articles\"", &format!("{USERS}  create_table \"articles\""))
        // Not without it: a Devise app's generator writes it, and it is what names `Devise` to the gems compiled from source.
        .write("config/initializers/devise.rb", "Devise.setup do |config|\n  config.mailer_sender = \"please-change-me@example.com\"\n  require \"devise/orm/active_record\"\nend\n")
        .write("app/models/user.rb", &format!("class User < ApplicationRecord\n  devise {modules}\nend\n"))
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{routes}"));
    for (name, version) in GEMS {
        let gem = installed(name, version);
        let mut files = Vec::new();
        rb_files(&gem, &gem.join("lib"), &mut files);
        for file in files {
            let text = std::fs::read_to_string(gem.join(&file)).unwrap();
            overlay = overlay.write(&format!("vendor/bundle/ruby/3.4.0/gems/{name}-{version}/{}", file.display()), &text);
        }
    }
    overlay
}

fn drawn(overlay: emit_and_run::Overlay) -> (String, String) {
    let (tree, _) = overlay.emit(BuildTarget::Ruby);
    let routes = std::fs::read_to_string(tree.join("config/routes.rb")).expect("routes.rb");
    let helpers = std::fs::read_to_string(tree.join("app/route_helpers.rb")).expect("route_helpers.rb");
    (routes, helpers)
}

/// The routes are the modules' (`Devise::Mapping#used_routes`): no `:confirmable`, no confirmation routes.
#[test]
fn devise_for_draws_the_routes_of_the_models_modules() {
    let (routes, helpers) = drawn(app(":database_authenticatable, :registerable, :recoverable", "  devise_for :users\n"));
    for route in [
        r#"Route.new("GET", "/users/sign_in", :devise__sessions, :new)"#,
        r#"Route.new("DELETE", "/users/sign_out", :devise__sessions, :destroy)"#,
        r#"Route.new("GET", "/users/cancel", :devise__registrations, :cancel)"#,
        r#"Route.new("PUT", "/users/password", :devise__passwords, :update)"#,
    ] {
        assert!(routes.contains(route), "{route}\n{routes}");
    }
    assert!(!routes.contains("confirmation"), "{routes}");
    for helper in ["new_user_session_path", "destroy_user_session_path", "cancel_user_registration_path", "edit_user_password_path"] {
        assert!(helpers.contains(&format!("def self.{helper}")), "{helper}\n{helpers}");
    }

    let (routes, _) = drawn(app(":database_authenticatable", "  devise_for :users\n"));
    assert!(routes.contains(r#"Route.new("GET", "/users/sign_in", :devise__sessions, :new)"#), "{routes}");
    assert!(!routes.contains("registrations") && !routes.contains("passwords"), "{routes}");

    let (routes, _) = drawn(app(":database_authenticatable, :registerable, :recoverable", "  devise_for :users, skip: [:registrations]\n"));
    assert!(routes.contains(r#"Route.new("GET", "/users/password/new", :devise__passwords, :new)"#), "{routes}");
    assert!(!routes.contains("registrations"), "{routes}");
}

/// Under `scope module:` Devise takes the scope's module in place of its own `devise` default,
/// and a `controllers:` override is the controller as written (`with_devise_exclusive_scope`).
#[test]
fn devise_for_options_and_an_enclosing_module_are_devises() {
    let (routes, _) = drawn(app(
        ":database_authenticatable, :registerable",
        "  scope module: :auth do\n    devise_for :users, path: \"account\", path_names: { sign_in: \"login\" }, controllers: { registrations: \"users/registrations\" }\n  end\n",
    ));
    for route in [
        r#"Route.new("GET", "/account/login", :auth__sessions, :new)"#,
        r#"Route.new("DELETE", "/account/sign_out", :auth__sessions, :destroy)"#,
        r#"Route.new("GET", "/account/sign_up", :users__registrations, :new)"#,
    ] {
        assert!(routes.contains(route), "{route}\n{routes}");
    }
    assert!(!routes.contains("auth/devise/") && !routes.contains("auth__users__"), "{routes}");
}

/// A subclass with no table of its own (`SuperAdmin < User`) has its superclass's modules, as `devise_modules` is a `class_attribute`.
#[test]
fn devise_for_a_subclass_draws_its_superclasss_modules() {
    let (routes, helpers) = drawn(
        app(":database_authenticatable, :recoverable", "  devise_for :admins, path: \"admin\"\n").write("app/models/admin.rb", "class Admin < User\nend\n"),
    );
    assert!(routes.contains(r#"Route.new("GET", "/admin/sign_in", :devise__sessions, :new)"#), "{routes}");
    assert!(routes.contains(r#"Route.new("GET", "/admin/password/new", :devise__passwords, :new)"#), "{routes}");
    assert!(helpers.contains("def self.new_admin_session_path"), "{helpers}");
}
