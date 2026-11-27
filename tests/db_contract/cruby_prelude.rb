# CRuby lanes: the runtime's own ActiveRecord error classes and the
# connection pool db_cruby.rb leases from, loaded from the checkout
# (ROUNDHOUSE_ROOT) rather than copied.
root = ENV.fetch("ROUNDHOUSE_ROOT")
require File.join(root, "runtime/ruby/active_record/errors")
require File.join(root, "runtime/ruby/active_record/connection_pool")

module ActiveRecord
  # runtime/ruby/active_record/connection.rb defines it with the rest of
  # the transaction surface this suite does not load.
  class Rollback < StandardError
  end
end

# The app runtime supplies ActiveSupport.present (zone conversion, in
# runtime/ruby/active_support_ext.rb); parse_db_time, which the
# PostgreSQL shim reads timestamps through, only needs it to hand the
# Time back.
module ActiveSupport
  def self.present(t)
    t
  end
end
