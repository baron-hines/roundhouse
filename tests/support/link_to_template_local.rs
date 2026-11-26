//! #1898: a record held in a local the template assigns
//! (`<% rec = @article %>`) handed to `link_to`'s url helper. Rails fills
//! the segment from the record's `to_param`, its id here, and the strict
//! targets' generated helpers take that scalar. Each line is tagged
//! `L<n>:`, and `RENDERS` is what Rails 8.1.4 renders (the ruby target
//! and Spinel check each line's whole output, the strict targets that the
//! page holds each one, `content_tag_options::app_rendering`).

use super::emit_and_run::Overlay;

pub const VIEW: &str = concat!(
    "<% rec = @article %>\n",
    r#"L0:<%= link_to "x", article_path(rec) %>"#, "\n",
    r#"L1:<%= link_to "x", edit_article_path(rec) %>"#, "\n",
);

pub const RENDERS: &[&str] = &[r#"L0:<a href="/articles/7">x</a>"#, r#"L1:<a href="/articles/7/edit">x</a>"#];

/// `base` (a probe app over `fixtures/real-blog`) with the record the
/// view reads.
pub fn app(base: Overlay) -> Overlay {
    super::nested_route_helper_record::app(base, false)
}

/// The strict targets' probe app.
pub fn strict_app() -> Overlay {
    app(super::content_tag_options::app_rendering(VIEW, RENDERS))
}
