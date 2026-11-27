use super::emit_and_run;

/// A route segment named `<x>_id` takes an Integer by name, unless
/// something fills it with a String. A record whose model overrides
/// `to_param` is such a filling (lobsters' `story_image_url(story)` fills
/// `:short_id` with Story#to_param); the segment was Integer and Spinel
/// refused the String the call passes.
fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/card/:short_id\" => \"articles#show\", as: :card\n",
        )
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true

  def to_param
    \"a-#{id}\"
  end
",
        )
        .write(
            "app/models/card_links.rb",
            "class CardLinks\n  class << self\n    include Rails.application.routes.url_helpers\n\n    def card(article)\n      card_path(article)\n    end\n  end\nend\n",
        )
}

#[test]
fn a_to_param_record_fills_an_id_named_segment_with_a_string() {
    let (emitted, errors) = app().emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let sig = std::fs::read_to_string(emitted.join("app/route_helpers.rbs")).expect("route_helpers.rbs");
    assert!(sig.contains("def self.card_path: (String short_id) -> String"), "{sig}");
    app()
        .run_ruby(r#"a = Article.create!(title: "Card", body: "A sufficiently long body.")
raise "card: #{CardLinks.card(a)}" unless CardLinks.card(a) == "/card/a-#{a.id}"
puts "card passed"
"#)
        .assert_passes();
}
