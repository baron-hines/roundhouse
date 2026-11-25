# frozen_string_literal: true
# Not left to the stdlib: neither Ruby's SecureRandom nor Spinel's answers ActiveSupport's base58 / base36 (lobsters' seeds).
module SecureRandom
  BASE58_ALPHABET = ("0".."9").to_a + ("A".."Z").to_a + ("a".."z").to_a - ["0", "O", "I", "l"]
  BASE36_ALPHABET = ("0".."9").to_a + ("a".."z").to_a

  def self.base58(n = 16)
    out = +""
    (n.nil? ? 16 : n).times { out << BASE58_ALPHABET[SecureRandom.random_number(58)] }
    out
  end

  def self.base36(n = 16)
    out = +""
    (n.nil? ? 16 : n).times { out << BASE36_ALPHABET[SecureRandom.random_number(36)] }
    out
  end
end
