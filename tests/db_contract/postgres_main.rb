# The Db contract suite and the PostgreSQL cases on a PostgreSQL shim:
# runtime/spinel/db_pg.rb under Spinel today (tests/spinel_pg_db.rs).
# The harness copies the shim here as db.rb, with the lane's prelude.rb
# (ActiveRecord's error classes), its lane file as lane.rb, and the
# suite files. ROUNDHOUSE_PG_TEST_URL names the server and
# ROUNDHOUSE_PG_TEST_SCHEMA the run's disposable schema, created here
# and dropped at the end with everything in it.
require_relative "prelude"
require_relative "db"
require_relative "support"
require_relative "contract_cases"
require_relative "postgres_cases"
require_relative "lane"

def ph(n)
  "$" + n.to_s
end

def key_column
  "bigserial PRIMARY KEY"
end

url = ENV.fetch("ROUNDHOUSE_PG_TEST_URL", "")
schema = ENV.fetch("ROUNDHOUSE_PG_TEST_SCHEMA", "")
raise "ROUNDHOUSE_PG_TEST_URL and ROUNDHOUSE_PG_TEST_SCHEMA must be set" if url == "" || schema == ""

lane_before
lane_checks = $checks
$checks = 0

Db.configure(url, pool_size: 2)
Db.exec("DROP SCHEMA IF EXISTS " + schema + " CASCADE")
Db.exec("CREATE SCHEMA " + schema)
begin
  run_contract_cases(schema + ".")
  report_suite("contract")
  run_postgres_cases(schema + ".")
  report_suite("postgres")
  lane_cases(url, schema)
ensure
  # Close every session first: a failed case can leave one holding locks
  # the DROP would otherwise wait on forever.
  Db.close
  Db.configure(url, pool_size: 1)
  Db.exec("DROP SCHEMA IF EXISTS " + schema + " CASCADE")
  Db.exec("DROP SCHEMA IF EXISTS " + schema + "_own CASCADE")
  Db.close
end
lane_after(url)
$checks += lane_checks
report_suite(lane_name)
