use super::emit_and_run;

/// A block over a union receiver runs on each variant, so its parameter
/// is the union of what each yields (lobsters' search results: a Story or
/// Comment relation, or a hydrator yielding Comments). It took the first
/// variant's alone, and every read of the other's methods was
/// send_dispatch_failed.
fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true

  def self.labels(kind)
    results = kind == :comments ? Comment.all : Article.all
    out = []
    results.each do |r|
      out << (kind == :comments ? r.commenter : r.title)
    end
    out
  end
",
        )
}

#[test]
fn a_union_receiver_s_block_sees_every_variant() {
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let run = app().run_ruby(r#"Article.delete_all
a = Article.create!(title: "Hello", body: "A sufficiently long body.")
Comment.create!(article: a, commenter: "Ann", body: "Nice")
raise "articles: #{Article.labels(:articles).inspect}" unless Article.labels(:articles) == ["Hello"]
raise "comments: #{Article.labels(:comments).inspect}" unless Article.labels(:comments) == ["Ann"]
puts "union block passed"
"#);
    run.assert_passes();
    assert!(run.stdout.contains("union block passed"));
}
