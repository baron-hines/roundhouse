//! A has_many's `before_add`, `after_add`, `before_remove` and
//! `after_remove` callbacks run around `build`, `create!`, `<<` and
//! `delete`, in that order, as Rails' `add_to_target` and `remove_records`
//! run them. Ignored as a fork spec until the compiler does; the expected
//! values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/playlist.rb", r##"class Playlist < ApplicationRecord
  has_many :tracks,
    before_add: :before_track_added, after_add: :after_track_added,
    before_remove: :before_track_removed, after_remove: :after_track_removed

  def events
    @events ||= []
  end

  def before_track_added(track)
    events << "before_add #{track.title} #{track.persisted?}"
  end

  def after_track_added(track)
    events << "after_add #{track.title} #{track.persisted?}"
  end

  def before_track_removed(track)
    events << "before_remove #{track.title}"
  end

  def after_track_removed(track)
    events << "after_remove #{track.title}"
  end
end
"##)
        .write("app/models/track.rb", r##"class Track < ApplicationRecord
  belongs_to :playlist, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    p = Playlist.create!(name: "late")
    p.tracks.build(title: "a")
    b = p.tracks.create!(title: "b")
    p.tracks << Track.new(title: "c")
    p.tracks.delete(b)
    expected = [
      "before_add a false", "after_add a false",
      "before_add b false", "after_add b true",
      "before_add c false", "after_add c true",
      "before_remove b", "after_remove b"
    ]
    # Rails: the list above
    check("callback order", p.events == expected)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "playlists", force: :cascade do |t|
    t.string "name"
  end
  create_table "tracks", force: :cascade do |t|
    t.string "title"
    t.integer "playlist_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: association targets"]
fn has_many_add_and_remove_callbacks_run_around_the_writes() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn has_many_add_and_remove_callbacks_run_around_the_writes_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
