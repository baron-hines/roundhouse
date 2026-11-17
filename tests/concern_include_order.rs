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
    let mut routes = String::from("Rails.application.routes.draw do\n");
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
    for (name, _, label) in CASES {
        script.push_str(&format!("got[{name:?}] = probe(\"/{name}\")\nwant[{name:?}] = [200, %({{\"label\":{label:?}}})]\n"));
    }
    script.push_str("wrong = got.reject { |k, v| v == want[k] }\nraise \"wrong: #{wrong.inspect}\" unless wrong.empty?\nputs \"include order passed\"\n");
    let run = app().run_ruby(&script);
    run.assert_passes();
    assert!(run.stdout.contains("include order passed"), "{}", run.stdout);
}
