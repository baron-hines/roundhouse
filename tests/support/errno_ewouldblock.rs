#![allow(dead_code)]

pub const SOURCE: &str = r#"class WouldBlockProbe
  ERROR_CLASS = Errno::EWOULDBLOCK

  def self.message
    raise Errno::EWOULDBLOCK, "locked"
  rescue Errno::EWOULDBLOCK => error
    error.message
  end

  def self.catch_eagain
    raise Errno::EAGAIN, "busy"
  rescue Errno::EWOULDBLOCK
    "would_block"
  rescue SystemCallError
    "system"
  end

  def self.catch_ewouldblock
    raise Errno::EWOULDBLOCK, "busy"
  rescue Errno::EAGAIN
    "again"
  rescue SystemCallError
    "system"
  end

  def self.rooted
    raise ::Errno::EWOULDBLOCK
  rescue ::Errno::EWOULDBLOCK
    "rooted"
  end

  def self.unrelated
    raise Errno::ENOENT
  rescue Errno::EWOULDBLOCK
    "wrong"
  rescue Errno::ENOENT
    "missing"
  end
end
"#;

pub const ASSERTIONS: &str = r#"
raise "class identity" unless WouldBlockProbe::ERROR_CLASS.equal?(Errno::EWOULDBLOCK)
same_number = Errno::EWOULDBLOCK::Errno == Errno::EAGAIN::Errno
raise "alias identity" unless Errno::EWOULDBLOCK.equal?(Errno::EAGAIN) == same_number
raise "message" unless WouldBlockProbe.message == Errno::EWOULDBLOCK.new("locked").message
raise "EAGAIN rescue" unless WouldBlockProbe.catch_eagain == (same_number ? "would_block" : "system")
raise "EWOULDBLOCK rescue" unless WouldBlockProbe.catch_ewouldblock == (same_number ? "again" : "system")
raise "rooted" unless WouldBlockProbe.rooted == "rooted"
raise "unrelated error" unless WouldBlockProbe.unrelated == "missing"
puts "EWOULDBLOCK contract passed"
"#;

pub fn overlay() -> super::emit_and_run::Overlay {
    super::emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
        .write("app/services/would_block_probe.rb", SOURCE)
}
