# frozen_string_literal: true
# Instance-method form of ActiveSupport::NumberHelper. Kept separate from
# the module-function stem because the runtime source checker matches methods
# by their short class name and cannot distinguish the two forms in one file.
# Loaded by active_support_number_helper.rb, after the module functions
# these call: requiring that file back from here was a circular require,
# which warns under $VERBOSE.

module ActiveSupport
  module NumberHelper
    def number_to_currency(number, options = {})
      ActiveSupport::NumberHelper.number_to_currency(number, options)
    end

    def number_to_delimited(number, options = {})
      ActiveSupport::NumberHelper.number_to_delimited(number, options)
    end

    def number_to_human(number, options = {})
      ActiveSupport::NumberHelper.number_to_human(number, options)
    end

    def number_to_human_size(number, options = {})
      ActiveSupport::NumberHelper.number_to_human_size(number, options)
    end

    def number_to_percentage(number, options = {})
      ActiveSupport::NumberHelper.number_to_percentage(number, options)
    end

    def number_to_phone(number, options = {})
      ActiveSupport::NumberHelper.number_to_phone(number, options)
    end

    def number_to_rounded(number, options = {})
      ActiveSupport::NumberHelper.number_to_rounded(number, options)
    end
  end
end
