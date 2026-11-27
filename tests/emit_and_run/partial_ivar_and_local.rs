//! A partial that reads both `@tag` and a local `tag` reads two values in
//! Rails: the ivar is its caller's, the local is what the render hands
//! it (`locals: { tag: … }`, `tag: …`) or nil. They used to share one
//! param, so one of the two read the other's value: the ivar won over a
//! handed local (`render "byline", author: @other` printed `@author`
//! twice), a local named like a helper (`csrf_meta_tags`) lost to the
//! helper, a bare `notice` handed `notice:` read `@notice`, and a
//! convention partial's `local_assigns[:post]` read `@post` where no
//! render hands it a `post`. An action's `render partial:` hands its
//! `locals:` to the locals too, never to the partial's ivars. Such a
//! partial's `render @writer` renders `writers/_writer`, and the
//! template a `render template:` in it renders reads the caller's ivar.

use super::emit_and_run;

const POSTS: &str = r#"class PostsController < ApplicationController
  def tagged
    @tag = "IVAR"
  end

  def meta
    @csrf_meta_tags = "IVARV"
  end

  def byline
    @author = "Ann"
    @other = "Bob"
  end

  def teaser
    @post = Post.find_by(title: "Rails tips")
  end

  def handed
    @notice = "ivar-notice"
    flash.now[:notice] = "flash-notice"
  end

  def rendered_summary
    @post = Post.find_by(title: "Rails tips")
    render partial: "posts/summary", locals: { post: Post.find_by(title: "Go tips") }
  end

  def rendered_byline
    @author = "Ann"
    render partial: "posts/byline", locals: { author: "Bob" }
  end

  def card
    @writer = Writer.find_by(name: "Ann")
  end

  def cards
    @writers = Writer.all
  end

  def tagbox
    @tag = "IVAR"
  end

  def details
    @tag = "IVAR"
  end

  def twin
    @tag = "IVAR"
  end
end
"#;

const ROUTES: &str = r#"Rails.application.routes.draw do
  get "/posts/tagged", to: "posts#tagged"
  get "/posts/meta", to: "posts#meta"
  get "/posts/byline", to: "posts#byline"
  get "/posts/teaser", to: "posts#teaser"
  get "/posts/handed", to: "posts#handed"
  get "/posts/rendered_summary", to: "posts#rendered_summary"
  get "/posts/rendered_byline", to: "posts#rendered_byline"
  get "/posts/card", to: "posts#card"
  get "/posts/cards", to: "posts#cards"
  get "/posts/tagbox", to: "posts#tagbox"
  get "/posts/details", to: "posts#details"
  get "/posts/twin", to: "posts#twin"
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define(version: 1) do\n  create_table :posts do |t|\n    t.string :title\n  end\n  create_table :writers do |t|\n    t.string :name\n  end\nend\n",
        )
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n",
        )
        .write("app/models/post.rb", "class Post < ApplicationRecord\nend\n")
        .write("app/models/writer.rb", "class Writer < ApplicationRecord\nend\n")
        .write(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\nend\n",
        )
        .write("app/controllers/posts_controller.rb", POSTS)
        .write("config/routes.rb", ROUTES)
        .write("app/views/layouts/application.html.erb", "<main><%= yield %></main>\n")
        .write("app/views/posts/_tagged.html.erb", "<p class=\"tagged\">ivar=<%= @tag %> local=<%= tag %></p>\n")
        .write(
            "app/views/posts/tagged.html.erb",
            "<div><%= render partial: \"tagged\", locals: { tag: \"LOCAL\" } %></div>\n",
        )
        .write(
            "app/views/posts/_meta.html.erb",
            "<p class=\"meta\">ivar=<%= @csrf_meta_tags %> local=<%= csrf_meta_tags %></p>\n",
        )
        .write(
            "app/views/posts/meta.html.erb",
            "<div><%= render partial: \"meta\", locals: { csrf_meta_tags: \"LOCALV\" } %></div>\n",
        )
        .write("app/views/posts/_byline.html.erb", "<i><%= @author %>/<%= author %></i>\n")
        .write(
            "app/views/posts/byline.html.erb",
            "<div><%= render partial: \"byline\", locals: { author: @other } %><%= render \"byline\", author: @author %></div>\n",
        )
        .write(
            "app/views/posts/_teaser.html.erb",
            "<p class=\"teaser\">ivar=<%= @post.title %> local=<%= local_assigns[:post].nil? ? \"none\" : local_assigns[:post].title %></p>\n",
        )
        .write("app/views/posts/teaser.html.erb", "<div><%= render \"teaser\" %><%= render \"summary\" %></div>\n")
        .write("app/views/posts/_banner.html.erb", "<p class=\"banner\">ivar=<%= @notice %> bare=<%= notice %></p>\n")
        .write("app/views/posts/handed.html.erb", "<div><%= render \"banner\", notice: \"local-notice\" %></div>\n")
        .write("app/views/posts/_summary.html.erb", "<p class=\"summary\"><%= @post.title %></p>\n")
        .write("app/views/writers/_writer.html.erb", "<b><%= writer.name %></b>\n")
        .write("app/views/posts/_card.html.erb", "<p class=\"card\"><%= local_assigns[:writer] %>|<%= render @writer %></p>\n")
        .write("app/views/posts/card.html.erb", "<div><%= render \"card\", writer: \"LOCAL\" %></div>\n")
        .write("app/views/posts/_cards.html.erb", "<p class=\"cards\"><%= local_assigns[:writers] %>|<%= render @writers %></p>\n")
        .write("app/views/posts/cards.html.erb", "<div><%= render \"cards\", writers: \"LOCAL\" %></div>\n")
        .write("app/views/posts/details.html.erb", "<u><%= @tag %></u>\n")
        .write(
            "app/views/posts/_tagbox.html.erb",
            "<p class=\"tagbox\"><%= local_assigns[:tag] %>|<%= render template: \"posts/details\" %></p>\n",
        )
        .write("app/views/posts/tagbox.html.erb", "<div><%= render \"tagbox\", tag: \"LOCAL\" %></div>\n")
        .write(
            "app/views/posts/_twin.html.erb",
            "<p class=\"twin\"><%= local_assigns[:tag_] %>|<%= local_assigns[:tag] %>|<%= @tag %></p>\n",
        )
        .write("app/views/posts/twin.html.erb", "<div><%= render \"twin\", tag_: \"U\", tag: \"L\" %></div>\n")
}

const ASSERTIONS: &str = r#"
require_relative "app/controllers/posts_controller"

Post.create!(title: "Rails tips")
Post.create!(title: "Go tips")
Writer.create!(name: "Ann")
Writer.create!(name: "Bob")

def body_of(c, action)
  c.params = {}
  ActionController::Current.controller = c
  c.process_action(action)
  c.body
rescue NoMethodError => e
  e.message
end

missing = []
{
  tagged: ["<p class=\"tagged\">ivar=IVAR local=LOCAL</p>"],
  meta: ["<p class=\"meta\">ivar=IVARV local=LOCALV</p>"],
  byline: ["<i>Ann/Bob</i>", "<i>Ann/Ann</i>"],
  teaser: ["<p class=\"teaser\">ivar=Rails tips local=none</p>"],
  handed: ["<p class=\"banner\">ivar=ivar-notice bare=local-notice</p>"],
  rendered_summary: ["<p class=\"summary\">Rails tips</p>"],
  rendered_byline: ["<i>Ann/Bob</i>"],
  card: ["<p class=\"card\">LOCAL|<b>Ann</b>"],
  cards: ["<p class=\"cards\">LOCAL|<b>Ann</b>", "<b>Bob</b>"],
  tagbox: ["<p class=\"tagbox\">LOCAL|<u>IVAR</u>"],
  twin: ["<p class=\"twin\">U|L|IVAR</p>"],
}.each do |action, wants|
  body = body_of(PostsController.new, action)
  missing << "posts##{action}: #{body}" unless wants.all? { |want| body.include?(want) }
end
raise missing.join("\n") unless missing.empty?
puts "every partial kept its ivar and its local apart"
"#;

#[test]
fn a_partial_keeps_an_ivar_and_a_same_named_local_apart() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("every partial kept its ivar and its local apart"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_partial_keeps_an_ivar_and_a_same_named_local_apart_on_spinel() {
    let run = app().run_spinel_with_db(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("every partial kept its ivar and its local apart"), "{}", run.stdout);
}
