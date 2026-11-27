# frozen_string_literal: true
module Rack
  class MiniProfiler
    # The store the Railtie picks (`tmp/miniprofiler`), so a results request answered by another prefork worker still finds the page.
    class FileStore
      EXPIRES_IN_SECONDS = 60 * 60 * 24
      CLEANUP_CYCLE = 3600
      MAX_TOKEN_AGE = 1800

      def initialize
        @timer_lock = Mutex.new
        @view_lock = Mutex.new
        @token_lock = Mutex.new
        @next_cleanup = 0.0
      end

      def path
        MiniProfiler.config.storage_path
      end

      # Not Marshal, which the gem writes: nothing but this store reads the files, and the page is kept as the JSON it is served as.
      def save(id, json)
        @timer_lock.synchronize do
          prepare
          File.write(file("mp_timers", id), json)
        end
        nil
      end

      def load(id)
        f = file("mp_timers", id)
        @timer_lock.synchronize do
          File.exist?(f) ? File.read(f) : ""
        end
      end

      def set_unviewed(user, id)
        @view_lock.synchronize do
          ids = read_ids(user)
          ids << id
          write_ids(user, ids.uniq)
        end
        nil
      end

      def set_viewed(user, id)
        @view_lock.synchronize do
          ids = read_ids(user)
          ids.delete(id)
          write_ids(user, ids.uniq)
        end
        nil
      end

      def set_all_unviewed(user, ids)
        @view_lock.synchronize do
          write_ids(user, ids.uniq)
        end
        nil
      end

      def get_unviewed_ids(user)
        @view_lock.synchronize do
          read_ids(user)
        end
      end

      # A pair rotated every MAX_TOKEN_AGE, shared through the file so every worker accepts the cookie another one wrote.
      def allowed_tokens
        @token_lock.synchronize do
          f = file("tokens", "")
          token1 = ""
          token2 = ""
          cycle_at = 0.0
          if File.exist?(f)
            lines = File.read(f).split("\n")
            token1 = lines.length > 0 ? lines[0] : ""
            token2 = lines.length > 1 ? lines[1] : ""
            cycle_at = lines.length > 2 ? lines[2].to_f : 0.0
          end
          now = Process.clock_gettime(Process::CLOCK_MONOTONIC)
          unless cycle_at > now
            token2 = token1
            token1 = SecureRandom.hex
            cycle_at = now + MAX_TOKEN_AGE
          end
          prepare
          File.write(f, token1 + "\n" + token2 + "\n" + cycle_at.to_s + "\n")
          tokens = [token1]
          tokens << token2 unless token2.empty?
          tokens
        end
      end

      # Not the gem's cleanup thread: it would sleep in every prefork worker, and checking on save expires the same files.
      def cleanup_cache
        now = Time.now
        Dir.children(path).each do |name|
          next unless name.start_with?("mp_timers") || name.start_with?("mp_views")
          f = path + "/" + name
          File.delete(f) if (now - File.mtime(f)) > EXPIRES_IN_SECONDS
        end
        nil
      end

      def prepare
        dir = path.start_with?("/") ? "/" : ""
        path.split("/").each do |part|
          next if part.empty?
          dir = dir.empty? || dir.end_with?("/") ? dir + part : dir + "/" + part
          next if Dir.exist?(dir)
          begin
            Dir.mkdir(dir)
          rescue SystemCallError
            # Not raised: another worker may make it between the check and the mkdir.
          end
        end
        now = Process.clock_gettime(Process::CLOCK_MONOTONIC)
        if now >= @next_cleanup
          @next_cleanup = now + CLEANUP_CYCLE
          cleanup_cache
        end
        nil
      end

      def file(prefix, key)
        path + "/" + prefix + "_" + key
      end

      def read_ids(user)
        f = file("mp_views", user)
        ids = [""]
        ids.pop
        return ids unless File.exist?(f)
        File.read(f).split("\n").each { |id| ids << id unless id.empty? }
        ids
      end

      def write_ids(user, ids)
        prepare
        File.write(file("mp_views", user), ids.join("\n"))
        nil
      end
    end
  end
end
