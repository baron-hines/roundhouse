//! `OpenSSL::SSL::VERIFY_NONE` read in a source gem's nested class (aws's
//! `Seahorse::Client::H2::Connection`, excon, stripe): the path starts from the
//! top-level `OpenSSL` wherever the read sits.

use super::emit_and_run;

const GEM_DIR: &str = "vendor/bundle/ruby/3.4.0/gems/tls-1.0.0";

const TLS: &str = r#"require "openssl"

module Tls
  module Client
    class Connection
      def initialize(verify) = @verify = verify

      def mode = @verify ? OpenSSL::SSL::VERIFY_PEER : OpenSSL::SSL::VERIFY_NONE
    end
  end
end
"#;

const PROBE: &str = r#"class TlsProbe
  def self.modes = [Tls::Client::Connection.new(true).mode, Tls::Client::Connection.new(false).mode]
end
"#;

fn tls_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    tls (1.0.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  tls\n")
        .edit("Gemfile", "gem \"rails\", \"~> 8.1.4\"\n", "gem \"rails\", \"~> 8.1.4\"\ngem \"tls\"\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{GEM_DIR}/lib/tls.rb"), TLS)
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"tls\"}\n")
        .write("app/models/tls_probe.rb", PROBE)
}

const ASSERTIONS: &str = r##"raise TlsProbe.modes.inspect unless TlsProbe.modes == [1, 0]
puts "nested core constant passed"
"##;

#[test]
fn a_nested_gem_class_reads_openssl_constants() {
    let run = tls_app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("nested core constant passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_nested_gem_class_reads_openssl_constants_on_spinel() {
    let run = tls_app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("nested core constant passed"), "{}", run.stdout);
}
