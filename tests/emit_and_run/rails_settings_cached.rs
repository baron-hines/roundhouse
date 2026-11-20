//! rails-settings-cached compiled from source (maybe's `class Setting < RailsSettings::Base`): `field` defines a
//! class-side reader that answers the default until a value is saved, a writer that saves one, and a boolean's `?`,
//! as rails-settings-cached 2.9.6 answers on Rails.

#[path = "../support/prerequisite.rs"]
mod prerequisite;
use super::emit_and_run;

const VERSION: &str = "2.9.6";

fn installed() -> Option<std::path::PathBuf> {
    let out = std::process::Command::new("ruby")
        .args(["-e", &format!("print Gem::Specification.find_by_name('rails-settings-cached', '{VERSION}').gem_dir")])
        .output();
    match out {
        Ok(out) if out.status.success() => Some(std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).into_owned())),
        _ => {
            prerequisite::missing(&format!("ruby with rails-settings-cached {VERSION} installed"));
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

const SETTING: &str = "class Setting < RailsSettings::Base\n  field :site_name, type: :string, default: ENV.fetch(\"PROBE_SITE_NAME\", \"Blog\")\n  field :open_signup, type: :boolean, default: false\nend\n";

const SCRIPT: &str = r#"
got = [Setting.site_name, Setting.open_signup, Setting.open_signup?]
Setting.site_name = "Mine"
Setting.open_signup = true
got += [Setting.site_name, Setting.open_signup?, Setting.count]
raise "got #{got.inspect}" unless got == ["Blog", false, false, "Mine", true, 2]
puts "settings passed"
"#;

fn app() -> Option<emit_and_run::Overlay> {
    let gem = installed()?;
    let fixture = roundhouse::fixtures::real_blog();
    let lock = std::fs::read_to_string(fixture.join("Gemfile.lock")).unwrap()
        .replacen("\n    rack (", &format!("\n    rails-settings-cached ({VERSION})\n      activerecord (>= 5.0.0)\n      railties (>= 5.0.0)\n    rack ("), 1)
        .replacen("DEPENDENCIES\n", "DEPENDENCIES\n  rails-settings-cached\n", 1);
    let gemfile = std::fs::read_to_string(fixture.join("Gemfile")).unwrap() + "gem \"rails-settings-cached\"\n";
    let mut overlay = emit_and_run::real_blog()
        .write("Gemfile", &gemfile)
        .write("Gemfile.lock", &lock)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .edit(
            "db/schema.rb",
            "  create_table \"articles\", force: :cascade do |t|\n",
            "  create_table \"settings\", force: :cascade do |t|\n    t.string \"var\", null: false\n    t.text \"value\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"articles\", force: :cascade do |t|\n",
        )
        .write("app/models/setting.rb", SETTING);
    let mut list = Vec::new();
    files(&gem, &gem.join("lib"), &mut list);
    for file in list {
        let Ok(text) = std::fs::read_to_string(gem.join(&file)) else { continue };
        overlay = overlay.write(&format!("vendor/bundle/ruby/3.4.0/gems/rails-settings-cached-{VERSION}/{}", file.display()), &text);
    }
    Some(overlay)
}

#[test]
fn rails_settings_fields_read_their_defaults_and_save_values_on_ruby() {
    let Some(app) = app() else { return };
    let run = app.run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("settings passed"), "{}", run.stdout);
}
