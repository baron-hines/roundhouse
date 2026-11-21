//! aasm compiled from its installed source (the app is support/aasm_gem.rs's): `include AASM` gives the model `aasm` (the gem's `included` hook
//! extends it with `AASM::ClassMethods`), and its `aasm column: … do … end` block runs as the class loads. The
//! record starts in the initial state (the hook's `after_initialize`), fires an event, refuses one its state
//! does not allow, persists the state column and answers the state's scope, as aasm 5.5.1 answers on Rails.

use super::aasm_gem::{app, KNOWN_GAPS, SCRIPT};

#[test]
fn aasm_fires_events_and_persists_the_state_on_ruby() {
    let Some(app) = app() else { return };
    let run = app.run_ruby(SCRIPT);
    let unknown: Vec<_> = run.errors.iter().filter(|e| !KNOWN_GAPS.iter().any(|gap| e.contains(gap))).collect();
    assert!(unknown.is_empty(), "{unknown:#?}");
    for gap in KNOWN_GAPS {
        assert!(run.errors.iter().any(|e| e.contains(gap)), "{gap} is answered now: drop it from KNOWN_GAPS\n{:#?}", run.errors);
    }
    assert!(run.program_succeeded(), "{}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("aasm passed"), "{}", run.stdout);
}

/// maybe's `@account.active?` / `@account.disable!` from a controller: the methods aasm's block defines as the
/// class loads are known to the analysis (`analyze::aasm_surface`), so the controller's calls to them are typed and
/// run as aasm 5.5.1 answers them, on a machine the size of maybe's `Account` (four states, four events) with a
/// machine-wide callback as maybe's `Sync` has.
#[test]
fn a_controllers_calls_to_aasm_methods_are_typed_and_run() {
    let Some(app) = app() else { return };
    let run = app
        .edit("app/models/article.rb", "    state :published\n", "    state :published\n    state :archived\n    state :removed\n\n    after_all_transitions :note_transition\n\n    event :archive do\n      transitions from: [:draft, :published], to: :archived\n    end\n\n    event :restore do\n      transitions from: :archived, to: :draft\n    end\n\n    event :remove do\n      transitions from: [:draft, :published, :archived], to: :removed\n    end\n")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  def note_transition\n    @transitions = (@transitions || 0) + 1\n  end\n\n  def transitions_seen = @transitions.to_i\n\n")
        .edit("app/controllers/articles_controller.rb", "  def show\n  end\n", "  def show\n    @article.publish! if @article.draft? && @article.may_publish?\n    @article.archive!\n    @review = [@article.published?, @article.archived?, @article.may_restore?, @article.may_publish?, @article.transitions_seen].join(\",\")\n  end\n")
        .edit("app/views/articles/show.html.erb", "<hr class=\"my-8\">\n", "<hr class=\"my-8\">\n<div id=\"review\"><%= @review %></div>\n")
        .run_ruby(r##"
article = Article.create!(title: "Hello", body: "A body long enough")
status, _h, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/articles/#{article.id}", "rack.input" => StringIO.new(""))
got = body.join[%r{<div id="review">(.*?)</div>}m, 1]
raise "status #{status}, review #{got.inspect}" unless status == 200 && got == "false,true,true,false,2"
raise "status column" unless Article.find(article.id).status == "archived"
puts "typed aasm calls passed"
"##);
    let unknown: Vec<_> = run.errors.iter().filter(|e| !KNOWN_GAPS.iter().any(|gap| e.contains(gap))).collect();
    assert!(unknown.is_empty(), "{unknown:#?}");
    assert!(run.program_succeeded(), "{}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("typed aasm calls passed"), "{}", run.stdout);
}

/// maybe's `family.accounts.active` / `Sync.failed`: the scope aasm creates per state, on the class, on a relation
/// and after an association read, which is lowered to the association's relation so the runtime's scope answers it.
#[test]
fn aasm_state_scopes_answer_on_a_class_a_relation_and_an_association() {
    let Some(app) = app() else { return };
    let run = app
        .edit("db/schema.rb", "  create_table \"comments\", force: :cascade do |t|\n", "  create_table \"comments\", force: :cascade do |t|\n    t.string \"status\"\n")
        .edit(
            "app/models/comment.rb",
            "class Comment < ApplicationRecord\n",
            "class Comment < ApplicationRecord\n  include AASM\n\n  aasm column: :status do\n    state :pending, initial: true\n    state :approved\n\n    event :approve do\n      transitions from: :pending, to: :approved\n    end\n  end\n\n",
        )
        .edit("app/controllers/articles_controller.rb", "  def show\n  end\n", "  def show\n    @approved = @article.comments.approved.map(&:commenter).join(\",\")\n    @pending = Comment.pending.where(article_id: @article.id).count\n  end\n")
        .edit("app/views/articles/show.html.erb", "<hr class=\"my-8\">\n", "<hr class=\"my-8\">\n<div id=\"scopes\"><%= @approved %>|<%= @pending %></div>\n")
        .run_ruby(r##"
a = Article.create!(title: "Hello", body: "A body long enough")
b = Article.create!(title: "Other", body: "A body long enough")
Comment.create!(article: a, commenter: "Ann", body: "x").approve!
Comment.create!(article: a, commenter: "Bob", body: "y")
Comment.create!(article: b, commenter: "Cy", body: "z").approve!
status, _h, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/articles/#{a.id}", "rack.input" => StringIO.new(""))
got = body.join[%r{<div id="scopes">(.*?)</div>}m, 1]
raise "status #{status}, scopes #{got.inspect}" unless status == 200 && got == "Ann|1"
puts "aasm scopes passed"
"##);
    let unknown: Vec<_> = run.errors.iter().filter(|e| !KNOWN_GAPS.iter().any(|gap| e.contains(gap))).collect();
    assert!(unknown.is_empty(), "{unknown:#?}");
    assert!(run.program_succeeded(), "{}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("aasm scopes passed"), "{}", run.stdout);
}

/// A state named like a class method an extended module gives the model (`extend Finder` with `Finder#approved`):
/// aasm creates no scope over it, so the call stays the module's.
#[test]
fn an_extended_modules_method_of_a_states_name_is_not_a_scope() {
    let Some(app) = app() else { return };
    let errors = app
        .edit("db/schema.rb", "  create_table \"comments\", force: :cascade do |t|\n", "  create_table \"comments\", force: :cascade do |t|\n    t.string \"status\"\n")
        .write("app/models/finder.rb", "module Finder\n  def approved\n    7\n  end\nend\n")
        .edit(
            "app/models/comment.rb",
            "class Comment < ApplicationRecord\n",
            "class Comment < ApplicationRecord\n  include AASM\n  extend Finder\n\n  aasm column: :status do\n    state :pending, initial: true\n    state :approved\n  end\n\n  def self.approved_plus_one = approved + 1\n\n",
        )
        .check_errors();
    let unknown: Vec<_> = errors.iter().filter(|e| !KNOWN_GAPS.iter().any(|gap| e.contains(gap))).collect();
    assert!(unknown.is_empty(), "{unknown:#?}");
}
