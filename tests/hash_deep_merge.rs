//! Rails 8.1.4's no-block Hash#deep_merge, and its mutating deep_merge! where
//! the receiver's type already holds the argument's keys and values.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const SOURCE: &str = r#"class HashDeepMergeProbe
  def self.run
    left = {
      nested: { keep: "left", conflict: "left", subtree: { old: true } },
      array: [1, 2],
      hash_to_scalar: { old: true },
      scalar_to_hash: "old",
      left_only: :left
    }
    right = {
      nested: { conflict: "right", added: "right", subtree: { new: true } },
      array: [3],
      hash_to_scalar: "right",
      scalar_to_hash: { added: true },
      right_only: :right
    }
    merged = left.deep_merge(right)
    expected = {
      nested: { keep: "left", conflict: "right", added: "right", subtree: { old: true, new: true } },
      array: [3],
      hash_to_scalar: "right",
      scalar_to_hash: { added: true },
      left_only: :left,
      right_only: :right
    }
    raise "wrong merged value: #{merged.inspect}" unless merged == expected
    raise "left input mutated" unless left[:nested][:conflict] == "left" && left[:array] == [1, 2]
    raise "right input mutated" unless right[:nested][:conflict] == "right" && right[:array] == [3]
    raise "empty-right merge changed content" unless left.deep_merge({}) == left
    raise "empty-left merge changed content" unless ({}).deep_merge(right) == right
    raise "empty merge changed content" unless ({}).deep_merge({}) == {}
    raise "merge returned receiver" if left.deep_merge(right).equal?(left)
    defaulted_left = Hash.new({ nested: { default: true } })
    defaulted_right = { missing: { nested: { right: true } } }
    raise "merged an absent key's default" unless defaulted_left.deep_merge(defaulted_right) == defaulted_right
    puts "Hash#deep_merge caller contract passed"
    BangProbe.run
  end
end

class BangProbe
  def self.run
    inner = { a: 1 }
    target = { nested: inner, keep: { k: 1 } }
    extra = { nested: { b: 2 }, added: { z: 3 } }
    answer = target.deep_merge!(extra)
    raise "bang answered another Hash" unless answer.equal?(target)
    raise "bang value: #{target.inspect}" unless target == { nested: { a: 1, b: 2 }, keep: { k: 1 }, added: { z: 3 } }
    raise "nested Hash mutated in place" unless inner == { a: 1 }
    raise "nested Hash not replaced" if target[:nested].equal?(inner)
    raise "key order" unless target.keys == [:nested, :keep, :added]
    puts "Hash#deep_merge! caller contract passed"
  end
end
"#;

const ASSERTIONS: &str = r#"HashDeepMergeProbe.run
"#;

#[test]
fn emitted_cruby_caller_observes_deep_merge_contract() {
    let run = emit_and_run::real_blog()
        .write("app/models/hash_deep_merge_probe.rb", SOURCE)
        .run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(
        run.stdout
            .contains("Hash#deep_merge caller contract passed")
    );
    assert!(run.stdout.contains("Hash#deep_merge! caller contract passed"));
    let emitted = std::fs::read_to_string(run.emitted.join("app/models/hash_deep_merge_probe.rb"))
        .expect("emitted HashDeepMergeProbe");
    assert!(
        emitted.contains("ActiveSupport.deep_merge(left, right)"),
        "{emitted}"
    );
    assert!(!emitted.contains("left.deep_merge(right)"), "{emitted}");
    let bang = std::fs::read_to_string(run.emitted.join("app/models/bang_probe.rb")).expect("emitted BangProbe");
    assert!(bang.contains("ActiveSupport.deep_merge!(target, extra)"), "{bang}");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn emitted_spinel_caller_observes_deep_merge_contract_natively() {
    let run = emit_and_run::real_blog()
        .write("app/models/hash_deep_merge_probe.rb", SOURCE)
        .run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(
        run.stdout
            .contains("Hash#deep_merge caller contract passed")
    );
    assert!(run.stdout.contains("Hash#deep_merge! caller contract passed"));
}

#[test]
fn deep_merge_block_and_widening_bang_forms_remain_unsupported() {
    for (name, expected_construct, expression) in [
        (
            "block",
            "Hash#deep_merge conflict block",
            "left.deep_merge(right) { |_key, old, new| new }",
        ),
        ("bang_widening", "Hash#deep_merge!", "left.deep_merge!({ key: \"two\" })"),
        ("bang_block", "Hash#deep_merge!", "left.deep_merge!(right) { |_key, old, new| new }"),
    ] {
        let (_, _, diagnostics) = emit_and_run::real_blog()
            .write(
                "app/models/hash_deep_merge_unsupported.rb",
                &format!(
                    "class HashDeepMergeUnsupported\n  def self.{}\n    left = {{ key: 1 }}\n    right = {{ key: 2 }}\n    {}\n  end\nend\n",
                    name, expression
                ),
            )
            .emit_with_app(roundhouse::project::BuildTarget::Ruby);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == roundhouse::diagnostic::Severity::Error)
            .filter(|diagnostic| {
                matches!(
                    &diagnostic.kind,
                    roundhouse::diagnostic::DiagnosticKind::Unsupported { construct, .. }
                        if construct.as_str() == expected_construct
                )
            })
            .collect();
        assert_eq!(
            errors.len(),
            1,
            "{expected_construct} diagnostic identity/count: {diagnostics:#?}"
        );
    }
}
