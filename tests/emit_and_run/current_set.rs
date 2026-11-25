//! `Current.set(attr: value) { … }` in an emitted project: the block runs
//! with the attributes set and they are put back on every way out. The
//! expected values are what Rails' `Object#with` answers; the emitted
//! project does not load ActiveSupport. Ported from #232.

use super::emit_and_run;

const CURRENT: &str = r#"class Current < ActiveSupport::CurrentAttributes
  attribute :user, :account, :mode
  TRACE = ["seed"]
  def user=(value)
    TRACE << "user"
    super(value)
    self.account = "side" if value == 83 && (mode == :side || mode == :read)
    raise "restore" if value == 47 && mode == :restore
  end
  def account=(value)
    TRACE << "account"
    super(value)
    @observed_account = value
    raise "setup" if value == "bad"
  end
  def account
    raise "read" if user == 83 && mode == :read
    @observed_account
  end
end
"#;

const PROBE: &str = r#"class ScopeProbe
  def self.seed
    Current.mode = :none
    Current.user = 47
    Current.account = "Z"
  end
  def self.expression
    Current.set(user: 83) { 31 }
  end
  def self.argument
    [Current.set(user: 83) { Current.user }, Current.user]
  end
  def self.body
    seen = 0
    Current.set(user: 83) { seen = Current.user }
    seen
  end
  def self.shadow
    context = 7
    result = Current.set(user: 83) { |context| context.user }
    [result, context]
  end
  def self.two_params
    extra = "outer"
    result = Current.set(user: 83) { |context, extra| [context.user, extra] }
    [result, extra]
  end
  def self.next_value
    Current.set(user: 83) { next 41; raise "unreachable" }
  end
  def self.break_value
    Current.set(user: 83) { break 59; raise "unreachable" }
  end
  def self.method_return
    Current.set(user: 83) { return [31, Current.user] }
    raise "unreachable"
  end
  def self.nested
    seen = [Current.user]
    Current.set(user: 83) do
      seen << Current.user
      Current.set(user: 109) { seen << Current.user }
      seen << Current.user
    end
    seen
  end
  def self.nested_exit
    Current.set(user: 83) do
      Current.set(user: 109) { break 67 }
      Current.user
    end
  end
  def self.nested_each
    seen = []
    result = Current.set(user: 83) do
      [2, 5].each { |n| seen << n + Current.user }
      seen
    end
    [result, Current.user]
  end
  def self.nil_value
    Current.set(user: nil) { |context| context.user }
  end
  def self.identity
    Current.set(user: 83) { |context| context.equal?(Current.instance) }
  end
  def self.downstream
    Current.set(user: 83) { 29 }.positive?
  end
  def self.ordered
    Current.set(user: first_rhs, account: second_rhs) { |context| [context.user, context.account] }
  end
  def self.first_rhs
    Current::TRACE << "first"
    83
  end
  def self.second_rhs
    Current::TRACE << "second"
    "C"
  end
  def self.rhs_failure
    Current.set(user: first_rhs, account: raising_rhs) { raise "body" }
  end
  def self.raising_rhs
    Current::TRACE << "rhs-error"
    raise "rhs"
  end
  def self.writer_effect
    Current.set(user: 83, account: "C") { |context| [context.user, context.account] }
  end
  def self.setup_failure
    Current.set(user: 83, account: "bad") { raise "body" }
  end
  def self.reader_failure
    Current.set(user: 83, account: "C") { raise "body" }
  end
  def self.restore_failure
    Current.set(user: 83, account: "C") { 31 }
  end
  def self.restore_order
    Current.set(user: 47, account: "C") { 31 }
  end
  def self.body_failure
    Current.set(user: 83) { raise "body" }
  end
end
"#;

const ASSERTIONS: &str = r##"def check(name, expected, actual)
  raise "#{name}: expected #{expected.inspect}, got #{actual.inspect}" unless expected == actual
end
def fresh
  ScopeProbe.seed
  Current::TRACE.clear
end
def error_from
  yield
  "did not raise"
rescue RuntimeError => error
  error.message
end
fresh; check("expression", [31, 47], [ScopeProbe.expression, Current.user])
fresh; check("argument", [83, 47], ScopeProbe.argument)
fresh; check("body", [83, 47], [ScopeProbe.body, Current.user])
fresh; check("shadow", [[83, 7], 47], [ScopeProbe.shadow, Current.user])
fresh; check("extra parameter", [[[83, nil], "outer"], 47], [ScopeProbe.two_params, Current.user])
fresh; check("next", [41, 47], [ScopeProbe.next_value, Current.user])
fresh; check("break", [59, 47], [ScopeProbe.break_value, Current.user])
fresh; check("method return", [[31, 83], 47], [ScopeProbe.method_return, Current.user])
fresh; check("nested", [[47, 83, 109, 83], 47], [ScopeProbe.nested, Current.user])
fresh; check("nested break", [83, 47], [ScopeProbe.nested_exit, Current.user])
fresh; check("nested each", [[[85, 88], 47], 47], [ScopeProbe.nested_each, Current.user])
fresh; check("nil", [nil, 47], [ScopeProbe.nil_value, Current.user])
fresh; check("instance identity", [true, 47], [ScopeProbe.identity, Current.user])
fresh; check("downstream", true, ScopeProbe.downstream)
fresh; check("arguments in order", [[83, "C"], ["first", "second", "user", "account", "user", "account"]], [ScopeProbe.ordered, Current::TRACE.dup])
fresh; check("body raise", ["body", 47], [error_from { ScopeProbe.body_failure }, Current.user])
fresh; check("argument raise", ["rhs", 47, "Z", ["first", "rhs-error"]], [error_from { ScopeProbe.rhs_failure }, Current.user, Current.account, Current::TRACE.dup])
fresh; Current.mode = :side
check("writer side effect", [[83, "C"], 47, "side"], [ScopeProbe.writer_effect, Current.user, Current.account])
fresh; Current.mode = :side
check("writer raise", ["setup", 47, "side", ["user", "account", "account", "user", "account"]], [error_from { ScopeProbe.setup_failure }, Current.user, Current.account, Current::TRACE.dup])
fresh; Current.mode = :read
check("reader raise", ["read", 47, "side", ["user", "account", "user"]], [error_from { ScopeProbe.reader_failure }, Current.user, Current.account, Current::TRACE.dup])
fresh; Current.mode = :restore
check("a restore that raises stops the rest", ["restore", 47, "C", ["user", "account", "user"]], [error_from { ScopeProbe.restore_failure }, Current.user, Current.account, Current::TRACE.dup])
fresh; Current.mode = :side; Current.user = 83; Current.account = "Z"
check("restores in order", [31, 83, "Z"], [ScopeProbe.restore_order, Current.user, Current.account])
puts "Current.set: 22 checks passed"
"##;

#[test]
fn current_set_runs_its_block_and_restores_on_ruby() {
    let run = emit_and_run::real_blog()
        .write("app/models/current.rb", CURRENT)
        .write("app/models/scope_probe.rb", PROBE)
        .run_ruby(ASSERTIONS);
    run.assert_passes();
    assert_eq!(run.stdout.trim(), "Current.set: 22 checks passed");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn current_set_runs_its_block_and_restores_on_spinel() {
    let run = emit_and_run::real_blog()
        .write("app/models/current.rb", CURRENT)
        .write("app/models/scope_probe.rb", PROBE)
        .run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("Current.set: 22 checks passed"), "{}", run.stdout);
}
