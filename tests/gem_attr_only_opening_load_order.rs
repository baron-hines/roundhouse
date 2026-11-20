//! A gem class opened in several files is emitted with its openings in the
//! order the gem's requires load them. An opening that holds only an
//! `attr_reader`, or only a class-level `@ivar = …`, is put in its file's
//! place like any other: Ruby runs it there, so a later file's `def` of the
//! same name replaces the reader.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

/// `lib/shelf.rb` requires `a.rb`, then `b.rb`, then opens `Shelf` for a
/// nested class only. With two openings that name no file, neither can be
/// placed by elimination, so `a.rb`'s has to be placed by its own span.
const SHELF: &str = "require \"shelf/a\"\nrequire \"shelf/b\"\n\nclass Shelf\n  class Cap\n  end\nend\n";
const B: &str = "class Shelf\n  @mode = \"b\"\n\n  def self.mode\n    @mode\n  end\n\n  def name\n    \"b\"\n  end\nend\n";
const LOCK: &str = "PATH\n  remote: vendor/gems/shelf-1.0.0\n  specs:\n    shelf (1.0.0)\n\nGEM\n  remote: https://rubygems.org/\n  specs:\n\nDEPENDENCIES\n  shelf!\n";

fn app(a: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("roundhouse.yml", "")
        .write("Gemfile.lock", LOCK)
        .write("Gemfile", "source \"https://rubygems.org\"\n\ngem \"shelf\"\n")
        .write("config/application.rb", "Bundler.require(*Rails.groups)\n")
        .write("vendor/gems/shelf-1.0.0/lib/shelf.rb", SHELF)
        .write("vendor/gems/shelf-1.0.0/lib/shelf/a.rb", a)
        .write("vendor/gems/shelf-1.0.0/lib/shelf/b.rb", B)
}

/// Ruby 4.0.7, the gem as written: `b.rb`'s `name` replaces `a.rb`'s reader.
#[test]
fn an_attr_reader_only_opening_runs_before_the_file_loaded_after_it() {
    let run = app("class Shelf\n  attr_reader :name\nend\n").run_ruby("puts Shelf.new.name.inspect\n");
    run.assert_passes();
    assert_eq!(run.stdout.lines().last(), Some("\"b\""), "{}", run.stderr);
}

/// The same with an opening that only sets a class-level `@ivar`: `b.rb`
/// sets it again after, so it reads `b.rb`'s value.
#[test]
fn a_class_ivar_only_opening_runs_before_the_file_loaded_after_it() {
    let a = "class Shelf\n  @mode = \"a\"\nend\n";
    let run = app(a).run_ruby("puts Shelf.mode.inspect\n");
    run.assert_passes();
    assert_eq!(run.stdout.lines().last(), Some("\"b\""), "{}", run.stderr);
}
