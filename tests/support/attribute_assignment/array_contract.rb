record = AttributeRecord.create!(tags: ["saved"], numbers: [1])
result = record.assign_attributes(tags: ["a", "b"], numbers: [3, 4])
raise "return value" unless result.nil?
raise "array shape" unless record.tags == ["a", "b"] && record.numbers == [3, 4]
saved = AttributeRecord.find(record.id)
raise "assignment saved" unless saved.tags == ["saved"] && saved.numbers == [1]
record.save!
saved = AttributeRecord.find(record.id)
raise "save after assignment" unless saved.tags == ["a", "b"] && saved.numbers == [3, 4]
record.assign_attributes(tags: [], numbers: [])
raise "empty arrays" unless record.tags == [] && record.numbers == []
record.save!
saved = AttributeRecord.find(record.id)
raise "empty array storage" unless saved.tags == [] && saved.numbers == []
record.assign_attributes(tags: nil, numbers: nil)
raise "nil arrays" unless record.tags.nil? && record.numbers.nil?
record.save!
saved = AttributeRecord.find(record.id)
raise "nil array storage" unless saved.tags.nil? && saved.numbers.nil?
puts "stored array attribute assignment passed"
