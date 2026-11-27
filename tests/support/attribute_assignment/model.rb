class AttributeRecord < ApplicationRecord
  attr_accessor :note, :writer_order
  before_save { raise "assignment saved the record" if state == "unsaved" }

  def phase=(value)
    self.writer_order = writer_order.to_s + "phase:#{state};"
    super(value.nil? ? nil : value.upcase)
  end

  def note=(value)
    @note = value
  end

  def update_from_payload(payload)
    attrs = { state: payload["state"], phase: payload["phase"], exit_status: payload["exit_status"] }
    assign_attributes(attrs)
    self.state
  end

  def pending_assignment?
    attribute_changed?("state")
  end

  def deny=(value)
    raise "private writer was called"
  end
  private :deny=
end
