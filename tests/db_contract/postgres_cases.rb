# What any PostgreSQL `Db` shows on top of the contract (contract_cases.rb):
# SQLSTATE mapping (23505, 23502, 22001, and 25P02 after an error in a
# transaction), `exec_returning`, the connection released when COMMIT
# raises, and statements that stay usable whatever the server does with
# its prepared ones.
#
# These cases use only the contract surface plus one hook the lane file
# supplies (see db_pg_lane.rb):
#
#   pg_exec_on_other_session(sql) — run `sql` on a server session other
#                                   than the one this thread holds.
#
# Tables live in the run's disposable schema; `s` is "<schema>.".

def pg_count(s, table, where)
  h = Db.prepare_uncached("SELECT count(*) FROM " + s + table + " WHERE " + where)
  Db.step?(h)
  n = Db.column_int(h, 0)
  Db.finalize(h)
  n
end

def pg_cached_read(s, id)
  h = Db.prepare("SELECT name FROM " + s + "pg_widgets WHERE id = $1")
  Db.bind_int(h, 1, id)
  found = Db.step?(h)
  Db.finalize(h)
  found
end

# Every prepared statement this session holds on the server, whatever
# the shim names them. Read through prepare_uncached, which a shim runs
# without a statement of its own.
def pg_prepared_count
  h = Db.prepare_uncached("SELECT count(*) FROM pg_prepared_statements")
  Db.step?(h)
  n = Db.column_int(h, 0)
  Db.finalize(h)
  n
end

def pg_backend_pid
  h = Db.prepare_uncached("SELECT pg_backend_pid()")
  Db.step?(h)
  pid = Db.column_int(h, 0)
  Db.finalize(h)
  pid
end

def pg_01_tables(s)
  Db.exec("CREATE TABLE " + s + "pg_widgets (id bigserial PRIMARY KEY, name text NOT NULL, weight float8)")
  Db.exec("INSERT INTO " + s + "pg_widgets (name) VALUES ('one'), ('two'), ('three')")
  Db.exec("CREATE TABLE " + s + "pg_gadgets (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), label text)")
  Db.exec("CREATE TABLE " + s + "pg_tags (id serial PRIMARY KEY, name varchar(3) NOT NULL UNIQUE)")
  Db.exec("INSERT INTO " + s + "pg_tags (name) VALUES ('red')")
  check_int("the setup rows", 3, pg_count(s, "pg_widgets", "true"))
  nil
end

def pg_02_sqlstate(s)
  # ── SQLSTATE mapping ──
  dup = ""
  begin
    Db.exec("INSERT INTO " + s + "pg_tags (name) VALUES ('red')")
  rescue ActiveRecord::RecordNotUnique => e
    dup = e.message
  end
  check("23505 is RecordNotUnique", dup.include?("duplicate key"))
  dup = ""
  begin
    h = Db.exec_returning("INSERT INTO " + s + "pg_tags (name) VALUES ('red') RETURNING id")
    Db.finalize(h)
  rescue ActiveRecord::RecordNotUnique => e
    dup = e.message
  end
  check("23505 from exec_returning", dup.include?("duplicate key"))
  h = Db.prepare("INSERT INTO " + s + "pg_tags (name) VALUES ($1) RETURNING id")
  Db.bind_text(h, 1, "red")
  dup = ""
  begin
    Db.step?(h)
  rescue ActiveRecord::RecordNotUnique => e
    dup = e.message
  end
  Db.finalize(h)
  check("23505 from a bound statement", dup.include?("duplicate key"))
  long = ""
  begin
    Db.exec("INSERT INTO " + s + "pg_tags (name) VALUES ('purple')")
  rescue ActiveRecord::ValueTooLong => e
    long = e.message
  end
  check("22001 is ValueTooLong", long.include?("too long"))
  null_violation = ""
  begin
    Db.exec("INSERT INTO " + s + "pg_tags (name) VALUES (NULL)")
  rescue ActiveRecord::NotNullViolation => e
    null_violation = e.message
  end
  check("23502 is NotNullViolation", null_violation.include?("null value"))
  syntax = ""
  begin
    Db.exec("SELEC 1")
  rescue ActiveRecord::StatementInvalid => e
    syntax = e.message
  end
  check("a server error with no class of its own is StatementInvalid", syntax.include?("syntax error"))
  h = Db.prepare("SELECT $1::int + 1")
  Db.bind_int(h, 1, 41)
  check("usable after server errors", Db.step?(h))
  check_int("usable after server errors, value", 42, Db.column_int(h, 0))
  Db.finalize(h)
  nil
end

def pg_03_aborted_transaction(s)
  # ── 25P02: a transaction refuses statements after an error ──
  Db.with_connection do
    Db.exec("BEGIN")
    dup = false
    begin
      Db.exec("INSERT INTO " + s + "pg_tags (name) VALUES ('red')")
    rescue ActiveRecord::RecordNotUnique
      dup = true
    end
    check("duplicate inside a transaction", dup)
    check("the next statement is refused (25P02)",
          statement_error_of { Db.exec("SELECT 1") }.include?("current transaction is aborted"))
    check("a cached read is refused too (25P02)",
          statement_error_of { pg_cached_read(s, 1) }.include?("current transaction is aborted"))
    Db.exec("ROLLBACK")
    Db.exec("INSERT INTO " + s + "pg_tags (name) VALUES ('blu')")
    check_int("usable after ROLLBACK", 1, Db.changes)
    check("a cached read after ROLLBACK", pg_cached_read(s, 1))
  end

  # 22012 inside a transaction, then a read the aborted transaction
  # refused, parsed again after ROLLBACK.
  Db.with_connection do
    Db.exec("BEGIN")
    check("an error fails the transaction (22012)",
          statement_error_of { Db.exec("SELECT 1/0") }.include?("division by zero"))
    refused = statement_error_of do
      h = Db.prepare("SELECT id FROM " + s + "pg_widgets WHERE name = $1")
      Db.bind_text(h, 1, "two")
      Db.step?(h)
    end
    check("a new shape is refused (25P02)", refused.include?("current transaction is aborted"))
    Db.exec("ROLLBACK")
    h = Db.prepare("SELECT id FROM " + s + "pg_widgets WHERE name = $1")
    Db.bind_text(h, 1, "two")
    check("the refused shape reads after ROLLBACK", Db.step?(h))
    check_int("the refused shape's row", 2, Db.column_int(h, 0))
    Db.finalize(h)
  end

  # A lease that ends in a failed transaction leaves the next one usable.
  Db.with_connection do
    Db.exec("BEGIN")
    statement_error_of { Db.exec("SELECT 1/0") }
  end
  Db.with_connection do
    check("no transaction after a failed one's lease", !Db.transaction_open?)
    check("the next lease reads", pg_cached_read(s, 2))
  end
  nil
end

def pg_04_returning(s)
  # ── Writes that return rows ──
  h = Db.exec_returning("INSERT INTO " + s + "pg_widgets (name) VALUES ('ret-a'), ('ret-b') RETURNING id, name")
  check_int("returning changes", 2, Db.changes)
  check_int("returning column count", 2, Db.column_count(h))
  check("first returned row", Db.step?(h))
  first_id = Db.column_int(h, 0)
  check_str("first returned name", "ret-a", Db.column_text(h, 1))
  check("second returned row", Db.step?(h))
  check_int("returned ids ascend", first_id + 1, Db.column_int(h, 0))
  check_str("second returned name", "ret-b", Db.column_text(h, 1))
  check("returned rows end", !Db.step?(h))
  Db.finalize(h)
  h = Db.exec_returning("INSERT INTO " + s + "pg_gadgets (label) VALUES ('ret-uuid') RETURNING id")
  check("uuid key returned", Db.step?(h))
  check_int("uuid key as text", 36, Db.column_text(h, 0).length)
  Db.finalize(h)
  h = Db.exec_returning("UPDATE " + s + "pg_widgets SET weight = 0 WHERE name LIKE 'ret-%' RETURNING id")
  check_int("update returning changes", 2, Db.changes)
  Db.finalize(h)
  h = Db.exec_returning("DELETE FROM " + s + "pg_widgets WHERE id = -1 RETURNING id")
  check_int("no-match returning changes", 0, Db.changes)
  check("no-match returns no rows", !Db.step?(h))
  Db.finalize(h)
  ar_transaction do
    h = Db.exec_returning("DELETE FROM " + s + "pg_widgets WHERE name LIKE 'ret-%' RETURNING id")
    n = 0
    while Db.step?(h)
      n += 1
    end
    Db.finalize(h)
    check_int("returning inside a transaction", 2, n)
  end
  check_int("the transaction's returning write committed", 0, pg_count(s, "pg_widgets", "name LIKE 'ret-%'"))
  nil
end

def pg_05_prepared_statements(s)
  # ── Prepared statements: none leak, and the server may drop them ──
  Db.with_connection do
    base = pg_prepared_count
    i = 0
    found = 0
    while i < 100
      found += 1 if pg_cached_read(s, 1 + (i % 3))
      i += 1
    end
    check_int("100 reads of one shape", 100, found)
    check("one shape holds at most one prepared statement", pg_prepared_count <= base + 1)

    # Deallocated behind the shim's back, outside a transaction: the
    # caller never sees it.
    Db.exec("DEALLOCATE ALL")
    check("a read after DEALLOCATE ALL", pg_cached_read(s, 1))
    check("a second read after DEALLOCATE ALL", pg_cached_read(s, 2))
  end
  nil
end

def pg_06_commit_raises(s)
  # ── A COMMIT that raises still releases the transaction's connection ──
  # A BEGIN outside a lease binds a connection to this thread. Its
  # session is ended from another one, so the COMMIT raises; the thread
  # must not stay bound to the dead connection, and the next unleased
  # read must reach the server.
  Db.exec("BEGIN")
  check("BEGIN outside a lease binds the thread", Db.in_lease?)
  pid = pg_backend_pid
  pg_exec_on_other_session("SELECT pg_terminate_backend(" + pid.to_s + ")")
  raised = false
  begin
    Db.exec("COMMIT")
  rescue StandardError
    raised = true
  end
  check("COMMIT on a terminated session raises", raised)
  check("the binding is released though COMMIT raised", !Db.in_lease?)
  check("no transaction after the failed COMMIT", !Db.transaction_open?)
  check("an unleased read reconnects", pg_cached_read(s, 1))
  check("on a new session", pg_backend_pid != pid)
  nil
end

def run_postgres_cases(s)
  pg_01_tables(s)
  pg_02_sqlstate(s)
  pg_03_aborted_transaction(s)
  pg_04_returning(s)
  pg_05_prepared_statements(s)
  pg_06_commit_raises(s)
  nil
end
