require_relative "action_controller/message_verifier"

module ActiveSupport
  module SecurityUtils
    def self.secure_compare(a, b)
      ActionController::MessageVerifier.secure_compare(a, b)
    end

    def self.fixed_length_secure_compare(a, b)
      raise ArgumentError, "string length mismatch." unless a.bytesize == b.bytesize

      MessageDigest.secure_compare(a, b)
    end
  end
end
