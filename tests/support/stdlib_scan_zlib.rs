//! `StringScanner`, `Math::PI` and `Zlib`, shared by the Ruby lane and
//! the Spinel lane: the ruby family's own libraries, spinel's
//! `packages/strscan` and `packages/zlib`. The app writes no require
//! (Rails has loaded them). The answers are CRuby 3.4's.

use super::emit_and_run;

pub const TOKENIZER: &str = r##"class Tokenizer
  def self.words(text)
    s = StringScanner.new(text)
    out = []
    until s.eos?
      s.skip(/\s+/)
      word = s.scan(/\w+/)
      break if word.nil?
      out << "#{word}@#{s.pos}"
    end
    out
  end

  def self.rest_after(text, marker)
    s = StringScanner.new(text)
    s.skip_until(/#{marker}/)
    [s.rest, s.rest_size, s.matched?]
  end

  def self.degrees(radians)
    (radians * 180 / Math::PI).round
  end

  def self.round_trip(text)
    packed = Zlib.deflate(text)
    [Zlib.inflate(packed), Zlib.gunzip(Zlib.gzip(text))]
  end
end
"##;

pub const ASSERTIONS: &str = r##"raise "words #{Tokenizer.words("ab cd  e").inspect}" unless Tokenizer.words("ab cd  e") == ["ab@2", "cd@5", "e@8"]
raise "rest #{Tokenizer.rest_after("key: value", ":").inspect}" unless Tokenizer.rest_after("key: value", ":") == [" value", 6, true]
raise "deg" unless Tokenizer.degrees(Math::PI / 2) == 90
raise "zlib" unless Tokenizer.round_trip("hello hello hello") == ["hello hello hello", "hello hello hello"]
puts "stdlib passed"
"##;

#[allow(dead_code)]
pub fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/tokenizer.rb", TOKENIZER)
}
