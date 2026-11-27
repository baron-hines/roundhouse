//! A gem compiled from source that writes `begin require "x" … rescue LoadError … end` at its top
//! level loads the `begin` side when the tree provides `x`, as Ruby does; devise's
//! `rails/deprecated_constant_accessor.rb` so names `ActiveSupport::Deprecation::DeprecatedConstantAccessor`,
//! and a module that includes the constant holding it includes that module.

use roundhouse::build_target::BuildTarget;

use super::emit_and_run;

const SHELF: &str = r#"begin
  require "active_support/deprecation/constant_accessor"

  module Shelf
    Accessor = ActiveSupport::Deprecation::DeprecatedConstantAccessor
    LOADED = "rails"
  end
rescue LoadError
  module Shelf
    module Accessor
    end
    LOADED = "fallback"
  end
end

module Shelf
  class Box
    include Shelf::Accessor

    def self.loaded
      Shelf::LOADED
    end
  end

  class Crate
    include Accessor

    def self.kind
      "crate"
    end
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    let fixture = roundhouse::fixtures::real_blog();
    let lock = std::fs::read_to_string(fixture.join("Gemfile.lock")).unwrap();
    assert!(lock.contains("\n    rack (") && lock.contains("DEPENDENCIES\n"), "the fixture's Gemfile.lock changed shape");
    let lock = lock.replacen("\n    rack (", "\n    shelf (1.0.0)\n    rack (", 1).replacen("DEPENDENCIES\n", "DEPENDENCIES\n  shelf\n", 1);
    let gemfile = std::fs::read_to_string(fixture.join("Gemfile")).unwrap() + "gem \"shelf\"\n";
    emit_and_run::real_blog()
        .write("Gemfile", &gemfile)
        .write("Gemfile.lock", &lock)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("vendor/bundle/ruby/3.4.0/gems/shelf-1.0.0/lib/shelf.rb", SHELF)
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  def self.shelf_loaded\n    Shelf::Box.loaded + \"/\" + Shelf::Crate.kind\n  end\n\n")
}

#[test]
fn a_gems_load_error_fallback_is_left_where_the_tree_provides_the_feature() {
    let (tree, errors) = app().emit(BuildTarget::Spinel);
    assert!(!errors.iter().any(|e| e.contains("shelf")), "{errors:#?}");
    let shelf = std::fs::read_to_string(tree.join("app/models/shelf.rb")).expect("shelf.rb");
    assert!(shelf.contains("Accessor = ActiveSupport::Deprecation::DeprecatedConstantAccessor"), "{shelf}");
    assert!(!shelf.contains("fallback"), "{shelf}");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_gems_load_error_fallback_is_left_natively() {
    let run = app().run_spinel("raise Article.shelf_loaded unless Article.shelf_loaded == \"rails/crate\"\nputs \"loaded begin passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("loaded begin passed"), "{}", run.stdout);
}

fn method_body(source: &[u8]) -> Result<roundhouse::expr::Expr, String> {
    let result = ruby_prism::parse(source);
    let program = result.node();
    let def = program.as_program_node().unwrap().statements().body().iter().next().unwrap();
    let body = def.as_def_node().unwrap().body().unwrap();
    roundhouse::ingest::expr::ingest_expr(&body, "t.rb").map_err(|e| format!("{e:?}"))
}

/// `class << mod` in a method defines on `mod`; it is refused rather than read as `class << self`.
#[test]
fn a_singleton_class_of_another_object_in_a_method_is_refused() {
    let err = method_body(b"def config(mod)\n  class << mod\n    def x; end\n  end\nend\n").expect_err("refused");
    assert!(err.contains("other than self"), "{err}");
}

/// Attribute macros alone are `mod.singleton_class.attr_accessor(:x)`, which Ruby's `class << mod` runs.
#[test]
fn a_singleton_class_of_another_object_holding_only_attributes_is_its_singleton_classs_call() {
    let expr = method_body(b"def config(mod)\n  class << mod; attr_accessor :x; end\nend\n").expect("ingested");
    let roundhouse::expr::ExprNode::Send { recv: Some(recv), method, .. } = &*expr.node else { panic!("{expr:?}") };
    assert_eq!(method.as_str(), "attr_accessor");
    assert!(matches!(&*recv.node, roundhouse::expr::ExprNode::Send { method, .. } if method.as_str() == "singleton_class"), "{recv:?}");
}
