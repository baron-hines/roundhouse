class LockRecord < ApplicationRecord
  def self.locked_name(id)
    transaction do
      record = lock.find_by(id: id)
      record.nil? ? "missing" : record.name.to_s
    end
  end

  def locked_name
    self.class.transaction do
      record = self.class.lock.find_by(id: id)
      record.nil? ? "missing" : record.name.to_s
    end
  end
end
