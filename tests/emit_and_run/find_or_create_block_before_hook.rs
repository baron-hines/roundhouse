//! `find_or_create_by(attrs) { |record| … }` on a model with an `after_initialize` callback: Rails' `new` assigns the
//! attributes, yields the record to the block, then runs the initialization callbacks, so the callback sees what the
//! block set. lobsters' `Origin.find_or_create_by(identifier:) { |o| o.domain ||= self }` beside the Token concern's
//! `after_initialize` was refused: the inlined factory called `new`, which ran the callback, before the block.

use super::emit_and_run;

#[path = "../support/prerequisite.rs"]
mod prerequisite;

const SCHEMA: &str = "ActiveRecord::Schema.define do\ncreate_table(:badges) { |t| t.string :name; t.string :label; t.string :token }\nend\n";

const MODEL: &str = r##"class Badge < ActiveRecord::Base
  after_initialize do
    self.token ||= "t-#{label}" if new_record?
  end
end
"##;

const PROBE: &str = r##"class BadgeProbe
  def self.make(name)
    badge = Badge.find_or_create_by(name: name) { |b| b.label = "from block" }
    [badge.name, badge.label, badge.token, badge.persisted?].join(",")
  end
end
"##;

const SCRIPT: &str = r##"first = BadgeProbe.make("a")
again = BadgeProbe.make("a")
out = "#{first} #{again} #{Badge.count}"
"##;

const EXPECTED: &str = "a,from block,t-from block,true a,from block,t-from block,true 1";

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("app/models/badge.rb", MODEL)
        .write("lib/badge_probe.rb", PROBE)
}

fn script() -> String {
    format!("{SCRIPT}raise \"badges: #{{out}}\" unless out == {EXPECTED:?}\nputs \"find_or_create block passed\"\n")
}

#[test]
fn a_creation_block_runs_before_the_initialization_callbacks() {
    app().run_ruby(&script()).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_creation_block_runs_before_the_initialization_callbacks_on_spinel() {
    app().run_spinel_with_db(&script()).assert_passes();
}

#[test]
fn a_creation_block_order_matches_rails() {
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/rails_oracle.rb");
    let script = format!(
        r##"require {helper:?}
unless RailsOracle.available?("activerecord", sqlite3: ">= 2.9")
  warn "skipping find_or_create oracle: #{{RailsOracle.label}} needs activerecord and sqlite3"
  exit 77
end
RailsOracle.activate("activerecord", sqlite3: ">= 2.9")
require "active_record"
ActiveRecord::Base.logger = nil
ActiveRecord::Migration.verbose = false
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
{SCHEMA}{MODEL}{PROBE}{SCRIPT}print out
"##
    );
    let output = std::process::Command::new("ruby").args(["-e", &script]).output().expect("Rails oracle");
    if output.status.code() == Some(77) {
        prerequisite::missing(String::from_utf8_lossy(&output.stderr).trim());
        return;
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
}

/// A private or protected `def after_initialize` the factory cannot call on the record, and an ancestor's `initialize` that runs
/// before the block could: both keep the refusal.
#[test]
fn a_private_hook_or_an_ancestors_initialize_keeps_the_refusal() {
    let private_hook = "class Badge < ActiveRecord::Base\n  private\n\n  def after_initialize\n    self.token ||= \"t\"\n  end\nend\n";
    let ancestor = "class Base < ActiveRecord::Base\n  self.abstract_class = true\n\n  def initialize(attrs = {})\n    super\n  end\nend\n";
    let child = MODEL.replace("class Badge < ActiveRecord::Base", "class Badge < Base");
    let protected_hook = private_hook.replace("  private\n", "  protected\n");
    for (model, base) in [(private_hook.to_string(), None), (protected_hook, None), (child, Some(ancestor))] {
        let mut overlay = app().write("app/models/badge.rb", &model);
        if let Some(base) = base {
            overlay = overlay.write("app/models/base.rb", base);
        }
        let (_, errors) = overlay.emit(roundhouse::project::BuildTarget::Ruby);
        assert!(errors.iter().any(|e| e.contains("creation block must run inside initialize")), "{model}: {errors:?}");
    }
}
