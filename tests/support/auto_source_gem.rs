//! An installed plain-Ruby gem the compiler neither models nor ports,
//! shared by the Ruby lane and the Spinel lane: the Spinel tree compiles
//! its source with the app, along with the gem it declares and names,
//! while the ruby family keeps loading the real gem. The Spinel tree
//! carries only the part the app reaches.

use super::emit_and_run;

const GEMS: &str = "vendor/bundle/ruby/3.4.0/gems";

pub fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "Gemfile.lock",
            "    debug (1.11.1)\n",
            "    exclaim (0.1)\n    greetings (1.0)\n      exclaim\n    debug (1.11.1)\n",
        )
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  greetings\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{GEMS}/greetings-1.0/lib/greetings.rb"), "require \"exclaim\"\nrequire \"greetings/formatter\"\nrequire \"greetings/bindings\"\nrequire \"greetings/card\"\n")
        .write(
            &format!("{GEMS}/greetings-1.0/lib/greetings/formatter.rb"),
            "module Greetings\n  class Formatter\n    def self.shout(text)\n      Exclaim.mark(text.upcase)\n    end\n\n    def self.bound(values)\n      Bindings.new(values)\n    end\n  end\nend\n",
        )
        // Per-object methods, as parslet's `Context` defines them: spinel refuses the shape, and
        // only `Formatter.bound`, which the app never calls, reaches it.
        .write(
            &format!("{GEMS}/greetings-1.0/lib/greetings/bindings.rb"),
            "module Greetings\n  class Bindings\n    def initialize(values)\n      values.each do |key, value|\n        singleton_class.send(:define_method, key) { value }\n      end\n    end\n  end\nend\n",
        )
        // Option writers reached only by a name built at run time, as svg-graph's
        // `init_with` reaches `TimeSeries#min_x_value=`.
        .write(
            &format!("{GEMS}/greetings-1.0/lib/greetings/card.rb"),
            "module Greetings\n  class Panel\n    attr_accessor :width\n\n    def initialize(options)\n      options.each { |key, value| send(key.to_s + \"=\", value) }\n    end\n  end\n\n  class Card < Panel\n    protected\n\n    def width=(value)\n      @width = value.to_i * 2\n    end\n  end\nend\n",
        )
        .write(
            &format!("{GEMS}/exclaim-0.1/lib/exclaim.rb"),
            "module Exclaim\n  def self.mark(text)\n    text + \"!\"\n  end\nend\n",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def shouted_title\n    Greetings::Formatter.shout(title)\n  end\n\n  def card_width\n    Greetings::Card.new({ width: \"7\" }).width\n  end\n\n",
        )
}

pub const ASSERTIONS: &str = r#"got = Article.new(title: "hello", body: "a body that is long enough").shouted_title
raise "got #{got.inspect}" unless got == "HELLO!"
width = Article.new(title: "hello", body: "a body that is long enough").card_width
raise "width #{width.inspect}" unless width == 14
puts "auto source gem OK"
"#;

/// message_bus's `Codec::Base`: a gem method raises a bare constant the gem
/// defines only in another class (`Backends::Base::MustImplement`). From
/// `Codec::Base`, Ruby looks in `Codec::Base`, `Codec`, `Greetings`, Object
/// and the class's ancestors, finds none, and raises NameError, which the
/// emitted program does too. A nested constant no gem defines, read under
/// `defined?` (redis's `::RedisClient::Cluster::InitialSetupError` without
/// redis-clustering), is absent the same way.
pub fn misnamed_overlay() -> emit_and_run::Overlay {
    overlay()
        .edit(
            &format!("{GEMS}/greetings-1.0/lib/greetings.rb"),
            "require \"greetings/card\"\n",
            "require \"greetings/card\"\nrequire \"greetings/backends\"\nrequire \"greetings/codec\"\n",
        )
        .write(
            &format!("{GEMS}/greetings-1.0/lib/greetings/backends.rb"),
            "module Greetings\n  module Backends\n    class Base\n      MustImplement = Class.new(StandardError)\n\n      def deliver\n        raise MustImplement\n      end\n    end\n  end\nend\n",
        )
        .write(
            &format!("{GEMS}/greetings-1.0/lib/greetings/codec.rb"),
            "module Greetings\n  module Codec\n    class Base\n      def encode(text)\n        raise MustImplement\n      end\n    end\n\n    class Plain < Base\n      def encode(text)\n        text.to_s\n      end\n\n      def self.cluster_error?(error)\n        defined?(::Greetings::Cluster::SetupError) && error.is_a?(::Greetings::Cluster::SetupError)\n      end\n    end\n  end\nend\n",
        )
        .edit(
            "app/models/article.rb",
            "  def shouted_title\n",
            "  def encoded_title\n    Greetings::Codec::Plain.new.encode(title)\n  end\n\n  def abstract_encode\n    Greetings::Codec::Base.new.encode(title)\n  rescue NameError => e\n    e.message\n  end\n\n  def cluster_error\n    Greetings::Codec::Plain.cluster_error?(StandardError.new)\n  end\n\n  def abstract_deliver\n    Greetings::Backends::Base.new.deliver\n  rescue StandardError => e\n    e.class.name\n  end\n\n  def shouted_title\n",
        )
}

pub const MISNAMED_ASSERTIONS: &str = r#"a = Article.new(title: "hello", body: "a body that is long enough")
got = [a.encoded_title, a.abstract_encode, a.abstract_deliver, a.cluster_error]
raise got.inspect unless got == ["hello", "uninitialized constant Greetings::Codec::Base::MustImplement", "Greetings::Backends::Base::MustImplement", nil]
puts "misnamed gem constant OK"
"#;

/// message_bus's `MessageBus::BACKENDS = {}` and redis-client's
/// `CommandError::ERRORS = { … }` in `class RedisClient`: a constant a gem
/// assigns into another class, at a file's top level and in a class body.
/// Ruby defines it on that class.
pub fn top_level_constant_overlay() -> emit_and_run::Overlay {
    misnamed_overlay()
        .write(&format!("{GEMS}/greetings-1.0/lib/greetings/registry.rb"), "module Greetings; end\nGreetings::REGISTRY = { plain: \"Plain\" }\n\nmodule Greetings\n  class Registry\n    def self.lookup(name)\n      Greetings::REGISTRY[name]\n    end\n\n    def self.kind(code)\n      Codes::KINDS.fetch(code, Unknown).name\n    end\n  end\n\n  class Codes\n  end\n\n  Unknown = Class.new(StandardError)\n  Missing = Class.new(StandardError)\n\n  Codes::KINDS = {\n    \"MISSING\" => Missing,\n  }.freeze\nend\n")
        .edit(&format!("{GEMS}/greetings-1.0/lib/greetings.rb"), "require \"greetings/codec\"\n", "require \"greetings/codec\"\nrequire \"greetings/registry\"\n")
        .edit("app/models/article.rb", "  def shouted_title\n", "  def registered\n    [Greetings::Registry.lookup(:plain), Greetings::Registry.kind(\"MISSING\"), Greetings::Registry.kind(\"OTHER\")]\n  end\n\n  def shouted_title\n")
}

pub const TOP_LEVEL_CONSTANT_ASSERTIONS: &str = r#"a = Article.new(title: "hello", body: "a body that is long enough")
got = a.registered
raise got.inspect unless got == ["Plain", "Greetings::Missing", "Greetings::Unknown"]
puts "top-level constant OK"
"#;

/// redis's `RedisClient::Config.prepend(Redis::LibIdentity)`: a gem file's
/// top-level mixin of its own module into its own class, which Ruby runs as
/// the file loads.
pub fn top_level_prepend_overlay() -> emit_and_run::Overlay {
    misnamed_overlay()
        .write(&format!("{GEMS}/greetings-1.0/lib/greetings/identity.rb"), "module Greetings\n  module Identity\n    def encode(text)\n      super + \"!\"\n    end\n  end\nend\n\nGreetings::Codec::Plain.prepend(Greetings::Identity)\n")
        .edit(&format!("{GEMS}/greetings-1.0/lib/greetings.rb"), "require \"greetings/codec\"\n", "require \"greetings/codec\"\nrequire \"greetings/identity\"\n")
}

pub const TOP_LEVEL_PREPEND_ASSERTIONS: &str = r#"a = Article.new(title: "hello", body: "a body that is long enough")
raise a.encoded_title.inspect unless a.encoded_title == "hello!"
puts "top-level prepend OK"
"#;

/// redis-client's keep-alive options: a load-time branch per platform, the
/// Linux one reading `Socket::SOL_TCP` and `Socket::TCP_KEEPIDLE`, which
/// macOS lacks, after testing `Socket` for them.
pub fn platform_constant_overlay() -> emit_and_run::Overlay {
    misnamed_overlay()
        .write(&format!("{GEMS}/greetings-1.0/lib/greetings/keep_alive.rb"), "require \"socket\"\n\nmodule Greetings\n  class KeepAlive\n    if %i[SOL_TCP TCP_KEEPIDLE].all? { |c| Socket.const_defined? c } # Linux\n      def self.options\n        [Socket::SOL_TCP, Socket::TCP_KEEPIDLE]\n      end\n    elsif Socket.const_defined?(:IPPROTO_TCP) && Socket.const_defined?(:TCP_KEEPINTVL)\n      def self.options\n        [Socket::IPPROTO_TCP, Socket::TCP_KEEPINTVL]\n      end\n    else\n      def self.options\n        []\n      end\n    end\n  end\nend\n")
        .edit(&format!("{GEMS}/greetings-1.0/lib/greetings.rb"), "require \"greetings/codec\"\n", "require \"greetings/codec\"\nrequire \"greetings/keep_alive\"\n")
        .edit("app/models/article.rb", "  def shouted_title\n", "  def keep_alive_options\n    Greetings::KeepAlive.options\n  end\n\n  def shouted_title\n")
}

pub const PLATFORM_CONSTANT_ASSERTIONS: &str = r#"got = Article.new(title: "hello", body: "a body that is long enough").keep_alive_options
raise got.inspect unless got.size == 2 && got.all? { |v| v.is_a?(Integer) }
puts "platform constant OK"
"#;
