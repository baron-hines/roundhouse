module AssignmentHooks
  attr_reader :status_input, :settings_input, :reviewed_at_input

  def reviewed_at=(value)
    @reviewed_at_input = value
    super(value)
  end

  def status=(value)
    @status_input = value
    super(value)
  end

  def settings=(value)
    @settings_input = value
    super(value)
  end
end
