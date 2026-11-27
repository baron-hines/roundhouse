//! Net's exception classes are ordinary values on Ruby and native Spinel.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;
use std::process::Command;

const CLASSES: &[(&str, bool)] = &[
    ("Net::HTTPBadResponse", false), ("Net::HTTPHeaderSyntaxError", false),
    ("Net::HTTPError", true), ("Net::HTTPRetriableError", true),
    ("Net::HTTPClientException", true), ("Net::HTTPFatalError", true),
    ("Net::ProtocolError", false), ("Net::ProtoFatalError", false),
    ("Net::ProtoServerError", false), ("Net::ProtoRetriableError", false),
];
const OVERRIDE: &str = r#"require "net/http"
module HeaderText
  def message
    42
  end
end
class Net::HTTPHeaderSyntaxError
  include HeaderText
end
class HeaderProbe
  def self.value
    Net::HTTPHeaderSyntaxError.new("header").message
  end
end
"#;

fn source() -> String {
    let mut source = String::from("require \"net/http\"\nclass HttpExceptionBoundary\n");
    // Keep class values out of containers: each constant, copy, raise,
    // rescue list and is_a? is exercised independently of Array inference.
    for (index, (name, response)) in CLASSES.iter().enumerate() {
        let args = if *response { "\"message\", nil" } else { "\"message\"" };
        source.push_str(&format!(r#"
  ERROR_{index} = {name}
  def self.error_{index}
    klass = ERROR_{index}
    raise {name}.new({args})
  rescue IOError, ::{name} => error
    klass == ::{name} && error.is_a?(klass) && error.message == "message"
  end
"#));
    }
    source.push_str(r#"
  WRITE_TIMEOUT = ::Net::WriteTimeout
  INFORMATIONAL = ::Net::HTTPInformation

  def self.write_timeout_rescue
    raise Net::WriteTimeout
  rescue Net::ReadTimeout
    false
  rescue ::Net::WriteTimeout => error
    error.is_a?(WRITE_TIMEOUT) && error.is_a?(Timeout::Error) && error.message.start_with?("Net::WriteTimeout")
  end

  def self.write_timeout_parent_rescue
    klass = WRITE_TIMEOUT
    raise klass
  rescue Timeout::Error => error
    klass == Net::WriteTimeout && error.is_a?(Net::WriteTimeout) && error.message.start_with?("Net::WriteTimeout")
  end

  def self.informational_response_class
    klass = INFORMATIONAL
    klass == Net::HTTPInformation && klass.superclass == Net::HTTPResponse && klass != Net::HTTPSuccess
  end

  def self.raise_alias
    raise ERROR_0, "alias"
  rescue IOError, Net::HTTPBadResponse => error
    error.is_a?(ERROR_0) && error.message == "alias"
  end

  def self.ancestor_rescue
    raise Net::HTTPFatalError.new("fatal", nil)
  rescue Net::ProtoFatalError => error
    error.is_a?(Net::ProtocolError) && error.message == "fatal"
  end
end
"#);
    source
}

fn assertions() -> String {
    let mut assertions = String::new();
    for index in 0..CLASSES.len() {
        assertions.push_str(&format!("raise \"exception {index}\" unless HttpExceptionBoundary.error_{index}\n"));
    }
    assertions.push_str("raise \"alias\" unless HttpExceptionBoundary.raise_alias\n\
        raise \"ancestor\" unless HttpExceptionBoundary.ancestor_rescue\n\
        raise \"write timeout\" unless HttpExceptionBoundary.write_timeout_rescue\n\
        raise \"write timeout parent\" unless HttpExceptionBoundary.write_timeout_parent_rescue\n\
        raise \"informational response\" unless HttpExceptionBoundary.informational_response_class\n\
        puts \"Net HTTP exception values OK\"\n");
    assertions
}

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::empty_app().write("app/models/http_exception_boundary.rb", &source())
}

#[test]
fn net_http_exception_values_pass_strict_emission() {
    let (_, errors) = overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn net_http_exception_values_execute_after_ruby_emission() {
    overlay().run_ruby(&assertions()).assert_passes();
}

#[test]
fn net_http_exception_values_match_cruby() {
    let output = Command::new("ruby").args(["-e", &(source() + &assertions())])
        .output().expect("run CRuby oracle");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "Net HTTP exception values OK");
}

#[test]
fn net_http_exception_values_remain_unavailable_on_crystal() {
    let (_, errors) = overlay().emit(BuildTarget::Crystal);
    for (name, _) in CLASSES {
        assert!(errors.iter().any(|e| e.contains(&format!("{name} is not available as a class/module value on crystal"))),
            "{name}: {errors:#?}");
    }
    for name in ["Net::WriteTimeout", "Net::HTTPInformation"] {
        assert!(errors.iter().any(|e| e.contains(&format!("{name} is not available as a class/module value on crystal"))),
            "{name}: {errors:#?}");
    }
}

#[test]
fn unprovided_net_exception_still_fails_strict_emission() {
    let (_, errors) = emit_and_run::empty_app().write("app/models/unprovided.rb",
        "class Unprovided\n  def self.value\n    Net::UnprovidedException\n  end\nend\n")
        .emit(BuildTarget::Spinel);
    assert!(!errors.is_empty(), "unprovided Net constant accepted");
}

fn override_overlay() -> emit_and_run::Overlay {
    emit_and_run::empty_app().write("app/models/header_probe.rb", OVERRIDE)
}

#[test]
fn included_exception_message_keeps_its_own_return_type() {
    override_overlay().run_ruby("raise \"included override\" unless HeaderProbe.value == 42").assert_passes();
    let (tree, errors) = override_overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let sidecar = std::fs::read_to_string(tree.join("app/models/header_probe.rbs")).unwrap();
    assert!(sidecar.contains("def self.value: () -> Integer"), "{sidecar}");
}

fn run_native(target: BuildTarget) {
    let sidecars = if matches!(target, BuildTarget::Ruby) { "sig/app/models" } else { "app/models" };
    let (tree, errors) = overlay().emit(target);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(tree.join(sidecars).join("http_exception_boundary.rbs").is_file(), "missing generated sidecar");
    compile_and_run(&tree, sidecars, &format!(
        "require_relative \"app/models/http_exception_boundary\"\n{}", assertions()), "Net HTTP exception values OK");
}

fn compile_and_run(tree: &emit_and_run::Emitted, sidecars: &str, script: &str, expected: &str) {
    std::fs::write(tree.join("contract.rb"), format!(
        "{script}")).unwrap();
    eprintln!("native fixture: {}", tree.display());
    let compiler = std::env::var("SPINEL").unwrap_or_else(|_| "spinel".into());
    let build = Command::new(compiler).args(["--rbs", sidecars, "contract.rb", "-o", "contract"])
        .current_dir(&tree).output().expect("compile emitted program");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let run = Command::new(tree.join("contract")).current_dir(&tree).output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), expected);
}

#[test]
#[ignore = "requires Spinel's native Net exception classes (da4485a58 or later)"]
fn net_http_exception_values_native_through_ruby_emission() {
    run_native(BuildTarget::Ruby);
}

#[test]
#[ignore = "requires Spinel's native Net exception classes (da4485a58 or later)"]
fn net_http_exception_values_native_through_strict_emission() {
    run_native(BuildTarget::Spinel);
}

#[test]
#[ignore = "requires Spinel's native Net exception classes (da4485a58 or later)"]
fn included_exception_message_runs_natively_with_sidecars() {
    let (tree, errors) = override_overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    compile_and_run(&tree, "app/models", "require \"net/http\"\n\
        require_relative \"app/models/header_text\"\n\
        require_relative \"app/models/net/httpheader_syntax_error\"\n\
        require_relative \"app/models/header_probe\"\n\
        raise \"included override\" unless HeaderProbe.value == 42\n\
        puts \"Net HTTP source override OK\"\n", "Net HTTP source override OK");
}
