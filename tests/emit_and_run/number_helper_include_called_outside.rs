//! A class that only includes ActionView's NumberHelper answers its
//! number helpers when other objects call them on its instance, as Ruby's
//! include answers them: the class itself makes no such call. Ignored as a
//! fork spec until the compiler does; the test passes unchanged in the
//! generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/tally_sheet_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/models/figures.rb", "class Figures\n  include ActionView::Helpers::NumberHelper\nend\n")
        .write(
            "app/models/tally_sheet.rb",
            r#"class TallySheet
  def initialize(total)
    @total = total
    @figures = Figures.new
  end

  def line
    "Total: #{@figures.number_with_delimiter(@total)} (#{@figures.number_to_percentage(12.5, precision: 1)})"
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class TallySheetTest < ActiveSupport::TestCase
  test "another object calls the included number helpers" do
    assert_equal "Total: 1,234,567 (12.5%)", TallySheet.new(1234567).line
    assert_equal "9,876", Figures.new.number_with_delimiter(9876)
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: views helpers"]
fn an_included_number_helper_answers_other_callers() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: views helpers"]
fn an_included_number_helper_answers_other_callers_on_spinel() {
    app().run_spin_test("test/tally_sheet_test.rb").assert_passes();
}
