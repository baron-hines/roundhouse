class AttributeRecord < ApplicationRecord
  enum :status, { open: 0, closed: 1 }
  has_json :settings, enabled: false, count: 10, greeting: "hello"

  def report
    "#{settings.enabled}:#{settings.count}:#{settings.greeting}"
  end
end
