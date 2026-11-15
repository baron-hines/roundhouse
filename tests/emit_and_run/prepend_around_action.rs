//! Rails' `prepend_around_action` registers an around filter at the head
//! of the controller's chain (`_insert_callbacks` with `prepend: true`):
//! it wraps every filter registered before it, inherited ones included,
//! and runs inside a `prepend_before_action` registered after it. Of two
//! prepended arounds the one registered last is outermost, and a chain
//! halted inside them returns to their code after `yield`. Prepending a
//! filter the parent registered moves it rather than adding a second
//! one, an empty `only:` runs nowhere, and a scoped skip narrows the
//! filter's own `only:`. The test below
//! passes unchanged in the generated blog on Rails 8.1 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/journeys_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/journey\", to: \"journeys#show\"\n",
        )
        .write(
            "app/controllers/concerns/stamped.rb",
            r#"module Stamped
  extend ActiveSupport::Concern

  included do
    before_action :board
  end

  private

  def board
    (@stops ||= []) << "board"
  end
end
"#,
        )
        .write(
            "app/controllers/transit_controller.rb",
            r#"class TransitController < ApplicationController
  around_action :station

  private

  def station
    @stops << "station-in"
    yield
    @stops << "station-out"
  end
end
"#,
        )
        .write(
            "app/controllers/journeys_controller.rb",
            r#"class JourneysController < TransitController
  include Stamped

  before_action :ticket
  around_action :carriage
  prepend_around_action :station
  prepend_before_action :platform
  prepend_around_action :city
  prepend_around_action :detour, only: []
  prepend_around_action :survey, only: :show
  skip_around_action :survey, only: :show

  def show
    @stops << "ride"
    render plain: @stops.join(" ")
  end

  private

  def platform
    (@stops ||= []) << "platform"
  end

  def ticket
    @stops << "ticket"
    redirect_to "/journey" if params[:sold_out]
  end

  def carriage
    @stops << "carriage-in"
    yield
    @stops << "carriage-out"
  end

  def detour
    render plain: "detour"
  end

  def survey
    render plain: "survey"
  end

  def city
    (@stops ||= []) << "city-in"
    yield
    @stops << "city-out"
    response.headers["X-Stops"] = @stops.join(" ")
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class JourneysControllerTest < ActionDispatch::IntegrationTest
  test "prepended arounds lead the chain" do
    get "/journey"
    assert_response :success
    assert_equal "city-in platform station-in board ticket carriage-in ride", response.body
    assert_equal "city-in platform station-in board ticket carriage-in ride carriage-out station-out city-out", response.headers["X-Stops"]
  end

  test "a halted chain returns to the prepended arounds" do
    get "/journey", params: { sold_out: "1" }
    assert_redirected_to "/journey"
    assert_equal "city-in platform station-in board ticket station-out city-out", response.headers["X-Stops"]
  end
end
"#,
        )
}

#[test]
fn prepend_around_action_leads_the_chain() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "requires spin and spinel on PATH plus SQLite development libraries"]
fn prepend_around_action_leads_the_chain_on_spinel() {
    let (emitted, errors) = app().emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let file = "test/journeys_controller_test.rb";
    assert!(emitted.join(file).exists(), "{file} was not emitted in {}", emitted.display());
    let output = std::process::Command::new("spin").args(["test", file]).current_dir(&*emitted).output().expect("spawn spin");
    assert!(
        output.status.success(),
        "spin test failed in {}\n=== stdout ===\n{}\n=== stderr ===\n{}",
        emitted.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
