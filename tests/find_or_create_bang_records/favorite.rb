class Favorite < ApplicationRecord
  belongs_to :article
  belongs_to :user

  def self.add(article, user)
    Favorite.find_or_create_by!(article: article, user: user)
  end

  def self.contradictory_scope
    Favorite.where(article_id: 1).find_or_create_by!(article_id: 2, user_id: 1)
  end
end
