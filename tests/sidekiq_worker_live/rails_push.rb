# Pushes jobs the way a Rails app on queue_adapter :sidekiq does: real
# activejob through Sidekiq's own ActiveJob adapter, to REDIS_URL. Each
# argument is `JobClass:label`; the job classes are the ones worker.rb
# runs, so the Roundhouse worker reads a payload Rails wrote.
require "active_job"
require "sidekiq"
require File.join(Gem.loaded_specs["sidekiq"].full_gem_path,
  "lib/active_job/queue_adapters/sidekiq_adapter")

Sidekiq.configure_client do |config|
  config.redis = { url: ENV.fetch("REDIS_URL") }
  config.logger.level = Logger::WARN
end
ActiveJob::Base.queue_adapter = :sidekiq
ActiveJob::Base.logger = Logger.new(nil)

class RecordJob < ActiveJob::Base
  def perform(label); end
end

class SlowJob < ActiveJob::Base
  def perform(label); end
end

ARGV.each do |spec|
  job_class, label = spec.split(":", 2)
  Object.const_get(job_class).perform_later(label)
end
puts "pushed #{ARGV.length}"
