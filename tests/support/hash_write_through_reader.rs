//! One contract for a gem registry whose hash is written and read only
//! through a memoizing reader, shared by the interpreted (CRuby overlay)
//! and native (spinel) lanes. `add` stores `_kinds[label] = kind` and
//! `_kinds` is `@kinds ||= {}`: the only writes to `@kinds` go through
//! the reader. `update` reads the entry back with `_kinds[label]` and
//! passes it to `add` again, so `add`'s parameter is typed from a hash
//! its own writes fill. Every caller passes a class, so the `||=`
//! never assigns and `Class.new`, which Spinel cannot build, is dead
//! once the entry `update` reads back is typed as what `add` stored.
//! `add` asks `kind.method_defined?(:run)`, a Module query Spinel answers
//! only on a value proven to be one of those classes: the cycle through
//! the hash must not keep a `Class` arm from a round where `update`'s
//! argument was still untyped and the default could still run.
//!
//! The expected lines are CRuby's output for the same code.

pub fn overlay() -> super::emit_and_run::Overlay {
    let gem = "vendor/bundle/ruby/3.4.0/gems/kinds-1.0";
    super::emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    kinds (1.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  kinds\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"kinds\"}\n")
        .write(&format!("{gem}/lib/kinds.rb"), "require \"kinds/registry\"\n")
        .write(&format!("{gem}/lib/kinds/registry.rb"), REGISTRY)
        .write("config/initializers/registry.rb", INIT)
}

/// [`overlay`], with the default answered by a local lambda: its `.call`
/// gives the lambda its parameter types and retypes the `||=` statement,
/// which must stay as unknown as the first typing left it. Bound to the
/// lambda's `Integer` there, `kind` would carry it through the hash.
pub fn overlay_lambda_default() -> super::emit_and_run::Overlay {
    let gem = "vendor/bundle/ruby/3.4.0/gems/kinds-1.0";
    let default = "      kind ||= Class.new(Registry::Base)\n";
    assert!(REGISTRY.contains(default));
    let registry = REGISTRY.replace(
        default,
        "      fallback = ->(n) { n }\n      kind ||= fallback.call(1)\n",
    );
    overlay().write(&format!("{gem}/lib/kinds/registry.rb"), &registry)
}

/// [`overlay`], with the default in a method of its own whose body is
/// the one `||=`: `add` takes `pick(kind)`'s return. That body is no
/// sequence of statements, and its `Class.new` must stay out of the
/// return the first typing harvests, or `add` stores it in the hash.
pub fn overlay_default_method() -> super::emit_and_run::Overlay {
    let gem = "vendor/bundle/ruby/3.4.0/gems/kinds-1.0";
    let default = "      kind ||= Class.new(Registry::Base)\n";
    assert!(REGISTRY.contains(default));
    let registry = REGISTRY.replace(default, "      kind = pick(kind)\n").replace(
        "    def _kinds\n",
        "    def pick(kind = nil)\n      kind ||= Class.new(Registry::Base)\n    end\n\n    def _kinds\n",
    );
    overlay().write(&format!("{gem}/lib/kinds/registry.rb"), &registry)
}

const REGISTRY: &str = r##"class Registry
  class Base
    def run
      "base"
    end
  end

  class Plain < Base
    def run
      "plain"
    end
  end

  class Fancy < Base
    def run
      "fancy"
    end
  end

  class << self
    def add(label, kind = nil)
      kind ||= Class.new(Registry::Base)
      raise "#{kind} cannot run" unless kind.method_defined?(:run)
      _kinds[label] = kind
    end

    def update(label)
      kind = _kinds[label]
      raise "unknown kind #{label.inspect}" unless kind
      add(label, kind)
      kind.new.run
    end

    def _kinds
      @kinds ||= {}
    end
  end
end
"##;

const INIT: &str = "Registry.add(:plain, Registry::Plain)\nRegistry.add(:fancy, Registry::Fancy)\n";

pub const SCRIPT: &str = r##"
puts Registry.update(:plain)
puts Registry.update(:fancy)
"##;

pub const EXPECTED: &str = "plain\nfancy\n";
