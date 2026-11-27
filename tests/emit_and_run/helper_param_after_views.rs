//! A helper's param typed from a view site keeps the type the site has
//! once every helper return settles. lobsters' comment partial does
//! `score = comment_score_for_user(comment, @user)` then
//! `upvoter_score(score)`; the first only widens to `Integer | String`
//! after the views are typed (its `comment` is typed from them), and
//! `upvoter_score` kept the `String` of that earlier round. Spinel
//! enforces the sidecar, so every comment page raised TypeError.

use super::emit_and_run;
use roundhouse::project::BuildTarget;

const HELPER: &str = r##"module ScoreHelper
  def shown_score(article)
    if article.title.size > 3
      article.id
    else
      "~"
    end
  end

  def score_label(score)
    if score.is_a? Integer
      "n#{score}"
    else
      "~"
    end
  end
end
"##;

#[test]
fn a_view_fed_helper_param_takes_the_settled_type() {
    let (emitted, errors) = emit_and_run::real_blog()
        .write("app/helpers/score_helper.rb", HELPER)
        .edit(
            "app/views/articles/show.html.erb",
            "<% content_for :title, \"Showing article\" %>\n",
            "<% content_for :title, \"Showing article\" %>\n<% score = shown_score(@article) %>\n<%= score_label(score) %>\n",
        )
        .emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let sig = std::fs::read_to_string(emitted.join("app/models/score_helper.rbs")).expect("score_helper.rbs");
    assert!(sig.contains("def self.shown_score: (Article article) -> (Integer | String)"), "{sig}");
    assert!(sig.contains("def self.score_label: ((Integer | String) score) -> String"), "{sig}");
}
