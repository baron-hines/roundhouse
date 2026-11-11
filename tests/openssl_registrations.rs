//! OpenSSL's native library surface must pass the strict constant boundary.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;
use std::process::Command;

const SOURCE: &str = include_str!("support/openssl_registrations.rb");
const OVERRIDE: &str = r#"require "openssl"
module OpenSSL
  module KDF
    def self.hkdf(value, salt:, info:, length:, hash:)
      length
    end
  end
end
class OpenSSL::PKey::EC
  def group(value)
    value
  end
end
"#;
const OVERRIDE_ASSERTIONS: &str = r#"
raise "source class override" unless OpenSSL::KDF.hkdf("key", salt: "", info: "", length: 7, hash: "SHA256") == 7
raise "source instance override" unless OpenSSL::PKey::EC.generate("prime256v1").group(9) == 9
puts "OpenSSL source overrides OK"
"#;
const SUBCLASSES: &str = r#"require "openssl"
class FactoryDigest < OpenSSL::Digest::SHA384
  def hexdigest(data)
    17
  end
end
class FactoryEC < OpenSSL::PKey::EC
  def group
    19
  end
end
class FactoryConsumer
  def self.digest
    FactoryDigest.new.hexdigest("message")
  end
  def self.group
    FactoryEC.generate("prime256v1").group
  end
end
"#;
const SUBCLASS_ASSERTIONS: &str = r#"
raise "Digest subclass" unless FactoryConsumer.digest == 17
raise "EC subclass" unless FactoryConsumer.group == 19
puts "OpenSSL factory subclasses OK"
"#;
const ERROR_ALIAS: &str = r#"require "openssl"
class OpenSSL::PKey::PKeyError
end
class OpenSSL::PKey::ECError
  def message
    42
  end
end
class AliasConsumer
  def self.value
    OpenSSL::PKey::PKeyError.new("error").message
  end
end
"#;
const ASSERTIONS: &str = r#"
raise "HKDF RFC 5869" unless CryptoBoundary.hkdf ==
  "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
raise "comparisons" unless CryptoBoundary.comparisons && CryptoBoundary.comparison_length_error
sha384 = "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
sha512 = "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a" +
  "2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
raise "SHA384" unless CryptoBoundary.sha384 == ["SHA384", sha384, sha384, sha384, sha384]
raise "SHA512" unless CryptoBoundary.sha512 == ["SHA512", sha512, sha512, sha512, sha512]
hmac384 = "30ddb9c8f347cffbfb44e519d814f074cf4047a55d6f563324f1c6a33920e5edfb2a34bac60bdc96cd33a95623d7d638"
hmac512 = "3926a207c8c42b0c41792cbd3e1a1aaaf5f7a25704f62dfc939c4987dd7ce060" +
  "009c5bb1c2447355b3216f10b537e9afa7b64a4e5391b0d631172d07939e087a"
raise "HMAC384" unless CryptoBoundary.hmac384 == [hmac384, hmac384]
raise "HMAC512" unless CryptoBoundary.hmac512 == [hmac512, hmac512]
raise "EC round trip" unless CryptoBoundary.ec
raise "Ed25519 RFC 8032" unless CryptoBoundary.ed25519 ==
  "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155" +
  "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
raise "errors" unless CryptoBoundary.bad_key && CryptoBoundary.bad_curve &&
  CryptoBoundary.bad_digest && CryptoBoundary.bad_hkdf
puts "OpenSSL registrations OK"
"#;

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::empty_app().write("app/models/crypto_boundary.rb", SOURCE)
}

#[test]
fn openssl_constants_pass_strict_emission() {
    let (_, errors) = overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn openssl_emitted_ruby_matches_vectors() {
    overlay().run_ruby(ASSERTIONS).assert_passes();
}

#[test]
fn openssl_registration_preserves_source_methods() {
    override_overlay().run_ruby(OVERRIDE_ASSERTIONS).assert_passes();
    let (tree, errors) = override_overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    for path in ["open_ssl/kdf", "open_ssl/pkey/ec"] {
        let sidecar = std::fs::read_to_string(tree.join(format!("app/models/{path}.rbs"))).unwrap();
        assert!(sidecar.contains("-> untyped"), "source override retains a native seed: {sidecar}");
    }
}

fn override_overlay() -> emit_and_run::Overlay {
    emit_and_run::empty_app().write("app/models/local_crypto.rb", OVERRIDE)
}

fn subclass_overlay() -> emit_and_run::Overlay {
    emit_and_run::empty_app().write("app/models/factory_consumer.rb", SUBCLASSES)
}

#[test]
fn openssl_factories_do_not_erase_subclass_overrides() {
    subclass_overlay().run_ruby(SUBCLASS_ASSERTIONS).assert_passes();
    let (tree, errors) = subclass_overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let sidecar = std::fs::read_to_string(tree.join("app/models/factory_consumer.rbs")).unwrap();
    assert!(!sidecar.contains("-> String") && !sidecar.contains("-> OpenSSL::PKey::EC::Group"), "{sidecar}");
}

fn alias_overlay() -> emit_and_run::Overlay {
    emit_and_run::empty_app().write("app/models/alias_consumer.rb", ERROR_ALIAS)
}

#[test]
fn openssl_error_alias_does_not_restore_a_concrete_native_return() {
    alias_overlay().run_ruby("raise \"alias override\" unless AliasConsumer.value == 42").assert_passes();
    let (tree, errors) = alias_overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let sidecar = std::fs::read_to_string(tree.join("app/models/alias_consumer.rbs")).unwrap();
    assert!(!sidecar.contains("-> String"), "{sidecar}");
}

#[test]
fn openssl_registration_leaves_other_methods_untyped() {
    let (tree, errors) = emit_and_run::empty_app().write("app/models/missing_crypto_api.rb",
        "class MissingCryptoApi\n  def self.value\n    OpenSSL::KDF.unprovided_crypto_api\n  end\nend\n")
        .emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let sidecar = std::fs::read_to_string(tree.join("app/models/missing_crypto_api.rbs")).unwrap();
    assert!(sidecar.contains("def self.value: () -> untyped"), "{sidecar}");
}

#[test]
fn openssl_constants_remain_unavailable_on_crystal() {
    let (_, errors) = overlay().emit(BuildTarget::Crystal);
    for name in ["OpenSSL", "OpenSSL::KDF", "OpenSSL::PKey", "OpenSSL::PKey::EC",
        "OpenSSL::Digest::SHA384", "OpenSSL::Digest::SHA512"] {
        assert!(errors.iter().any(|e| e.contains(&format!("{name} is not available as a class/module value on crystal"))),
            "{name}: {errors:#?}");
    }
}

fn run_native(target: BuildTarget) {
    let sidecars = if matches!(target, BuildTarget::Ruby) { "sig/app/models" } else { "app/models" };
    let (tree, errors) = overlay().emit(target);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(tree.join(sidecars).join("crypto_boundary.rbs").is_file(), "missing generated sidecar");
    compile_and_run(&tree, sidecars,
        &format!("require_relative \"app/models/crypto_boundary\"\n{ASSERTIONS}"), "OpenSSL registrations OK");
}

fn compile_and_run(tree: &emit_and_run::Emitted, sidecars: &str, script: &str, expected: &str) {
    std::fs::write(tree.join("contract.rb"),
        script).unwrap();
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
#[ignore = "requires Spinel with OpenSSL support (da4485a58 or later)"]
fn openssl_native_through_ruby_emission() {
    run_native(BuildTarget::Ruby);
}

#[test]
#[ignore = "requires Spinel with OpenSSL support (da4485a58 or later)"]
fn openssl_native_through_strict_emission() {
    run_native(BuildTarget::Spinel);
}

#[test]
#[ignore = "requires Spinel with OpenSSL support (da4485a58 or later)"]
fn openssl_source_overrides_run_natively_with_sidecars() {
    let (tree, errors) = override_overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    compile_and_run(&tree, "app/models", &format!(
        "require \"openssl\"\nrequire_relative \"app/models/open_ssl/kdf\"\n\
         require_relative \"app/models/open_ssl/pkey/ec\"\n{OVERRIDE_ASSERTIONS}"), "OpenSSL source overrides OK");
}

#[test]
#[ignore = "requires Spinel with OpenSSL support (da4485a58 or later)"]
fn openssl_subclass_overrides_run_natively_with_sidecars() {
    let (tree, errors) = subclass_overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    compile_and_run(&tree, "app/models", &format!(
        "require \"openssl\"\nrequire_relative \"app/models/factory_digest\"\n\
         require_relative \"app/models/factory_ec\"\n\
         require_relative \"app/models/factory_consumer\"\n{SUBCLASS_ASSERTIONS}"), "OpenSSL factory subclasses OK");
}

#[test]
#[ignore = "requires Spinel with OpenSSL support (da4485a58 or later)"]
fn openssl_error_alias_override_runs_natively_with_sidecars() {
    let (tree, errors) = alias_overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    compile_and_run(&tree, "app/models", "require \"openssl\"\n\
        require_relative \"app/models/open_ssl/pkey/pkey_error\"\n\
        require_relative \"app/models/open_ssl/pkey/ecerror\"\n\
        require_relative \"app/models/alias_consumer\"\n\
        raise \"alias override\" unless AliasConsumer.value == 42\n\
        puts \"OpenSSL alias override OK\"\n", "OpenSSL alias override OK");
}

const CIPHER: &str = include_str!("support/cipher_registrations.rb");

fn cipher_overlay() -> emit_and_run::Overlay {
    emit_and_run::empty_app().write("app/models/cipher_boundary.rb", CIPHER.split_once("\nct, tag =").unwrap().0)
}

fn cipher_assertions() -> &'static str {
    &CIPHER[CIPHER.find("\nct, tag =").unwrap()..]
}

#[test]
fn cipher_constants_pass_strict_emission() {
    let (_, errors) = cipher_overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn cipher_emitted_ruby_matches_nist_vectors_and_error_parents() {
    cipher_overlay().run_ruby(cipher_assertions()).assert_passes();
}

#[test]
fn cipher_ruby_keeps_bundled_non_gcm_and_computed_names() {
    let source = r#"require "openssl"
class BundledCipher
  def self.value(name)
    c = OpenSSL::Cipher.new(name)
    c.encrypt
    c.key = "k" * 32
    c.iv = "i" * 16
    (c.update("hello cipher") + c.final).unpack1("H*")
  end
end
"#;
    emit_and_run::empty_app().write("app/models/bundled_cipher.rb", source).run_ruby(
        "raise \"bundled CBC\" unless BundledCipher.value(\"aes-256-cbc\") == \"a89ad56b39c88372415c2772b6cb63ec\"").assert_passes();
    let (_, errors) = emit_and_run::empty_app().write("app/models/bundled_cipher.rb", source).emit(BuildTarget::Jruby);
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn cipher_names_require_gcm_literals_on_spinel() {
    for call in [
        "OpenSSL::Cipher.new(\"aes-256-cbc\")",
        "::OpenSSL::Cipher.new(\"AES-128-GCM\")",
        "OpenSSL::Cipher.new(name)",
        "OpenSSL::Cipher.public_send(:new, name)",
        "OpenSSL::Cipher.send(\"new\", name)",
        "OpenSSL::Cipher.__send__(:new, name)",
        "factory = OpenSSL::Cipher; factory.new(name)",
        "copy = \"aes-128-gcm\"; OpenSSL::Cipher.new(copy)",
        "OpenSSL::Cipher.new(name ? \"aes-128-gcm\" : \"aes-256-gcm\")",
        "OpenSSL::Cipher.new",
        "OpenSSL::Cipher.new(\"aes-128-gcm\", name)",
    ] {
        let source = format!("require \"openssl\"\nclass CipherName\n  def self.value(name)\n    {call}\n  end\nend\n");
        let (_, errors) = emit_and_run::empty_app().write("app/models/cipher_name.rb", &source).emit(BuildTarget::Spinel);
        assert!(errors.iter().any(|e| e.contains("requires a literal aes-128-gcm or aes-256-gcm")), "{call}: {errors:#?}");
        let (_, errors) = emit_and_run::empty_app().write("app/models/cipher_name.rb", &source).emit(BuildTarget::Ruby);
        assert!(errors.is_empty(), "Ruby {call}: {errors:#?}");
    }
}

#[test]
fn cipher_constants_keep_other_target_availability() {
    let (_, errors) = cipher_overlay().emit(BuildTarget::Crystal);
    for name in ["OpenSSL::Cipher", "OpenSSL::Cipher::CipherError", "OpenSSL::Cipher::AuthTagError"] {
        assert!(errors.iter().any(|e| e.contains(&format!("{name} is not available as a class/module value on crystal"))), "{name}: {errors:#?}");
    }
}

const CIPHER_OVERRIDE: &str = r#"require "openssl"
class OpenSSL::Cipher
  def self.new(name)
    17
  end
  def update(value)
    19
  end
end
class CipherOverride
  def self.value
    OpenSSL::Cipher.new("custom")
  end
end
"#;

#[test]
fn cipher_source_overrides_do_not_inherit_native_returns_or_name_limits() {
    let (tree, errors) = emit_and_run::empty_app().write("app/models/cipher_override.rb", CIPHER_OVERRIDE).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let sidecar = std::fs::read_to_string(tree.join("app/models/open_ssl/cipher.rbs")).unwrap();
    assert!(!sidecar.contains("-> String") && !sidecar.contains("-> OpenSSL::Cipher"), "{sidecar}");
    emit_and_run::empty_app().write("app/models/cipher_override.rb", CIPHER_OVERRIDE)
        .run_ruby("raise \"source override\" unless CipherOverride.value == 17").assert_passes();
}

#[test]
fn cipher_unregistered_methods_stay_gradual() {
    let (tree, errors) = emit_and_run::empty_app().write("app/models/cipher_unknown.rb", r#"require "openssl"
class CipherUnknown
  def self.value
    OpenSSL::Cipher.new("aes-128-gcm").unprovided_cipher_api
  end
end
"#).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let sidecar = std::fs::read_to_string(tree.join("app/models/cipher_unknown.rbs")).unwrap();
    assert!(sidecar.contains("-> untyped"), "{sidecar}");
}

#[test]
fn cipher_union_receivers_refuse_the_native_constructor_in_either_arm() {
    for receiver in ["flag ? OpenSSL::Cipher : OtherCipher", "flag ? OtherCipher : OpenSSL::Cipher"] {
        let source = format!(r#"require "openssl"
class OtherCipher
  def self.new(name)
    17
  end
end
class CipherUnion
  def self.value(flag, name)
    factory = {receiver}
    factory.new(name)
  end
end
"#);
        let (_, errors) = emit_and_run::empty_app().write("app/models/cipher_union.rb", &source).emit(BuildTarget::Spinel);
        assert!(errors.iter().any(|e| e.contains("requires a literal aes-128-gcm or aes-256-gcm")), "{receiver}: {errors:#?}");
    }
}

const CIPHER_SUBCLASS: &str = r#"require "openssl"
class CustomCipher < OpenSSL::Cipher
  def update(value)
    19
  end
end
class CipherSubclass
  def self.value
    CustomCipher.new("aes-128-gcm").encrypt.update("payload")
  end
end
"#;

#[test]
fn cipher_subclass_mode_switches_preserve_source_overrides() {
    let source = emit_and_run::empty_app().write("app/models/cipher_subclass.rb", CIPHER_SUBCLASS);
    source.run_ruby("raise \"subclass override\" unless CipherSubclass.value == 19").assert_passes();
    let (tree, errors) = emit_and_run::empty_app().write("app/models/cipher_subclass.rb", CIPHER_SUBCLASS).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let sidecar = std::fs::read_to_string(tree.join("app/models/cipher_subclass.rbs")).unwrap();
    assert!(!sidecar.contains("-> String"), "subclass override erased: {sidecar}");
}

#[test]
fn cipher_reopen_order_and_initialize_concerns_keep_source_behavior() {
    let constructor = r#"require "openssl"
module CipherConstructor
  def initialize(name)
    @local_name = name
  end
end
class OpenSSL::Cipher
  include CipherConstructor
end
"#;
    let method = r#"require "openssl"
class OpenSSL::Cipher
  def update(value)
    19
  end
end
class CipherConcern
  def self.value
    OpenSSL::Cipher.new("custom").update("payload")
  end
end
"#;
    for own in [false, true] {
        let constructor = if own { constructor.replace("  include CipherConstructor",
            "  include CipherConstructor\n  def initialize(name)\n    @local_name = name\n  end") } else { constructor.to_string() };
        for reverse in [false, true] {
            let (first, second) = if reverse { (method, constructor.as_str()) } else { (constructor.as_str(), method) };
            let overlay = || emit_and_run::empty_app().write("app/models/a_cipher.rb", first).write("app/models/z_cipher.rb", second);
            let (_, errors) = overlay().emit(BuildTarget::Spinel);
            if own {
                assert!(errors.is_empty(), "reverse={reverse}: {errors:#?}");
                overlay().run_ruby("raise \"own constructor\" unless CipherConcern.value == 19").assert_passes();
            } else {
                // A class's native initialize wins over an included module's initialize on CRuby.
                assert!(errors.iter().any(|e| e.contains("requires a literal aes-128-gcm or aes-256-gcm")), "{errors:#?}");
                overlay().run_ruby("begin; CipherConcern.value; raise \"lost native initializer\"; rescue OpenSSL::Cipher::CipherError; end").assert_passes();
            }
        }
    }
}

const CIPHER_RUNTIME_ASSERTIONS: &str = r#"
raise "valid runtime tag" unless MessageDigest.aes_256_gcm_decrypt("k" * 32, "i" * 12,
  ["b2e194185dd8c26d8b3c840a"].pack("H*"), ["03c4ad5dffea8e00a2e79b9676b4077e"].pack("H*")) == "hello cipher"
raise "invalid runtime tag" unless MessageDigest.aes_256_gcm_decrypt("k" * 32, "i" * 12, "abcdef", "x" * 16) == ""
raise "short runtime tag" unless MessageDigest.aes_256_gcm_decrypt("k" * 32, "i" * 12, "abcdef", "x") == ""
puts "Cipher runtime rescue OK"
"#;

#[test]
fn cipher_runtime_rescue_matches_cruby() {
    // CRuby does not implement Spinel's FFI declaration. Only the AES methods run.
    let script = format!("module SpCrypto; def self.ffi_func(*); end; end\nrequire {:?}\n{}",
        concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/spinel/message_digest.rb"), CIPHER_RUNTIME_ASSERTIONS);
    let run = Command::new("ruby").args(["-e", &script]).output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "Cipher runtime rescue OK");
}

#[test]
#[ignore = "requires Spinel with OpenSSL GCM support"]
fn cipher_native_vectors_and_runtime_rescue_with_sidecars() {
    let (tree, errors) = cipher_overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    compile_and_run(&tree, "app/models", &format!("require_relative \"app/models/cipher_boundary\"\n{}", cipher_assertions()),
        "Cipher registrations OK");
    std::fs::write(tree.join("cipher_runtime.rb"), include_str!("../runtime/spinel/message_digest.rb")).unwrap();
    compile_and_run(&tree, "app/models", &format!("require \"base64\"\nrequire_relative \"cipher_runtime\"\n{CIPHER_RUNTIME_ASSERTIONS}"),
        "Cipher runtime rescue OK");
    let (tree, errors) = emit_and_run::empty_app().write("app/models/cipher_subclass.rb", CIPHER_SUBCLASS).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    compile_and_run(&tree, "app/models", "require \"openssl\"\nrequire_relative \"app/models/custom_cipher\"\n\
        require_relative \"app/models/cipher_subclass\"\nraise \"subclass override\" unless CipherSubclass.value == 19\n\
        puts \"Cipher subclass OK\"\n", "Cipher subclass OK");
    let (tree, errors) = emit_and_run::empty_app().write("app/models/cipher_override.rb", CIPHER_OVERRIDE).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    compile_and_run(&tree, "app/models", "require \"openssl\"\nrequire_relative \"app/models/open_ssl/cipher\"\n\
        require_relative \"app/models/cipher_override\"\nraise \"source factory\" unless CipherOverride.value == 17\n\
        puts \"Cipher factory OK\"\n", "Cipher factory OK");
}
