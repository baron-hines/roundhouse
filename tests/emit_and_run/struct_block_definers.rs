//! A `Struct.new` block that defines methods in a loop over a literal list
//! (discourse's `TopicStatusUpdater::Status`), as Ruby 4.0 runs it.

use super::emit_and_run;

const STATUS: &str = "LoopedStatus =\n  Struct.new(:name, :enabled) do\n    %w[pinned closed].each do |status|\n      define_method(\"#{status}?\") { name == status }\n    end\n\n    def enabled?\n      enabled\n    end\n  end\n";

const SCRIPT: &str = "s = LoopedStatus.new(\"closed\", true)\ngot = [s.pinned?, s.closed?, s.enabled?]\nraise got.inspect unless got == [false, true, true]\nputs \"struct block definers passed\"\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/looped_status.rb", STATUS)
}

#[test]
fn a_struct_block_s_literal_definer_loop_runs() {
    let errors = app().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    app().run_ruby(SCRIPT).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_struct_block_s_literal_definer_loop_runs_on_spinel() {
    app().run_spinel(SCRIPT).assert_passes();
}

/// discourse's TopicStatusUpdater: a `Struct.new` block that writes another
/// `Status = Struct.new(…) do … end`. The block keeps the file's lexical
/// scope, so `Status` is the top level's constant, which the outer
/// struct's methods read.
const UPDATER: &str = "StatusUpdater =\n  Struct.new(:topic, :user) do\n    def update!(status, enabled)\n      s = Status.new(status, enabled)\n      [topic, user, s.name, s.closed?, s.enabled?]\n    end\n\n    Status =\n      Struct.new(:name, :enabled) do\n        %w[pinned closed].each do |status|\n          define_method(\"#{status}?\") { name == status }\n        end\n\n        def enabled?\n          enabled\n        end\n      end\n  end\n";

const UPDATER_SCRIPT: &str = "got = StatusUpdater.new(\"t\", \"u\").update!(\"closed\", false)\nraise got.inspect unless got == [\"t\", \"u\", \"closed\", true, false] && Status.new(\"pinned\", true).pinned?\nputs \"nested struct passed\"\n";

fn updater() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/status_updater.rb", UPDATER)
}

#[test]
fn a_struct_block_s_own_struct_constant_is_the_top_level_s() {
    let errors = updater().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    updater().run_ruby(UPDATER_SCRIPT).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_struct_block_s_own_struct_constant_is_the_top_level_s_on_spinel() {
    updater().run_spinel(UPDATER_SCRIPT).assert_passes();
}
