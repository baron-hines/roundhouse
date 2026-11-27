record = AttributeRecord.new
stamp = Time.utc(2024, 2, 3, 4, 5, 6)
record.assign_attributes(reviewed_at: stamp)
raise "temporal writer bypassed" unless record.reviewed_at_input == stamp
raise "temporal conversion lost" unless record.reviewed_at == stamp
record.assign_attributes(reviewed_at: nil)
raise "temporal nil input" unless record.reviewed_at_input.nil?
raise "temporal nil conversion" unless record.reviewed_at.nil?
puts "prepended temporal writer passed"
