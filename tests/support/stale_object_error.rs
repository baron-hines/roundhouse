#![allow(dead_code)]

pub const RECORD: &str = r#"class StaleRecord
  attr_reader :name
  def initialize(name)
    @name = name
  end
end
"#;

pub const SOURCE: &str = r#"class StaleProbe
  ERROR_CLASS = ActiveRecord::StaleObjectError

  def self.record
    StaleRecord.new("Ada")
  end

  def self.failure(record, action)
    raise ActiveRecord::StaleObjectError.new(record, action)
  rescue ActiveRecord::StaleObjectError => error
    error
  end

  def self.action(record, action)
    failure(record, action).attempted_action
  end

  def self.generic_message
    raise ActiveRecord::StaleObjectError, "record changed"
  rescue ActiveRecord::ActiveRecordError => error
    error.message
  end

  def self.rooted_message
    raise ::ActiveRecord::StaleObjectError
  rescue ::ActiveRecord::StaleObjectError => error
    error.message
  end

  def self.unrelated_error
    raise ArgumentError, "invalid"
  rescue ActiveRecord::StaleObjectError
    "wrong"
  rescue ArgumentError
    "argument"
  end
end
"#;

pub const ASSERTIONS: &str = r#"
raise "class identity" unless StaleProbe::ERROR_CLASS.equal?(ActiveRecord::StaleObjectError)
raise "parent" unless ActiveRecord::StaleObjectError.superclass == ActiveRecord::ActiveRecordError
raise "generic message" unless StaleProbe.generic_message == "Stale object error."
raise "rooted message" unless StaleProbe.rooted_message == "Stale object error."
raise "unrelated rescue" unless StaleProbe.unrelated_error == "argument"
record = StaleProbe.record
error = StaleProbe.failure(record, "update")
raise "record identity" unless error.record.equal?(record)
raise "action" unless error.attempted_action == "update"
raise "message" unless error.message == "Attempted to update a stale object: StaleRecord."
raise "symbol action" unless StaleProbe.action(record, :destroy) == :destroy
raise "symbol message" unless StaleProbe.failure(record, :destroy).message == "Attempted to destroy a stale object: StaleRecord."
raise "boolean action" unless StaleProbe.action(record, true) == true
raise "boolean message" unless StaleProbe.failure(record, true).message == "Attempted to true a stale object: StaleRecord."
action = Object.new
object_error = StaleProbe.failure(record, action)
raise "object action identity" unless object_error.attempted_action.equal?(action)
raise "object action message" unless object_error.message == "Attempted to #{action} a stale object: StaleRecord."
[StaleProbe.failure(record, nil), StaleProbe.failure(nil, "update"),
 StaleProbe.failure(false, "update"), StaleProbe.failure(record, false)].each do |generic|
  raise "generic message" unless generic.message == "Stale object error."
  raise "generic record" unless generic.record.nil?
  raise "generic action" unless generic.attempted_action.nil?
end
puts "StaleObjectError contract passed"
"#;

pub fn overlay() -> super::emit_and_run::Overlay {
    super::emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
        .write("app/services/stale_record.rb", RECORD)
        .write("app/services/stale_probe.rb", SOURCE)
}
