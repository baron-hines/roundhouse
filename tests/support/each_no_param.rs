//! #1910: a block with no parameter, which Ruby runs once per element
//! without binding it. Each line is tagged `L<n>:`, and `RENDERS` is what
//! Rails 8.1.4 renders (the ruby target and Spinel check each line's
//! whole output, the strict targets that the page holds each one,
//! `content_tag_options::app_rendering`).

use super::emit_and_run::Overlay;

pub const VIEW: &str = concat!(
    "<% [1, 2].each do %>L0:<%= @article.id %>|<% end %>\n",
    "L1:<%= [1, 2].map { \"x\" }.join(\",\") %>\n",
);

pub const RENDERS: &[&str] = &["L0:7|L0:7|", "L1:x,x"];

/// `base` (a probe app over `fixtures/real-blog`) with the record the
/// view reads.
pub fn app(base: Overlay) -> Overlay {
    super::nested_route_helper_record::app(base, false)
}

/// The strict targets' probe app.
pub fn strict_app() -> Overlay {
    app(super::content_tag_options::app_rendering(VIEW, RENDERS))
}

/// A block with no parameter whose body is two statements. Rails 8.1.4
/// renders it `"L0:x,x|"`; Python's comprehension takes only an
/// expression, so this shape is refused there, not emitted.
pub const MAP_STATEMENTS_VIEW: &str = "L0:<%= [1, 2].map { x = \"x\"; x }.join(\",\") %>|\n";

/// The strict targets' probe app for `MAP_STATEMENTS_VIEW`.
pub fn strict_map_statements_app() -> Overlay {
    app(super::content_tag_options::app_rendering(MAP_STATEMENTS_VIEW, &["L0:x,x|"]))
}

/// Blocks whose only parameter is a rest (`|*args|`). Their `params`
/// are empty too, but the body reads the rest, so they are not blocks
/// with no parameter. Rails 8.1.4 renders `"L0:1,1,|"` and `"L1:1,1|"`.
pub const REST_VIEW: &str = concat!(
    "L0:<% [1, 2].each do |*args| %><%= args.length %>,<% end %>|\n",
    "L1:<%= [1, 2].map { |*a| a.length }.join(\",\") %>|\n",
);

/// The strict targets' probe app for `REST_VIEW`.
pub fn strict_rest_app() -> Overlay {
    app(super::content_tag_options::app_rendering(REST_VIEW, &["L0:1,1,|", "L1:1,1|"]))
}

/// Empty blocks: `map` maps each element to nil and `each` runs nothing.
pub const EMPTY_MAP_VIEW: &str = concat!(
    "L0:<%= [1, 2].map {}.length %>|\n",
    "L1:<%= [1, 2].map { |x| }.length %>|\n",
    "L2:<% [1, 2].each {} %>|\n",
);

/// What Rails 8.1.4 renders for `EMPTY_MAP_VIEW`.
pub const EMPTY_MAP_RENDERS: &[&str] = &["L0:2|", "L1:2|", "L2:|"];

/// The strict targets' probe app for `EMPTY_MAP_VIEW`.
pub fn strict_empty_map_app() -> Overlay {
    app(super::content_tag_options::app_rendering(EMPTY_MAP_VIEW, EMPTY_MAP_RENDERS))
}

/// A no-parameter `each` over a strict-locals partial's local, which
/// Rust's `each` bridge borrows with `iter_mut()`, so the local must be
/// declared `mut`. Rails 8.1.4 renders `"L0:x|L0:x|"`.
pub const PARTIAL_LOCAL_RENDERS: &[&str] = &["L0:x|L0:x|"];

/// The strict targets' probe app for the partial local.
pub fn strict_partial_local_app() -> Overlay {
    app(super::content_tag_options::app_rendering("<%= render partial: \"probes/listing\", locals: { items: [1, 2] } %>\n", PARTIAL_LOCAL_RENDERS))
        .write("app/views/probes/_listing.html.erb", "<%# locals: (items:) -%>\n<% items.each do %>L0:x|<% end %>\n")
}
