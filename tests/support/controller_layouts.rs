//! One app and one table of pages for the interpreted and native lanes of
//! the controller layout contract (`tests/controller_layouts.rs`,
//! `spinel_toolchain/controller_layouts.rs`).
//!
//! The pages are what Rails 8.1.4 served for the same app: `layout "x"`,
//! `layout :method`, `layout false`, `only:`/`except:` (and a filter, a
//! public method run as a filter and a `rescue_from` handler rendering
//! under them), `render layout:`, inheritance (an action a subclass with
//! another layout inherits, or reaches through `super`), `layout nil`,
//! and the `layouts/<controller_path>` lookup.

use super::emit_and_run;

pub fn blog() -> emit_and_run::Overlay {
    let controllers: &[(&str, &str)] = &[
        ("application", "class ApplicationController < ActionController::Base\nend\n"),
        (
            "articles",
            "class ArticlesController < ApplicationController\n  layout \"reading\"\n\n  def index\n  end\nend\n",
        ),
        (
            "feeds",
            "class FeedsController < ApplicationController\n  layout :feed_layout\n\n  def index\n  end\n\nprivate\n\n  def feed_layout\n    \"print\"\n  end\nend\n",
        ),
        (
            "drafts",
            "class DraftsController < ApplicationController\n  layout false\n\n  def index\n  end\nend\n",
        ),
        (
            "archives",
            "class ArchivesController < ApplicationController\n  layout \"reading\", only: :index\n  before_action :closed_notice\n  rescue_from ActiveRecord::RecordNotFound, with: :missing\n\n  def index\n    Article.find(params[:id]) if params[:id]\n  end\n\n  def show\n    Article.find(params[:id]) if params[:id]\n  end\n\nprivate\n\n  def closed_notice\n    render \"shared/closed\" if params[:closed] == \"1\"\n  end\n\n  def missing\n    render \"shared/missing\", status: :not_found\n  end\nend\n",
        ),
        (
            "comments",
            "class CommentsController < ApplicationController\n  layout \"reading\", except: :show\n\n  def index\n  end\n\n  def show\n  end\nend\n",
        ),
        ("archived_comments", "class ArchivedCommentsController < CommentsController\n  layout \"print\"\nend\n"),
        (
            "exports",
            "class ExportsController < ApplicationController\n  def printable\n    render layout: \"print\"\n  end\n\n  def bare\n    render layout: false\n  end\n\n  def summary\n    render :overview, layout: \"print\"\n  end\nend\n",
        ),
        ("admin/base", "class Admin::BaseController < ApplicationController\n  layout \"admin\"\nend\n"),
        ("admin/stats", "class Admin::StatsController < Admin::BaseController\n  def index\n  end\nend\n"),
        ("authors", "class AuthorsController < ApplicationController\n  def index\n  end\nend\n"),
        (
            "guest_articles",
            "class GuestArticlesController < ArticlesController\n  layout nil\nend\n",
        ),
        ("tags", "class TagsController < ApplicationController\n  def index\n  end\nend\n"),
        (
            "notices",
            "class NoticesController < ApplicationController\n  layout \"reading\", only: :index\n  before_action :gate\n\n  def index\n  end\n\n  def show\n  end\n\n  def gate\n    render \"shared/closed\" if params[:closed] == \"1\"\n  end\nend\n",
        ),
        ("editions", "class EditionsController < ApplicationController\n  def index\n    render :index\n  end\nend\n"),
        (
            "special_editions",
            "class SpecialEditionsController < EditionsController\n  layout \"print\"\n\n  def index\n    super\n  end\nend\n",
        ),
    ];
    let mut app = emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"articles\", force: :cascade do |t|\n    t.string \"title\"\n  end\nend\n")
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        )
        .write("app/models/article.rb", "class Article < ApplicationRecord\nend\n");
    for (path, source) in controllers {
        app = app.write(&format!("app/controllers/{path}_controller.rb"), source);
    }
    for layout in ["application", "reading", "print", "admin", "authors"] {
        app = app.write(
            &format!("app/views/layouts/{layout}.html.erb"),
            &format!("<div>{}</div><%= yield %>\n", layout.to_uppercase()),
        );
    }
    for view in [
        "articles/index",
        "feeds/index",
        "drafts/index",
        "archives/index",
        "archives/show",
        "comments/index",
        "comments/show",
        "exports/printable",
        "exports/bare",
        "exports/overview",
        "admin/stats/index",
        "authors/index",
        "tags/index",
        "notices/index",
        "notices/show",
        "editions/index",
        "special_editions/index",
        "shared/closed",
        "shared/missing",
    ] {
        app = app.write(&format!("app/views/{view}.html.erb"), &format!("<p>{view}</p>\n"));
    }
    app.write(
        "config/routes.rb",
        r#"Rails.application.routes.draw do
  get "/articles", to: "articles#index"
  get "/feeds", to: "feeds#index"
  get "/drafts", to: "drafts#index"
  get "/archives", to: "archives#index"
  get "/archives/show", to: "archives#show"
  get "/comments", to: "comments#index"
  get "/comments/show", to: "comments#show"
  get "/archived_comments", to: "archived_comments#index"
  get "/archived_comments/show", to: "archived_comments#show"
  get "/exports/printable", to: "exports#printable"
  get "/exports/bare", to: "exports#bare"
  get "/exports/summary", to: "exports#summary"
  get "/admin/stats", to: "admin/stats#index"
  get "/authors", to: "authors#index"
  get "/guest_articles", to: "guest_articles#index"
  get "/tags", to: "tags#index"
  get "/notices", to: "notices#index"
  get "/notices/show", to: "notices#show"
  get "/editions", to: "editions#index"
  get "/special_editions", to: "special_editions#index"
end
"#,
    )
}

/// Each request (path, query) and the status and page Rails 8.1.4 serves
/// for it, whitespace collapsed.
pub const PAGES: &[(&str, &str, u16, &str)] = &[
    ("/articles", "", 200, "<div>READING</div><p>articles/index</p>"),
    ("/feeds", "", 200, "<div>PRINT</div><p>feeds/index</p>"),
    ("/drafts", "", 200, "<p>drafts/index</p>"),
    ("/archives", "", 200, "<div>READING</div><p>archives/index</p>"),
    ("/archives/show", "", 200, "<div>APPLICATION</div><p>archives/show</p>"),
    ("/archives", "closed=1", 200, "<div>READING</div><p>shared/closed</p>"),
    ("/archives/show", "closed=1", 200, "<div>APPLICATION</div><p>shared/closed</p>"),
    ("/archives", "id=404", 404, "<div>READING</div><p>shared/missing</p>"),
    ("/archives/show", "id=404", 404, "<div>APPLICATION</div><p>shared/missing</p>"),
    ("/comments", "", 200, "<div>READING</div><p>comments/index</p>"),
    ("/comments/show", "", 200, "<div>APPLICATION</div><p>comments/show</p>"),
    ("/archived_comments", "", 200, "<div>PRINT</div><p>comments/index</p>"),
    ("/archived_comments/show", "", 200, "<div>PRINT</div><p>comments/show</p>"),
    ("/exports/printable", "", 200, "<div>PRINT</div><p>exports/printable</p>"),
    ("/exports/bare", "", 200, "<p>exports/bare</p>"),
    ("/exports/summary", "", 200, "<div>PRINT</div><p>exports/overview</p>"),
    ("/admin/stats", "", 200, "<div>ADMIN</div><p>admin/stats/index</p>"),
    ("/authors", "", 200, "<div>AUTHORS</div><p>authors/index</p>"),
    ("/guest_articles", "", 200, "<div>READING</div><p>articles/index</p>"),
    ("/tags", "", 200, "<div>APPLICATION</div><p>tags/index</p>"),
    ("/notices", "", 200, "<div>READING</div><p>notices/index</p>"),
    ("/notices", "closed=1", 200, "<div>READING</div><p>shared/closed</p>"),
    ("/notices/show", "closed=1", 200, "<div>APPLICATION</div><p>shared/closed</p>"),
    ("/editions", "", 200, "<div>APPLICATION</div><p>editions/index</p>"),
    ("/special_editions", "", 200, "<div>PRINT</div><p>special_editions/index</p>"),
];
