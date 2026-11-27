//! A method a class macro defines is a plain method.
//!
//! `expand_class_macros` builds each `define_method` it follows as a
//! `MethodDef`. It once copied the kind (reader, writer, plain) from
//! whichever class method the ancestors' table happened to yield first, a
//! hash order that changes per process: `timestamps :created_at` came out
//! as `def created_at` in most runs and as an `attr_writer` in the RBS of
//! some, with the `.rb` unchanged (the emitted `sig/` differed between runs).

use std::path::PathBuf;

use roundhouse::dialect::{AccessorKind, MethodReceiver};
use roundhouse::ingest::ingest_app_from_tree;

const BASE: &str = r##"class Stamped
  class << self
    attr_writer :tag
    attr_accessor :registry

    def timestamps(*symbols)
      symbols.each do |sym|
        define_method(sym) do
          attributes[sym]
        end
      end
    end
  end
end
"##;

const CARD: &str = r##"class Card < Stamped
  timestamps :created_at, :updated_at
end
"##;

#[test]
fn a_macro_defined_method_is_a_plain_method_whatever_class_methods_the_table_holds() {
    // Each ingest builds its tables with a new hash seed: the table order is not the same twice.
    for round in 0..40 {
        let tree = vec![
            (PathBuf::from("lib/stamped.rb"), BASE.as_bytes().to_vec()),
            (PathBuf::from("lib/card.rb"), CARD.as_bytes().to_vec()),
        ]
        .into_iter()
        .collect();
        let app = ingest_app_from_tree(tree).expect("ingest");
        let card = app.library_classes.iter().find(|lc| lc.name.0.as_str() == "Card").expect("Card ingested");
        let defined: Vec<_> = card.methods.iter().filter(|m| m.receiver == MethodReceiver::Instance).collect();
        assert_eq!(
            defined.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            ["created_at", "updated_at"],
            "round {round}: the macro's methods"
        );
        for method in defined {
            assert_eq!(method.kind, AccessorKind::Method, "round {round}: {}", method.name.as_str());
        }
    }
}
