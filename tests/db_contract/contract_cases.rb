# The Db contract (runtime/ruby/db.rbs), checked the same way on every
# shim: binds (nil binds as NULL), NULL reads through the `*_opt`
# columns, stepping, zero rows, `column_value`, handle and lease
# lifecycle, constraint errors as ActiveRecord's classes, transaction
# depth, a nested transaction joining the outer one, and rollback on a
# raise.
#
# Nothing here is specific to one database or one runtime. The lane's
# main file (sqlite_main.rb, postgres_main.rb) supplies the two places
# the SQL dialects differ:
#
#   ph(n)       — the n-th placeholder: "?" on SQLite, "$n" on PostgreSQL.
#   key_column  — DDL for an integer key the database assigns.
#
# `s` prefixes every table name: "" on SQLite, "<schema>." on
# PostgreSQL, where each run gets a disposable schema.
#
# Each case is its own method: one top-level function holding every
# case makes Spinel's C compiler crawl.

def contract_count(s, table, where)
  h = Db.prepare_uncached("SELECT count(*) FROM " + s + table + " WHERE " + where)
  Db.step?(h)
  n = Db.column_int(h, 0)
  Db.finalize(h)
  n
end

def contract_probe_name(s)
  h = Db.prepare_uncached("SELECT name FROM " + s + "probes WHERE id = 1")
  Db.step?(h)
  v = Db.column_text(h, 0)
  Db.finalize(h)
  v
end

def contract_01_tables(s)
  Db.exec("CREATE TABLE " + s + "widgets (id " + key_column + ", name text NOT NULL, " +
          "active boolean, weight float8, made_at timestamp(6), gadget_id integer)")
  Db.exec("CREATE TABLE " + s + "tags (id " + key_column + ", name text NOT NULL UNIQUE)")
  Db.exec("CREATE TABLE " + s + "probes (id integer PRIMARY KEY, name text NOT NULL)")
  Db.exec("INSERT INTO " + s + "probes (id, name) VALUES (1, 'before')")
  check_int("depth starts at 0", 0, Db._txn_depth)
  check("no lease at the start", !Db.in_lease?)
  nil
end

def contract_02_inline_writes(s)
  # ── Inline writes through the escape helpers ──
  Db.exec("INSERT INTO " + s + "widgets (name, active, weight, made_at, gadget_id) VALUES (" +
          Db.escape_string("Widget's \"first\"") + ", " + Db.escape_bool(true) + ", " +
          Db.escape_float_opt(1.5) + ", '2024-01-31 12:34:56.123456', " + Db.escape_int(7) + ")")
  check_int("insert changes", 1, Db.changes)
  check_int("first assigned key", 1, Db.last_insert_rowid)
  Db.exec("INSERT INTO " + s + "widgets (name, active, weight, made_at, gadget_id) VALUES (" +
          Db.escape_string_opt("gädget ✓ \\ back") + ", " + Db.escape_bool_opt(false) + ", " +
          Db.escape_float_opt(nil) + ", NULL, " + Db.escape_int_opt(nil) + ")")
  check_int("second assigned key", 2, Db.last_insert_rowid)
  Db.exec("INSERT INTO " + s + "widgets (name, active, weight, gadget_id) VALUES (" +
          Db.escape_string("third") + ", " + Db.escape_bool_opt(nil) + ", 3.25, " + Db.escape_int_opt(7) + ")")
  check_int("third assigned key", 3, Db.last_insert_rowid)
  # The list helpers are checked by what the database makes of them:
  # their spacing is each shim's own.
  check_int("escape_int_list", 2, contract_count(s, "widgets", "id IN (" + Db.escape_int_list([1, 3, 99]) + ")"))
  check_int("escape_int_list empty matches nothing", 0, contract_count(s, "widgets", "id IN (" + Db.escape_int_list([]) + ")"))
  check_int("escape_string_list", 2, contract_count(s, "widgets", "name IN (" + Db.escape_string_list(["third", "Widget's \"first\""]) + ")"))
  check_str("escape_string_opt nil", "NULL", Db.escape_string_opt(nil))
  check_str("escape_int_opt nil", "NULL", Db.escape_int_opt(nil))
  check_str("escape_float_opt nil", "NULL", Db.escape_float_opt(nil))
  check_str("escape_bool_opt nil", "NULL", Db.escape_bool_opt(nil))
  nil
end

def contract_03_bound_reads(s)
  # ── Bound reads ──
  h = Db.prepare("SELECT id, name, active, weight, made_at, gadget_id FROM " + s + "widgets WHERE id = " + ph(1))
  check_raises_any("bind index 0 is refused (binds are one-based)") { Db.bind_int(h, 0, 1) }
  Db.finalize(h)

  h = Db.prepare("SELECT id, name, active, weight, made_at, gadget_id FROM " + s + "widgets WHERE id = " + ph(1))
  Db.bind_int(h, 1, 1)
  check_int("column_count", 6, Db.column_count(h))
  check_str("column_name", "gadget_id", Db.column_name(h, 5))
  check("bound int finds the row", Db.step?(h))
  check_int("column_int", 1, Db.column_int(h, 0))
  check_str("quotes round-trip", "Widget's \"first\"", Db.column_text(h, 1))
  check("column_bool true", Db.column_bool(h, 2))
  check("column_bool_opt true", Db.column_bool_opt(h, 2) == true)
  check_int("bool through column_int", 1, Db.column_int(h, 2))
  check("column_float", Db.column_float(h, 3) == 1.5)
  check("column_float_opt present", Db.column_float_opt(h, 3) == 1.5)
  check_int("column_int_opt present", 7, Db.column_int_opt(h, 5))
  check_str("timestamp text round-trips", "2024-01-31 12:34:56.123456", Db.column_text_opt(h, 4))
  check("one row only", !Db.step?(h))
  # Not checked here: a step? after the one that answered false. db.rb
  # hands it to sqlite3_step, which resets a finished statement and runs
  # it again, so the read starts over; db_cruby.rb and db_pg.rb stay
  # done (db_pg_lane.rb checks it). Nothing the lowerer emits steps past
  # the end.
  Db.finalize(h)

  h = Db.prepare("SELECT name, active, weight, made_at, gadget_id FROM " + s + "widgets WHERE id = " + ph(1))
  Db.bind_int(h, 1, 2)
  check("second row", Db.step?(h))
  check_str("UTF-8 and backslash round-trip", "gädget ✓ \\ back", Db.column_text(h, 0))
  check_str("column_text_opt present", "gädget ✓ \\ back", Db.column_text_opt(h, 0))
  check("false reads false", !Db.column_bool(h, 1))
  check("false reads false when nullable", Db.column_bool_opt(h, 1) == false)
  check_int("bool false through column_int", 0, Db.column_int(h, 1))
  check("NULL float reads nil", Db.column_float_opt(h, 2).nil?)
  check("NULL float reads 0.0", Db.column_float(h, 2) == 0.0)
  check("NULL text reads nil", Db.column_text_opt(h, 3).nil?)
  check_str("NULL text reads empty", "", Db.column_text(h, 3))
  check("NULL int reads nil", Db.column_int_opt(h, 4).nil?)
  check_int("NULL int reads 0", 0, Db.column_int(h, 4))
  Db.finalize(h)

  h = Db.prepare("SELECT active FROM " + s + "widgets WHERE id = " + ph(1))
  Db.bind_int(h, 1, 3)
  check("third row", Db.step?(h))
  check("NULL bool reads nil", Db.column_bool_opt(h, 0).nil?)
  check("NULL bool reads false", !Db.column_bool(h, 0))
  Db.finalize(h)
  nil
end

def contract_04_binds(s)
  # Text and bool binds together.
  h = Db.prepare("SELECT id FROM " + s + "widgets WHERE name = " + ph(1) + " AND active = " + ph(2))
  Db.bind_text(h, 1, "gädget ✓ \\ back")
  Db.bind_bool(h, 2, false)
  check("text + bool binds", Db.step?(h))
  check_int("text + bool row", 2, Db.column_int(h, 0))
  Db.finalize(h)

  # A nullable predicate renders IS NULL with no slot; the next value
  # keeps position 1.
  h = Db.prepare("SELECT id FROM " + s + "widgets WHERE gadget_id IS NULL AND name = " + ph(1))
  Db.bind_text(h, 1, "gädget ✓ \\ back")
  check("IS NULL keeps positions aligned", Db.step?(h))
  check_int("IS NULL row", 2, Db.column_int(h, 0))
  Db.finalize(h)

  # nil binds are SQL NULL, never the type's zero.
  h = Db.prepare("SELECT CAST(" + ph(1) + " AS integer) IS NULL, CAST(" + ph(2) + " AS text) IS NULL, " +
                 "CAST(" + ph(3) + " AS boolean) IS NULL, CAST(" + ph(4) + " AS integer) IS NULL, " +
                 "CAST(" + ph(5) + " AS boolean) IS NULL, CAST(" + ph(6) + " AS boolean)")
  Db.bind_int_opt(h, 1, nil)
  Db.bind_text_opt(h, 2, nil)
  Db.bind_bool_opt(h, 3, nil)
  Db.bind_int(h, 4, nil)
  Db.bind_bool(h, 5, nil)
  Db.bind_bool(h, 6, true)
  check("nil binds row", Db.step?(h))
  check("bind_int_opt nil is NULL", Db.column_bool(h, 0))
  check("bind_text_opt nil is NULL", Db.column_bool(h, 1))
  check("bind_bool_opt nil is NULL", Db.column_bool(h, 2))
  check("bind_int nil is NULL", Db.column_bool(h, 3))
  check("bind_bool nil is NULL", Db.column_bool(h, 4))
  check("bind_bool true is true", Db.column_bool(h, 5))
  Db.finalize(h)

  # Present values through the nullable binds.
  h = Db.prepare("SELECT id FROM " + s + "widgets WHERE gadget_id = " + ph(1) + " AND name = " + ph(2) +
                 " AND active = " + ph(3))
  Db.bind_int_opt(h, 1, 7)
  Db.bind_text_opt(h, 2, "Widget's \"first\"")
  Db.bind_bool_opt(h, 3, true)
  check("present nullable binds", Db.step?(h))
  check_int("present nullable binds row", 1, Db.column_int(h, 0))
  Db.finalize(h)
  nil
end

def contract_05_stepping(s)
  # Several rows, in order, then done.
  h = Db.prepare_uncached("SELECT id FROM " + s + "widgets WHERE id >= " + ph(1) + " ORDER BY id")
  Db.bind_int(h, 1, 1)
  ids = []
  while Db.step?(h)
    ids.push(Db.column_int(h, 0))
  end
  Db.finalize(h)
  check_str("multi-row stepping", "[1, 2, 3]", ids.inspect)

  # The same shape, cached, read twice: each read starts at the first row.
  i = 0
  while i < 2
    h = Db.prepare("SELECT id FROM " + s + "widgets WHERE id >= " + ph(1) + " ORDER BY id")
    Db.bind_int(h, 1, 2)
    check("a cached shape reads from its first row", Db.step?(h))
    check_int("a cached shape's first row", 2, Db.column_int(h, 0))
    Db.finalize(h)
    i += 1
  end

  # Zero rows.
  h = Db.prepare("SELECT id, name FROM " + s + "widgets WHERE id = " + ph(1))
  Db.bind_int(h, 1, 999)
  check("zero rows", !Db.step?(h))
  check_int("column_count with zero rows", 2, Db.column_count(h))
  Db.finalize(h)
  nil
end

def contract_06_column_value(s)
  # column_value: the driver's native kinds, booleans as 1/0.
  h = Db.prepare("SELECT CAST(42 AS bigint), CAST(2.5 AS float8), CAST('x' AS text), " +
                 "CAST(NULL AS integer), active FROM " + s + "widgets WHERE id = " + ph(1))
  Db.bind_int(h, 1, 1)
  check("column_value row", Db.step?(h))
  check_str("column_value int", "42", Db.column_value(h, 0).inspect)
  check_str("column_value float", "2.5", Db.column_value(h, 1).inspect)
  check_str("column_value text", "\"x\"", Db.column_value(h, 2).inspect)
  check("column_value NULL", Db.column_value(h, 3).nil?)
  check_str("column_value true", "1", Db.column_value(h, 4).inspect)
  Db.finalize(h)
  h = Db.prepare("SELECT active FROM " + s + "widgets WHERE id = " + ph(1))
  Db.bind_int(h, 1, 2)
  check("column_value false row", Db.step?(h))
  check_str("column_value false", "0", Db.column_value(h, 0).inspect)
  Db.finalize(h)
  nil
end

def contract_07_writes(s)
  # ── Writes and their row counts ──
  Db.exec("UPDATE " + s + "widgets SET weight = 9.5 WHERE gadget_id = " + Db.escape_int(7))
  check_int("update changes", 2, Db.changes)
  Db.exec("UPDATE " + s + "widgets SET weight = 1 WHERE id = 999")
  check_int("update with no match", 0, Db.changes)
  check_int("updated rows visible", 2, contract_count(s, "widgets", "weight = 9.5"))
  Db.exec("DELETE FROM " + s + "widgets WHERE id = 3")
  check_int("delete changes", 1, Db.changes)
  Db.exec("INSERT INTO " + s + "widgets (name) VALUES ('fourth')")
  check("a key after a delete is a new one", Db.last_insert_rowid > 2)
  Db.exec("DELETE FROM " + s + "widgets WHERE name = 'fourth'")
  nil
end

def contract_08_handles(s)
  # ── Handle lifecycle ──
  # A read finalized part way releases its handle; the shape reads
  # again from the start.
  h = Db.prepare("SELECT id FROM " + s + "widgets ORDER BY id")
  check("partial read", Db.step?(h))
  Db.finalize(h)
  h = Db.prepare("SELECT id FROM " + s + "widgets ORDER BY id")
  check("re-prepared after a partial read", Db.step?(h))
  check_int("re-prepared from the first row", 1, Db.column_int(h, 0))
  # Two handles at once read independently.
  h2 = Db.prepare_uncached("SELECT id FROM " + s + "widgets ORDER BY id DESC")
  check("a second handle", Db.step?(h2))
  check("two handles are distinct", h2 != h)
  check_int("the second handle's row", 2, Db.column_int(h2, 0))
  check("the first handle keeps its place", Db.step?(h))
  check_int("the first handle's next row", 2, Db.column_int(h, 0))
  Db.finalize(h2)
  Db.finalize(h)
  nil
end

def contract_09_leases(s)
  # ── Leases ──
  check("no lease outside", !Db.in_lease?)
  got = Db.with_connection do
    check("lease inside", Db.in_lease?)
    nested = Db.with_connection { Db.in_lease? }
    check("a nested lease is re-entrant", nested)
    check("still leased after a nested lease", Db.in_lease?)
    41 + 1
  end
  check_int("a lease answers its block's value", 42, got)
  check("no lease after the block", !Db.in_lease?)

  # A handle left open at the lease's end does not outlive it: the
  # shape reads afresh in the next lease.
  Db.with_connection do
    h = Db.prepare("SELECT id FROM " + s + "widgets WHERE id >= " + ph(1) + " ORDER BY id")
    Db.bind_int(h, 1, 1)
    Db.step?(h)
  end
  Db.with_connection do
    h = Db.prepare("SELECT id FROM " + s + "widgets WHERE id >= " + ph(1) + " ORDER BY id")
    Db.bind_int(h, 1, 2)
    check("a shape left open by a lease reads afresh", Db.step?(h))
    check_int("its first row in the next lease", 2, Db.column_int(h, 0))
    Db.finalize(h)
  end

  # A lease releases its connection when the block raises, more times
  # than the pool holds: a leak would leave the last lease waiting.
  raised = 0
  i = 0
  while i < 6
    begin
      Db.with_connection do
        Db.prepare("SELECT 1")
        raise "widget failure"
      end
    rescue RuntimeError => e
      raised += 1 if e.message == "widget failure"
    end
    i += 1
  end
  check_int("every raise reaches the caller", 6, raised)
  check("no lease after the raises", !Db.in_lease?)
  check_int("a lease after the raises still serves", 7, Db.with_connection { 7 })
  nil
end

def contract_10_errors(s)
  # ── Constraint errors are ActiveRecord's classes ──
  # The messages are each database's own; PostgreSQL's SQLSTATE mapping
  # is checked in postgres_cases.rb.
  Db.exec("INSERT INTO " + s + "tags (name) VALUES ('red')")
  dup = false
  begin
    Db.exec("INSERT INTO " + s + "tags (name) VALUES ('red')")
  rescue ActiveRecord::RecordNotUnique
    dup = true
  end
  check("a unique violation is RecordNotUnique", dup)
  null_violation = false
  begin
    Db.exec("INSERT INTO " + s + "tags (name) VALUES (NULL)")
  rescue ActiveRecord::NotNullViolation
    null_violation = true
  end
  check("a NOT NULL violation is NotNullViolation", null_violation)
  invalid = false
  begin
    Db.exec("SELEC 1")
  rescue ActiveRecord::StatementInvalid
    invalid = true
  end
  check("a syntax error is StatementInvalid", invalid)
  h = Db.prepare("SELECT count(*) FROM " + s + "tags WHERE name = " + ph(1))
  Db.bind_text(h, 1, "red")
  check("usable after errors", Db.step?(h))
  check_int("the failed writes left one row", 1, Db.column_int(h, 0))
  Db.finalize(h)
  nil
end

def contract_11_raw_transactions(s)
  # ── Transactions spelled as statements ──
  check("no transaction open", !Db.transaction_open?)
  Db.exec("BEGIN")
  check("BEGIN opens a transaction", Db.transaction_open?)
  Db.exec("INSERT INTO " + s + "tags (name) VALUES ('tmp')")
  Db.exec("ROLLBACK")
  check("ROLLBACK closes it", !Db.transaction_open?)
  check_int("a rolled-back transaction leaves no row", 0, contract_count(s, "tags", "name = 'tmp'"))
  check("unpinned after ROLLBACK", !Db.in_lease?)

  Db.exec("BEGIN")
  Db.exec("INSERT INTO " + s + "tags (name) VALUES ('kept')")
  Db.exec("COMMIT")
  check("COMMIT closes it", !Db.transaction_open?)
  check_int("a committed transaction keeps its row", 1, contract_count(s, "tags", "name = 'kept'"))

  # A lease that ends inside a transaction rolls it back before the
  # connection is reused.
  check_raises("abandoned transaction", "tag failure") do
    Db.with_connection do
      Db.exec("BEGIN")
      Db.exec("INSERT INTO " + s + "tags (name) VALUES ('grn')")
      raise "tag failure"
    end
  end
  check_int("an abandoned transaction is rolled back", 0, contract_count(s, "tags", "name = 'grn'"))
  check("no transaction after the abandoned lease", !Db.transaction_open?)

  # A BEGIN outside a lease keeps its connection for a lease taken
  # inside it: the lease's write belongs to the transaction.
  Db.exec("BEGIN")
  Db.with_connection do
    Db.exec("INSERT INTO " + s + "tags (name) VALUES ('yel')")
  end
  Db.exec("ROLLBACK")
  check("unpinned after the joined lease", !Db.in_lease?)
  check_int("a lease inside a transaction joins it", 0, contract_count(s, "tags", "name = 'yel'"))
  nil
end

def contract_12_ar_flat(s)
  # ── ActiveRecord::Base.transaction on the shim: flat ──
  ar_transaction { Db.exec("UPDATE " + s + "probes SET name = 'committed' WHERE id = 1") }
  check_int("depth returns to 0 after a commit", 0, Db._txn_depth)
  check_str("a flat transaction commits", "committed", contract_probe_name(s))

  raised = ""
  begin
    ar_transaction do
      Db.exec("UPDATE " + s + "probes SET name = 'should-not-stick' WHERE id = 1")
      raise "boom"
    end
  rescue StandardError => e
    raised = e.message
  end
  check_str("a flat transaction's raise propagates", "boom", raised)
  check_int("depth returns to 0 after a rollback", 0, Db._txn_depth)
  check_str("a flat transaction rolls back on raise", "committed", contract_probe_name(s))
  check("no transaction left open", !Db.transaction_open?)

  ar_transaction do
    Db.exec("UPDATE " + s + "probes SET name = 'rolled-back' WHERE id = 1")
    raise ActiveRecord::Rollback
  end
  check_int("depth returns to 0 after Rollback", 0, Db._txn_depth)
  check_str("ActiveRecord::Rollback rolls back quietly", "committed", contract_probe_name(s))
  nil
end

def contract_13_ar_nested(s)
  # ── A nested transaction joins the outer one ──
  # A nested `transaction` sends no BEGIN of its own (Rails'
  # `requires_new: false` default); depth 0 -> 1 -> 2 -> 1 -> 0 and one
  # commit covering both writes is the observable proof of the join.
  ar_transaction do
    check_int("depth is 1 in the outer block", 1, Db._txn_depth)
    check("the outer block is in a transaction", Db.transaction_open?)
    Db.exec("UPDATE " + s + "probes SET name = 'outer' WHERE id = 1")
    ar_transaction do
      check_int("depth is 2 in the joined nested block", 2, Db._txn_depth)
      Db.exec("UPDATE " + s + "probes SET name = 'inner-committed' WHERE id = 1")
    end
    check_int("depth returns to 1 after the nested block returns", 1, Db._txn_depth)
  end
  check_int("depth returns to 0 after the outer block commits", 0, Db._txn_depth)
  check_str("nested and outer commit together", "inner-committed", contract_probe_name(s))

  # A raise out of the outer block, after a joined nested block
  # returned, rolls back the nested block's write with the outer one.
  raised = ""
  begin
    ar_transaction do
      ar_transaction do
        Db.exec("UPDATE " + s + "probes SET name = 'joined-then-undone' WHERE id = 1")
      end
      raise "outer failure"
    end
  rescue StandardError => e
    raised = e.message
  end
  check_str("the outer raise propagates", "outer failure", raised)
  check_int("depth returns to 0 after the outer rollback", 0, Db._txn_depth)
  check_str("a joined write rolls back with the outer one", "inner-committed", contract_probe_name(s))
  nil
end

def contract_14_capture(s)
  # ── Query capture ──
  log = Db.capture_sql do
    h = Db.prepare_uncached("SELECT 1")
    Db.finalize(h)
    Db.exec("UPDATE " + s + "probes SET name = name WHERE id = 0")
  end
  check_str("capture_sql", "[\"SELECT 1\", \"UPDATE " + s + "probes SET name = name WHERE id = 0\"]", log.inspect)
  nil
end

def run_contract_cases(s)
  contract_01_tables(s)
  contract_02_inline_writes(s)
  contract_03_bound_reads(s)
  contract_04_binds(s)
  contract_05_stepping(s)
  contract_06_column_value(s)
  contract_07_writes(s)
  contract_08_handles(s)
  contract_09_leases(s)
  contract_10_errors(s)
  contract_11_raw_transactions(s)
  contract_12_ar_flat(s)
  contract_13_ar_nested(s)
  contract_14_capture(s)
  nil
end
