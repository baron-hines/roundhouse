//! One contract for the IO and process constants an app's terminal code
//! names, shared by the interpreted (CRuby overlay) and native (spinel)
//! lanes: `Errno::*`, `EOFError`, `File::NULL`, `Encoding::UTF_8`,
//! `Shellwords` and `PTY`. Each used to stop `check` with "constant not
//! supported (all targets)".
//!
//! The expected lines are CRuby 4.0's output for the same code.

pub fn overlay() -> super::emit_and_run::Overlay {
    super::emit_and_run::real_blog().write("app/services/terminal_probe.rb", SOURCE)
}

const SOURCE: &str = r#"require "pty"

class TerminalProbe
  def self.missing_file
    File.read("/nonexistent/roundhouse-io-constants")
    "read"
  rescue Errno::ENOENT
    "enoent"
  end

  def self.missing_dir
    Dir.children("/nonexistent/roundhouse-io-constants")
    "listed"
  rescue SystemCallError => e
    e.class.name
  end

  def self.discarded
    File.write(File::NULL, "noise")
  end

  def self.quoted(text)
    "echo #{Shellwords.escape(text)}"
  end

  def self.utf8(bytes)
    bytes.dup.force_encoding(Encoding::UTF_8).encoding.to_s
  end

  def self.terminal
    out, input, pid = PTY.spawn("printf", "hi")
    data = +""
    begin
      loop { data << out.readpartial(64) }
    rescue EOFError, Errno::EIO, IOError
      data << "|end"
    end
    open = !input.closed?
    input.close
    out.close
    [data, open, pid.is_a?(Integer)]
  end

  def self.spawn_with_block
    PTY.spawn("true") { |r, w, _pid| r.close; w.close }
  end
end
"#;

pub const SCRIPT: &str = r#"
puts TerminalProbe.missing_file
puts TerminalProbe.missing_dir
p TerminalProbe.discarded
puts TerminalProbe.quoted("a b'c")
puts TerminalProbe.utf8("zaż")
p TerminalProbe.terminal
TerminalProbe.spawn_with_block
puts "spawned"
"#;

pub const EXPECTED: &str = r#"enoent
Errno::ENOENT
5
echo a\ b\'c
UTF-8
["hi|end", true, true]
spawned
"#;
