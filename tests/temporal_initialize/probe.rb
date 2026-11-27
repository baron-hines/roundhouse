t = Time.utc(2024, 2, 3, 4, 5, 6)
date = Date.new(2024, 2, 29)
entry = TemporalEntry.create!(happened_at: t, required_at: t, due_on: date, opens_at: t)
entry.reload
raise "timestamp lost" unless entry.happened_at.strftime("%Y-%m-%d %H:%M:%S") == "2024-02-03 04:05:06"
raise "required timestamp lost" unless entry.required_at.strftime("%Y-%m-%d %H:%M:%S") == "2024-02-03 04:05:06"
raise "date lost" unless entry.due_on.iso8601 == "2024-02-29"
raise "time lost" unless entry.opens_at.strftime("%H:%M:%S") == "04:05:06"
puts [entry.happened_at.strftime("%Y-%m-%d %H:%M:%S"), entry.due_on.iso8601,
      entry.opens_at.strftime("%H:%M:%S")].join(" | ")

empty = TemporalEntry.new
raise "unset timestamp" unless empty.happened_at.nil?
raise "unset date" unless empty.due_on.nil?
raise "unset time" unless empty.opens_at.nil?
explicit_nil = TemporalEntry.new(happened_at: nil, due_on: nil, opens_at: nil)
raise "explicit nil timestamp" unless explicit_nil.happened_at.nil?
raise "explicit nil date" unless explicit_nil.due_on.nil?
raise "explicit nil time" unless explicit_nil.opens_at.nil?
text = TemporalEntry.new(happened_at: "2024-03-04 05:06:07", required_at: "2024-03-04 05:06:07",
                         due_on: "2024-03-31", opens_at: "2000-01-01 05:06:07")
raise "stored timestamp lost" unless text.happened_at.strftime("%Y-%m-%d %H:%M:%S") == "2024-03-04 05:06:07"
raise "stored date lost" unless text.due_on.iso8601 == "2024-03-31"
raise "stored time lost" unless text.opens_at.strftime("%H:%M:%S") == "05:06:07"
puts "temporal construction passed"
