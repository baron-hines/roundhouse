# Spinel lanes: the ActiveRecord classes the shims raise and rescue,
# declared here because the real ones (runtime/ruby/active_record/)
# arrive with the rest of the ActiveRecord surface this Db-level suite
# does not compile.

# The app runtime supplies ActiveSupport.present (zone conversion);
# parse_db_time only needs it to hand the Time back.
module ActiveSupport
  def self.present(t)
    t
  end
end

module ActiveRecord
  class ActiveRecordError < StandardError
  end
  class StatementInvalid < ActiveRecordError
  end
  class WrappedDatabaseException < StatementInvalid
  end
  class RecordNotUnique < WrappedDatabaseException
  end
  class InvalidForeignKey < WrappedDatabaseException
  end
  class ValueTooLong < StatementInvalid
  end
  class RangeError < StatementInvalid
  end
  class NotNullViolation < StatementInvalid
  end
  class CheckViolation < StatementInvalid
  end
  class ExclusionViolation < StatementInvalid
  end
  class TransactionRollbackError < StatementInvalid
  end
  class SerializationFailure < TransactionRollbackError
  end
  class Deadlocked < TransactionRollbackError
  end
  class QueryAborted < StatementInvalid
  end
  class LockWaitTimeout < StatementInvalid
  end
  class StatementTimeout < QueryAborted
  end
  class QueryCanceled < QueryAborted
  end

  # Real ActiveRecord::Rollback (runtime/ruby/active_record/connection.rb);
  # ar_transaction rescues it exactly as ActiveRecord::Base.transaction does.
  class Rollback < StandardError
  end
end
