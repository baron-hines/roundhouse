//! Class/Module factories and singleton evaluation bind self to a class
//! object. An ordinary constructor or an overridden core method does not.

use super::emit_and_run;
use roundhouse::project::BuildTarget;

const PROBE: &str = r#"class BlockProtocol
  def factory
    Class.new(BlockProtocol) do
      define_method(:value) { 17 }
      self.define_method(:other) { 23 }
    end
  end

  def module_factory
    Module.new do
      define_method(:value) { 31 }
    end
  end

  def absolute_factory
    ::Class.new { define_method(:value) { 37 } }
  end

  def absolute_module
    ::Module.new { define_method(:value) { 41 } }
  end

  def root_result
    klass = ::Class.new
    klass.define_method(:value) { 43 }
    klass
  end

  def self.install
    singleton_class.instance_eval do
      define_method(:added) { 47 }
    end
  end

  def self.remove
    singleton_class.instance_eval { remove_method(:added) }
  end

  def self.explicit
    self.singleton_class.instance_exec { define_method(:extra) { 53 } }
  end

  def ephemeral
    define_singleton_method(:later) { 61 }
    value = later
    singleton_class.remove_method(:later)
    [value, respond_to?(:later)]
  end

  class << self
    def singleton_syntax
      singleton_class.instance_eval { define_method(:from_singleton) { 67 } }
    end
  end
end
"#;

const ASSERTIONS: &str = r#"klass = BlockProtocol.new.factory
raise unless [klass.new.value, klass.new.other] == [17, 23]
raise unless klass.superclass == BlockProtocol
holder = Class.new
holder.include(BlockProtocol.new.module_factory)
raise unless holder.new.value == 31
raise unless BlockProtocol.new.absolute_factory.new.value == 37
holder.include(BlockProtocol.new.absolute_module)
raise unless holder.new.value == 41
raise unless BlockProtocol.new.root_result.new.value == 43
BlockProtocol.install
raise unless BlockProtocol.added == 47
BlockProtocol.remove
raise if BlockProtocol.respond_to?(:added)
BlockProtocol.explicit
raise unless BlockProtocol.extra == 53
raise unless BlockProtocol.new.ephemeral == [61, false]
BlockProtocol.singleton_syntax
raise unless BlockProtocol.from_singleton == 67
puts "block protocol passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/block_protocol.rb", PROBE)
}

#[test]
fn factory_and_singleton_block_self_runs() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("block protocol passed"), "{}", run.stdout);
}

#[test]
fn native_block_self_agrees() {
    let output = emit_and_run::ruby().arg("-e").arg(format!("{PROBE}\n{ASSERTIONS}")).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "block protocol passed\n");
}

#[test]
fn class_mutation_is_refused_on_other_targets() {
    use BuildTarget::*;
    for target in [Spinel, Rust, Typescript, TypescriptWorker, Go, Python, CSharp, Kotlin, Swift, Elixir, Crystal] {
        let (_, errors) = app().emit(target);
        for method in ["new", "singleton_class"] {
            let count = errors.iter().filter(|e| e.contains("runtime class object")
                && e.contains(&format!("`{method}` requires"))).count();
            let expected = 5;
            assert!(count >= expected, "{target:?} {method}: {errors:#?}");
        }
    }
    let (_, errors) = app().emit(Jruby);
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn unknown_or_replaced_receivers_stay_refused() {
    for source in [
        "class BlockProtocol\n def go(unknown)\n unknown.define_method(:x) { 1 }\n end\nend\n",
        "class BlockProtocol\n def self.singleton_class\n  1\n end\n def self.go\n  singleton_class.instance_eval { define_method(:x) { 1 } }\n end\nend\n",
        "class BlockProtocol\n def factory\n  BlockProtocol.new { define_method(:x) { 1 } }\n end\nend\n",
        "class Class\n def self.new\n  yield\n end\nend\nclass BlockProtocol\n def factory\n  Class.new { define_method(:x) { 1 } }\n end\nend\n",
        "class BlockProtocol\n def factory\n  Class.new { define_method(:x) { self.const_get(:X) } }\n end\nend\n",
        "class BlockProtocol\n def factory\n  Module.new { define_method(:x) { const_get(:X) } }\n end\nend\n",
        "class Class\n def instance_eval\n  yield\n end\nend\nclass BlockProtocol\n def factory\n  singleton_class.instance_eval { define_method(:x) { 1 } }\n end\nend\n",
        "class BlockProtocol < BasicObject\n def factory\n  singleton_class.define_method(:x) { 1 }\n end\nend\n",
        "class Class\n def initialize(*)\n  yield\n end\nend\nclass BlockProtocol\n def factory\n  Class.new { define_method(:x) { 1 } }\n end\nend\n",
        "class Module\n def initialize(*)\n  yield\n end\nend\nclass BlockProtocol\n def factory\n  Module.new { define_method(:x) { 1 } }\n end\nend\n",
        "class BlockProtocol\n def factory\n  Class.new do\n   inst = new\n   inst.const_get(:BlockProtocol)\n  end\n end\nend\n",
        "class BlockProtocol\n def factory\n  Module.new do\n   inst = new\n   inst.const_get(:BlockProtocol)\n  end\n end\nend\n",
        "class BlockProtocol\n def self.install\n  singleton_class.instance_eval do\n   inst = new\n   inst.const_get(:BlockProtocol)\n  end\n end\nend\n",
    ] {
        let (_, errors) = emit_and_run::real_blog()
            .write("app/models/block_protocol.rb", source).emit(BuildTarget::Ruby);
        assert!(errors.iter().any(|e| e.contains("Module protocol")), "{source}: {errors:#?}");
    }
}
