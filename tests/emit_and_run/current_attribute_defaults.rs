//! A CurrentAttributes class as Rails builds it: `attribute` takes a
//! `default:` (a literal, or a callable run on each reset), `reset` keeps
//! the instance and reassigns each attribute its default, and an
//! app-written reader or `reset` reaches the generated method with `super`.

use super::emit_and_run;

const CURRENT: &str = "class Current < ActiveSupport::CurrentAttributes
  attribute :user, :label
  attribute :theme, default: \"light\"
  attribute :tags, default: -> { [] }
  attribute :mode, default: \"first\"
  attribute :mode, default: \"second\"
  attribute :greeting, default: -> { return \"hi\" }
  attr_accessor :resets

  def label
    super || \"guest\"
  end

  def reset
    super
    self.resets = (resets || 0) + 1
  end
end
";

const SCRIPT: &str = r#"
raise "default theme: #{Current.theme.inspect}" unless Current.theme == "light"
before = Current.instance
raise "reader super: #{Current.label.inspect}" unless Current.label == "guest"
Current.label = "x"
raise "set label: #{Current.label.inspect}" unless Current.label == "x"
Current.tags << "a"
Current.reset
raise "after reset: #{Current.label.inspect}" unless Current.label == "guest"
raise "reset replaced the instance" unless Current.instance.equal?(before)
raise "callable default not rerun: #{Current.tags.inspect}" unless Current.tags.empty?
raise "reset override: #{Current.instance.resets.inspect}" unless Current.instance.resets == 1
raise "redeclared default: #{Current.mode.inspect}" unless Current.mode == "second"
raise "returned default: #{Current.greeting.inspect}" unless Current.greeting == "hi"
puts "current attributes OK"
"#;

#[test]
fn current_attribute_defaults_reset_and_super_match_rails() {
    emit_and_run::real_blog().write("app/models/current.rb", CURRENT).run_ruby(SCRIPT).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn current_attribute_defaults_reset_and_super_match_rails_on_spinel() {
    emit_and_run::real_blog().write("app/models/current.rb", CURRENT).run_spinel(SCRIPT).assert_passes();
}

/// A default with no fresh-copy shape here (a constant Rails would `dup`)
/// is reported, not dropped.
#[test]
fn an_unreproduced_default_is_reported() {
    let (_emitted, errors) = emit_and_run::real_blog()
        .write("app/models/current.rb", "class Current < ActiveSupport::CurrentAttributes\n  THEME = \"light\"\n  attribute :theme, default: THEME\nend\n")
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(!errors.is_empty(), "a constant default must not vanish silently");
}

/// Defaults whose meaning would change once inlined into the instance's
/// reset: the callable's `self` and helpers are the class's, and a nested
/// literal is shared by Rails' shallow `dup`.
#[test]
fn a_default_inlining_would_change_is_reported() {
    for default in ["-> { self }", "-> { helper }", "-> { @x }", "[[]]", "{ a: [] }", "-> { return 1 if Time.now.year > 0; 2 }"] {
        let source = format!("class Current < ActiveSupport::CurrentAttributes\n  attribute :theme, default: {default}\nend\n");
        let (_emitted, errors) = emit_and_run::real_blog()
            .write("app/models/current.rb", &source)
            .emit(roundhouse::project::BuildTarget::Ruby);
        assert!(!errors.is_empty(), "`default: {default}` must be reported");
    }
}
