# frozen_string_literal: true
# Where runtime/spinel/db_pg.rb's two drivers differ, for the pg gem
# (libpq) on CRuby, shipped as pg_driver.rb in place of spinel-pg's
# (runtime/spinel/pg_driver.rb). Everything else db_pg.rb asks of the
# driver is the API both share.
module PgDriver
  # A session. The gem's positional form means something else (host,
  # port, options, tty, dbname, user, password), so the settings go by
  # name. An empty password is no password, as it is to spinel-pg.
  def self.open(host, port, database, user, password)
    PG.connect(host: host, port: port, dbname: database, user: user, password: password.empty? ? nil : password)
  end

  # A result's command tag (`INSERT 0 1`), which the gem calls its status.
  def self.command_tag(result)
    result.cmd_status
  end

  # A driver error's SQLSTATE; empty for one without a server result, as
  # the gem raises a lost session (PG::ConnectionBad, PG::UnableToSend).
  def self.sqlstate(error)
    result = error.result
    result ? result.error_field(PG::PG_DIAG_SQLSTATE).to_s : ""
  end
end
