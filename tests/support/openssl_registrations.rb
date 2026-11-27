require "openssl"

class CryptoBoundary
  def self.hkdf
    ::OpenSSL::KDF.hkdf("\x0b" * 22, salt: ["000102030405060708090a0b0c"].pack("H*"),
      info: ["f0f1f2f3f4f5f6f7f8f9"].pack("H*"), length: 42, hash: "SHA256").unpack1("H*")
  end

  def self.comparisons
    same = "a\0b"
    copy = same.dup
    OpenSSL.fixed_length_secure_compare(same, copy) &&
      !OpenSSL.fixed_length_secure_compare(same, "a\0c") &&
      OpenSSL.secure_compare(same, copy) && !OpenSSL.secure_compare(same, "short")
  end

  def self.comparison_length_error
    OpenSSL.fixed_length_secure_compare("a", "bb")
    false
  rescue ArgumentError
    true
  end

  def self.sha384
    digest = OpenSSL::Digest::SHA384.new
    [digest.name, digest.digest("abc").unpack1("H*"), digest.hexdigest("abc"),
      OpenSSL::Digest::SHA384.digest("abc").unpack1("H*"), OpenSSL::Digest::SHA384.hexdigest("abc")]
  end

  def self.sha512
    digest = OpenSSL::Digest::SHA512.new
    [digest.name, digest.digest("abc").unpack1("H*"), digest.hexdigest("abc"),
      OpenSSL::Digest::SHA512.digest("abc").unpack1("H*"), OpenSSL::Digest::SHA512.hexdigest("abc")]
  end

  def self.hmac384
    [OpenSSL::HMAC.digest("SHA384", "key", "abc").unpack1("H*"),
      OpenSSL::HMAC.hexdigest("SHA384", "key", "abc")]
  end

  def self.hmac512
    [OpenSSL::HMAC.digest("SHA512", "key", "abc").unpack1("H*"),
      OpenSSL::HMAC.hexdigest("SHA512", "key", "abc")]
  end

  def self.ec
    key = OpenSSL::PKey::EC.generate("prime256v1")
    restored = OpenSSL::PKey.read(key.private_to_der)
    pem = OpenSSL::PKey.read(key.private_to_pem)
    public_key = OpenSSL::PKey.read(key.public_to_der)
    return false unless restored.is_a?(OpenSSL::PKey::EC) && pem.is_a?(OpenSSL::PKey::EC) &&
      public_key.is_a?(OpenSSL::PKey::EC)
    digest = OpenSSL::Digest::SHA256.digest("original")
    signature = restored.dsa_sign_asn1(digest)
    key.private? && pem.private? && restored.private? && !public_key.private? &&
      public_key.group.is_a?(OpenSSL::PKey::EC::Group) && public_key.group.curve_name == "prime256v1" &&
      public_key.dsa_verify_asn1(digest, signature) &&
      !public_key.dsa_verify_asn1(OpenSSL::Digest::SHA256.digest("changed"), signature)
  end

  def self.ed25519
    seed = ["9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"].pack("H*")
    key = OpenSSL::PKey.new_raw_private_key("ED25519", seed)
    raise "Ed25519 key class" unless key.is_a?(OpenSSL::PKey::PKey)
    key.sign(nil, "").unpack1("H*")
  end

  def self.bad_key
    OpenSSL::PKey.read("not a key")
    false
  rescue OpenSSL::PKey::PKeyError
    true
  end

  def self.bad_curve
    OpenSSL::PKey::EC.generate("no-such-curve")
    false
  rescue OpenSSL::PKey::ECError
    true
  end

  def self.bad_digest
    OpenSSL::Digest.new("no-such-digest")
    false
  rescue OpenSSL::Digest::DigestError
    true
  end

  def self.bad_hkdf
    OpenSSL::KDF.hkdf("key", salt: "", info: "", length: 8161, hash: "SHA256")
    false
  rescue OpenSSL::KDF::KDFError
    true
  end
end
