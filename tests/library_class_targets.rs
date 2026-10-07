//! A plain Ruby class in the app (`app/models/probe.rb` holding
//! `class Probe`) is an `app.library_classes` entry. The ruby family
//! and TypeScript emit it. The other emitters did not, and said
//! nothing: the test calling `Probe.run` was emitted against a class
//! that was not, and the transpile reported zero errors. Each of those
//! targets now reports the class as unsupported.

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::analyze::Analyzer;
use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{target_files, BuildTarget};

fn app() -> roundhouse::App {
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    tree.insert(
        PathBuf::from("db/schema.rb"),
        b"ActiveRecord::Schema[8.1].define(version: 1) do\n  create_table \"widgets\", force: :cascade do |t|\n    t.string \"name\", null: false\n  end\nend\n".to_vec(),
    );
    tree.insert(
        PathBuf::from("app/models/widget.rb"),
        b"class Widget < ApplicationRecord\nend\n".to_vec(),
    );
    tree.insert(
        PathBuf::from("app/models/probe.rb"),
        b"class Probe\n  def self.run\n    1 + 2\n  end\nend\n".to_vec(),
    );
    // Framework-based classes are not plain Ruby and stay unreported.
    tree.insert(
        PathBuf::from("app/jobs/application_job.rb"),
        b"class ApplicationJob < ActiveJob::Base\nend\n".to_vec(),
    );
    tree.insert(
        PathBuf::from("app/mailers/application_mailer.rb"),
        b"class ApplicationMailer < ActionMailer::Base\n  default from: \"a@b.c\"\nend\n".to_vec(),
    );
    tree.insert(
        PathBuf::from("app/models/concerns/greeting.rb"),
        b"module Greeting\n  def greet\n    \"hi\"\n  end\nend\n".to_vec(),
    );
    let mut app = ingest_app_from_tree(tree).expect("ingest");
    let mut analyzer = Analyzer::new(&app);
    analyzer.analyze(&mut app);
    app
}

fn reported(app: &roundhouse::App, target: BuildTarget) -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tiny-blog");
    let (_, diags) = roundhouse::emit::diagnostics::scope(|| target_files(app, &root, target));
    diags
        .into_iter()
        .filter(|d| d.message.contains("plain Ruby class"))
        .map(|d| d.message)
        .collect()
}

#[test]
fn a_plain_class_is_unsupported_on_the_targets_that_do_not_emit_it() {
    let app = app();
    for target in [
        BuildTarget::Rust,
        BuildTarget::Python,
        BuildTarget::CSharp,
        BuildTarget::Go,
        BuildTarget::Kotlin,
        BuildTarget::Swift,
        BuildTarget::Crystal,
        BuildTarget::Elixir,
    ] {
        let msgs = reported(&app, target);
        assert_eq!(msgs.len(), 1, "{}: {msgs:?}", target.as_str());
        assert!(msgs[0].contains("class `Probe`"), "{}", msgs[0]);
        assert!(msgs[0].contains(&format!("({})", target.as_str())), "{}", msgs[0]);
    }
}

#[test]
fn the_targets_that_emit_a_plain_class_do_not_report_it() {
    let app = app();
    for target in [BuildTarget::Ruby, BuildTarget::Spinel, BuildTarget::Typescript] {
        assert_eq!(reported(&app, target), Vec::<String>::new(), "{}", target.as_str());
    }
}
