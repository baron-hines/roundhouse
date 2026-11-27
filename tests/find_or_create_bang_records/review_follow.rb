class Follow < ApplicationRecord
  belongs_to :follower, class_name: "User"
  belongs_to :followed, class_name: "User"
  belongs_to :other_follower, class_name: "User", foreign_key: :follower_id
  validates :followed_id, presence: true
  after_initialize do
    self.initialized_for = "#{follower_id}:#{followed_id}"
  end

  def self.direct(follower, followed)
    Follow.find_or_create_by!(follower: follower, followed: followed)
  end

  def self.conflicting_keys(follower, follower_id, followed)
    Follow.find_or_create_by!(follower: follower, follower_id: follower_id, followed: followed)
  end

  def self.reversed_keys(follower_id, follower, followed)
    Follow.find_or_create_by!(follower_id: follower_id, follower: follower, followed: followed)
  end

  def self.shared_key(follower, other_follower, followed)
    Follow.find_or_create_by!(follower: follower, other_follower: other_follower, followed: followed)
  end

  def self.with_block(follower, followed)
    Follow.find_or_create_by!(follower: follower, followed: followed) do |record|
      record.initialized_for = "from block"
    end
  end

  def self.block_fixes_validation(follower, followed)
    follower_id = follower.id
    Follow.find_or_create_by!(follower_id: follower_id, followed_id: nil) do |record|
      record.followed = followed
    end
  end

  def self.literal_block
    Follow.find_or_create_by!(follower_id: 2, followed_id: 1) do |record|
      record.initialized_for = "from block"
    end
  end
end
