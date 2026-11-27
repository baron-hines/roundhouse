//! A written superclass or `include` keeps the constant Ruby binds it
//! to when Rubydex's answer for the name would be a different one.
//!
//! Two shapes that ingest used to leave or rewrite wrong:
//!
//! - `include Testing::Assertions` inside `module ActiveSupport` names
//!   `ActiveSupport::Testing::Assertions` when the app declares
//!   `ActiveSupport::Testing` and a gem completes the name: Ruby binds the
//!   first segment lexically. Rubydex has no namespace for the whole name,
//!   so it was stored as written, which reads as the top-level
//!   `Testing::Assertions`;
//! - a scoped superclass `Graph::Schema::Object` never names the top-level
//!   `Object`: Ruby does not search `Object` for a scoped lookup (a
//!   NameError since 2.5). Rubydex answered `Object` when the app reopens
//!   `Graph::Schema` without declaring `Object`, and the class lost its
//!   real parent.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::BuildTarget;

const TESTING_OURS: &str = "module ActiveSupport\n  module Testing\n    module Ours\n    end\n  end\nend\n";

const PROBE: &str = r#"
module ActiveSupport
  class Probe
    include Testing::Assertions
    include Testing::Ours
  end
end
"#;

/// The shadowing scope is the app's own `Outer::Gemmed`; `Mixin` is a gem's.
const HOLDER: &str = r#"
module Outer
  module Gemmed
    class Holder
      include Gemmed::Mixin
    end
  end
end
"#;

/// The app reopens `Graph::Schema` (a gem's) and declares no `Object` in it.
const GRAPH_REOPEN: &str = "module Graph\n  class Schema\n    class Scalar\n    end\n  end\nend\n";

/// An app class that also declares namesakes of two framework roots, in
/// files that may load after the class that names the framework's.
const BILLING_RAILTIE: &str = r#"
class Billing
  class Railtie < Rails::Railtie
  end

  class Store < ActiveSupport::Cache::Store
    include ActiveSupport::Rescuable
  end
end
"#;
const BILLING_RAILS: &str = "class Billing\n  module Rails\n    class Serializer\n    end\n  end\nend\n";
const BILLING_ACTIVE_SUPPORT: &str = "class Billing\n  module ActiveSupport\n    class Serializer\n    end\n  end\nend\n";

const BOX: &str = "class Box < Graph::Schema::Object\nend\n";

fn app(files: &[(&str, &str)]) -> roundhouse::App {
    let tree: HashMap<PathBuf, Vec<u8>> = [
        ("db/schema.rb", "ActiveRecord::Schema.define do\nend\n"),
        ("config/routes.rb", "Rails.application.routes.draw do\nend\n"),
        ("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\nend\n"),
    ]
    .iter()
    .chain(files)
    .map(|(p, c)| (PathBuf::from(p), c.as_bytes().to_vec()))
    .collect();
    let mut app = ingest_app_from_tree(tree).expect("ingest tree");
    roundhouse::session::analyze_and_lower(&mut app);
    app
}

fn class<'a>(app: &'a roundhouse::App, name: &str) -> &'a roundhouse::dialect::LibraryClass {
    app.library_classes.iter().find(|c| c.name.0.as_str() == name).unwrap_or_else(|| panic!("no library class {name}"))
}

fn includes(app: &roundhouse::App, name: &str) -> Vec<String> {
    class(app, name).includes.iter().map(|i| i.0.as_str().to_string()).collect()
}

#[test]
fn an_include_through_an_app_namespace_keeps_that_namespace() {
    let app = app(&[
        ("app/services/active_support/testing/ours.rb", TESTING_OURS),
        ("app/services/active_support/probe.rb", PROBE),
        ("app/services/outer/gemmed/holder.rb", HOLDER),
    ]);
    assert_eq!(includes(&app, "ActiveSupport::Probe"), ["ActiveSupport::Testing::Assertions", "ActiveSupport::Testing::Ours"]);
    assert_eq!(includes(&app, "Outer::Gemmed::Holder"), ["Outer::Gemmed::Mixin"]);

    let files = roundhouse::project::target_files(&app, Path::new("nonexistent"), BuildTarget::Ruby).expect("target files");
    let text = |suffix: &str| {
        files.iter().find(|(path, _)| path.ends_with(suffix)).map(|(_, c)| c.clone()).unwrap_or_else(|| panic!("no emitted {suffix}"))
    };
    let probe = text("active_support/probe.rb");
    assert!(probe.contains("include ActiveSupport::Testing::Assertions"), "{probe}");
    let holder = text("outer/gemmed/holder.rb");
    assert!(holder.contains("include Outer::Gemmed::Mixin"), "{holder}");
}

#[test]
fn a_scoped_superclass_is_never_the_top_level_constant() {
    let app = app(&[("app/models/graph/schema/scalar.rb", GRAPH_REOPEN), ("app/models/box.rb", BOX)]);
    let parent = class(&app, "Box").parent.as_ref().map(|p| p.0.as_str().to_string());
    assert_eq!(parent.as_deref(), Some("Graph::Schema::Object"));
}

#[test]
fn a_framework_rooted_name_is_not_bound_to_an_app_namesake() {
    let app = app(&[
        ("app/services/billing/railtie.rb", BILLING_RAILTIE),
        ("app/services/billing/rails/serializer.rb", BILLING_RAILS),
        ("app/services/billing/active_support/serializer.rb", BILLING_ACTIVE_SUPPORT),
    ]);
    let parent = |name: &str| class(&app, name).parent.as_ref().map(|p| p.0.as_str().to_string());
    // The Railtie is the framework's own subclass and is not modeled, but
    // it must not have been bound to `Billing::Rails::Railtie` on the way.
    assert!(app.library_classes.iter().all(|c| c.parent.as_ref().is_none_or(|p| !p.0.as_str().starts_with("Billing::Rails::"))));
    assert_eq!(parent("Billing::Store").as_deref(), Some("ActiveSupport::Cache::Store"));
    assert_eq!(includes(&app, "Billing::Store"), ["ActiveSupport::Rescuable"]);
}
