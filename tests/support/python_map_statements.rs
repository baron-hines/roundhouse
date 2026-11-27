//! #1939: a `map` block whose body is several statements. Python's
//! comprehension takes one expression, so the emit refuses this shape
//! rather than writing the statements into the brackets. Rails 8.1.4
//! renders `"L0:x,x|"`.

use super::emit_and_run::Overlay;

pub const VIEW: &str = "L0:<%= [1, 2].map { |i| x = \"x\"; x }.join(\",\") %>|\n";

pub const RENDERS: &[&str] = &["L0:x,x|"];

/// `base` (a probe app over `fixtures/real-blog`) with the record the
/// probe controller sets.
pub fn app(base: Overlay) -> Overlay {
    super::nested_route_helper_record::app(base, false)
}

/// The strict targets' probe app.
pub fn strict_app() -> Overlay {
    app(super::content_tag_options::app_rendering(VIEW, RENDERS))
}
