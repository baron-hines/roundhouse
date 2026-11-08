//! #1946: `each` on a receiver with an effect runs the receiver once.
//! `two_ids` and `id_map` each append "a" to the body, so the body shows
//! how many times they ran. Each line is tagged `L<n>:`, and `RENDERS` is
//! what Rails 8.1.4 renders.

use super::emit_and_run::Overlay;

pub const VIEW: &str = concat!(
    "L0:<% @article.two_ids.each do |i| %><%= i %><% end %>|<%= @article.body %>|\n",
    "L1:<% @article.two_ids.each do %>x<% end %>|<%= @article.body %>|\n",
    "L2:<% @article.id_map.each do |k, v| %><%= k %>=<%= v %><% end %>|<%= @article.body %>|\n",
);

pub const RENDERS: &[&str] = &[
    "L0:12|a body long enougha|",
    "L1:xx|a body long enoughaa|",
    "L2:1=2|a body long enoughaaa|",
];

/// `base` with the record the view reads and the two methods. Without
/// the RBS, Go types them `interface{}`.
pub fn app(base: Overlay) -> Overlay {
    let model = include_str!("../../fixtures/real-blog/app/models/article.rb");
    let body = model.strip_suffix("end\n").expect("the model ends with `end`");
    super::nested_route_helper_record::app(base, false)
        .write(
            "app/models/article.rb",
            &format!("{body}\n  def two_ids\n    self.body = body + \"a\"\n    [1, 2]\n  end\n\n  def id_map\n    self.body = body + \"a\"\n    {{ 1 => 2 }}\n  end\nend\n"),
        )
        .write("sig/article_marks.rbs", "class Article\n  def two_ids: () -> Array[Integer]\n  def id_map: () -> Hash[Integer, Integer]\nend\n")
}

/// The strict targets' probe app.
pub fn strict_app() -> Overlay {
    app(super::content_tag_options::app_rendering(VIEW, RENDERS))
}
