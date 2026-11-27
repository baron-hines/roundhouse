record = LockRecord.create!(name: "one")
raise "class lock" unless LockRecord.locked_name(record.id) == "one"
raise "self.class lock" unless record.locked_name == "one"
raise "missing row" unless LockRecord.locked_name(9999) == "missing"
raise "named clause" unless LockRecord.lock("FOR UPDATE NOWAIT").find_by(id: record.id).id == record.id
base = LockRecord.where(name: "one")
base.to_a
fresh = base.lock(false)
raise "reused relation" if fresh.equal?(base)
LockRecord.create!(name: "one")
raise "reused loaded cache" unless fresh.to_a.length == 2 && base.to_a.length == 1
raise "lock clause in SQL" if fresh.to_sql.include?("FOR UPDATE")
raise "where lost" unless LockRecord.lock(nil).where(name: "absent").count == 0
puts "class lock passed"
