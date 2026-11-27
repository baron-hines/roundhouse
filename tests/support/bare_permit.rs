//! A params helper that names no resource, as lobsters' search and
//! moderation log write them, shared by the Ruby lane and the Spinel lane.

use super::emit_and_run;

const CONTROLLER: &str = r#"class FindsController < ApplicationController
  def index
    @finder = Finder.new(find_params)
    @paging = find_params
    @pairs = @finder.pairs
    @who = log_params.fetch("who", "(All)")
    @stories = log_params.dig(:what, :stories).to_s
    @bogus = log_params.dig(:what, :bogus).to_s
    @page = log_params.fetch(:page) { 1 }.to_i
    @articles = Article.all
    @articles = @articles.joins(:comments).where(comments: { commenter: @who }) unless @who == "(All)"
    @count = @articles.count
  end

  def pager
    @paging = find_params
    render partial: "shared/pager"
  end

  def helper_pager
    @paging = find_params
    show_pager
  end

  private

  def show_pager
    render partial: "shared/pager"
  end

  def find_params
    params.permit(:q)
  end

  def log_params
    @log_params ||= params.permit(:who, :page, what: %i[stories comments])
  end
end
"#;

const FINDER: &str = r#"class Finder
  attr_reader :q

  def initialize(params)
    @q = params[:q].to_s
  end

  def pairs
    [{ q: @q }, { page: "one" }]
  end

  def label(value)
    "<#{value}>"
  end
end
"#;

const VIEW: &str = r#"<% @pairs.each do |pair| %>
  <% kind, value = pair.first %>
  <% case kind %>
  <% when :q %>
    <p id="q"><%= @finder.label(value) %></p>
  <% else %>
    <p id="other"><%= value %></p>
  <% end %>
<% end %>
<p id="log"><%= @who %>|<%= @stories %>|<%= @bogus %>|<%= @page %>|<%= @count %></p>
<p><%= link_to "next", @paging.merge(page: 2), id: "next" %></p>
<%= render "shared/pager" %>
<%= render "shared/strict_pager", label: "last" %>
"#;

/// A partial outside the controller's directory, so the request's
/// controller and action reach it from the view rendering it.
const PAGER: &str = r#"<p><%= link_to "later", @paging.merge(page: 3), id: "later" %></p>
"#;

/// The same through a strict-locals partial, which takes the request
/// context as keywords.
const STRICT_PAGER: &str = r#"<%# locals: (label: "end") -%>
<p><%= link_to label, @paging.merge(page: 4), id: "strict" %></p>
"#;

#[allow(dead_code)]
pub const QUERY: &str = "/finds?q=rails&who=Alice&page=2&what%5Bstories%5D=1&what%5Bbogus%5D=x&extra=y";

#[allow(dead_code)]
pub fn overlay() -> emit_and_run::Overlay {
    apply(emit_and_run::real_blog())
}

/// Not a native build of its own: the Spinel lane serves it from params_helper_no_permit's.
pub fn apply(overlay: emit_and_run::Overlay) -> emit_and_run::Overlay {
    overlay
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  resources :finds, only: [:index]\n  get \"/finds/page/:page\" => \"finds#index\"\n  get \"/finds/pager\" => \"finds#pager\"\n  get \"/finds/helper_pager\" => \"finds#helper_pager\"\n",
        )
        .write("app/controllers/finds_controller.rb", CONTROLLER)
        .write("app/models/finder.rb", FINDER)
        .write("app/views/finds/index.html.erb", VIEW)
        .write("app/views/shared/_pager.html.erb", PAGER)
        .write("app/views/shared/_strict_pager.html.erb", STRICT_PAGER)
}
