ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "events" do |t|
    t.datetime "recorded_at", default: -> { "CURRENT_TIMESTAMP" }
    t.string "literal", default: "CURRENT_TIMESTAMP"
    t.json "payload", default: -> { "'[]'" }
    t.string "tag", default: -> { "'O''Reilly'" }
    t.string "replacement", default: -> { "'first'" }, default: "last"
    t.string "cleared", default: -> { "'first'" }, default: nil
    t.column :token, :uuid, null: false, default: -> { "'550e8400-e29b-41d4-a716-446655440000'" }
  end
end
