//! #1921: an `unless` with no `else` in a view. Its `then` branch is
//! empty, and walked it was an append of nothing, which the Go,
//! TypeScript, Crystal and Python buffers don't take. Each line is tagged
//! `L<n>:`, and `RENDERS` is what Rails 8.1.4 renders (the ruby target
//! and Spinel check each line's whole output, the strict targets that
//! the page holds each one, `content_tag_options::app_rendering`).

use super::emit_and_run::Overlay;

pub const VIEW: &str = concat!(
    "L0:<% unless @article.id == 7 %>y<% end %>|\n",
    "L1:<% unless @article.id == 8 %>y<% end %>|\n",
    "L2:<% unless @article.id == 8 %>a<%= @article.id %>b<% end %>|\n",
    "L3:<% unless @article.id == 7 %>y<% else %>n<% end %>|\n",
);

pub const RENDERS: &[&str] = &["L0:|", "L1:y|", "L2:a7b|", "L3:n|"];

/// `base` (a probe app over `fixtures/real-blog`) with the record the
/// view reads.
pub fn app(base: Overlay) -> Overlay {
    super::nested_route_helper_record::app(base, false)
}

/// The strict targets' probe app.
pub fn strict_app() -> Overlay {
    app(super::content_tag_options::app_rendering(VIEW, RENDERS))
}
