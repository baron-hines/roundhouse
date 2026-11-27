require "openssl"

class CipherBoundary
  # NIST SP 800-38D, a zero key/IV and one zero plaintext block.
  def self.vector128
    c = ::OpenSSL::Cipher.new("aes-128-gcm")
    raise "encrypt self" unless c.encrypt.equal?(c)
    c.key = "\0" * 16
    c.iv = "\0" * 12
    c.auth_data = ""
    [c.update("\0" * 16) + c.final, c.auth_tag]
  end

  def self.vector256
    c = OpenSSL::Cipher.new("aes-256-gcm")
    c.encrypt
    c.key = "\0" * 32
    c.iv = "\0" * 12
    c.auth_data = ""
    [c.update("\0" * 16) + c.final, c.auth_tag]
  end

  def self.decrypt(ct, tag)
    d = OpenSSL::Cipher.new("aes-128-gcm")
    raise "decrypt self" unless d.decrypt.equal?(d)
    d.key = "\0" * 16
    d.iv = "\0" * 12
    d.auth_tag = tag
    d.auth_data = ""
    d.update(ct) + d.final
  end

  def self.wrong_tag_class(ct)
    decrypt(ct, "x" * 16)
    false
  rescue OpenSSL::Cipher::CipherError => error
    error.is_a?(OpenSSL::Cipher::AuthTagError)
  end

  def self.wrong_tag_parent(ct)
    decrypt(ct, "x" * 16)
    false
  rescue OpenSSL::OpenSSLError
    true
  end

  def self.random_lengths
    c = OpenSSL::Cipher.new("aes-128-gcm")
    c.encrypt
    [c.random_key.bytesize, c.random_iv.bytesize]
  end
end

ct, tag = CipherBoundary.vector128
raise "128 ciphertext" unless ct.unpack1("H*") == "0388dace60b6a392f328c2b971b2fe78"
raise "128 tag" unless tag.unpack1("H*") == "ab6e47d42cec13bdf53a67b21257bddf"
ct256, tag256 = CipherBoundary.vector256
raise "256 ciphertext" unless ct256.unpack1("H*") == "cea7403d4d606b6e074ec5d3baf39d18"
raise "256 tag" unless tag256.unpack1("H*") == "d0d1c8a799996bf0265b98b5d48ab919"
raise "round trip" unless CipherBoundary.decrypt(ct, tag) == "\0" * 16
raise "class rescue" unless CipherBoundary.wrong_tag_class(ct)
raise "parent rescue" unless CipherBoundary.wrong_tag_parent(ct)
raise "CipherError parent" unless OpenSSL::Cipher::CipherError.superclass == OpenSSL::OpenSSLError
raise "AuthTagError parent" unless OpenSSL::Cipher::AuthTagError.superclass == OpenSSL::Cipher::CipherError
raise "random lengths" unless CipherBoundary.random_lengths == [16, 12]
puts "Cipher registrations OK"
