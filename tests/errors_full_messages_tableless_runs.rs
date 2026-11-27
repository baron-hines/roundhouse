//! `errors.full_messages` on a tableless `ActiveModel::Model` class,
//! run. The framework runtime's `errors` is the plain Array of full
//! messages, so the extra hop must fold away even when the app defines
//! its own `full_messages` on an unrelated collection; otherwise the
//! emitted call raises `undefined method 'full_messages' for an instance
//! of Array`.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

#[test]
fn a_tableless_models_errors_full_messages_runs_although_the_app_defines_full_messages() {
    emit_and_run::empty_app()
        .write(
            "app/models/draft.rb",
            "class Draft\n  include ActiveModel::Model\n  attr_accessor :title\n  validates :title, presence: true\n\n  def save\n    return true if valid?\n\n    errors.full_messages.join(\", \")\n  end\n\n  def own\n    self.errors.full_messages\n  end\nend\n",
        )
        .write(
            "app/models/collection.rb",
            "class Collection\n  def full_messages\n    [\"custom\"]\n  end\nend\n",
        )
        .run_ruby(
            "d = Draft.new\n\
             raise \"save: #{d.save.inspect}\" unless d.save == \"Title can't be blank\"\n\
             raise \"own: #{d.own.inspect}\" unless d.own == [\"Title can't be blank\"]\n\
             raise 'collection' unless Collection.new.full_messages == ['custom']\n\
             puts 'ok'\n",
        )
        .assert_passes();
}
