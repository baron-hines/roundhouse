execute("INSERT INTO events DEFAULT VALUES")
raise "SQL timestamp default was lost" unless scalar("SELECT COUNT(*) FROM events WHERE recorded_at IS NOT NULL AND recorded_at <= CURRENT_TIMESTAMP") == 1
raise "literal became an expression" unless scalar("SELECT COUNT(*) FROM events WHERE literal = 'CURRENT_TIMESTAMP'") == 1
raise "SQL JSON literal default was lost" unless scalar("SELECT COUNT(*) FROM events WHERE payload = '[]'") == 1
raise "SQL quoted literal default was lost" unless scalar("SELECT COUNT(*) FROM events WHERE tag = 'O''Reilly'") == 1
raise "later literal default was lost" unless scalar("SELECT COUNT(*) FROM events WHERE replacement = 'last'") == 1
raise "later nil default was lost" unless scalar("SELECT COUNT(*) FROM events WHERE cleared IS NULL") == 1
raise "UUID SQL default was lost" unless scalar("SELECT COUNT(*) FROM events WHERE token = '550e8400-e29b-41d4-a716-446655440000'") == 1
null_uuid_rejected = false
begin
  execute("INSERT INTO events (token) VALUES (NULL)")
rescue StandardError
  null_uuid_rejected = true
end
raise "UUID accepted explicit NULL" unless null_uuid_rejected
execute("INSERT INTO events (recorded_at) VALUES (NULL)")
raise "explicit NULL acquired a default" unless scalar("SELECT COUNT(*) FROM events WHERE recorded_at IS NULL") == 1
puts "SQL default fills omitted values; literal and explicit NULL stay distinct"
