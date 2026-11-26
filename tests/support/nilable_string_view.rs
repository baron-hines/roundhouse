//! #1937: a view reads a nullable string column (`articles.title`)
//! with `==`, `!=`, `empty?` and `length`. The column is `*string` on
//! Go, `String | Nil` on Crystal and `Option<String>` on Rust, and each
//! call read it as a string. Each line is tagged `L<n>:`, and `RENDERS`
//! is what Rails 8.1.4 renders with the title present; `NIL_RENDERS`
//! is what it renders for the comparisons with the title nil, where
//! `==` is false and `!=` true (`empty?` and `length` raise on nil).

use super::emit_and_run::Overlay;

pub const VIEW: &str = concat!(
    "L0:<% if @article.title == \"t\" %>y<% end %>|\n",
    "L1:<% if @article.title.empty? %>y<% else %>n<% end %>|\n",
    "L2:<%= @article.title.length %>|\n",
    "L3:<% if @article.title != \"u\" %>y<% end %>|\n",
);

pub const RENDERS: &[&str] = &["L0:y|", "L1:n|", "L2:1|", "L3:y|"];

pub const NIL_VIEW: &str = concat!(
    "L0:<% if @article.title == \"t\" %>y<% end %>|\n",
    "L1:<% if @article.title != \"u\" %>y<% end %>|\n",
    "L2:<% if @article.title == \"\" %>y<% else %>n<% end %>|\n",
    "L3:<% if @article.title != \"\" %>y<% else %>n<% end %>|\n",
);

pub const NIL_RENDERS: &[&str] = &["L0:|", "L1:y|", "L2:n|", "L3:y|"];

/// The String literal on the left. Rails 8.1.4 renders `NIL_LEFT_RENDERS`
/// with the title nil.
pub const NIL_LEFT_VIEW: &str = concat!(
    "L0:<% if \"\" == @article.title %>y<% else %>n<% end %>|\n",
    "L1:<% if \"\" != @article.title %>y<% else %>n<% end %>|\n",
);

pub const NIL_LEFT_RENDERS: &[&str] = &["L0:n|", "L1:y|"];

/// `base` (a probe app over `fixtures/real-blog`) with an article whose
/// title is "t".
pub fn app(base: Overlay) -> Overlay {
    super::nested_route_helper_record::app(base, false)
}

/// `base` with an article whose title is nil: never assigned.
pub fn nil_app(base: Overlay) -> Overlay {
    base.write(
        "app/controllers/probes_controller.rb",
        "class ProbesController < ApplicationController\n  def show\n    @article = Article.new(id: 7, body: \"a body long enough\")\n  end\nend\n",
    )
}

/// The strict targets' probe apps.
pub fn strict_app() -> Overlay {
    app(super::content_tag_options::app_rendering(VIEW, RENDERS))
}

pub fn strict_nil_app() -> Overlay {
    nil_app(super::content_tag_options::app_rendering(NIL_VIEW, NIL_RENDERS))
}

pub fn strict_nil_left_app() -> Overlay {
    nil_app(super::content_tag_options::app_rendering(NIL_LEFT_VIEW, NIL_LEFT_RENDERS))
}

/// Operands with an effect. `mark_a` appends "a" to the body and returns
/// the record; `mark_b` appends "b" and returns "t". Each runs once, left
/// then right, whether or not the title is nil, so the body shows how
/// many times each ran and in what order (Rails 8.1.4).
pub const EFFECT_VIEW: &str = concat!(
    "L0:<% if @article.mark_a.title == @article.mark_b %>y<% end %>|<%= @article.body %>|\n",
    "L1:<% if @article.mark_a.title != @article.mark_b %>y<% end %>|<%= @article.body %>|\n",
    "L2:<% if @article.title == @article.mark_b %>y<% end %>|<%= @article.body %>|\n",
);

pub const EFFECT_RENDERS: &[&str] = &[
    "L0:y|a body long enoughab|",
    "L1:|a body long enoughabab|",
    "L2:y|a body long enoughababb|",
];

pub const NIL_EFFECT_RENDERS: &[&str] = &[
    "L0:|a body long enoughab|",
    "L1:y|a body long enoughabab|",
    "L2:|a body long enoughababb|",
];

fn with_marks(base: Overlay) -> Overlay {
    let model = include_str!("../../fixtures/real-blog/app/models/article.rb");
    let body = model.strip_suffix("end\n").expect("the model ends with `end`");
    base.write(
        "app/models/article.rb",
        &format!("{body}\n  def mark_a\n    self.body = body + \"a\"\n    self\n  end\n\n  def mark_b\n    self.body = body + \"b\"\n    \"t\"\n  end\nend\n"),
    )
    .write("sig/article_marks.rbs", "class Article\n  def mark_a: () -> Article\n  def mark_b: () -> String\nend\n")
}

/// `base` with the title "t" and the two marking methods.
pub fn effect_app(base: Overlay) -> Overlay {
    with_marks(app(base))
}

/// `base` with a nil title and the two marking methods.
pub fn nil_effect_app(base: Overlay) -> Overlay {
    with_marks(nil_app(base))
}

pub fn strict_effect_app() -> Overlay {
    effect_app(super::content_tag_options::app_rendering(EFFECT_VIEW, EFFECT_RENDERS))
}

pub fn strict_nil_effect_app() -> Overlay {
    nil_effect_app(super::content_tag_options::app_rendering(EFFECT_VIEW, NIL_EFFECT_RENDERS))
}
