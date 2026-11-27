//! Devise's `Devise::Mapping.add_module(m)` defines `#{m}?` by a `class_eval` string as its files load
//! (`Devise.with_options model: true do |d| d.add_module :validatable end`), and its controllers ask
//! `devise_mapping.validatable?`. A gem's load-time string definer, run as its files load, gives the class
//! those methods, and its body, nothing but the string, runs as nothing at boot.

use roundhouse::build_target::BuildTarget;

use super::emit_and_run;

const FLAGS: &str = r##"module Flags
  PATH = "/flags"

  class Card
    def self.add_flag(name)
      class_eval <<-METHOD, __FILE__, __LINE__ + 1
        def #{name}?
          true
        end
      METHOD
    end
  end
end

Flags::Card.add_flag(:shiny)

module ActionDispatch::Routing
  class Mapper
    def flag_routes
      get Flags::PATH, to: "articles#index"
    end
  end
end
"##;

fn app() -> emit_and_run::Overlay {
    let fixture = roundhouse::fixtures::real_blog();
    let lock = std::fs::read_to_string(fixture.join("Gemfile.lock")).unwrap();
    assert!(lock.contains("\n    rack (") && lock.contains("DEPENDENCIES\n"), "the fixture's Gemfile.lock changed shape");
    let lock = lock.replacen("\n    rack (", "\n    flags (1.0.0)\n    rack (", 1).replacen("DEPENDENCIES\n", "DEPENDENCIES\n  flags\n", 1);
    let gemfile = std::fs::read_to_string(fixture.join("Gemfile")).unwrap() + "gem \"flags\"\n";
    emit_and_run::real_blog()
        .write("Gemfile", &gemfile)
        .write("Gemfile.lock", &lock)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("vendor/bundle/ruby/3.4.0/gems/flags-1.0.0/lib/flags.rb", FLAGS)
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  flag_routes\n")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  def self.shiny_card?\n    Flags::Card.new.shiny?\n  end\n\n")
}

#[test]
fn a_gems_load_time_string_definer_gives_the_class_its_methods() {
    let (tree, errors) = app().emit(BuildTarget::Spinel);
    assert!(!errors.iter().any(|e| e.contains("flags")), "{errors:#?}");
    let card = std::fs::read_to_string(tree.join("app/models/flags/card.rb")).expect("card.rb");
    assert!(card.contains("def shiny?"), "{card}");
    assert!(!card.contains("class_eval"), "{card}");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_gems_load_time_string_definer_gives_the_class_its_methods_natively() {
    let run = app().run_spinel("raise \"shiny\" unless Article.shiny_card?\nputs \"string definer passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("string definer passed"), "{}", run.stdout);
}
