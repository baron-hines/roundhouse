alice = User.create!(name: "Alice")
bob = User.create!(name: "Bob")
carol = User.create!(name: "Carol")
article = Article.create!(title: "A title")

direct = Follow.direct(alice, bob)
raise "direct attributes" unless direct.follower_id == alice.id && direct.followed_id == bob.id
raise "direct not persisted" unless direct.persisted?
raise "callback ran before attributes" unless direct.initialized_for == "#{alice.id}:#{bob.id}"
again = Follow.direct(alice, bob)
raise "direct duplicated" unless again.id == direct.id
conditional = Follow.conditional(alice, carol)
raise "unless lost attributes" unless conditional.follower_id == alice.id && conditional.followed_id == carol.id
raise "unless duplicated" unless Follow.conditional(alice, carol).id == conditional.id
raise "unless took skipped branch" unless Follow.conditional(alice, alice).nil?
rescued = Follow.rescued(bob, carol)
raise "rescue body lost result" unless rescued.follower_id == bob.id && rescued.followed_id == carol.id
raise "rescue body duplicated" unless Follow.rescued(bob, carol).id == rescued.id
favorite = Favorite.add(article, alice)
raise "favorite attributes" unless favorite.article_id == article.id && favorite.user_id == alice.id
raise "favorite duplicated" unless Favorite.add(article, alice).id == favorite.id

other_article = Article.create!(title: "Another title")
prior = Favorite.add(other_article, alice)
count_before = Favorite.count
conflict = Favorite.contradictory_scope
raise "lookup dropped a scope predicate" if conflict.id == prior.id
raise "scope conflict did not insert" unless Favorite.count == count_before + 1
raise "creation did not prefer explicit attributes" unless conflict.article_id == other_article.id

begin
  Follow.invalid(alice)
  raise "bang did not raise"
rescue ActiveRecord::RecordInvalid
end
raise "rescue branch did not catch validation" unless Follow.invalid_rescued(alice).nil?
invalid = Follow.invalid_plain(alice)
raise "plain invalid record saved" if invalid.persisted?
raise "plain lost validation errors" if invalid.errors.empty?
puts [Follow.count, Favorite.count, invalid.persisted?].join(" | ")
puts "bang find-or-create passed"
