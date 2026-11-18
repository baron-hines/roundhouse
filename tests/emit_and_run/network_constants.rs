//! The socket, openssl, io/wait and zlib names a network client reaches as it
//! configures a connection, rescues a wait, checks a certificate class or
//! inflates a body (excon, httpclient, redis-client). The libraries' own on
//! the ruby family, spinel's packages and runtime on spinel; the strict
//! targets report them.

use super::emit_and_run;

const PROBE: &str = r#"require "socket"
require "openssl"
require "zlib"

class NetProbe
  def self.options = [Socket::IPPROTO_TCP, Socket::SOL_SOCKET, Socket::SO_KEEPALIVE, Socket::TCP_NODELAY, Socket::AF_INET]

  def self.waits = [IO::WaitReadable, IO::WaitWritable].map(&:name)

  def self.classes = [OpenSSL::SSL::SSLContext, OpenSSL::SSL::SSLSocket, OpenSSL::X509::Certificate, OpenSSL::PKey::RSA].map(&:name)

  def self.md5 = OpenSSL::Digest::MD5.hexdigest("a")

  def self.inflated = Zlib::Inflate.inflate(Zlib.deflate("hello"))
end
"#;

fn probe_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/net_probe.rb", PROBE)
}

const ASSERTIONS: &str = r##"got = NetProbe.options
raise got.inspect unless got.all? { |v| v.is_a?(Integer) } && got == [6, 65535, 8, 1, 2]
raise NetProbe.waits.inspect unless NetProbe.waits == ["IO::WaitReadable", "IO::WaitWritable"]
names = ["OpenSSL::SSL::SSLContext", "OpenSSL::SSL::SSLSocket", "OpenSSL::X509::Certificate", "OpenSSL::PKey::RSA"]
raise NetProbe.classes.inspect unless NetProbe.classes == names
raise NetProbe.md5 unless NetProbe.md5 == "0cc175b9c0f1b6a831c399e269772661"
raise NetProbe.inflated unless NetProbe.inflated == "hello"
puts "network constants passed"
"##;

#[test]
fn network_constants_run() {
    let run = probe_app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("network constants passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn network_constants_run_on_spinel() {
    let run = probe_app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("network constants passed"), "{}", run.stdout);
}

#[test]
fn network_constants_are_reported_on_strict_targets() {
    use roundhouse::project::BuildTarget::*;
    for target in [Rust, Typescript, Go, Python, CSharp, Kotlin, Swift, Elixir, Crystal] {
        let (_emitted, errors) = probe_app().emit(target);
        for name in ["Socket::", "IO::WaitReadable", "OpenSSL::SSL::SSLContext", "Zlib::Inflate"] {
            assert!(errors.iter().any(|e| e.contains(name)), "{target:?} {name}: {errors:#?}");
        }
    }
}
