ActiveRecord::Schema.define do
  create_table :attribute_records do |t|
    t.string :state
    t.integer :status, default: 0
    t.json :settings
    t.datetime :reviewed_at
  end
  create_table :private_records do |t|
    t.string :state
  end
end
