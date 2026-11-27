record = AttributeRecord.create!(settings: {"enabled" => "1", "count" => "4", "greeting" => "kept"})
raise "constructor casts" unless record.report == "true:4:kept"
result = record.assign_attributes(settings: {"enabled" => "0", "count" => "7"})
raise "return value" unless result.nil?
raise "key casts and partial merge" unless record.report == "false:7:kept"
raise "assignment saved" unless AttributeRecord.find(record.id).report == "true:4:kept"
record.assign_attributes(settings: {"greeting" => 45})
raise "string key cast" unless record.report == "false:7:45"
record.save!
raise "save after assignment" unless AttributeRecord.find(record.id).report == "false:7:45"
record.assign_attributes(settings: nil)
raise "nil resets defaults" unless record.report == "false:10:hello"
record.assign_attributes(settings: '{"enabled":true,"count":8,"greeting":"serialized"}')
raise "serialized input" unless record.report == "true:8:serialized"
puts "declared JSON attribute assignment passed"
