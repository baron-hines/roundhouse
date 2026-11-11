# frozen_string_literal: true
# Attribute names only known when the program runs (Devise's FailureApp: `scope_class.human_attribute_name(key)`).
# Ruby family and Spinel only: the humanize walk is theirs.
module ActiveRecord
  class Base
    # Not the locale's name for the attribute: `lower::human_attribute_name` folds a literal one from it, and no table of them is kept here.
    def self.human_attribute_name(attr, options = {})
      ActiveSupport.humanize(attr.to_s.tr(".", "_"))
    end
  end
end
