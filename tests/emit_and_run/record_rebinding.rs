use super::emit_and_run;

/// A condition lambda or callback block reads its one parameter as the
/// record, so the lowering swaps that parameter's reads for `self`. A
/// body that binds the name again (a nested `|record = 0|`, or
/// `record = nil`) is declined instead: past the rebinding, the name is
/// no longer the record.
#[test]
fn a_rebound_record_parameter_is_not_read_as_self() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n  validates :body, length: { minimum: 3 }, if: ->(record) { [1].any? { |record = 0| record == 1 } }\n  before_save { |record| record = nil; self.title = record.to_s }\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let model = std::fs::read_to_string(emitted.join("app/models/article.rb")).expect("article.rb");
    assert!(!model.contains("self.title = self.to_s"), "{model}");
    assert!(!model.contains("|record = 0| self"), "{model}");
}
