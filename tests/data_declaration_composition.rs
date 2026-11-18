#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/data_declaration_composition.rs"]
mod contract;
#[path = "support/prerequisite.rs"]
mod prerequisite;

fn run_case(index: usize) {
    let (name, source, assertions, expected) = contract::CASES[index];
    let (tree, errors) = contract::app(source).emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{name}: {errors:?}");
    std::fs::write(tree.join("driver.rb"), format!("require_relative 'app/models'\n{assertions}")).unwrap();
    let output = std::process::Command::new("ruby").arg("driver.rb").current_dir(&tree).output().unwrap();
    assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{name}");
}
#[test] fn qualified_constants() { run_case(0); }
#[test] fn reopening_constants_and_methods() { run_case(1); }
#[test] fn reopening_before_and_after_first_use() { run_case(2); }
#[test] fn private_class_method_constructors() { run_case(3); }
#[test] fn singleton_private_constructors() { run_case(4); }
#[test] fn nested_delegate_and_method() { run_case(5); }
#[test] fn prefixed_nil_delegate_and_method() { run_case(6); }
#[test] fn visibility_and_override_neighbors() {
    for index in 7..9 { run_case(index); }
}
#[test] fn nil_delegation_preserves_rails_errors() { run_case(9); }
#[test] fn literal_initializer_spellings_and_prior_constants() { run_case(10); }

#[test]
fn source_no_method_errors_keep_a_named_native_refusal() {
    let source = contract::CASES[9].1.replace("def code = \"entry\"", "def code; raise NoMethodError, \"inside entry\"; end");
    let (_, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.iter().any(|error| error.contains("Data delegation to source methods raising NoMethodError")), "{errors:?}");
}

#[test]
fn source_nil_queries_keep_a_named_refusal() {
    for query in ["nil?", "respond_to?"] {
        let source = contract::CASES[9].1.replace("def code = \"entry\"", &format!("def code = \"entry\"\n  def {query}(*); false; end"));
        let (_, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.iter().any(|error| error.contains("Data delegation with source nil? or respond_to? overrides")), "{errors:?}");
    }
}

#[test]
fn unmodeled_delegates_still_refuse_emission() {
    for options in ["prefix: Object.new", "allow_nil: Object.new", "unknown: true", "**{}"] {
        let source = contract::CASES[5].1.replace("to: :entry", &format!("to: :entry, {options}"));
        let (_, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(!errors.is_empty(), "unread delegate options silently disappeared: {options}");
    }
}

#[test]
fn private_constant_remains_refused_until_native_privacy_matches_ruby() {
    let source = contract::CASES[1].1.replace("  COPY = ORIGIN", "  COPY = ORIGIN\n  private_constant :ORIGIN");
    let (_, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
    assert!(!errors.is_empty(), "the native compiler exposes this private constant");
}

#[test]
fn qualified_writes_do_not_change_the_rhs_self() {
    for value in ["new(0, 0)", "self", "@origin", "(@origin = 1)"] {
        let source = format!("Point = Data.define(:x, :y)\nPoint::ORIGIN = {value}\n");
        let (_, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(!errors.is_empty(), "file-level self was moved into the Data class: {value}");
    }
}

#[test]
fn cross_file_reopenings_are_refused_in_both_orders() {
    for (factory, reopening) in [("lib/a.rb", "lib/z.rb"), ("lib/z.rb", "lib/a.rb")] {
        let (_, errors) = emit_and_run::empty_app()
            .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
            .write("db/schema.rb", "")
            .write("config/database.yml", "test:\n  adapter: sqlite3\n  database: storage/test.sqlite3\n")
            .write(factory, "Point = Data.define(:x)\n")
            .write(reopening, "class Point; ORIGIN = new(0); def norm = x; end\n")
            .emit(roundhouse::project::BuildTarget::Spinel);
        assert!(!errors.is_empty(), "cross-file load order was guessed: {factory}");
    }
}

#[test]
fn synthetic_repros_match_an_initialized_rails_oracle() {
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/rails_oracle.rb");
    for (name, source, assertions, expected) in contract::CASES.iter().copied().chain(
        contract::REVIEW_CASES.iter().chain(contract::LITERAL_CASES).filter(|case| case.3 == "match").map(|case| (case.0, case.1, case.2, case.4))) {
        let script = format!(r#"require {helper:?}
unless RailsOracle.available?("railties", "activesupport")
  warn "skipping Data composition oracle: #{{RailsOracle.label}} needs railties and activesupport"
  exit 77
end
RailsOracle.activate("railties", "activesupport")
require "rails"
require "active_support/core_ext/module/delegation"
require "active_support/core_ext/numeric/bytes"
require "active_support/core_ext/string/inquiry"
class SyntheticDataOracle < Rails::Application
  config.load_defaults 8.1
  config.eager_load = false
  config.logger = Logger.new(File::NULL)
  config.secret_key_base = "synthetic-data-oracle"
end
SyntheticDataOracle.initialize!
{source}
{assertions}
"#);
        let output = std::process::Command::new("ruby").args(["-e", &script]).output().unwrap();
        if output.status.code() == Some(77) {
            prerequisite::missing(String::from_utf8_lossy(&output.stderr).trim());
            return;
        }
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{name}");
    }
}

fn check_review_case(index: usize) {
    let (name, source, observe, kind, expected) = contract::REVIEW_CASES[index];
    let (tree, errors) = contract::app(source).emit(roundhouse::project::BuildTarget::Spinel);
    if kind == "refuse" {
        assert!(errors.iter().any(|error| error.contains(expected)), "{name}: expected named refusal {expected}; got {errors:?}");
    } else {
        assert!(errors.is_empty(), "{name}: {errors:?}");
        std::fs::write(tree.join("driver.rb"), format!("require_relative 'app/models'\n{observe}")).unwrap();
        let output = std::process::Command::new("ruby").arg("driver.rb").current_dir(&tree).output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{name}");
    }
}
#[test] fn boundary_qualified_overwrite() { check_review_case(0); }
#[test] fn boundary_qualified_cycle() { check_review_case(1); }
#[test] fn boundary_qualified_reopening_scope() { check_review_case(2); }
#[test] fn boundary_private_subclass_alias() { check_review_case(3); }
#[test] fn boundary_delegate_macro_override() { check_review_case(4); }
#[test] fn boundary_delegate_constant_target() { check_review_case(5); }
#[test] fn boundary_delegate_nil_denied() { check_review_case(6); }
#[test] fn boundary_delegate_positional_hash() { check_review_case(7); }
#[test] fn boundary_constant_member_override() { check_review_case(8); }
#[test] fn boundary_inherited_data_method_after_use() { check_review_case(9); }
#[test] fn boundary_qualified_expression_bytes() { check_review_case(10); }
#[test] fn boundary_qualified_expression_inquiry() { check_review_case(11); }
#[test] fn boundary_qualified_dependency_cycle() { check_review_case(12); }

fn check_narrow_case(index: usize) {
    let (name, source, _, _, message) = contract::NARROW_CASES[index];
    let (_, errors) = contract::app(source).emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.iter().any(|error| error.contains(message)), "{name}: expected named refusal {message}; got {errors:?}");
}
#[test] fn narrowing_group_private_alias_return() { check_narrow_case(0); }
#[test] fn narrowing_inherited_instance_dup() { check_narrow_case(1); }
#[test] fn narrowing_inherited_instance_clone() { check_narrow_case(2); }
#[test] fn narrowing_app_private_constructor_macro() { check_narrow_case(3); }
#[test] fn narrowing_inherited_class_members() { check_narrow_case(4); }
#[test] fn narrowing_members_without_override() { check_narrow_case(5); }

#[test]
fn initializers_do_not_call_source_methods_or_overridden_constructors() {
    for source in [
        "Point = Data.define(:x) do; def read = dup; end\nclass Point; FIRST = new(1).read; def dup = with(x: 9); end",
        "Point = Data.define(:x) do; def read(value = dup) = value; end\nclass Point; FIRST = new(1).read; def dup = with(x: 9); end",
        "Point = Data.define(:x) do; def read = x; end\nclass Point; FIRST = new(1).read; def read = 9; end",
        "Point = Data.define(:x)\nclass Point; FIRST = new(1); def initialize(x); super(x: 9); end; end",
    ] {
        let (_, errors) = contract::app(source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.iter().any(|error| error.contains("Data constant initializer is not a literal or a constructor call")), "{source}: {errors:?}");
    }
}

#[test]
fn new_constructor_visibility_requires_literal_symbols_and_the_builtin_owner() {
    for marker in ["private_class_method 'new', :[], :allocate", "private_class_method *[:new, :[], :allocate]", "self.private_class_method :new, :[], :allocate"] {
        let source = format!("Token = Data.define(:value) do\n def self.issue(v) = new(value: v)\n {marker}\nend");
        let (_, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.iter().any(|error| error.contains("Data visibility macro")), "{source}: {errors:?}");
    }
}

#[test]
fn data_reflection_is_refused_through_helpers_and_explicit_receivers() {
    for value in ["members", "self.members", "Point.members", "send(:members)", "facts"] {
        let source = format!("Point = Data.define(:x) do; def self.facts = members; end\nclass Point; FIRST = {value}; end");
        let (_, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.iter().any(|error| error.contains("Data constant initializer is not a literal or a constructor call")), "{source}: {errors:?}");
    }
}

fn check_literal_case(index: usize) {
    let (name, source, observe, kind, expected) = contract::LITERAL_CASES[index];
    let (tree, errors) = contract::app(source).emit(roundhouse::project::BuildTarget::Spinel);
    if kind == "refuse" {
        assert!(errors.iter().any(|error| error.contains(expected)), "{name}: {errors:?}");
    } else {
        assert!(errors.is_empty(), "{name}: {errors:?}");
        std::fs::write(tree.join("driver.rb"), format!("require_relative 'app/models'\n{observe}")).unwrap();
        let output = std::process::Command::new("ruby").arg("driver.rb").current_dir(&tree).output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{name}");
    }
}
#[test] fn literals_narrow_external_copy() { check_literal_case(0); }
#[test] fn literals_narrow_interpolated_to_s() { check_literal_case(1); }
#[test] fn literals_narrow_case_equality() { check_literal_case(2); }
#[test] fn literals_unrelated_nil_query() { check_literal_case(3); }
#[test] fn literals_unrelated_raising_method() { check_literal_case(4); }

#[test]
fn initializer_allow_list_rejects_expressions_even_without_later_overrides() {
    for value in ["new(1).x", "Copier.copy(new(1))", "\"#{new(1)}\"", "case 1; when 1; 2; end",
        "if true; 1; end", "[1].map { |n| n }", "members", "1 + new(1).x", "[new(1)]", "{x: new(1)}", "LATER", "self.[](1)", "self[1]"] {
        let source = format!("Point = Data.define(:x)\nclass Point; VALUE = {value}; LATER = 1; end");
        let (_, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.iter().any(|error| error.contains("Data constant initializer is not a literal or a constructor call")), "{value}: {errors:?}");
    }
}

#[test]
fn known_delegate_targets_still_refuse_their_own_and_inherited_overrides() {
    for definition in ["def nil? = false", "def respond_to?(*args) = false", "def code; raise NoMethodError, 'target'; end"] {
        for parent in [false, true] {
            let source = if parent {
                contract::LITERAL_CASES[3].1.replace("class Entry", &format!("class Parent\n {definition}\nend\nclass Entry < Parent"))
            } else {
                contract::LITERAL_CASES[3].1.replace("def code = \"abc\"", definition)
            };
            let (_, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
            assert!(errors.iter().any(|error| error.contains("Data delegation")), "{definition}, inherited={parent}: {errors:?}");
        }
    }
}

#[test] fn base_data_visibility_forms_survive_with_a_mixin() { run_case(11); }

#[test]
fn base_data_visibility_wrappers_and_named_strings_keep_their_meaning() {
    for body in [
        "private def secret = value\npublic def label = secret",
        "protected def secret = value\npublic def label = secret",
        "def secret = value\nprivate 'secret'\ndef label = secret",
        "def secret = value\nprotected 'secret'\ndef label = secret",
        "private\ndef label = value\npublic 'label'",
        "private_class_method def self.build(v) = new(v)\ndef self.issue(v) = build(v)\npublic_class_method 'issue'\ndef label = value",
        "class << self\nprivate\ndef build(v) = new(v)\npublic\ndef issue(v) = build(v)\nend\ndef label = value",
    ] {
        let source = format!("module Helpers; def stamp = 'helper'; end\nRecord = Data.define(:value) do\ninclude Helpers\n{body}\nend");
        let (tree, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.is_empty(), "{body}: {errors:?}");
        std::fs::write(tree.join("driver.rb"), "require_relative 'app/models'\nputs Record.new('ok').label\n").unwrap();
        let output = std::process::Command::new("ruby").arg("driver.rb").current_dir(&tree).output().unwrap();
        assert!(output.status.success(), "{body}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\n");
    }
}

fn assert_query_refusal(source: &str) {
    let (_, errors) = contract::app(source).emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.iter().any(|error| error.contains("Data delegation with source nil? or respond_to? overrides")), "{source}: {errors:?}");
}

#[test]
fn a_reopened_targets_query_override_stays_refused() {
    let source = contract::LITERAL_CASES[3].1.replace("class Unrelated\n def nil? = false\nend", "module QueryOverride\n  def nil? = true\nend\nclass Entry\n  include QueryOverride\nend");
    assert!(source.contains("include QueryOverride"));
    assert_query_refusal(&source);
}

#[test]
fn a_targets_delegated_query_override_stays_refused() {
    let source = contract::LITERAL_CASES[3].1.replace("class Unrelated\n def nil? = false\nend", "class Entry\n  def probe = nil\n  delegate :nil?, to: :probe\nend");
    assert_query_refusal(&source);
    assert!(source.contains("delegate :nil?"));
    let prefixed = source.replace("delegate :nil?, to: :probe", "delegate :nil?, to: :probe, prefix: true");
    let (_, errors) = contract::app(&prefixed).emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "a prefixed method does not override nil?: {errors:?}");
}

#[test]
fn app_visibility_macros_are_refused_before_the_builtin_skip() {
    for marker in ["private", "public", "protected"] {
        let source = format!("Token = Data.define(:value) do\n  def self.{marker} = nil\n  {marker}\n  def label = value\nend\n");
        let (_, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.iter().any(|error| error.contains(&format!("Data visibility macro `{marker}`"))), "{source}: {errors:?}");
    }
}

#[test]
fn explicit_and_missing_delegation_stays_refused() {
    for calls in [
        "delegate :code, to: :entry\n    delegate_missing_to :entry",
        "delegate_missing_to :entry\n    delegate :code, to: :entry",
    ] {
        for nested in [false, true] {
            let body = format!("View = Data.define(:entry) do\n    {calls}\n    def label = code\nend\n");
            let source = format!("class Entry; def code = 'abc'; def detail = 'details'; end\n{}", if nested { format!("class Envelope\n{body}end\n") } else { body });
            let (_, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
            assert!(errors.iter().any(|error| error.contains("Data.define blocks combining delegate and delegate_missing_to are not modeled")), "{errors:?}");
        }
    }
}

#[test]
fn missing_delegation_in_a_data_reopening_stays_refused() {
    for source in [
        "class Entry; def code = 'abc'; end\nclass Envelope\n View = Data.define(:entry)\n class View\n  delegate_missing_to :entry\n  def label = code\n end\nend\n",
        "class Entry; def code = 'abc'; end\nView = Data.define(:entry)\nclass View\n  delegate_missing_to :entry\n  def label = code\nend\n",
        "class Entry; def code = 'abc'; end\nView = Data.define(:entry) do\n  delegate :code, to: :entry\n  def label = code\nend\nclass View\n  delegate_missing_to :entry\nend\n",
    ] {
        let (_, errors) = contract::app(source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.iter().any(|error| error.contains("delegate_missing_to in a Data reopening is not modeled") || error.contains("Data.define beside another declaration of the same class")
            || error.contains("Data.define reopening with load-time statements")), "{errors:?}");
    }
}

#[test]
fn replayed_visibility_keeps_the_data_method_it_observes() {
    for marker in ["private_class_method", "public_class_method", "private", "protected", "public"] {
        let receiver = if marker.ends_with("_class_method") { "self." } else { "" };
        let source = format!("Token = Data.define(:value) do\n  def {receiver}label = 1\nend\nclass Token\n  {marker} :label\nend\nclass Token\n  def {receiver}label = 2\nend\n");
        let (tree, errors) = contract::app(&source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.is_empty(), "{marker}: {errors:?}");
        let target = if receiver.is_empty() { "Token.new(0)" } else { "Token" };
        std::fs::write(tree.join("driver.rb"), format!("require_relative 'app/models'\nobject = {target}\np [object.send(:label), object.respond_to?(:label)]\n")).unwrap();
        let output = std::process::Command::new("ruby").arg("driver.rb").current_dir(&tree).output().unwrap();
        assert!(output.status.success(), "{marker}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "[2, true]\n", "{marker}");
    }
}

#[test]
fn a_reassigned_data_factory_constant_stays_refused() {
    let source = "A = Data.define(:x)\nA = 7\nB = Data.define(:x)\nB::VALUE = A\n";
    let (_, errors) = contract::app(source).emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.iter().any(|error| error.contains("reassignment of Data.define constant")), "{errors:?}");
}

#[test]
fn data_factory_reassignment_neighbors_keep_their_boundaries() {
    let (_, errors) = contract::app("A = Data.define(:x)\n::A = 7\n").emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.iter().any(|error| error.contains("reassignment of Data.define constant")), "{errors:?}");
    let (_, errors) = contract::app("A = Data.define(:x)\nA ||= 7\n").emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.iter().any(|error| error.contains("file-level statements beside Data.define")), "{errors:?}");
    for (source, observe, expected) in [
        ("A = 7\nA = Data.define(:x)\n", "p A.new(2).x\n", "2\n"),
        ("module Outer\n A = Data.define(:x)\n A = 7\nend\n", "p Outer::A\n", "7\n"),
        ("module Outer\n A = Data.define(:x)\n A ||= 7\nend\n", "p Outer::A.members\n", "[:x]\n"),
    ] {
        let (tree, errors) = contract::app(source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.is_empty(), "{source}: {errors:?}");
        std::fs::write(tree.join("driver.rb"), format!("require_relative 'app/models'\n{observe}")).unwrap();
        let output = std::process::Command::new("ruby").arg("driver.rb").current_dir(&tree).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{source}");
    }
}
