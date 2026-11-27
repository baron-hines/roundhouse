class StiProbe
  def self.seed
    a = Article.create!(title: "a", body: "a body long enough")
    f = Feature.create!(title: "f", body: "f body long enough")
    s = SpecialFeature.create!(title: "s", body: "s body long enough")
    [a.id, f.id, s.id]
  end

  def self.types
    Article.order(:id).pluck(:type)
  end

  def self.counts
    [Article.count, Feature.count, SpecialFeature.count]
  end

  def self.where_titles
    Feature.where(title: ["a", "f", "s"]).order(:id).pluck(:title)
  end

  def self.scope_titles
    Feature.titled_a_or_f.pluck(:title)
  end

  def self.id_sum
    Feature.sum(:id)
  end

  def self.take_title
    Feature.order(:id).take.title
  end

  def self.first_class
    Feature.order(:id).first.class.name
  end

  def self.find_class(id)
    Feature.find(id).class.name
  rescue ActiveRecord::RecordNotFound
    "RecordNotFound"
  end

  def self.find_by_title(title)
    Feature.find_by(title: title)
  end

  def self.each_titles
    titles = []
    Feature.find_each { |record| titles << record.title }
    titles
  end

  def self.found_or_built_type
    record = Feature.find_or_create_by(title: "n")
    record.type
  end

  def self.select_titles
    Feature.select(:id, :title).order(:id).map(&:title)
  end

  def self.none_count
    Feature.none.count
  end

  def self.exists(id)
    Feature.exists?(id)
  end

  def self.pick_title
    Feature.order(:id).pick(:title)
  end

  def self.found_deeper
    record = Feature.find_or_create_by(title: "s")
    [record.class.name, record.id]
  end

  def self.select_first_class
    Feature.select(:id, :title).order(:id).first.class.name
  end

  def self.select_classes
    Feature.select(:id, :title).order(:id).map { |record| record.class.name }
  end

  def self.select_typed_classes
    Feature.select(:id, :title, :type).order(:id).map { |record| record.class.name }
  end
end
