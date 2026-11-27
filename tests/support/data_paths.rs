//! What lobsters' write and read paths needed once it ran against real
//! rows, shared by the Ruby lane and the Spinel lane.

use super::emit_and_run;

const ARTICLE_METHODS: &str = r#"  validates :title, presence: true

  typed_store :settings do |s|
    s.string :theme, default: "light"
  end
  validates :theme, inclusion: %w[light dark]

  def slug=(s)
    super(s.strip.downcase)
  end

  def self.ordered_titles
    order([title: :desc]).map(&:title)
  end

  def self.by_title(title)
    where(title: title).first_or_initialize
  end

  def self.mailer_host
    Rails.application.config.action_mailer.default_url_options[:host]
  end
"#;

const BANNER: &str = r#"<%# locals: (banner_article:) -%>
<p id="banner"><%= link_to banner_article.title, article_path(banner_article) %></p>
"#;

pub const ASSERTIONS: &str = r#"
Comment.delete_all
Article.delete_all
a = Article.new(title: "Hello", body: "A sufficiently long body.")
raise "typed_store default validates: #{a.errors.to_a.inspect}" unless a.valid?
a.save!
raise "writer not called for an absent key" unless a.slug.nil?
b = Article.new(title: "Two", body: "A sufficiently long body.", slug: "  MiXed ")
raise "writer for a given key: #{b.slug.inspect}" unless b.slug == "mixed"
b.save!
raise "order array: #{Article.ordered_titles.inspect}" unless Article.ordered_titles == ["Two", "Hello"]
n = Article.by_title("Fresh")
raise "first_or_initialize: #{n.title.inspect}" unless n.title == "Fresh" && !n.persisted?
raise "first_or_initialize existing" unless Article.by_title("Hello").persisted?
raise "mailer host: #{Article.mailer_host.inspect}" unless Article.mailer_host == "localhost"
puts "data paths contract passed"
"#;

/// The model half, which the Spinel lane compiles with lobsters_zero's contract.
pub fn apply_models(overlay: emit_and_run::Overlay) -> emit_and_run::Overlay {
    overlay
        .edit(
            "db/schema.rb",
            "    t.string \"title\"\n    t.text \"body\"\n",
            "    t.string \"title\"\n    t.text \"body\"\n    t.text \"settings\"\n    t.string \"slug\"\n",
        )
        .edit("app/models/article.rb", "  validates :title, presence: true\n", ARTICLE_METHODS)
}

#[allow(dead_code)]
pub fn overlay() -> emit_and_run::Overlay {
    apply_models(emit_and_run::real_blog())
        .edit(
            "app/models/article.rb",
            "  def slug=(s)\n",
            "  def to_param\n    \"a#{id}\"\n  end\n\n  def slug=(s)\n",
        )
        .edit(
            "app/controllers/articles_controller.rb",
            "    @articles = Article.includes(:comments).order(created_at: :desc)\n",
            "    @articles = Article.includes(:comments).order(created_at: :desc)\n    @above = { partial: \"articles/banner\", locals: { banner_article: Article.order(:id).first } }\n",
        )
        .edit(
            "app/views/articles/index.html.erb",
            "<%= turbo_stream_from \"articles\" %>\n",
            "<%= turbo_stream_from \"articles\" %>\n<%= render @above if @above %>\n",
        )
        .write("app/views/articles/_banner.html.erb", BANNER)
}
