class Follow < ApplicationRecord
  belongs_to :follower, class_name: "User"
  belongs_to :followed, class_name: "User"
  validates :followed_id, presence: true
  after_initialize do
    self.initialized_for = "#{follower_id}:#{followed_id}"
  end

  def self.direct(follower, followed)
    Follow.find_or_create_by!(follower: follower, followed: followed)
  end

  def self.conditional(follower, followed)
    Follow.find_or_create_by!(follower: follower, followed: followed) unless follower == followed
  end

  def self.rescued(follower, followed)
    begin
      follow = Follow.find_or_create_by!(follower: follower, followed: followed)
    rescue ActiveRecord::RecordInvalid
      nil
    end
  end

  def self.invalid(follower)
    follower_id = follower.id
    Follow.find_or_create_by!(follower_id: follower_id, followed_id: nil)
  end

  def self.invalid_rescued(follower)
    follower_id = follower.id
    begin
      Follow.find_or_create_by!(follower_id: follower_id, followed_id: nil)
    rescue ActiveRecord::RecordInvalid
      nil
    end
  end

  def self.invalid_plain(follower)
    follower_id = follower.id
    Follow.find_or_create_by(follower_id: follower_id, followed_id: nil)
  end
end
