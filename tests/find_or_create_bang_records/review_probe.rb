alice = User.create!(name: "Alice")
bob = User.create!(name: "Bob")
carol = User.create!(name: "Carol")
prior = Follow.direct(alice, bob)

[
  [:conflicting_keys, [carol, alice.id, bob], alice.id],
  [:reversed_keys, [alice.id, carol, bob], carol.id],
  [:shared_key, [carol, alice, bob], alice.id]
].each do |method, args, follower_id|
  count_before = Follow.count
  record = Follow.public_send(method, *args)
  raise "lost a lookup predicate: #{method}" if record.id == prior.id
  raise "did not insert: #{method}" unless Follow.count == count_before + 1
  raise "attribute order changed: #{method}" unless record.follower_id == follower_id
  puts [method, record.id == prior.id, Follow.count - count_before, record.follower_id].join(" | ")
end

record = Follow.with_block(carol, alice)
stored = Follow.connection.select_value("SELECT initialized_for FROM follows WHERE id = #{record.id}")
raise "block ran after callback" unless record.initialized_for == "3:1" && stored == "3:1"
puts "block | #{record.initialized_for} | #{stored}"
record.initialized_for = "existing body"
record.save!
again = Follow.with_block(carol, alice)
stored = Follow.connection.select_value("SELECT initialized_for FROM follows WHERE id = #{record.id}")
raise "existing row ran creation block" unless again.id == record.id && again.initialized_for == "3:1" && stored == "existing body"
puts "existing | #{again.id == record.id} | #{stored}"

repaired = Follow.block_fixes_validation(carol, bob)
raise "block did not repair validation before callback" unless repaired.persisted? && repaired.initialized_for == "3:2"
puts "repair | #{repaired.persisted?} | #{repaired.initialized_for}"
literal = Follow.literal_block
raise "literal block ran after callback" unless literal.initialized_for == "2:1"
puts "literal | #{literal.initialized_for}"
