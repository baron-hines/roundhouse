# frozen_string_literal: true
# Attribute names only known when the program runs (Devise's FailureApp: `scope_class.human_attribute_name(key)`),
# and `base_class` (orm_adapter's polymorphic condition). Ruby family and Spinel only.
module ActiveRecord
  class Base
    # Not the locale's name for the attribute: `lower::human_attribute_name` folds a literal one from it, and no table of them is kept here.
    def self.human_attribute_name(attr, options = {})
      ActiveSupport.humanize(attr.to_s.tr(".", "_"))
    end

    # Rails' `base_class`: the class itself, unless a model's subclass answers its base model (`lower::sti_scope`).
    # Not in the shared runtime: a strict target has no class value for `self` in a class method.
    def self.base_class
      self
    end
  end
end
