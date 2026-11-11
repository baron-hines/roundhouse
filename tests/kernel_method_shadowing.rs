//! Receiver methods precede Kernel's private methods for a bare call.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const SOURCE: &str = r#"class Catalog
end
Catalog::Row = Data.define(:code, :memo) do
  def self.load(value)
    new(code: value, memo: nil)
  end
  def self.load_reference(value)
    CALLload(value)
  end
end
class Loader
  def self.load(value) = 23
  def load(value) = "instance"
  def self.reference = CALLload(nil)
  def reference = CALLload(nil)
  def self.format(value) = 17
  def self.formatted = CALLformat(nil)
  def print(value) = "printed"
  def printed = CALLprint(nil)
end
"#;

fn app(source: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app().write("app/services/catalog.rb", source)
}

fn rbs(source: &str, class: &str) -> String {
    let (tree, errors) = app(source).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::read_to_string(tree.join(format!("app/models/{class}.rbs"))).unwrap()
}

#[test]
fn bare_and_explicit_calls_keep_the_receivers_return_types() {
    for receiver in ["", "self."] {
        let source = SOURCE.replace("CALL", receiver);
        let row = rbs(&source, "catalog");
        assert!(
            row.contains("def self.load_reference: (untyped value) -> Catalog::Row"),
            "{row}"
        );
        let loader = rbs(&source, "loader");
        for signature in [
            "def self.reference: () -> Integer",
            "def reference: () -> String",
            "def self.formatted: () -> Integer",
            "def printed: () -> String",
        ] {
            assert!(loader.contains(signature), "{signature}\n{loader}");
        }
    }
}

#[test]
fn inherited_and_included_methods_precede_kernel() {
    let source = r#"module Printing
  def print(value) = 19
end
class Parent
  def self.load(value) = "parent"
  def format(value) = 29
end
class Child < Parent
  include Printing
  def self.reference = load(nil)
  def formatted = format(nil)
  def printed = print(nil)
end
"#;
    let signatures = rbs(source, "child");
    for signature in [
        "def self.reference: () -> String",
        "def formatted: () -> Integer",
        "def printed: () -> Integer",
    ] {
        assert!(signatures.contains(signature), "{signature}\n{signatures}");
    }
    let run = app(source).run_ruby("p [Child.reference, Child.new.formatted, Child.new.printed]\n");
    run.assert_passes();
    assert_eq!(run.stdout, "[\"parent\", 29, 19]\n");
}

#[test]
fn an_inherited_singleton_method_is_not_an_instance_method_on_the_child() {
    let source = r#"class Parent
  def self.load(value) = "singleton"
  def format(value) = 31
end
class Child < Parent
  def load(value) = 17
  def self.format(value) = "other side"
  def self.reference = CALLload(nil)
  def formatted = CALLformat(nil)
end
"#;
    for receiver in ["", "self."] {
        let source = source.replace("CALL", receiver);
        let signatures = rbs(&source, "child");
        for signature in [
            "def self.reference: () -> String",
            "def formatted: () -> Integer",
        ] {
            assert!(signatures.contains(signature), "{signature}\n{signatures}");
        }
        let run = app(&source).run_ruby("puts Child.reference\nputs Child.new.formatted\n");
        run.assert_passes();
        assert_eq!(run.stdout, "singleton\n31\n");
    }
}

#[test]
fn methods_on_the_other_side_do_not_shadow_kernel() {
    let source = r#"class KernelCalls
  def load(value) = "instance"
  def self.print(value) = 37
  def self.read_file(path) = load(path)
  def printed = print("")
  def self.formatted = format("%s", "kernel")
end
"#;
    let signatures = rbs(source, "kernel_calls");
    for signature in [
        "def self.read_file: (untyped path) -> bool",
        "def printed: () -> nil",
        "def self.formatted: () -> String",
    ] {
        assert!(signatures.contains(signature), "{signature}\n{signatures}");
    }
    let run = app(source).run_ruby("puts KernelCalls.formatted\np KernelCalls.new.printed\n");
    run.assert_passes();
    assert_eq!(run.stdout, "kernel\nnil\n");
    let builtin = rbs(
        "class BuiltinOnly\n  def self.read_file(path) = load(path)\nend\n",
        "builtin_only",
    );
    assert!(
        builtin.contains("def self.read_file: (untyped path) -> bool"),
        "{builtin}"
    );
}

const REPORT: &str = "puts Catalog::Row.load_reference(\"kept\").code\nputs Loader.reference\nputs Loader.new.reference\nputs Loader.formatted\nputs Loader.new.printed\n";

#[test]
fn emitted_calls_match_cruby() {
    for receiver in ["", "self."] {
        let source = SOURCE.replace("CALL", receiver);
        let oracle = emit_and_run::ruby()
            .arg("-e")
            .arg(format!("{source}\n{REPORT}"))
            .output()
            .unwrap();
        assert!(
            oracle.status.success(),
            "{}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&oracle.stdout),
            "kept\n23\ninstance\n17\nprinted\n"
        );
        let run = app(&source).run_ruby(REPORT);
        run.assert_passes();
        assert_eq!(run.stdout.as_bytes(), oracle.stdout);
    }
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn calls_compile_with_unmodified_signatures() {
    for receiver in ["", "self."] {
        let (tree, errors) = app(&SOURCE.replace("CALL", receiver)).emit(BuildTarget::Spinel);
        assert!(errors.is_empty(), "{errors:?}");
        std::fs::write(tree.join("contract.rb"), format!(
            "require_relative \"app/models/catalog\"\nrequire_relative \"app/models/loader\"\n{REPORT}"
        )).unwrap();
        let built =
            std::process::Command::new(std::env::var("SPINEL").unwrap_or_else(|_| "spinel".into()))
                .args([
                    "--strict-require",
                    "--rbs",
                    "app/models",
                    "contract.rb",
                    "-o",
                    "contract",
                ])
                .current_dir(&tree)
                .output()
                .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let run = std::process::Command::new(tree.join("contract"))
            .current_dir(&tree)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "kept\n23\ninstance\n17\nprinted\n"
        );
    }
}
