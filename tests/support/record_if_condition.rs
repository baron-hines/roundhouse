//! #1909: a record as an `if` condition. Ruby tests the record's
//! truthiness, any object but `nil` and `false` being true. Each line is
//! tagged `L<n>:`, and `RENDERS` is what Rails 8.1.4 renders (the ruby
//! target and Spinel check each line's whole output, the strict targets
//! that the page holds each one, `content_tag_options::app_rendering`).

use super::emit_and_run::Overlay;

pub const VIEW: &str = concat!(
    "<% if @article %>L0:<%= @article.title %><% end %>\n",
    "<% rec = @article %><% if rec %>L1:<%= rec.title %><% end %>\n",
    "L2:<% if @article %>y<% else %>n<% end %>\n",
);

pub const RENDERS: &[&str] = &["L0:t", "L1:t", "L2:y"];

/// `base` (a probe app over `fixtures/real-blog`) with the record the
/// view reads.
pub fn app(base: Overlay) -> Overlay {
    super::nested_route_helper_record::app(base, false)
}

/// An assignment as the condition, the record read in the branch. Its
/// own view: TypeScript, Crystal and Python don't keep the local this
/// condition assigns (`got is not defined`), so the ruby target, Spinel,
/// Go and Rust check it.
pub const ASSIGN_VIEW: &str = "L0:<% if (got = @article) %><%= got.title %><% end %>|\n";

pub const ASSIGN_RENDERS: &[&str] = &["L0:t|"];

/// The strict targets' probe app for `ASSIGN_VIEW`.
pub fn strict_assign_app() -> Overlay {
    app(super::content_tag_options::app_rendering(ASSIGN_VIEW, ASSIGN_RENDERS))
}

/// The strict targets' probe app.
pub fn strict_app() -> Overlay {
    app(super::content_tag_options::app_rendering(VIEW, RENDERS))
}
