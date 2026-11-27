module PrivateAssignmentHooks
  private

  def state=(value)
    raise "private state writer called" unless value.nil?
    super(value)
  end

  def note=(value)
    raise "private note writer called" unless value.nil?
  end
end
