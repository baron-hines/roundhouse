# Check helpers and the ActiveRecord transaction shape the Db contract
# suite runs on every shim (tests/db_contract.rs and
# tests/spinel_db_contract.rs copy this file next to the shim under
# test). Plain Ruby that Spinel compiles: no metaprogramming, no
# keyword arguments, one counter per suite.

$checks = 0

# Report on stderr and exit, rather than raise: an exception's message
# can come back stale on Spinel once its storage is reused, and a
# failing gate should name the check that failed.
def fail_check(text)
  $stderr.puts "FAIL " + text
  exit 1
end

def check(label, ok)
  fail_check(label) if !ok
  $checks += 1
  nil
end

def check_int(label, want, got)
  fail_check(label + ": want " + want.to_s + ", got " + got.to_s) if want != got
  $checks += 1
  nil
end

def check_str(label, want, got)
  fail_check(label + ": want " + want.inspect + ", got " + got.inspect) if want != got
  $checks += 1
  nil
end

def check_raises(label, fragment)
  message = ""
  begin
    yield
  rescue StandardError => e
    message = e.message
  end
  if !message.include?(fragment)
    fail_check(label + ": want an error containing " + fragment.inspect + ", got " + message.inspect)
  end
  $checks += 1
  nil
end

# The block raises a StandardError, whatever its message: for the
# contract checks whose wording is each driver's own.
def check_raises_any(label)
  raised = false
  begin
    yield
  rescue StandardError
    raised = true
  end
  fail_check(label + ": want an error, got none") if !raised
  $checks += 1
  nil
end

# The message of the ActiveRecord::StatementInvalid the block raises, or
# "" when it raises none. Not the SQLSTATE: the shims raise
# ActiveRecord's class, as Rails does, and Spinel's exceptions keep no
# `cause` to read it from.
def statement_error_of
  message = ""
  begin
    yield
  rescue ActiveRecord::StatementInvalid => e
    message = e.message
  end
  message
end

# Ends a suite: prints the line the harness counts, and resets the
# counter for the next suite in the same program.
def report_suite(name)
  puts name + ": " + $checks.to_s + " checks passed"
  $checks = 0
  nil
end

# A copy, not a require, of ActiveRecord::Base.transaction's control
# flow (runtime/ruby/active_record/connection.rb): connection.rb pulls
# in the rest of the ActiveRecord surface (the adapter, Relation, schema
# columns) that this Db-level suite does not set up. The part #693
# added Db._txn_depth for, and the part the PostgreSQL regression broke
# (#766), is reproduced so the cases exercise the real shape.
def ar_transaction
  depth = Db._txn_depth
  if depth > 0
    Db._txn_depth = depth + 1
    begin
      result = yield
    rescue ActiveRecord::Rollback
      Db._txn_depth = depth
      result = nil
    rescue Exception => e
      Db._txn_depth = depth
      raise e
    ensure
      Db._txn_depth = depth
    end
  else
    Db.exec("BEGIN")
    Db._txn_depth = 1
    rolled_back = false
    begin
      result = yield
    rescue ActiveRecord::Rollback
      rolled_back = true
      Db._txn_depth = 0
      Db.exec("ROLLBACK")
      result = nil
    rescue Exception => e
      rolled_back = true
      Db._txn_depth = 0
      begin
        Db.exec("ROLLBACK")
      rescue StandardError
        nil
      end
      raise e
    ensure
      if rolled_back
        nil
      else
        Db._txn_depth = 0
        Db.exec("COMMIT")
      end
    end
  end
end

# One integer from a single-row, single-column read.
def read_int(sql)
  h = Db.prepare_uncached(sql)
  Db.step?(h)
  v = Db.column_int(h, 0)
  Db.finalize(h)
  v
end
