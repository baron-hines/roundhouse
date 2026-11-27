# frozen_string_literal: true
# Broadcasts between processes, for a tree that doesn't send them through
# Redis: every broadcast stays in the process that made it, as Rails'
# `async` adapter keeps it. A tree on `queue_adapter :sidekiq` whose
# config/cable.yml names `adapter: redis` for production gets
# `runtime/spinel/sidekiq/redis_cable.rb` here instead, the same swap as
# `sidekiq_redis.rb`.
module RedisCable
  def self.enabled?
    false
  end

  def self.publish(stream, message_json)
    nil
  end

  def self.ensure_subscriber
    nil
  end
end
