//! Of two modules a controller reaches that define the same method, the
//! spliced one is the one Ruby's ancestors find first (#2137).
//!
//! Each `include` puts the module's ancestors directly above the class.
//! When that walk meets a module already there, it continues below it
//! instead of moving it. So among separate statements the later one comes
//! first, while `include A, B` keeps `A` ahead of `B`.
//!
//! A module that extends `ActiveSupport::Concern` defers its own includes
//! of other concerns: they are included into the class first, and then the
//! concern itself. That changes the answer when two concerns share a
//! dependency.
//!
//! The answers are Ruby 4.0.7's, with ActiveSupport 8.1.4.1, for the same
//! modules and includes outside Rails.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const CONCERNS: &[(&str, &str)] = &[
    ("first_label", "module FirstLabel\n  extend ActiveSupport::Concern\n\n  def label\n    \"first\"\n  end\nend\n"),
    ("second_label", "module SecondLabel\n  extend ActiveSupport::Concern\n\n  def label\n    \"second\"\n  end\nend\n"),
    ("third_label", "module ThirdLabel\n  extend ActiveSupport::Concern\n\n  def label\n    \"third\"\n  end\nend\n"),
    ("both_labels", "module BothLabels\n  extend ActiveSupport::Concern\n  include FirstLabel\n  include SecondLabel\nend\n"),
    // Plain modules: A, B, and AB including both.
    ("plain_a", "module PlainA\n  def label\n    \"plain a\"\n  end\nend\n"),
    ("plain_b", "module PlainB\n  def label\n    \"plain b\"\n  end\nend\n"),
    ("plain_ab", "module PlainAb\n  include PlainA\n  include PlainB\nend\n"),
    // The same three as concerns.
    ("concern_a", "module ConcernA\n  extend ActiveSupport::Concern\n\n  def label\n    \"concern a\"\n  end\nend\n"),
    ("concern_b", "module ConcernB\n  extend ActiveSupport::Concern\n\n  def label\n    \"concern b\"\n  end\nend\n"),
    ("concern_ab", "module ConcernAb\n  extend ActiveSupport::Concern\n  include ConcernA\n  include ConcernB\nend\n"),
    // A concern including a concern and a plain module.
    ("mixed_plain", "module MixedPlain\n  def label\n    \"mixed plain\"\n  end\nend\n"),
    ("mixed_concern", "module MixedConcern\n  extend ActiveSupport::Concern\n\n  def label\n    \"mixed concern\"\n  end\nend\n"),
    ("mixed_both", "module MixedBoth\n  extend ActiveSupport::Concern\n  include MixedConcern\n  include MixedPlain\nend\n"),
    // X includes C, B, A: its ancestors are [X, A, B, C]. B defines no
    // `label`, only a method of its own.
    ("cursor_a", "module CursorA\n  def label\n    \"cursor a\"\n  end\nend\n"),
    ("cursor_b", "module CursorB\n  def other_label\n    \"cursor b\"\n  end\nend\n"),
    ("cursor_c", "module CursorC\n  def label\n    \"cursor c\"\n  end\nend\n"),
    ("cursor_x", "module CursorX\n  include CursorC\n  include CursorB\n  include CursorA\nend\n"),
    // A module the superclass includes, and another.
    ("super_m", "module SuperM\n  def label\n    \"super m\"\n  end\nend\n"),
    ("super_n", "module SuperN\n  def label\n    \"super n\"\n  end\nend\n"),
    // A def written in `included do` is the includer's own.
    ("hook_def", "module HookDef\n  extend ActiveSupport::Concern\n\n  included do\n    def label\n      \"hook\"\n    end\n  end\nend\n"),
    ("plain_def", "module PlainDef\n  extend ActiveSupport::Concern\n\n  def label\n    \"plain\"\n  end\nend\n"),
    // `included do` delegates run in hook order: the last defines it.
    ("delegate_one", "module DelegateOne\n  extend ActiveSupport::Concern\n\n  included do\n    delegate :label, to: :source_one\n  end\nend\n"),
    ("delegate_two", "module DelegateTwo\n  extend ActiveSupport::Concern\n\n  included do\n    delegate :label, to: :source_two\n  end\nend\n"),
    // `self.extend ActiveSupport::Concern` is the same concern.
    ("self_a", "module SelfA\n  self.extend ActiveSupport::Concern\n\n  def label\n    \"self a\"\n  end\nend\n"),
    ("self_b", "module SelfB\n  self.extend ActiveSupport::Concern\n\n  def label\n    \"self b\"\n  end\nend\n"),
    ("self_ab", "module SelfAb\n  self.extend ActiveSupport::Concern\n  include SelfA\n  include SelfB\nend\n"),
    // `prepend` puts its module below the class, ahead of it.
    ("prepend_a", "module PrependA\n  def label\n    \"prepended\"\n  end\nend\n"),
    ("prepend_b", "module PrependB\n  def label\n    \"included\"\n  end\nend\n"),
    // A hook that includes a module.
    ("hook_a", "module HookA\n  def label\n    \"hook a\"\n  end\nend\n"),
    ("hook_b", "module HookB\n  def label\n    \"hook b\"\n  end\nend\n"),
    ("hook_wrapper", "module HookWrapper\n  extend ActiveSupport::Concern\n\n  included do\n    include HookA\n  end\nend\n"),
];

/// (controller, its include lines, Ruby's `label`).
const CASES: &[(&str, &str, &str)] = &[
    // [Separate, SecondLabel, FirstLabel]
    ("separate", "  include FirstLabel\n  include SecondLabel\n", "second"),
    // [Concern, BothLabels, SecondLabel, FirstLabel]
    ("concern", "  include BothLabels\n", "second"),
    // [MultiArg, FirstLabel, SecondLabel]
    ("multi_arg", "  include FirstLabel, SecondLabel\n", "first"),
    // [Again, BothLabels, SecondLabel, FirstLabel, ThirdLabel]: the last
    // `include FirstLabel` finds it already there and moves nothing.
    ("again", "  include ThirdLabel\n  include BothLabels\n  include FirstLabel\n", "second"),
    // [PlainDiamond, PlainAb, PlainB, PlainA]
    ("plain_diamond", "  include PlainB\n  include PlainAb\n", "plain b"),
    // [ConcernDiamond, ConcernAb, ConcernA, ConcernB]
    ("concern_diamond", "  include ConcernB\n  include ConcernAb\n", "concern a"),
    // [Mixed, MixedBoth, MixedConcern, MixedPlain]
    ("mixed", "  include MixedPlain\n  include MixedBoth\n", "mixed concern"),
    // [Cursor, CursorX, CursorB, CursorA, CursorC]: CursorX's walk meets
    // CursorA below CursorB and CursorB above it; CursorC goes below
    // CursorA, not back up between them.
    ("cursor", "  include CursorA\n  include CursorB\n  include CursorX\n", "cursor a"),
    // [SelfDiamond, SelfAb, SelfA, SelfB], as with `extend`.
    ("self_diamond", "  include SelfB\n  include SelfAb\n", "self a"),
    // [PrependA, Prepended, PrependB]
    ("prepended", "  prepend PrependA\n  include PrependB\n", "prepended"),
    // [HookInclude, HookB, HookA, HookWrapper]: the hook put HookA there,
    // so the last `include HookA` moves nothing.
    ("hook_include", "  include HookWrapper\n  include HookB\n  include HookA\n", "hook b"),
];

/// Cases that need more than a bare controller: (controller, its source,
/// Ruby's `label`).
const SPECIAL: &[(&str, &str, &str)] = &[
    // [SuperNm, SuperN, SuperParent, SuperM]: SuperM is the superclass's
    // already, so the child's `include SuperM` adds nothing.
    ("super_nm", "class SuperNmController < SuperParentController\n  include SuperN\n  include SuperM\n\n  def show\n    render json: { label: label }\n  end\nend\n", "super n"),
    // [SuperMn, SuperN, SuperParent, SuperM]
    ("super_mn", "class SuperMnController < SuperParentController\n  include SuperM\n  include SuperN\n\n  def show\n    render json: { label: label }\n  end\nend\n", "super n"),
    // HookDef's `included do` defines `label` on the controller itself.
    ("hoisted", "class HoistedController < ApplicationController\n  include HookDef\n  include PlainDef\n\n  def show\n    render json: { label: label }\n  end\nend\n", "hook"),
    // DelegateTwo's hook runs last, so its `delegate` defines `label`.
    ("delegated", "class DelegatedController < ApplicationController\n  include DelegateOne\n  include DelegateTwo\n\n  def show\n    render json: { label: label }\n  end\n\n  private\n\n  def source_one\n    LabelSource.new(\"source one\")\n  end\n\n  def source_two\n    LabelSource.new(\"source two\")\n  end\nend\n", "source two"),
];

fn camel(snake: &str) -> String {
    snake.split('_').map(|w| w[..1].to_uppercase() + &w[1..]).collect()
}

fn app() -> emit_and_run::Overlay {
    let mut app = emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::API\nend\n");
    for (file, source) in CONCERNS {
        app = app.write(&format!("app/controllers/concerns/{file}.rb"), source);
    }
    app = app
        .write("app/controllers/super_parent_controller.rb", "class SuperParentController < ApplicationController\n  include SuperM\nend\n")
        .write("app/models/label_source.rb", "class LabelSource\n  def initialize(label)\n    @label = label\n  end\n\n  attr_reader :label\nend\n");
    let mut routes = String::from("Rails.application.routes.draw do\n");
    for (name, source, _) in SPECIAL {
        app = app.write(&format!("app/controllers/{name}_controller.rb"), source);
        routes.push_str(&format!("  get \"/{name}\" => \"{name}#show\"\n"));
    }
    for (name, includes, _) in CASES {
        app = app.write(
            &format!("app/controllers/{name}_controller.rb"),
            &format!(
                "class {}Controller < ApplicationController\n{includes}\n  def show\n    render json: {{ label: label }}\n  end\nend\n",
                camel(name)
            ),
        );
        routes.push_str(&format!("  get \"/{name}\" => \"{name}#show\"\n"));
    }
    routes.push_str("end\n");
    app.write("config/routes.rb", &routes)
}

#[test]
fn the_spliced_method_is_the_one_rubys_ancestors_find_first() {
    let mut script = String::from(
        r##"def probe(path)
  status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
  text = +""
  body.each { |part| text << part }
  [status, text]
end
got = {}
want = {}
"##,
    );
    for (name, _, label) in CASES.iter().chain(SPECIAL) {
        script.push_str(&format!("got[{name:?}] = begin\n  probe(\"/{name}\")\nrescue StandardError => e\n  [e.class.name, e.message[0, 80]]\nend\nwant[{name:?}] = [200, %({{\"label\":{label:?}}})]\n"));
    }
    script.push_str("wrong = got.reject { |k, v| v == want[k] }\nraise \"wrong: #{wrong.inspect}\" unless wrong.empty?\nputs \"include order passed\"\n");
    let run = app().run_ruby(&script);
    run.assert_passes();
    assert!(run.stdout.contains("include order passed"), "{}", run.stdout);
}
