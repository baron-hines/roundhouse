class HookProbe
  def self.base_find_by
    Article.find_by(title: "f").hook_seen
  end

  def self.first
    Feature.first.hook_seen
  end

  def self.select_first
    Feature.select(:id, :title).first.hook_seen
  end
end
