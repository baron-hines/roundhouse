//! A plain-Ruby gem the app's code names, compiled from source on Spinel,
//! whose class the app reopens in `lib/` from an initializer, as an app
//! adds to a gem's class. Ruby loads the gem first (Bundler) and the
//! reopen after it, so the class holds the gem's methods and the app's,
//! and a method both define is the app's. Shared by the Ruby lane and the
//! Spinel lane; the answers are Ruby 4.0.7's.

use super::emit_and_run;

pub const GEM_DIR: &str = "vendor/bundle/ruby/3.4.0/gems/shelf-1.0.0";

pub const SHELF: &str = r##"module Shelf
  class Box
    def self.kind
      "box"
    end

    def self.label
      "shelf box"
    end
  end
end
"##;

/// Adds a method and redefines one the gem has.
pub const LOCAL: &str = r##"module Shelf
  class Box
    def self.local_tag
      "local"
    end

    def self.label
      "app box"
    end
  end
end
"##;

/// [`LOCAL`] in the compact form, as an initializer often writes it.
pub const LOCAL_COMPACT: &str = r##"class Shelf::Box
  def self.local_tag
    "local"
  end

  def self.label
    "app box"
  end
end
"##;

/// [`LOCAL`] as a `class_eval` block in an initializer: `self` in the
/// block is the class, so `def self.…` defines its class methods.
pub const CLASS_EVAL: &str = r##"Shelf::Box.class_eval do
  def self.local_tag
    "local"
  end

  def self.label
    "app box"
  end
end
"##;

/// A `class_eval` block that also writes a constant. Ruby puts `LIMIT` in
/// the block's lexical scope (the top level), not in `Shelf::Box`, so the
/// block is not the same as a reopen and is left as it is.
pub const CLASS_EVAL_LIMIT: &str = r##"Shelf::Box.class_eval do
  LIMIT = 3

  def self.local_tag
    "local"
  end
end
"##;

/// Adds only a constant and an `include`, no method.
pub const EXTRAS: &str = r##"module Shelf
  class Box
    LIMIT = 3
    include Tagging
  end
end
"##;

pub const TAGGING: &str = r##"module Tagging
  def tag
    "tagged"
  end
end
"##;

pub const ARTICLE_BOX: &str = r##"class ArticleBox
  def self.describe
    [Shelf::Box.kind, Shelf::Box.label, Shelf::Box.local_tag].join(" / ")
  end
end
"##;

/// The app's use of the class a constant-only reopen extends. It names
/// `Shelf::Box` alone: a constant path under it (`Shelf::Box::LIMIT`) is
/// not a name the app declares, and brought the gem in already.
pub const BOX_LIMITS: &str = r##"class BoxLimits
  def self.describe
    [Shelf::Box.kind, Shelf::Box.new.tag]
  end
end
"##;

pub const LOCAL_ASSERTIONS: &str = r##"got = ArticleBox.describe
raise "describe #{got.inspect}" unless got == "box / app box / local"
puts "reopen passed"
"##;

pub const EXTRAS_ASSERTIONS: &str = r##"got = BoxLimits.describe + [Shelf::Box::LIMIT]
raise "extras #{got.inspect}" unless got == ["box", "tagged", 3]
puts "extras passed"
"##;

/// real-blog with the `shelf` gem installed under `vendor/bundle` and
/// locked, but not listed in `source_gems`: the app's naming of
/// `Shelf::Box` is what brings it in.
fn with_gem() -> emit_and_run::Overlay {
    with_gem_lib(SHELF)
}

/// [`with_gem`] with `lib/shelf.rb` holding `lib`.
fn with_gem_lib(lib: &str) -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    shelf (1.0.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  shelf\n")
        .edit("Gemfile", "gem \"rails\", \"~> 8.1.4\"\n", "gem \"rails\", \"~> 8.1.4\"\ngem \"shelf\"\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{GEM_DIR}/lib/shelf.rb"), lib)
}

/// The same app with the gem listed in `source_gems`, which compiles it
/// from source on every target: the Ruby family otherwise loads the
/// installed gem through its Gemfile, which the test tree does not have.
#[allow(dead_code)]
pub fn listed(app: emit_and_run::Overlay) -> emit_and_run::Overlay {
    app.write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"shelf\"}\n")
}

/// The reopen that adds `local_tag` and redefines `label`.
#[allow(dead_code)]
pub fn local_app() -> emit_and_run::Overlay {
    with_gem()
        .write("lib/shelf_box_local.rb", LOCAL)
        .write("config/initializers/shelf_box_local.rb", "require Rails.root.join(\"lib/shelf_box_local\").to_s\n")
        .write("app/models/article_box.rb", ARTICLE_BOX)
}

/// The reopen that adds only `LIMIT` and `include Tagging`.
#[allow(dead_code)]
pub fn extras_app() -> emit_and_run::Overlay {
    with_gem()
        .write("app/models/tagging.rb", TAGGING)
        .write("lib/shelf_box_extras.rb", EXTRAS)
        .write("config/initializers/shelf_box_extras.rb", "require Rails.root.join(\"lib/shelf_box_extras\").to_s\n")
        .write("app/models/box_limits.rb", BOX_LIMITS)
}

/// The reopen written in the initializer itself rather than in `lib/`:
/// Rails runs it at boot, after Bundler has loaded the gem.
#[allow(dead_code)]
pub fn initializer_app() -> emit_and_run::Overlay {
    with_gem()
        .write("config/initializers/shelf_box.rb", LOCAL)
        .write("app/models/article_box.rb", ARTICLE_BOX)
}

/// [`initializer_app`] with the reopen in the compact `class Shelf::Box` form.
#[allow(dead_code)]
pub fn compact_initializer_app() -> emit_and_run::Overlay {
    with_gem()
        .write("config/initializers/shelf_box.rb", LOCAL_COMPACT)
        .write("app/models/article_box.rb", ARTICLE_BOX)
}

/// The reopen as a `class_eval` block in an initializer (#2093's second form).
#[allow(dead_code)]
pub fn class_eval_app() -> emit_and_run::Overlay {
    with_gem()
        .write("config/initializers/shelf_box.rb", CLASS_EVAL)
        .write("app/models/article_box.rb", ARTICLE_BOX)
}

/// [`class_eval_app`] with a block that also writes a constant.
#[allow(dead_code)]
pub fn class_eval_limit_app() -> emit_and_run::Overlay {
    with_gem()
        .write("config/initializers/shelf_box.rb", CLASS_EVAL_LIMIT)
        .write("app/models/article_box.rb", ARTICLE_BOX)
}

/// A gem module a controller includes.
pub const SHELF_TAGGING: &str = r##"module Shelf
  module Tagging
    def original
      "gem"
    end

    def label
      "gem label"
    end
  end
end
"##;

/// An initializer's reopen of it: a method of its own, and `label` redefined.
pub const TAGGING_REOPEN: &str = r##"module Shelf
  module Tagging
    def local
      "local"
    end

    def label
      "app label"
    end
  end
end
"##;

pub const SHELVES_CONTROLLER: &str = r##"class ShelvesController < ApplicationController
  include Shelf::Tagging

  def show
    render plain: [original, local, label].join(" / ")
  end
end
"##;

/// The controller including the gem module the initializer reopens.
/// Ruby answers `"gem / local / app label"` for `show`.
#[allow(dead_code)]
pub fn included_module_app() -> emit_and_run::Overlay {
    with_gem_lib(SHELF_TAGGING)
        .write("config/initializers/shelf_tagging.rb", TAGGING_REOPEN)
        .write("app/controllers/shelves_controller.rb", SHELVES_CONTROLLER)
}

/// [`included_module_app`] with the reopen in `lib/`, required by an
/// initializer (#2122's form). Ruby answers the same.
#[allow(dead_code)]
pub fn included_module_lib_app() -> emit_and_run::Overlay {
    with_gem_lib(SHELF_TAGGING)
        .write("lib/shelf_tagging.rb", TAGGING_REOPEN)
        .write("config/initializers/shelf_tagging.rb", "require Rails.root.join(\"lib/shelf_tagging\").to_s\n")
        .write("app/controllers/shelves_controller.rb", SHELVES_CONTROLLER)
}

/// A gem module with an empty body.
pub const SHELF_HOOKS: &str = r##"module Shelf
  module Hooks
  end
end
"##;

pub const HOOKS_REOPEN: &str = r##"module Shelf
  module Hooks
    def self.hook
      "hooked"
    end
  end
end
"##;

pub const HOOK_PROBE: &str = r##"class HookProbe
  def self.describe
    Shelf::Hooks.hook
  end
end
"##;

/// An initializer adding a method to a module the gem declares empty.
/// Ruby answers `"hooked"` for `HookProbe.describe`.
#[allow(dead_code)]
pub fn empty_module_app() -> emit_and_run::Overlay {
    with_gem_lib(SHELF_HOOKS)
        .write("config/initializers/shelf_hooks.rb", HOOKS_REOPEN)
        .write("app/models/hook_probe.rb", HOOK_PROBE)
}
