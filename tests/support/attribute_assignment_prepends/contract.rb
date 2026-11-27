private_record = PrivateRecord.new
begin
  private_record.assign_attributes(state: "x")
  raise "private schema writer accepted"
rescue ActiveModel::UnknownAttributeError => error
  raise "schema error metadata" unless error.attribute == "state" && error.record.equal?(private_record)
end
begin
  private_record.assign_attributes(note: "x")
  raise "private virtual writer accepted"
rescue ActiveModel::UnknownAttributeError => error
  raise "virtual error metadata" unless error.attribute == "note" && error.record.equal?(private_record)
end
record = AttributeRecord.new
record.assign_attributes(status: "closed")
raise "enum input changed" unless record.status_input == "closed"
raise "enum conversion lost" unless record.status == "closed"
input = {"enabled" => "0", "count" => "7", "greeting" => "kept"}
record.assign_attributes(settings: input)
raise "Hash input changed" unless record.settings_input.equal?(input)
raise "JSON casts lost" unless record.report == "false:7:kept"
record.assign_attributes(settings: {"count" => "9"})
raise "JSON partial merge lost" unless record.report == "false:9:kept"
record.save!
saved = AttributeRecord.find(record.id)
raise "enum persistence" unless saved.status == "closed"
raise "JSON persistence" unless saved.report == "false:9:kept"
record.assign_attributes(settings: nil)
raise "nil input changed" unless record.settings_input.nil?
raise "nil defaults" unless record.report == "false:10:hello"
serialized = '{"enabled":true,"count":8,"greeting":"serialized"}'
record.assign_attributes(settings: serialized)
raise "serialized input changed" unless record.settings_input.equal?(serialized)
raise "serialized value lost" unless record.report == "true:8:serialized"
puts "prepended attribute writers passed"
