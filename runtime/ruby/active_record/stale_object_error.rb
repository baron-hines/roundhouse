# frozen_string_literal: true
# ActiveRecord::StaleObjectError, with Rails 8.1.4's constructor.
#
# Ruby-family home (off the strict-target tables, unlike errors.rb), as
# `connection_not_established.rb`: its message is built before `super`,
# and its payloads are untyped Objects tested for truthiness, which the
# strict targets' transpiled errors file could not compile (Kotlin,
# Swift, C# and Crystal all refused it there).
module ActiveRecord
  class StaleObjectError < ActiveRecordError
    attr_reader :record, :attempted_action

    def initialize(record = nil, attempted_action = nil)
      @record = nil
      @attempted_action = nil
      message = "Stale object error."
      if record && attempted_action
        @record = record
        @attempted_action = attempted_action
        message = "Attempted to #{attempted_action} a stale object: #{record.class.name}."
      end
      super(message)
    end
  end
end
