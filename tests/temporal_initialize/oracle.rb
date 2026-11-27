require_relative "../support/rails_oracle"
RailsOracle.activate("activerecord", sqlite3: ">= 2.9")
require "active_record"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
require_relative "schema"
class TemporalEntry < ActiveRecord::Base
end
require_relative "probe"
