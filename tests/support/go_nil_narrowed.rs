//! A nullable String column compared with another, and after a nil check,
//! which narrows the analyzer's type to String on a local's reader while
//! the Go field stays `*string`. Each line is tagged `L<n>:`. The renders
//! are what Rails 8.1.4 renders.

use super::emit_and_run::Overlay;

const IF_EQUAL: &str = "<% if a.title == a.body %>y<% else %>n<% end %>";

/// `@article.title` and the like, then the same reads off a local.
pub fn view() -> String {
    let lines = [
        "<% if @article.title != nil %><% if @article.title == \"same text\" %>y<% else %>n<% end %><% end %>".to_string(),
        "<% if @article.title != nil %><% if \"same text\" == @article.title %>y<% else %>n<% end %><% end %>".to_string(),
        "<% if @article.title != nil %><% if @article.title != \"x\" %>y<% else %>n<% end %><% end %>".to_string(),
        "<% if @article.title != nil %><% if @article.title == @article.body %>y<% else %>n<% end %><% end %>".to_string(),
        "<% if @article.body != nil %><% if @article.title == @article.body %>y<% else %>n<% end %><% end %>".to_string(),
        "<% if @article.title == @article.body %>y<% else %>n<% end %>".to_string(),
        "<% if @article.title != @article.body %>y<% else %>n<% end %>".to_string(),
        format!("<% a = @article %><% if a.title != nil %>{IF_EQUAL}<% end %>"),
        format!("<% if a.body != nil %>{IF_EQUAL}<% end %>"),
        format!("<% if a.title != nil && a.body != nil %>{IF_EQUAL}<% end %>"),
        "<% if a.title != nil %><% if a.title != a.body %>y<% else %>n<% end %><% end %>".to_string(),
        "<% if a.title != nil %><% if a.title == \"same text\" %>y<% else %>n<% end %><% end %>".to_string(),
        "<% if a.title != nil %><% if \"same text\" == a.title %>y<% else %>n<% end %><% end %>".to_string(),
        "<% if @article.mark_a.title == @article.mark_c.body %>y<% else %>n<% end %>|<%= @article.body %>".to_string(),
    ];
    lines.iter().enumerate().map(|(i, line)| format!("L{i}:{line}|\n")).collect()
}

pub const RENDERS: &[&str] = &[
    "L0:y|", "L1:y|", "L2:y|", "L3:y|", "L4:y|", "L5:y|", "L6:n|", "L7:y|", "L8:y|", "L9:y|", "L10:n|",
    "L11:y|", "L12:y|", "L13:n|same textac|",
];

const COMPARE_VIEW: &str = concat!(
    "L0:<% if @article.title == @article.body %>y<% else %>n<% end %>|\n",
    "L1:<% if @article.title != @article.body %>y<% else %>n<% end %>|\n",
);

fn probe(base: Overlay, article: &str) -> Overlay {
    let controller = format!(
        "class ProbesController < ApplicationController\n  def show\n    @article = {article}\n  end\nend\n"
    );
    base.write("app/controllers/probes_controller.rb", &controller)
}

/// The title and the body hold the same text in two strings; `mark_a`
/// and `mark_c` append to the body and return the record.
pub fn strict_app() -> Overlay {
    probe(super::content_tag_options::app_rendering(&view(), RENDERS), "Article.new(id: 7, title: \"same text\", body: \"same text\")")
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def mark_a\n    self.body = body + \"a\"\n    self\n  end\n\n  def mark_c\n    self.body = body + \"c\"\n    self\n  end\n\n",
        )
        .write("sig/article_marks.rbs", "class Article\n  def mark_a: () -> Article\n  def mark_c: () -> Article\nend\n")
}

/// Each nil and present pairing: nil equals only nil.
pub fn strict_pair_apps() -> Vec<Overlay> {
    let app = |article: &str, renders: &[&str]| {
        probe(super::content_tag_options::app_rendering(COMPARE_VIEW, renders), article)
    };
    vec![
        app("Article.new", &["L0:y|", "L1:n|"]),
        app("Article.new(id: 7, body: \"x\")", &["L0:n|", "L1:y|"]),
        app("Article.new(id: 7, title: \"x\")", &["L0:n|", "L1:y|"]),
        app("Article.new(id: 7, title: \"x\", body: \"y\")", &["L0:n|", "L1:y|"]),
    ]
}

/// The nil check and the comparison in a controller action and in a model
/// method, on a local's and a parameter's readers.
pub fn strict_code_app() -> Overlay {
    let view = "L0:<%= @same %>|\nL1:<%= @other %>|\n";
    super::content_tag_options::app_rendering(view, &["L0:y|", "L1:y|"])
        .write(
            "app/controllers/probes_controller.rb",
            "class ProbesController < ApplicationController\n  def show\n    a = Article.new(id: 7, title: \"same text\", body: \"same text\")\n    @same = \"n\"\n    if a.title != nil\n      @same = \"y\" if a.title == a.body\n    end\n    @other = a.reads_same(a)\n  end\nend\n",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def reads_same(other)\n    if other.title != nil\n      return \"y\" if other.body == other.title\n    end\n    \"n\"\n  end\n\n",
        )
        .write("sig/article_reads.rbs", "class Article\n  def reads_same: (Article) -> String\nend\n")
}
