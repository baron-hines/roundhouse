//! A predicate Ruby's String really has is never a StringInquirer label.
//!
//! The inquiry fold turns `<String>.<label>?` into `<String> == "<label>"`
//! when no String answers `<label>?`. `valid_encoding?` and
//! `unicode_normalized?` were absent from the String catalog, so
//! `value.start_with?(prefix) && value.valid_encoding?` emitted
//! `value == "valid_encoding"`, false for every string.

fn emitted(source: &str) -> String {
    use std::collections::HashMap;
    use std::path::PathBuf;
    let files: HashMap<PathBuf, Vec<u8>> = [
        (PathBuf::from("db/schema.rb"), b"ActiveRecord::Schema.define do\n  create_table \"regions\", force: :cascade do |t|\n    t.string \"code\"\n  end\nend\n".to_vec()),
        (PathBuf::from("app/models/region.rb"), b"class Region < ApplicationRecord\nend\n".to_vec()),
        (PathBuf::from("app/models/string_check.rb"), source.as_bytes().to_vec()),
    ].into_iter().collect();
    let mut app = roundhouse::ingest::ingest_app_from_tree(files).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let mut files = roundhouse::emit::ruby::emit_lowered_models(&app);
    files.extend(roundhouse::emit::ruby::emit_library(&app));
    files.iter().find(|f| f.path.ends_with("string_check.rb")).expect("string_check.rb").content.clone()
}

#[test]
fn string_predicates_ruby_defines_stay_method_calls() {
    let out = emitted(
        r#"
class StringCheck
  #: (Object value, String prefix) -> bool
  def valid?(value, prefix)
    case value
    when String
      value.start_with?(prefix) && value.valid_encoding?
    else
      false
    end
  end

  #: (String value) -> bool
  def encoded?(value)
    value.valid_encoding?
  end

  #: (String value) -> bool
  def normal?(value)
    value.unicode_normalized?
  end

  #: (String value) -> bool
  def plain?(value)
    value.ascii_only?
  end
end
"#,
    );
    for label in ["valid_encoding", "unicode_normalized", "ascii_only"] {
        assert!(!out.contains(&format!("== \"{label}\"")), "{label}? was folded into a comparison:\n{out}");
        assert!(out.contains(&format!("{label}?")), "{label}? is gone:\n{out}");
    }
}
