//! The Redis broadcast transport against a real Redis, both ways, held
//! to real Action Cable.
//!
//! `tests/redis_cable_live/driver.rb` runs `RedisCable` (runtime/spinel/
//! sidekiq/redis_cable.rb) as a web process does: it subscribes, then
//! records what reaches its sockets. `tests/redis_cable_live/
//! rails_broadcast.rb` broadcasts `tests/redis_cable/rails.json`'s three
//! messages through actioncable 8.1.3's own Redis adapter. The driver
//! must be handed each one's stream and bytes, and then publishes each
//! back, which the Rails side must read as the bytes it sent. Under CRuby
//! over redis-rb, and over spinel-redis when Spinel compiles the driver.
//!
//!   SIDEKIQ_LIVE_REDIS_URL=redis://127.0.0.1:6379/15 \
//!   SPINEL_REDIS_DIR=/path/to/spinel-redis \
//!   cargo test --test redis_cable_live -- --ignored --nocapture --test-threads=1

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use serde_json::Value;

fn required_env(name: &str, what: &str) -> String {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => v,
        _ => panic!("{name} must name {what}; this gate fails rather than skips without it"),
    }
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The scratch directory, with the real transport as the `lane` gets it:
/// on CRuby, a plain redis-rb connection subscribes, as `project.rs`
/// rewrites it for the ruby tree.
fn scratch(name: &str, cruby: bool) -> PathBuf {
    let base = option_env!("CARGO_TARGET_TMPDIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir scratch");
    for (from, to) in [
        ("runtime/spinel/sidekiq/sidekiq_redis.rb", "sidekiq_redis.rb"),
        ("tests/redis_cable_live/driver.rb", "driver.rb"),
        ("tests/redis_cable_live/rails_broadcast.rb", "rails_broadcast.rb"),
        ("tests/redis_cable/rails.json", "rails.json"),
    ] {
        std::fs::copy(root().join(from), dir.join(to)).unwrap_or_else(|e| panic!("copy {from}: {e}"));
    }
    let mut cable = std::fs::read_to_string(root().join("runtime/spinel/sidekiq/redis_cable.rb")).unwrap();
    if cruby {
        assert!(cable.contains("    Redis.pubsub(url: url)\n"));
        cable = cable.replace("    Redis.pubsub(url: url)\n", "    Redis.new(url: url)\n");
    }
    std::fs::write(dir.join("redis_cable.rb"), cable).unwrap();
    dir
}

fn round_trip(dir: &Path, url: &str, mut driver: Command) {
    let mut child = driver
        .env("REDIS_URL", url)
        .env_remove("REDIS_PROVIDER")
        .current_dir(dir)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("{driver:?}: {e}"));
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut seen: Vec<String> = Vec::new();
    let wait_for = |text: &str, seen: &mut Vec<String>| {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !seen.iter().any(|l| l == text) {
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(line) => {
                    println!("driver: {line}");
                    seen.push(line);
                }
                Err(RecvTimeoutError::Timeout) => panic!("no {text:?} from the driver:\n{}", seen.join("\n")),
                Err(RecvTimeoutError::Disconnected) => panic!("the driver exited before {text:?}:\n{}", seen.join("\n")),
            }
        }
    };
    wait_for("subscribed", &mut seen);

    let rails = Command::new("timeout")
        .args(["60", "ruby", "rails_broadcast.rb"])
        .env("REDIS_URL", url)
        .current_dir(dir)
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&rails.stdout).to_string();
    print!("{out}");
    wait_for("echoed", &mut seen);
    let status = child.wait().unwrap();
    assert!(status.success(), "the driver exited {status}:\n{}", seen.join("\n"));
    assert!(
        rails.status.success() && out.ends_with("done\n"),
        "the Rails side did not finish\n{out}\n{}",
        String::from_utf8_lossy(&rails.stderr)
    );
    let failed: Vec<&str> = out.lines().filter(|l| l.starts_with("FAIL")).collect();
    assert!(failed.is_empty(), "Rails read the transport's publishes differently:\n{}", failed.join("\n"));

    let oracle: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("rails.json")).unwrap()).unwrap();
    let want: Vec<String> = oracle["publishes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| format!("got {} {}", p["stream"].as_str().unwrap(), p["payload"].as_str().unwrap()))
        .collect();
    // Its own echoes come back to it as well, as a web process's own
    // broadcasts reach its sockets through Redis; it may exit before
    // they arrive, so only Rails' three are held here.
    let got: Vec<String> = seen.iter().filter(|l| l.starts_with("got ") && !l.starts_with("got echo_")).cloned().collect();
    assert_eq!(got, want, "the subscriber was handed something other than what Rails published");
}

#[test]
#[ignore = "requires SIDEKIQ_LIVE_REDIS_URL and the redis and actioncable gems"]
fn the_cruby_transport_trades_broadcasts_with_rails() {
    let url = required_env("SIDEKIQ_LIVE_REDIS_URL", "a scratch Redis database");
    let dir = scratch("roundhouse-redis-cable-live-cruby", true);
    let mut driver = Command::new("timeout");
    driver.args(["60", "ruby", "driver.rb"]);
    round_trip(&dir, &url, driver);
}

#[test]
#[ignore = "requires Spinel, SPINEL_REDIS_DIR (spinel-redis v0.3.1+), SIDEKIQ_LIVE_REDIS_URL and the actioncable gem"]
fn the_spinel_transport_trades_broadcasts_with_rails() {
    let url = required_env("SIDEKIQ_LIVE_REDIS_URL", "a scratch Redis database");
    let redis_dir = required_env("SPINEL_REDIS_DIR", "a spinel-redis checkout");
    let dir = scratch("roundhouse-redis-cable-live-spinel", false);
    let compiler = std::env::var("SPINEL").unwrap_or_else(|_| "spinel".into());
    let build = Command::new(compiler).args(["-I", &redis_dir, "driver.rb", "-o", "driver"]).current_dir(&dir).output().unwrap();
    assert!(
        build.status.success(),
        "spinel failed to compile the driver\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let mut driver = Command::new("timeout");
    driver.args(["60"]).arg(dir.join("driver"));
    round_trip(&dir, &url, driver);
}
