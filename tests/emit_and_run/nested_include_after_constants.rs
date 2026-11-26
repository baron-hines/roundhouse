//! A model that includes a concern nested under its own name, below a
//! constant the concern's module body reads at load (`TYPES = …` above
//! `include Stamping`, `KINDS = Ledger::TYPES.map(&:upcase)` in
//! `Ledger::Stamping`). Rails loads the concern when the `include` names
//! it, after `TYPES` is set. The emitted file closes the class after the
//! constant, requires the concern's file at the top level, and reopens the
//! class for the `include`, the split a constant initializer already gets
//! when the file it requires reads an earlier constant.

use roundhouse::build_target::BuildTarget;

use super::emit_and_run;

const SCHEMA: &str = "ActiveRecord::Schema.define do\n  create_table :ledgers do |t|\n    t.string :name\n  end\nend\n";

fn app(ledger: &str, stamping: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        )
        .write(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\nend\n",
        )
        .write("db/schema.rb", SCHEMA)
        .write("app/models/ledger.rb", ledger)
        .write("app/models/ledger/stamping.rb", stamping)
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

const LEDGER: &str = "class Ledger < ApplicationRecord\n  TYPES = %w[a b].freeze\n  include Stamping\n\n  def label\n    \"#{name}:#{kinds.join(',')}\"\n  end\nend\n";

const READING_STAMPING: &str = "module Ledger::Stamping\n  extend ActiveSupport::Concern\n  KINDS = Ledger::TYPES.map(&:upcase)\n\n  def kinds = KINDS\nend\n";

const INERT_STAMPING: &str =
    "module Ledger::Stamping\n  extend ActiveSupport::Concern\n\n  def kinds = %w[A B]\nend\n";

fn ledger_file(tree: &std::path::Path) -> String {
    std::fs::read_to_string(tree.join("app/models/ledger.rb")).expect("app/models/ledger.rb is in the tree")
}

fn line_of(text: &str, pred: impl Fn(&str) -> bool) -> Option<usize> {
    text.lines().position(pred)
}

#[test]
fn a_nested_concern_reading_an_earlier_constant_runs_on_ruby() {
    app(LEDGER, READING_STAMPING)
        .run_ruby(
            r#"
p Ledger.new.kinds
raise "kinds: #{Ledger.new.kinds.inspect}" unless Ledger.new.kinds == ["A", "B"]
raise "label: #{Ledger.new(name: "q1").label.inspect}" unless Ledger.new(name: "q1").label == "q1:A,B"
raise "superclass: #{Ledger.superclass}" unless Ledger.superclass == ApplicationRecord
raise "ancestors" unless Ledger.include?(Ledger::Stamping)
puts "nested concern after the includer's constant passed"
"#,
        )
        .assert_passes();
}

#[test]
fn ledger_loaded_first_reads_the_concern_s_constant_on_ruby() {
    let (tree, errors) = app(LEDGER, READING_STAMPING).emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let out = std::process::Command::new("ruby")
        .current_dir(&*tree)
        .args(["-e", "require_relative 'app/models/ledger'; p Ledger::Stamping::KINDS"])
        .output()
        .expect("run ruby");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.trim() == r#"["A", "B"]"#,
        "ledger.rb as the entry did not load:\n{stdout}\n{}\n{}",
        String::from_utf8_lossy(&out.stderr),
        ledger_file(&tree)
    );
}

#[test]
fn the_spinel_tree_requires_the_concern_after_the_constant_it_reads() {
    let (tree, errors) = app(LEDGER, READING_STAMPING).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let text = ledger_file(&tree);
    let types = line_of(&text, |l| l.trim_start().starts_with("TYPES = "));
    let require = line_of(&text, |l| l == "require_relative \"ledger/stamping\"");
    let include = line_of(&text, |l| l.trim() == "include Ledger::Stamping");
    assert!(
        matches!((types, require, include), (Some(t), Some(r), Some(i)) if t < r && r < i),
        "the concern is not required (top level) and included after TYPES:\n{text}"
    );
    assert!(
        !text.lines().any(|l| l.starts_with(' ') && l.trim_start().starts_with("require_relative")),
        "a require is still in the class body:\n{text}"
    );
}

/// A concern that reads none of the includer's constants keeps one class
/// body, with its require above the class (#2310) and its `include` first.
#[test]
fn a_nested_concern_reading_no_includer_constant_keeps_one_class_body() {
    let (tree, errors) = app(LEDGER, INERT_STAMPING).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let text = ledger_file(&tree);
    assert_eq!(
        text.lines().filter(|l| l.starts_with("class Ledger")).count(),
        1,
        "the class was split:\n{text}"
    );
    let require = line_of(&text, |l| l == "require_relative \"ledger/stamping\"");
    let class = line_of(&text, |l| l.starts_with("class Ledger"));
    let include = line_of(&text, |l| l == "  include Ledger::Stamping");
    let types = line_of(&text, |l| l.starts_with("  TYPES = "));
    assert!(
        matches!((require, class, include, types), (Some(r), Some(c), Some(i), Some(t)) if r < c && c < i && i < t),
        "the output changed for a concern that reads no includer constant:\n{text}"
    );
}

/// The constant read is written BELOW the include: Rails raises there too,
/// and the emit keeps one class body.
#[test]
fn a_constant_below_the_include_does_not_split_the_class() {
    let ledger = "class Ledger < ApplicationRecord\n  include Stamping\n  TYPES = %w[a b].freeze\nend\n";
    let (tree, errors) = app(ledger, READING_STAMPING).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let text = ledger_file(&tree);
    assert_eq!(
        text.lines().filter(|l| l.starts_with("class Ledger")).count(),
        1,
        "the class was split:\n{text}"
    );
}

/// Includes around the late one keep the source's ancestors: the ones
/// written after it move with it, the ones before it stay first.
#[test]
fn includes_around_a_late_concern_keep_their_order_on_ruby() {
    let ledger = "class Ledger < ApplicationRecord\n  include Alpha\n  TYPES = %w[a b].freeze\n  include Stamping\n  include Beta\nend\n";
    app(ledger, READING_STAMPING)
        .write("app/models/alpha.rb", "module Alpha\n  def alpha = 1\nend\n")
        .write("app/models/beta.rb", "module Beta\n  def beta = 2\nend\n")
        .run_ruby(
            r#"
order = Ledger.ancestors.select { |a| [Alpha, Beta, Ledger::Stamping].include?(a) }
raise "ancestors: #{order.inspect}" unless order == [Beta, Ledger::Stamping, Alpha]
raise "kinds: #{Ledger.new.kinds.inspect}" unless Ledger.new.kinds == ["A", "B"]
puts "includes around a late concern passed"
"#,
        )
        .assert_passes();
}

/// A library module with source ivar initializers writes a hooked include
/// where the source has it, after the `def self.mark` the hook calls. The
/// concern's require must stay at the class top for it: the late-require
/// split must not drop it for an include it never moves. (The concern's
/// read of `TYPES` still runs before `TYPES` is written there, as on main:
/// only the require is pinned, not a Ruby run.)
#[test]
fn an_ordered_bodys_hooked_include_keeps_its_class_top_require() {
    let ledger = "module Ledger\n  @registry = []\n  TYPES = %w[a b].freeze\n\n  def self.mark\n    @registry << :marked\n  end\n\n  def self.registry = @registry\n\n  include Stamping\nend\n";
    let stamping = "module Ledger::Stamping\n  extend ActiveSupport::Concern\n  KINDS = Ledger::TYPES.map(&:upcase)\n\n  def self.included(base)\n    base.mark\n  end\nend\n";
    let build = || {
        emit_and_run::empty_app()
            .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
            .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
            .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
            .write("app/models/ledger.rb", ledger)
            .write("app/models/ledger/stamping.rb", stamping)
            .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
    };
    let (tree, errors) = build().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let text = std::fs::read_to_string(tree.join("app/models/ledger.rb")).expect("app/models/ledger.rb is in the tree");
    assert!(
        text.lines().any(|l| l.trim_start().starts_with("require_relative") && l.contains("ledger/stamping")),
        "the concern is not required anywhere:\n{text}"
    );
}

/// The includes come from two files: the model's file writes the late
/// concern and `include Hooky`, and `Hooky`'s hook writes `include Gamma`
/// in hooky.rb. `Gamma`'s span has no place in the model's file, so it
/// follows the include before it: the moved includes keep Ruby's order,
/// and the method two of them define resolves as in Ruby.
#[test]
fn includes_spanning_two_files_keep_their_order_on_ruby() {
    // `Hooky`'s `included` hook adds `include Gamma` to the class; that
    // include's span sits in hooky.rb, at an offset below the late
    // `include Stamping` in ledger.rb. Offsets from two files say nothing
    // about their order: Gamma follows Hooky, after the constant.
    let ledger = "# Ledger entries.\n# Kinds come from TYPES.\n# Tags come from the concerns.\nclass Ledger < ApplicationRecord\n  TYPES = %w[a b].freeze\n  include Stamping\n  include Hooky\nend\n";
    let stamping = "module Ledger::Stamping\n  extend ActiveSupport::Concern\n  KINDS = Ledger::TYPES.map(&:upcase)\n\n  def kinds = KINDS\n  def tag = :stamping\nend\n";
    let build = || {
        app(ledger, stamping)
            .write("app/models/hooky.rb", "module Hooky\n  extend ActiveSupport::Concern\n  included { include Gamma }\nend\n")
            .write("app/models/gamma.rb", "module Gamma\n  def tag = :gamma\nend\n")
    };
    let (tree, errors) = build().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let text = ledger_file(&tree);
    let at = |needle: &str| line_of(&text, |l| l.trim() == needle).unwrap_or_else(|| panic!("{needle} missing:\n{text}"));
    assert!(at("TYPES = %w[a b].freeze") < at("include Ledger::Stamping"), "{text}");
    assert!(at("include Ledger::Stamping") < at("include Hooky") && at("include Hooky") < at("include Gamma"), "{text}");
    build()
        .run_ruby(
            r#"
order = Ledger.ancestors.select { |a| [Gamma, Hooky, Ledger::Stamping].include?(a) }
raise "ancestors: #{order.inspect}" unless order == [Gamma, Hooky, Ledger::Stamping]
raise "tag: #{Ledger.new.tag.inspect}" unless Ledger.new.tag == :gamma
raise "kinds: #{Ledger.new.kinds.inspect}" unless Ledger.new.kinds == ["A", "B"]
puts "includes across files passed"
"#,
        )
        .assert_passes();
}

/// The concern reads the includer's constant rooted (`::Ledger::TYPES`):
/// the same read as `Ledger::TYPES`, so the concern still loads after it.
#[test]
fn a_nested_concern_reading_a_rooted_includer_constant_runs_on_ruby() {
    let stamping = READING_STAMPING.replace("= Ledger::TYPES", "= ::Ledger::TYPES");
    app(LEDGER, &stamping)
        .run_ruby(
            r#"
raise "kinds: #{Ledger.new.kinds.inspect}" unless Ledger.new.kinds == ["A", "B"]
puts "rooted constant read passed"
"#,
        )
        .assert_passes();
}
