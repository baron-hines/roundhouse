//! message_bus compiled from its installed source: a model publishes on a channel through the memory backend and
//! reads the channel's backlog back, as message_bus 5.0.0 answers on Rails. Its Railtie puts its middleware before
//! `ActionDispatch::Flash`, which clears the request's connections through `ActiveRecord::Base.connection_handler`.

#[path = "../support/prerequisite.rs"]
mod prerequisite;
use super::emit_and_run;

const VERSION: &str = "5.0.0";

fn installed_message_bus() -> Option<std::path::PathBuf> {
    let out = std::process::Command::new("ruby")
        .args(["-e", &format!("print Gem::Specification.find_by_name('message_bus', '{VERSION}').gem_dir")])
        .output();
    match out {
        Ok(out) if out.status.success() => Some(std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).into_owned())),
        _ => {
            prerequisite::missing(&format!("ruby with message_bus {VERSION} installed"));
            None
        }
    }
}

fn files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(root, &path, out);
        } else {
            out.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

const MODEL: &str = "class Notice\n  def self.round_trip\n    MessageBus.configure(backend: :memory)\n    MessageBus.publish(\"/greet\", \"hello\")\n    MessageBus.publish(\"/greet\", \"again\")\n    MessageBus.backlog(\"/greet\").map(&:data)\n  end\nend\n";

const SCRIPT: &str = "got = Notice.round_trip\nraise got.inspect unless got == [\"hello\", \"again\"]\nputs \"message_bus passed\"\n";

fn app() -> Option<emit_and_run::Overlay> {
    let gem = installed_message_bus()?;
    let fixture = roundhouse::fixtures::real_blog();
    let lock = std::fs::read_to_string(fixture.join("Gemfile.lock")).unwrap();
    let lock = lock
        // Not a bare "    rack (": a dependency line ("      rack (>= 2.2.4)") holds it too.
        .replacen("\n    rack (", &format!("\n    message_bus ({VERSION})\n      rack (>= 1.1.3)\n    rack ("), 1)
        .replacen("DEPENDENCIES\n", "DEPENDENCIES\n  message_bus\n", 1);
    let gemfile = std::fs::read_to_string(fixture.join("Gemfile")).unwrap() + "gem \"message_bus\"\n";
    let mut overlay = emit_and_run::real_blog()
        .write("Gemfile", &gemfile)
        .write("Gemfile.lock", &lock)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("app/models/notice.rb", MODEL);
    let mut list = Vec::new();
    files(&gem, &gem.join("lib"), &mut list);
    for file in list {
        let Ok(text) = std::fs::read_to_string(gem.join(&file)) else { continue };
        overlay = overlay.write(&format!("vendor/bundle/ruby/3.4.0/gems/message_bus-{VERSION}/{}", file.display()), &text);
    }
    Some(overlay)
}

/// What the gem still does that no tree answers, which this flow does not reach: `rack/thin_ext.rb`, the async response
/// the middleware loads only under Thin (`env["async.callback"]`), mixes in eventmachine's `EM::Deferrable` and rack's
/// `Rack::Response::Helpers`. Without eventmachine Ruby cannot load the file at all.
const KNOWN_GAPS: [&str; 2] = [
    "Thin::DeferrableBody includes unresolved EM::Deferrable",
    "Thin::AsyncResponse includes unresolved Rack::Response::Helpers",
];

#[test]
fn message_bus_publishes_and_reads_its_backlog_on_ruby() {
    let Some(app) = app() else { return };
    let run = app.run_ruby(SCRIPT);
    let unknown: Vec<_> = run.errors.iter().filter(|e| !KNOWN_GAPS.iter().any(|gap| e.contains(gap))).collect();
    assert!(unknown.is_empty(), "{unknown:#?}");
    for gap in KNOWN_GAPS {
        assert!(run.errors.iter().any(|e| e.contains(gap)), "{gap} is answered now: drop it from KNOWN_GAPS\n{:#?}", run.errors);
    }
    assert!(run.program_succeeded(), "{}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("message_bus passed"), "{}", run.stdout);
}

/// The Spinel tree has no load path, and message_bus picks its backend with
/// `require "message_bus/backends/#{backend}"`: a name built at run time,
/// which loads nothing there. It is reported, not left to raise LoadError.
#[test]
fn message_bus_s_built_backend_require_is_reported_on_spinel() {
    let Some(app) = app() else { return };
    let (_emitted, errors) = app.emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.iter().any(|e| e.contains("require of a name built at run time")), "{errors:#?}");
}
