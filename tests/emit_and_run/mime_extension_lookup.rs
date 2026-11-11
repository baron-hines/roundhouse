//! Devise's `navigational_formats` selects the formats `Mime::EXTENSION_LOOKUP[format.to_s]` registers:
//! the registered type for an extension, or nil, as `Mime::Type.lookup_by_extension` answers it.

use super::emit_and_run;
use roundhouse::project::BuildTarget;

const MODEL: &str = r#"
  def self.registered_formats(formats)
    formats.select { |format| Mime::EXTENSION_LOOKUP[format.to_s] }
  end

  def self.html_type
    Mime::EXTENSION_LOOKUP["html"].to_s
  end
"#;

const SCRIPT: &str = r#"got = Article.registered_formats(["html", "nope", "json"])
raise got.inspect unless got == ["html", "json"]
raise Article.html_type unless Article.html_type == "text/html"
puts "mime lookup passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit("app/models/article.rb", "class Article < ApplicationRecord\n", &format!("class Article < ApplicationRecord\n{MODEL}"))
}

#[test]
fn mime_extension_lookup_answers_the_registered_type() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("mime lookup passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn mime_extension_lookup_answers_the_registered_type_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("mime lookup passed"), "{}", run.stdout);
}

/// Not only a model's or a controller's body: a `direct` route helper's is emitted too.
#[test]
fn a_direct_route_helper_reads_the_registered_type_too() {
    let overlay = || {
        app().edit(
            "config/routes.rb",
            "Rails.application.routes.draw do\n",
            "Rails.application.routes.draw do\n  direct :mime_page do |ext|\n    \"/mime/#{Mime::EXTENSION_LOOKUP[ext]}\"\n  end\n",
        )
    };
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let (tree, _) = overlay().emit(target);
        let mut found = false;
        for path in walk(&tree) {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            assert!(!text.contains("EXTENSION_LOOKUP"), "{target:?}: {}", path.display());
            found |= text.contains("mime/#{Mime::Type.lookup_by_extension(ext)}");
        }
        assert!(found, "{target:?}: the direct helper's body");
    }
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().is_some_and(|e| e == "rb") {
            out.push(path);
        }
    }
    out
}
