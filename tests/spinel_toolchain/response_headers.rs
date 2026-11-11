//! A Spinel response carries what Rails would: the default security
//! headers, an absolute redirect Location, and — with
//! `config.assume_ssl` — a Secure session cookie and an https Location.
//!
//! The ruby-lane twin is `tests/environment_flags.rs`.

use super::{json_flash_contract as flash_contract, puma_boot_contract};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn head(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: blog.test:{port}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut out = String::new();
    let _ = stream.read_to_string(&mut out);
    out
}

struct Running {
    child: Child,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn responses_carry_rails_headers_and_absolute_redirects() {
    let app = puma_boot_contract::overlay()
        .edit(
            "config/environments/production.rb",
            "# config.assume_ssl = true",
            "config.assume_ssl = true",
        )
        .edit(
            "config/routes.rb",
            "root \"articles#index\"",
            "root \"articles#index\"\n  get \"go\" => \"articles#go\"\n  get \"record_redirect\" => \"articles#record_redirect\"",
        )
        .edit(
            "app/controllers/articles_controller.rb",
            "  # GET /articles or /articles.json\n",
            "  def go\n    redirect_to \"/articles\"\n  end\n\n  def record_redirect\n    start = ArticleStart.new\n    if start.save\n      redirect_to article_path(start.article)\n    end\n  end\n\n  # GET /articles or /articles.json\n",
        )
        .write(
            "app/channels/application_cable/connection.rb",
            "module ApplicationCable\n  class Connection < ActionCable::Connection::Base\n    def connect\n      raise 'path' unless request.path == '/cable'\n      raise 'listener' unless request.env['roundhouse.listener'] == 'tcp'\n      raise 'header' unless request.headers['X-Probe'] == 'present'\n      raise 'params' unless request.params['token'] == 'probe'\n    end\n  end\nend\n",
        )
        .write(
            "app/models/article_start.rb",
            "class ArticleStart\n  include ActiveModel::Model\n  attr_reader :article\n  def save\n    @article = Article.create!(title: 'reader', body: 'reader body')\n    true\n  end\nend\n",
        );
    let tree = flash_contract::with_flash(app).native_http_tree();
    let help = Command::new(tree.join("build/bin/blog"))
        .arg("--help")
        .current_dir(&*tree)
        .output()
        .expect("run --help");
    assert!(help.status.success());
    assert!(!String::from_utf8_lossy(&help.stdout).contains("puma hook"));
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let log_path = tree.join("server.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let err = log.try_clone().unwrap();
    let child = Command::new(tree.join("build/bin/blog"))
        .current_dir(&*tree)
        .env("PORT", port.to_string())
        .env("RAILS_ENV", "production")
        .env("SECRET_KEY_BASE", "x".repeat(64))
        .env("BLOG_DB", tree.join("headers.sqlite3"))
        .env("SPINEL_WORKERS", "1")
        .stdout(log)
        .stderr(err)
        .spawn()
        .expect("start");
    let mut running = Running { child };
    let deadline = Instant::now() + Duration::from_secs(30);
    while TcpStream::connect(("127.0.0.1", port)).is_err()
        || !std::fs::read_to_string(&log_path).unwrap_or_default().contains("puma hook second")
    {
        if let Some(status) = running.child.try_wait().unwrap() {
            panic!(
                "server exited ({status}):\n{}",
                std::fs::read_to_string(&log_path).unwrap_or_default()
            );
        }
        assert!(Instant::now() < deadline, "listener or boot callbacks did not become ready");
        std::thread::sleep(Duration::from_millis(100));
    }
    let page = head(port, "/articles");
    let redirect = head(port, "/go");
    let cable_started = Instant::now();
    let mut socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(socket, "GET /cable?token=probe HTTP/1.1\r\nHost: blog.test:{port}\r\nOrigin: https://blog.test:{port}\r\nX-Forwarded-Proto: https\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nX-Probe: present\r\n\r\n").unwrap();
    let mut response = [0; 4096];
    let size = socket.read(&mut response).unwrap();
    let response = String::from_utf8_lossy(&response[..size]);
    assert!(response.starts_with("HTTP/1.1 101"), "{response}");
    eprintln!("Cable request assertion: {:?}", cable_started.elapsed());
    drop(socket);
    for h in [
        "X-Frame-Options: SAMEORIGIN",
        "X-XSS-Protection: 0",
        "X-Content-Type-Options: nosniff",
        "X-Permitted-Cross-Domain-Policies: none",
        "Referrer-Policy: strict-origin-when-cross-origin",
    ] {
        assert!(page.contains(h), "{h} missing:\n{page}");
        assert!(
            redirect.contains(h),
            "{h} missing on the redirect:\n{redirect}"
        );
    }
    assert!(
        redirect.contains(&format!("Location: https://blog.test:{port}/articles")),
        "absolute, https under assume_ssl:\n{redirect}"
    );
    let record_redirect = head(port, "/record_redirect");
    assert!(record_redirect.starts_with("HTTP/1.1 302"), "{record_redirect}");
    assert!(
        record_redirect.contains(&format!("Location: https://blog.test:{port}/articles/1\r\n")),
        "a form object's record reader fills the id segment:\n{record_redirect}"
    );
    assert!(
        page.contains("; Secure"),
        "the session cookie is Secure under assume_ssl:\n{page}"
    );

    let output = Command::new("ruby")
        .env("FLASH_PORT", port.to_string())
        .args([
            "-e",
            &format!(
                "{}\n{}\n{}",
                flash_contract::HTTP_ADAPTER,
                flash_contract::RACK_ADAPTER,
                flash_contract::ASSERTIONS
            ),
        ])
        .output()
        .expect("Ruby HTTP client");
    assert!(
        output.status.success(),
        "{}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        std::fs::read_to_string(&log_path).unwrap_or_default()
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
    drop(running);
    let log = std::fs::read_to_string(&log_path).expect("read server log");
    assert_eq!(log.matches("puma hook first").count(), 1, "{log}");
    assert_eq!(log.matches("puma hook second").count(), 1, "{log}");
    assert!(log.find("puma hook first") < log.find("puma hook second"), "{log}");
}
