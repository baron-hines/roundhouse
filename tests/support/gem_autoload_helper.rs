//! An installed plain-Ruby gem whose top module registers its files through
//! its own class method, as the mail gem does:
//! `def self.register_autoload(name, path) = (@autoloads[name] = path; autoload(name, path))`,
//! called from the module body with a Symbol and a String. Shared by the
//! emit lane and the Spinel lane.

use super::emit_and_run;

const GEMS: &str = "vendor/bundle/ruby/3.4.0/gems";

pub fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    postbox (1.0)\n    debug (1.11.1)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  postbox\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(
            &format!("{GEMS}/postbox-1.0/lib/postbox.rb"),
            "module Postbox\n  @autoloads = {}\n\n  def self.register_autoload(name, path)\n    @autoloads[name] = path\n    autoload(name, path)\n  end\n\n  def self.registered\n    @autoloads.keys\n  end\n\n  register_autoload :Envelope, \"postbox/envelope\"\nend\n",
        )
        .write(
            &format!("{GEMS}/postbox-1.0/lib/postbox/envelope.rb"),
            "module Postbox\n  class Envelope\n    def self.stamp(text)\n      \"[\" + text + \"]\"\n    end\n  end\nend\n",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def stamped_title\n    Postbox::Envelope.stamp(title)\n  end\n\n  def postbox_names\n    Postbox.registered\n  end\n\n",
        )
}

pub const ASSERTIONS: &str = r#"article = Article.new(title: "hello", body: "a body that is long enough")
got = article.stamped_title
raise "got #{got.inspect}" unless got == "[hello]"
names = article.postbox_names
raise "names #{names.inspect}" unless names == [:Envelope]
puts "autoload helper OK"
"#;
