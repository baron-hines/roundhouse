ActiveRecord::Schema.define do
  create_table :attribute_records do |t|
    t.string :state
    t.string :phase
    t.integer :exit_status
    t.boolean :enabled
    t.datetime :started_at
    t.json :metadata
  end
end
