//! #1977: a String on the left of `==` and `!=`, a nullable String column
//! on the right. Each line is tagged `L<n>:`. The renders are what Rails
//! 8.1.4 renders with the title nil and with the title "t".
//!
//! `L4` has an effect on each side: `mark_b` appends "b" to the body and
//! `mark_a` appends "a", so the body shows each ran once, left then right.

use super::emit_and_run::Overlay;
use super::nilable_string_view;

pub const VIEW: &str = concat!(
    "L0:<% if \"\" == @article.title %>y<% else %>n<% end %>|\n",
    "L1:<% if \"\" != @article.title %>y<% else %>n<% end %>|\n",
    "L2:<% if \"t\" == @article.title %>y<% else %>n<% end %>|\n",
    "L3:<% if \"t\" != @article.title %>y<% else %>n<% end %>|\n",
    "L4:<% if @article.mark_b == @article.mark_a.title %>y<% else %>n<% end %>|<%= @article.body %>|\n",
);

pub const NIL_RENDERS: &[&str] = &["L0:n|", "L1:y|", "L2:n|", "L3:y|", "L4:n|a body long enoughba|"];

pub const RENDERS: &[&str] = &["L0:n|", "L1:y|", "L2:y|", "L3:n|", "L4:y|a body long enoughba|"];

/// The probe app with a nil title.
pub fn strict_nil_app() -> Overlay {
    nilable_string_view::nil_effect_app(super::content_tag_options::app_rendering(VIEW, NIL_RENDERS))
}

/// The probe app with the title "t".
pub fn strict_app() -> Overlay {
    nilable_string_view::effect_app(super::content_tag_options::app_rendering(VIEW, RENDERS))
}
