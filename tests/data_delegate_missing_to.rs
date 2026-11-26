//! `delegate_missing_to :target` in a `Data.define` block (maybe's `BalanceSheet::AccountTotals::AccountRow`): a name
//! the value class does not define is the target's, as Rails' delegation defines `method_missing` and
//! `respond_to_missing?` for it. Ingest refused the block ("Data.define blocks support only instance methods").

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const ROWS: &str = r###"class Rows
  Row = Data.define(:article, :rank) do
    def label = "##{rank}"

    delegate_missing_to :article
  end

  def self.report
    row = Row.new(article: Article.new(title: "Wrapped", body: "A body long enough."), rank: 2)
    [row.label, row.title, row.respond_to?(:body), row.respond_to?(:nothing)].inspect
  end
end
"###;

const EXPECTED: &str = "[\"#2\", \"Wrapped\", true, false]\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/rows.rb", ROWS)
}

#[test]
fn a_data_row_delegates_what_it_does_not_define_on_ruby() {
    let run = app().run_ruby("puts Rows.report\n");
    run.assert_passes();
    assert_eq!(run.stdout, EXPECTED);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_data_row_delegates_what_it_does_not_define_on_spinel() {
    let run = app().run_spinel_with_db("puts Rows.report\n");
    run.assert_passes();
    assert_eq!(run.stdout, EXPECTED);
}
