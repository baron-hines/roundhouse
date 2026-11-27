//! #1890: a route helper called inside a larger output expression, with
//! a record of the app's model as its argument. Rails fills the segment
//! from the record's `to_param`, and the strict targets' generated
//! helpers take that scalar. L3's argument is already a scalar. Each
//! line is tagged `L<n>:`, and its `*_RENDERS` entry is what Rails 8.1.4
//! renders (the ruby target and Spinel check each line's whole output,
//! the strict targets that the page holds each one,
//! `content_tag_options::app_rendering`).
//!
//! `SHADOW` (L5–L7) is the helpers whose body the app wrote, which take
//! the record as written: `draft_path` and `draft_url`, app helpers of a
//! route's names that a bare call reaches first, and `fresh_article_path`,
//! a `direct` route whose block reads the record. Only
//! the ruby target and Spinel render it; the other targets don't reach
//! an app helper from a view yet. `URL` (L8) is the `_url` form: the
//! ruby target checks it against Rails, the strict targets that it
//! builds, and Spinel's in-process render (no request host) leaves it
//! out.

use super::emit_and_run::Overlay;

pub const ROUTE_LINES: &str = concat!(
    "<% rec = @article %>\n",
    r#"L0:<%= "" + article_path(rec) %>"#, "\n",
    r#"L1:<%= link_to "x", "" + article_path(rec) %>"#, "\n",
    r#"L2:<%= "" + article_path(@article) %>"#, "\n",
    r#"<% n = 7 %>L3:<%= "" + article_path(n) %>"#, "\n",
    r##"L4:<%= "#{edit_article_path(rec)}?a=1" %>"##, "\n",
);

pub const ROUTE_RENDERS: &[&str] = &[
    "L0:/articles/7",
    r#"L1:<a href="/articles/7">x</a>"#,
    "L2:/articles/7",
    "L3:/articles/7",
    "L4:/articles/7/edit?a=1",
];

pub const SHADOW: &str = concat!(
    r#"L5:<%= "" + draft_path(rec) %>"#, "\n",
    r#"L6:<%= "" + draft_url(rec) %>"#, "\n",
    r#"L7:<%= "" + fresh_article_path(rec) %>"#, "\n",
);
pub const SHADOW_RENDERS: &[&str] = &["L5:/d/t", "L6:/u/t", "L7:/f/t"];

pub const URL: &str = concat!(r#"L8:<%= "" + article_url(rec) %>"#, "\n");
pub const URL_RENDERS: &str = "L8:http://www.example.com/articles/7";

/// The route lines, with `SHADOW` and `URL` where asked, and what
/// Rails renders for each line.
pub fn view(shadow: bool, url: bool) -> (String, Vec<&'static str>) {
    let mut view = ROUTE_LINES.to_string();
    let mut renders = ROUTE_RENDERS.to_vec();
    if shadow {
        view.push_str(SHADOW);
        renders.extend_from_slice(SHADOW_RENDERS);
    }
    if url {
        view.push_str(URL);
        renders.push(URL_RENDERS);
    }
    (view, renders)
}

/// `base` (a probe app over `fixtures/real-blog`) with the record the
/// view reads, and for `shadow` the `draft` route, the app helpers of
/// its names and the `fresh_article` direct route.
pub fn app(base: Overlay, shadow: bool) -> Overlay {
    let base = base.write(
        "app/controllers/probes_controller.rb",
        "class ProbesController < ApplicationController\n  def show\n    @article = Article.new(id: 7, title: \"t\", body: \"a body long enough\")\n  end\nend\n",
    );
    if !shadow {
        return base;
    }
    base.edit(
        "config/routes.rb",
        "  root \"articles#index\"\n",
        "  root \"articles#index\"\n  get \"/drafts/:id\", to: \"articles#show\", as: :draft\n  direct(:fresh_article) { |article| \"/f/#{article.title}\" }\n",
    )
        .edit(
            "app/helpers/application_helper.rb",
            "module ApplicationHelper\n",
            "module ApplicationHelper\n  def draft_path(article)\n    \"/d/#{article.title}\"\n  end\n\n  def draft_url(article)\n    \"/u/#{article.title}\"\n  end\n",
        )
}

/// The strict targets' probe app: the route lines, checked against
/// Rails, and the `_url` line, which only has to build (these runtimes
/// write a `_url` as its path, without the request's origin).
pub fn strict_app() -> (Overlay, Vec<&'static str>) {
    let (view, _) = view(false, true);
    (app(super::content_tag_options::app_rendering(&view, ROUTE_RENDERS), false), ROUTE_RENDERS.to_vec())
}
