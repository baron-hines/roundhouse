//! `generates_token_for` on a uuid-keyed model: Rails writes the key into the
//! payload as a JSON string (`["0b6d…","value"]`), so the tokens minted here
//! are Rails' bytes and Rails' own tokens find the record.

use super::emit_and_run;

fn uuid_widget_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        )
        .write(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\nend\n",
        )
        .write(
            "db/schema.rb",
            r#"ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "widgets", id: :uuid, force: :cascade do |t|
    t.string "name"
  end
end
"#,
        )
        .write(
            "app/models/widget.rb",
            "class Widget < ApplicationRecord\n  generates_token_for :share do\n    name\n  end\n  generates_token_for :plain\nend\n",
        )
}

// Minted by Rails 8.1.4 with `SECRET_KEY_BASE=test-secret` for this widget:
//   {"_rails":{"data":["0b6d2f1e-6a7c-4a5e-9c1d-3f2e1a0b9c8d","He\"llo<"],"pur":"Widget\nshare\n"}}
//   {"_rails":{"data":["0b6d2f1e-6a7c-4a5e-9c1d-3f2e1a0b9c8d"],"pur":"Widget\nplain\n"}}
const ASSERTIONS: &str = r#"
Rails.secret_key_base = "test-secret"
share = "eyJfcmFpbHMiOnsiZGF0YSI6WyIwYjZkMmYxZS02YTdjLTRhNWUtOWMxZC0zZjJlMWEwYjljOGQiLCJIZVwibGxvXHUwMDNjIl0sInB1ciI6IldpZGdldFxuc2hhcmVcbiJ9fQ==--03017943430c63333851fff410c6f2839f2affb4"
plain = "eyJfcmFpbHMiOnsiZGF0YSI6WyIwYjZkMmYxZS02YTdjLTRhNWUtOWMxZC0zZjJlMWEwYjljOGQiXSwicHVyIjoiV2lkZ2V0XG5wbGFpblxuIn19--33e14f602c9cbd85b568eb361e574bc785419285"
uid = "0b6d2f1e-6a7c-4a5e-9c1d-3f2e1a0b9c8d"
w = Widget.create!(id: uid, name: "He\"llo<")
raise "share differs from rails: #{w.generate_token_for(:share)}" unless w.generate_token_for(:share) == share
raise "plain differs from rails" unless w.generate_token_for(:plain) == plain
raise "rails share token rejected" unless Widget.find_by_token_for(:share, share)&.id == uid
raise "rails plain token rejected" unless Widget.find_by_token_for!(:plain, plain).id == uid
raise "tamper" unless Widget.find_by_token_for(:share, share + "x").nil?
raise "purpose" unless Widget.find_by_token_for(:plain, share).nil?
w.update!(name: "Changed")
raise "stale" unless Widget.find_by_token_for(:share, share).nil?
raise "plain survives" unless Widget.find_by_token_for(:plain, plain)&.id == uid
w.destroy
begin
  Widget.find_by_token_for!(:plain, plain)
  raise "no raise"
rescue ActiveRecord::RecordNotFound
end
puts "PASS uuid token_for"
"#;

#[test]
fn a_uuid_keyed_token_is_the_one_rails_mints() {
    let run = uuid_widget_app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("PASS uuid token_for"), "{}", run.stdout);
}
