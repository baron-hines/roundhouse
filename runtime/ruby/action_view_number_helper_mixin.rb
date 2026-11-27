# frozen_string_literal: true
# Instance-method facade of ActionView::Helpers::NumberHelper, separated
# from its module-function implementation for runtime source typing.
# Loaded by action_view_number_helper.rb, after the module functions
# these call: requiring that file back from here was a circular require,
# which warns under $VERBOSE.

module ActionView
  module Helpers
    module NumberHelper
      def number_to_currency(number, options = {})
        NumberHelper.number_to_currency(number, options)
      end

      def number_to_human(number, options = {})
        NumberHelper.number_to_human(number, options)
      end

      def number_to_human_size(number, options = {})
        NumberHelper.number_to_human_size(number, options)
      end

      def number_to_percentage(number, options = {})
        NumberHelper.number_to_percentage(number, options)
      end

      def number_to_phone(number, options = {})
        NumberHelper.number_to_phone(number, options)
      end

      def number_with_delimiter(number, options = {})
        NumberHelper.number_with_delimiter(number, options)
      end

      def number_with_precision(number, options = {})
        NumberHelper.number_with_precision(number, options)
      end
    end
  end
end
