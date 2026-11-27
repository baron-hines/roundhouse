# The Db contract suite on a SQLite shim: runtime/spinel/db.rb under
# Spinel (tests/spinel_db_contract.rs) and runtime/spinel/db_cruby.rb
# under CRuby (tests/db_contract.rs). The harness copies the shim here
# as db.rb, with the lane's prelude.rb (ActiveRecord's error classes)
# and the suite files, and passes a scratch database path as ARGV[0].
require_relative "prelude"
require_relative "db"
require_relative "support"
require_relative "contract_cases"

def ph(n)
  "?"
end

def key_column
  "integer PRIMARY KEY"
end

path = ARGV.length > 0 ? ARGV[0] : "contract.sqlite3"
Db.configure(path, pool_size: 2)
begin
  run_contract_cases("")
ensure
  Db.close
end
report_suite("contract")
