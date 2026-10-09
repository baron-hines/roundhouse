//! A parenthesized group among multi-write targets — campfire's
//! `ResponseCache#write` evicts with `_, (_, removed_size) = @entries.shift`.
use super::emit_and_run;

fn probe() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        "app/models/nested_write_probe.rb",
        r#"class NestedWriteProbe
  def initialize
    @entries = { "first" => ["a", 3], "second" => ["b", 5] }
    @bytes = 8
  end

  def evict
    _, (_, removed_size) = @entries.shift
    @bytes -= removed_size
    @bytes
  end

  def remaining
    @entries.keys
  end

  def deep
    a, (b, (c, d)) = 1, [2, [3, 4]]
    [a, b, c, d]
  end

  def padded
    a, (b, c) = 1, 2
    [a, b, c]
  end

  def value
    (x, (y, z) = [1, [2, 3]])
  end
end
"#,
    )
}

const ASSERTIONS: &str = r#"
probe = NestedWriteProbe.new
sizes = [probe.evict, probe.evict]
raise "wrong evicted sizes: #{sizes.inspect}" unless sizes == [5, 0]
raise "wrong remaining: #{probe.remaining.inspect}" unless probe.remaining == []
raise "wrong deep: #{probe.deep.inspect}" unless probe.deep == [1, 2, 3, 4]
raise "wrong padded: #{probe.padded.inspect}" unless probe.padded == [1, 2, nil]
raise "wrong value: #{probe.value.inspect}" unless probe.value == [1, [2, 3]]
puts "nested multi-write contract passed"
"#;

/// Each group destructures its own element, a scalar group value pads with
/// nil as Ruby's does, and the whole assignment evaluates to its RHS.
#[test]
fn nested_multi_write_destructures_each_group() {
    let run = probe().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("nested multi-write contract passed"), "{}", run.stdout);
}
