class AttributeRecord < ApplicationRecord
  has_json :settings, enabled: false, count: 10, greeting: "hello"

  def report
    "#{settings.enabled}:#{settings.count}:#{settings.greeting}"
  end
end
