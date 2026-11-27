//! A template local assigned from the controller ivar of its own name
//! (`<% article = @article %>`). The ivar is the view's parameter
//! `article`, so the assignment rebinds that parameter to itself. L2 is
//! the same rebind as the only statement of an `if`, a body the dropped
//! rebind leaves empty. Each line is tagged `L<n>:`, and `RENDERS` is what
//! Rails 8.1.4 renders.

use super::emit_and_run::Overlay;

pub const VIEW: &str = concat!(
    "<% article = @article %>\n",
    "L0:<%= article.title %>\n",
    "L1:<%= @article.title %>\n",
    "<% if @article.id > 0 %><% article = @article %><% end %>L2:<%= article.title %>\n",
);

pub const RENDERS: &[&str] = &["L0:t", "L1:t", "L2:t"];

/// `base` (a probe app over `fixtures/real-blog`) with the record the
/// view reads.
pub fn app(base: Overlay) -> Overlay {
    base.write(
        "app/controllers/probes_controller.rb",
        "class ProbesController < ApplicationController\n  def show\n    @article = Article.new(id: 7, title: \"t\", body: \"a body long enough\")\n  end\nend\n",
    )
}

/// The strict targets' probe app.
pub fn strict_app() -> Overlay {
    app(super::content_tag_options::app_rendering(VIEW, RENDERS))
}
