//! A gem's mailer, as Devise's `app/mailers/devise/mailer.rb` writes it inside `if defined?(ActionMailer)`: the
//! class is the app's like any `app/` class of the gem. Its actions run only where something calls them, here
//! through a model callback (`after_save :notify_saved`) of a module the model includes; the template of an
//! action nothing calls is left out of the Spinel tree with the action (`source_gem_shake`). The template reads
//! an ivar a module of the mailer sets, as Devise's `Devise::Mailers::Helpers#initialize_from_record` sets
//! `@resource` with `instance_variable_set`. The module's `mail` renders the template the calling action's name
//! finds, or the first of its `template_path` directories holds, as Devise's `devise_mail` passes them.

use roundhouse::project::BuildTarget;

use super::emit_and_run;

const GEMS: &str = "vendor/bundle/ruby/3.4.0/gems/notices-1.0";

fn installed() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    notices (1.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  notices\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{GEMS}/lib/notices.rb"), "require \"notices/engine\"\nrequire \"notices/notifiable\"\nrequire \"notices/mailers\"\n")
        .write(&format!("{GEMS}/lib/notices/engine.rb"), "module Notices\n  class Engine < ::Rails::Engine\n  end\nend\n")
        .write(
            &format!("{GEMS}/lib/notices/notifiable.rb"),
            "module Notices\n  module Notifiable\n    extend ActiveSupport::Concern\n\n    included do\n      after_save :notify_saved\n    end\n\n    def notify_saved\n      Notices::Mailer.saved(title).deliver_now\n      Notices::Mailer.renamed(title).deliver_now\n    end\n  end\nend\n",
        )
        .write(
            &format!("{GEMS}/lib/notices/mailers.rb"),
            "module Notices\n  module Mailers\n    def notify(title, action, dirs = nil)\n      @title = instance_variable_set(\"@#{action}_title\", title)\n      headers = { to: \"a@example.com\", subject: action.to_s }\n      headers[:template_path] = dirs if dirs\n      mail(headers)\n    end\n  end\nend\n",
        )
        .write(
            &format!("{GEMS}/app/mailers/notices/mailer.rb"),
            "if defined?(ActionMailer)\n  class Notices::Mailer < ActionMailer::Base\n    include Notices::Mailers\n\n    def saved(title)\n      notify(title, :saved)\n    end\n\n    def removed(title)\n      notify(title, :removed)\n    end\n\n    def renamed(title)\n      notify(title, :renamed, [\"notices/missing\", \"notices/scoped\"])\n    end\n  end\nend\n",
        )
        .write(&format!("{GEMS}/app/views/notices/mailer/saved.text.erb"), "Saved <%= @title %>\n")
        .write(&format!("{GEMS}/app/views/notices/mailer/removed.text.erb"), "Removed <%= @title %>\n")
        .write(&format!("{GEMS}/app/views/notices/mailer/renamed.text.erb"), "Renamed by default <%= @title %>\n")
        .write(&format!("{GEMS}/app/views/notices/scoped/renamed.text.erb"), "Renamed <%= @title %>\n")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  include Notices::Notifiable\n\n")
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn a_gem_mailer_keeps_the_action_a_callback_reaches_and_drops_the_other() {
    let (tree, _errors) = installed().emit(BuildTarget::Spinel);
    let texts: Vec<String> = walk(&tree).iter().filter_map(|p| std::fs::read_to_string(p).ok()).collect();
    let mentions = |needle: &str| texts.iter().any(|t| t.contains(needle));
    assert!(mentions("Mailer < "), "the gem's mailer is in the Spinel tree");
    assert!(mentions("def saved(title)"), "the action the callback reaches");
    assert!(!mentions("def removed(title)"), "an action nothing calls");
    assert!(mentions("Saved "), "the reached action's template");
    assert!(!mentions("Removed "), "the unreached action's template");
}

const SCRIPT: &str = r#"Article.create!(title: "Hello", body: "a body long enough")
sent = ActionMailer::Base.deliveries
raise "sent #{sent.size}" unless sent.size == 2
saved = sent[0]
raise "saved #{saved.body.to_s.inspect}" unless saved.body.to_s.include?("Saved Hello")
raise "subject #{saved.subject.inspect}" unless saved.subject == "saved"
renamed = sent[1]
raise "renamed #{renamed.body.to_s.inspect}" unless renamed.body.to_s.include?("Renamed Hello") && !renamed.body.to_s.include?("by default")
puts "module mail passed"
"#;

/// The `mail` its module calls renders the template of the action that calls it, or the one its `template_path`
/// finds first. Spinel only: the Ruby tree requires the gem itself.
#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_mail_in_a_mailers_module_renders_the_template_its_lookup_finds_natively() {
    let run = installed().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("module mail passed"), "{}", run.stdout);
}
