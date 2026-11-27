//! A whole-program fixpoint whose entries move between values forever
//! ends on a fixed point rather than on a round cap (`analyze::oscillation`).
//!
//! A constructor called with readers of its own instances flips the reader
//! between `Integer` and `Integer | nil` every round, and a recursive copy
//! reshapes its return every round. Each ran every loop to `FIXPOINT_CAP`.

use std::path::PathBuf;

use roundhouse::analyze::{Analyzer, FixpointRounds, LoopEnd};
use roundhouse::ident::{ClassId, Symbol};
use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::ty::Ty;

const REGEN: &str = r#"
class Chunk
  attr_accessor :start
  attr_accessor :lines

  def initialize(start:, lines:)
    @start = start
    @lines = lines
  end
end

class Regen
  def self.run(chunks, all)
    mapping = {}
    chunks.map do |c|
      Chunk.new(start: mapping[c.start] || c.start, lines: all[c.start - 1, c.lines.length] || [])
    end
  end

  def self.seed
    run([Chunk.new(start: 1, lines: ["a"])], ["x", "y"])
  end
end
"#;

const COPIER: &str = r#"
class Copier
  def self.deep_copy(value)
    case value
    when Hash
      value.each_with_object({}) { |(k, v), out| out[k] = deep_copy(v) }
    when Array
      value.map { |v| deep_copy(v) }
    else
      value
    end
  end

  def self.run
    deep_copy({ "a" => ["b", { "c" => "d" }] })
  end
end
"#;

const PLAIN: &str = r#"
class Point
  attr_reader :x

  def initialize(x)
    @x = x
  end
end

class Plot
  def self.run
    Point.new(1).x + Point.new(2).x
  end
end
"#;

fn analyze(src: &str, method: (&str, &str, bool)) -> (FixpointRounds, Ty) {
    let tree = [(PathBuf::from("app/lib/probe.rb"), src.as_bytes().to_vec())]
        .into_iter()
        .collect();
    let mut app = ingest_app_from_tree(tree).expect("ingest");
    let mut analyzer = Analyzer::new(&app);
    analyzer.analyze(&mut app);
    let (class, name, class_side) = method;
    let info = analyzer
        .class_registry()
        .get(&ClassId(Symbol::from(class)))
        .expect("registered");
    let table = if class_side {
        &info.class_methods
    } else {
        &info.instance_methods
    };
    let ty = table
        .get(&Symbol::from(name))
        .cloned()
        .expect("method registered");
    (analyzer.fixpoint_rounds(), ty)
}

fn arms(ty: &Ty) -> Vec<Ty> {
    match ty {
        Ty::Union { variants } => variants.iter().cloned().collect(),
        other => vec![other.clone()],
    }
}

fn assert_no_cap(rounds: FixpointRounds) {
    assert!(
        rounds.production != LoopEnd::RanToCap && rounds.absorb != LoopEnd::RanToCap,
        "a loop stopped on its round cap: {rounds:?}"
    );
}

#[test]
fn a_reader_fed_back_into_its_constructor_settles_on_the_join() {
    let (rounds, start) = analyze(REGEN, ("Chunk", "start", false));
    assert_no_cap(rounds);
    let arms = arms(&start);
    assert!(
        arms.contains(&Ty::Int) && arms.contains(&Ty::Nil) && arms.len() == 2,
        "{start:?}"
    );
}

#[test]
fn a_recursive_copy_settles_and_keeps_its_spine() {
    let (rounds, copy) = analyze(COPIER, ("Copier", "deep_copy", true));
    assert_no_cap(rounds);
    let shown = format!("{copy:?}");
    assert!(shown.contains("Hash") && shown.contains("Array"), "{shown}");
}

#[test]
fn a_reader_that_settles_on_its_own_keeps_its_type() {
    let (rounds, x) = analyze(PLAIN, ("Point", "x", false));
    assert_no_cap(rounds);
    assert_eq!(x, Ty::Int);
}

/// Views move no production signature here, so the absorb loop never runs
/// and its one unchecked harvest is the last. The body re-derives a narrower
/// return than the join each round, so nothing later restores the pin.
#[test]
fn the_harvest_that_replaces_the_absorb_loop_keeps_a_pinned_return() {
    let (rounds, copy) = analyze(COPIER, ("Copier", "deep_copy", true));
    assert_eq!(rounds.absorb, LoopEnd::NotRun, "{rounds:?}");
    assert_eq!(rounds.restamp.passes, 0, "{rounds:?}");
    let kept = arms(&copy)
        .iter()
        .any(|arm| matches!(arm, Ty::Array { elem } if !matches!(**elem, Ty::Untyped)));
    assert!(kept, "{copy:?}");
}
