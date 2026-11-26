//! Encryption declarations must not disappear into plaintext persistence.
#[path = "support/scratch.rs"]
mod scratch;

use std::path::PathBuf;
use std::process::Command;

struct App(PathBuf);

impl App {
    fn new(body: &str) -> Self {
        let app = Self(scratch::dir("roundhouse-encrypts"));
        app.write("db/schema.rb", "ActiveRecord::Schema[8.1].define do\n  create_table :credentials do |t|\n    t.string :token\n    t.string :secret\n  end\nend\n");
        app.write(
            "config/routes.rb",
            "Rails.application.routes.draw do\nend\n",
        );
        app.write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n",
        );
        app.write(
            "app/models/credential.rb",
            &format!("class Credential < ApplicationRecord\n{body}\nend\n"),
        );
        app
    }

    fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }

    fn emit(&self, target: &str, permissive: bool) -> (bool, String, PathBuf) {
        let out = self
            .0
            .join(if permissive { "permissive" } else { "strict" })
            .join(target);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_roundhouse"));
        cmd.args(["--target", target])
            .arg(&self.0)
            .arg("-o")
            .arg(&out);
        if permissive {
            cmd.arg("--allow-unsupported");
        }
        let result = cmd.output().expect("run roundhouse");
        (
            result.status.success(),
            String::from_utf8_lossy(&result.stderr).into_owned(),
            out,
        )
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn encryption_errors(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .filter(|line| line.contains("error[unsupported]: encrypts "))
        .collect()
}

fn assert_refused(app: &App, source: &str, line: usize) {
    let (ok, err, out) = app.emit("spinel", false);
    assert!(!ok, "strict emission succeeded: {err}");
    let errors = encryption_errors(&err);
    assert_eq!(errors.len(), 1, "{err}");
    assert!(
        errors[0].contains(&format!("{source}:{line}:3: error[unsupported]")),
        "{err}"
    );
    assert!(
        errors[0].contains("is not modelled, so its effect would be dropped from the output"),
        "{err}"
    );
    assert!(
        !out.exists(),
        "refused output was written to {}",
        out.display()
    );
}

#[test]
fn strict_refuses_each_argument_shape_at_the_declaration() {
    for declaration in [
        "encrypts :token",
        "encrypts :token, :secret",
        "encrypts \"token\"",
        "encrypts :token, deterministic: true, downcase: true",
        "encrypts :token, support_unencrypted_data: true, previous: [{ deterministic: true }]",
        "encrypts :token, **{ deterministic: true }",
        "encrypts(*[:token, :secret], deterministic: true)",
        "encrypts(\n    :token,\n    deterministic: true\n  )",
    ] {
        let app = App::new(&format!("  {declaration}"));
        assert_refused(&app, "app/models/credential.rb", 2);
    }
}

#[test]
fn distinct_declarations_have_distinct_source_errors() {
    let app = App::new("  encrypts :token\n  encrypts :secret, deterministic: true");
    let (ok, err, out) = app.emit("spinel", false);
    assert!(!ok, "{err}");
    assert_eq!(encryption_errors(&err).len(), 2, "{err}");
    for line in [2, 3] {
        assert!(
            err.contains(&format!(
                "credential.rb:{line}:3: error[unsupported]: encrypts"
            )),
            "{err}"
        );
    }
    assert!(!out.exists());
}

#[test]
fn permissive_emission_keeps_the_warning_and_writes_the_model() {
    let app = App::new("  encrypts :token, :secret, deterministic: true");
    let (ok, err, out) = app.emit("spinel", true);
    assert!(ok, "{err}");
    assert!(encryption_errors(&err).is_empty(), "{err}");
    assert_eq!(
        err.lines()
            .filter(|l| l.contains("warning[unsupported]: encrypts "))
            .count(),
        1,
        "{err}"
    );
    let model = std::fs::read_to_string(out.join("app/models/credential.rb")).unwrap();
    assert!(model.contains("def token="), "{model}");
}

#[test]
fn inherited_encryption_refuses_at_its_original_declaration() {
    let app = App::new("");
    app.write("app/models/encrypted_record.rb", "class EncryptedRecord < ApplicationRecord\n  self.abstract_class = true\n  self.table_name = \"credentials\"\n  encrypts :token\nend\n");
    app.write(
        "app/models/credential.rb",
        "class Credential < EncryptedRecord\nend\n",
    );
    assert_refused(&app, "app/models/encrypted_record.rb", 4);
}

#[test]
fn included_concern_encryption_refuses_at_its_original_declaration() {
    let app = App::new("  include TokenEncryption");
    app.write("app/models/concerns/token_encryption.rb", "module TokenEncryption\n  extend ActiveSupport::Concern\n  included do\n    encrypts :token\n  end\nend\n");
    let (ok, err, out) = app.emit("spinel", false);
    assert!(!ok, "{err}");
    let errors = encryption_errors(&err);
    assert_eq!(errors.len(), 1, "{err}");
    assert!(
        errors[0].contains("app/models/concerns/token_encryption.rb:4:5: error[unsupported]"),
        "{err}"
    );
    assert!(!out.exists());
}

#[test]
fn a_shared_concern_is_reported_once_per_declaration() {
    let app = App::new("  include TokenEncryption");
    app.write("app/models/other_credential.rb", "class OtherCredential < ApplicationRecord\n  self.table_name = \"credentials\"\n  include TokenEncryption\nend\n");
    app.write("app/models/concerns/token_encryption.rb", "module TokenEncryption\n  extend ActiveSupport::Concern\n  included do\n    encrypts :token\n  end\nend\n");
    let (ok, err, _) = app.emit("spinel", false);
    assert!(!ok, "{err}");
    assert_eq!(encryption_errors(&err).len(), 1, "{err}");
    assert!(
        err.contains("token_encryption.rb:4:5: error[unsupported]: encrypts"),
        "{err}"
    );
}

#[test]
fn an_app_defined_encrypts_method_keeps_its_existing_admission() {
    let app = App::new("  def self.encrypts(*names)\n    names\n  end\n  encrypts :token");
    let (ok, err, out) = app.emit("spinel", false);
    assert!(ok && out.is_dir(), "{err}");
    assert!(encryption_errors(&err).is_empty(), "{err}");
}

#[test]
fn refusal_is_shared_by_every_lowered_target_and_plain_models_still_emit() {
    for target in roundhouse::project::BuildTarget::TRANSPILE {
        let target = target.as_str();
        if ["roda", "futamura"].contains(&target) {
            continue; // These source-preserving targets bypass model lowering.
        }
        let clean = App::new("");
        let (ok, err, out) = clean.emit(target, false);
        assert!(ok && out.is_dir(), "{target} plain model: {err}");
        let encrypted = App::new("  encrypts :token");
        let (ok, err, out) = encrypted.emit(target, false);
        assert!(!ok, "{target} silently emitted: {err}");
        assert_eq!(encryption_errors(&err).len(), 1, "{target}: {err}");
        assert!(!out.exists(), "{target}");
    }
}

#[test]
fn targets_that_bypass_lowering_keep_their_existing_behavior() {
    for target in ["roda", "futamura"] {
        let app = App::new("  encrypts :token");
        let (ok, err, out) = app.emit(target, false);
        assert!(ok && out.is_dir(), "{target}: {err}");
        assert!(encryption_errors(&err).is_empty(), "{target}: {err}");
        if target == "futamura" {
            assert!(
                std::fs::read_to_string(out.join("app/models/credential.rb"))
                    .unwrap()
                    .contains("encrypts :token")
            );
        }
    }
}
