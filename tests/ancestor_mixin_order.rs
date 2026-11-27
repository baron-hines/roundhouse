//! Rubydex answers nothing for a bare mixin of a class that includes more
//! than one (Shopify/rubydex#1106), so `ingest::written_names` finds it as
//! Ruby's ancestor lookup does. Its order: a class's include calls last
//! first, a call's arguments in order (`include A, B` puts `A` first), each
//! module's own includes before the next module, and for the class itself
//! only the calls above the one being resolved (that last rule is pinned
//! by the unit tests in `src/ingest/written_names.rs`: on these shapes
//! Rubydex answers the current call's names itself).

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::ingest::ingest_app_from_tree;

fn app(files: &[(&str, &str)]) -> roundhouse::App {
    let tree: HashMap<PathBuf, Vec<u8>> = [
        ("db/schema.rb", "ActiveRecord::Schema.define(version: 1) do\nend\n"),
        ("config/routes.rb", "Rails.application.routes.draw do\nend\n"),
    ]
    .iter()
    .chain(files.iter())
    .map(|(p, c)| (PathBuf::from(p), c.as_bytes().to_vec()))
    .collect();
    ingest_app_from_tree(tree).expect("ingest tree")
}

fn includes(app: &roundhouse::App, class: &str) -> Vec<String> {
    app.library_classes
        .iter()
        .find(|lc| lc.name.0.as_str() == class)
        .unwrap_or_else(|| panic!("no library class {class}"))
        .includes
        .iter()
        .map(|c| c.0.as_str().to_string())
        .collect()
}

// Each shape is the one Rubydex misses: a compact `class Shop::X < Shop` including modules declared compactly
// under the superclass.
const SHOP: &[(&str, &str)] = &[
    ("app/models/shop.rb", "class Shop\nend\n"),
    ("app/models/shop/kind.rb", "module Shop::Kind\nend\n"),
    ("app/models/shop/tone.rb", "module Shop::Tone\nend\n"),
    ("app/models/shop/mood.rb", "module Shop::Mood\nend\n"),
    ("app/models/pack/a.rb", "module Pack\n  module A\n    module Hue\n      def hue = 1\n    end\n  end\nend\n"),
    ("app/models/pack/b.rb", "module Pack\n  module B\n    module Hue\n      def hue = 1\n    end\n  end\nend\n"),
    ("app/models/pack/inner.rb", "module Pack\n  module Inner\n    module Hue\n      def hue = 1\n    end\n  end\nend\n"),
    ("app/models/pack/outer.rb", "module Pack\n  module Outer\n    include Inner\n\n    def outer = 1\n  end\nend\n"),
    ("app/models/pack/other.rb", "module Pack\n  module Other\n    module Hue\n      def hue = 1\n    end\n  end\nend\n"),
];

fn shop(extra: &[(&str, &str)]) -> roundhouse::App {
    let files: Vec<(&str, &str)> = SHOP.iter().chain(extra.iter()).copied().collect();
    app(&files)
}

#[test]
fn a_calls_arguments_keep_their_order() {
    let app = shop(&[
        ("app/models/shop/base.rb", "class Shop::Base < Shop\n  include Pack::A, Pack::B\nend\n"),
        ("app/models/shop/sub.rb", "class Shop::Sub < Shop::Base\n  include Tone, Hue\nend\n"),
    ]);
    assert_eq!(includes(&app, "Shop::Sub")[1], "Pack::A::Hue");
}

#[test]
fn a_modules_own_includes_come_before_the_next_module() {
    let app = shop(&[
        ("app/models/shop/base.rb", "class Shop::Base < Shop\n  include Pack::Other\n  include Pack::Outer\nend\n"),
        ("app/models/shop/sub.rb", "class Shop::Sub < Shop::Base\n  include Tone, Hue\nend\n"),
    ]);
    assert_eq!(includes(&app, "Shop::Sub")[1], "Pack::Inner::Hue");
}
