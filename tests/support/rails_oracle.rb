# The Rails a test's native control loads, chosen in one place instead of
# a `gem "activerecord", "8.1.4"` line in every probe.
#
# By default it is the installed release gems at DEFAULT_VERSION, the
# release the rest of the suite pins.
#
#   RH_RAILS_ORACLE=8.1.4    the installed release gems of that version
#   RH_RAILS_ORACLE=<path>   a Rails checkout (a directory holding
#                            activesupport/, activerecord/, ...). Its
#                            components come from the checkout; the gems
#                            they depend on come from the current gem path.
#
# `RailsOracle.activate("activerecord", sqlite3: ">= 2.9")` must run before
# any Rails file is required. The named Rails components come from the
# oracle; any other gem takes a RubyGems requirement, so a control states
# the oldest release it needs rather than one exact build.
module RailsOracle
  DEFAULT_VERSION = "8.1.4"
  COMPONENTS = %w[
    activesupport activemodel activerecord activejob actionpack actionview
    actionmailer actioncable activestorage actiontext actionmailbox railties
  ].freeze

  def self.source
    value = ENV["RH_RAILS_ORACLE"].to_s
    value.empty? ? DEFAULT_VERSION : value
  end

  # A release version (`8.1.4`) rather than a checkout.
  def self.release?
    source.match?(/\A\d+(\.[0-9A-Za-z]+)+\z/)
  end

  # What the oracle is, for messages.
  def self.label
    release? ? "Rails #{source} (installed gems)" : "Rails #{version} (#{source})"
  end

  def self.version
    release? ? source : File.read(File.join(source, "RAILS_VERSION")).strip
  rescue SystemCallError
    "unknown"
  end

  # Whether `activate` would succeed for these components and
  # requirements, without activating anything, so a test can skip with a
  # reason instead of failing.
  def self.available?(*gems, **requirements)
    components = if release?
      gems.all? { |name| Gem::Specification.find_all_by_name(name, source).any? }
    else
      checkout_resolves?(gems)
    end
    components && requirements.all? { |name, requirement| Gem::Dependency.new(name.to_s, *Array(requirement)).matching_specs.any? }
  end

  # Whether these checkout components are there, and each gem they depend
  # on from outside the checkout is installed.
  def self.checkout_resolves?(names, seen = {})
    names.all? do |name|
      next true if seen[name]
      seen[name] = true
      spec = checkout_spec(name) or next false
      spec.runtime_dependencies.all? do |dependency|
        checkout_spec(dependency.name) ? checkout_resolves?([dependency.name], seen) : dependency.matching_specs.any?
      end
    end
  end

  # A component's gemspec from the checkout, or nil when it has none.
  def self.checkout_spec(name)
    dir = File.expand_path(File.join(source, name))
    path = File.join(dir, "#{name}.gemspec")
    return unless File.exist?(path)
    spec = Dir.chdir(dir) { Gem::Specification.load(path) } or abort "cannot load #{path}"
    spec.loaded_from = path
    spec.full_gem_path = dir
    spec
  end

  # The environment a child Ruby needs to load the same oracle. Upstream
  # that is nothing: the child inherits RH_RAILS_ORACLE and the gem path.
  def self.env
    {}
  end

  # Activates the named Rails components from the oracle and each other
  # gem at its requirement (`sqlite3: ">= 2.9"`).
  def self.activate(*gems, **requirements)
    if release?
      gems.each { |name| gem name, source }
    else
      abort "RH_RAILS_ORACLE: no Rails checkout at #{source}" unless File.directory?(File.join(source, "activesupport"))
      versions = register_checkout
      gems.each do |name|
        # Exactly the checkout's version: RubyGems prefers an installed
        # release over a checkout's prerelease (`8.2.0.alpha`) otherwise.
        version = versions.fetch(name) { abort "#{name} is not in the Rails checkout at #{source}" }
        gem name, "= #{version}"
        loaded = Gem.loaded_specs.fetch(name).full_gem_path
        abort "#{name} activated from #{loaded}, not #{source}" unless loaded.start_with?(File.join(File.expand_path(source), ""))
      end
    end
    requirements.each { |name, requirement| gem name.to_s, *Array(requirement) }
  end

  # Registers each component's gemspec from the checkout, as Bundler does
  # for a git source, so `gem "activesupport"` (and any gem depending on
  # it) activates the checkout. Answers each registered component's version.
  def self.register_checkout
    COMPONENTS.each_with_object({}) do |name, versions|
      spec = checkout_spec(name) or next
      # RubyGems treats an installed release of the same version as this
      # spec and keeps it, so a stable-branch checkout at 8.1.4 would lose
      # to an installed 8.1.4. The checkout replaces it.
      Gem::Specification.stubs_for(name).select { |installed| installed.version == spec.version }.each { |installed| Gem::Specification.remove_spec(installed) }
      Gem::Specification.add_spec(spec)
      versions[name] = spec.version.to_s
    end
  end
end
