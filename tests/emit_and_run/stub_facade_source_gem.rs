//! A gem the compiler models only by a façade that raises (rotp's `ROTP`,
//! every method `GemFacade.fail!`) is compiled from source on Spinel when
//! the app depends on it directly and it is installed as plain Ruby; the
//! ruby family keeps loading the real gem. lobsters' two-factor sign-in
//! raised on Spinel before.
//!
//! Also `roundhouse.yml`'s provisional `spinel.int_overflow`, which rotp
//! needs (its Base32 decode shifts past 64 bits).

use super::emit_and_run;
use roundhouse::project::BuildTarget;

const GEMS: &str = "vendor/bundle/ruby/3.4.0/gems";

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    rotp (6.3.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  rotp\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{GEMS}/rotp-6.3.0/lib/rotp.rb"), "require \"rotp/totp\"\n")
        .write(
            &format!("{GEMS}/rotp-6.3.0/lib/rotp/totp.rb"),
            "module ROTP\n  class TOTP\n    attr_reader :secret\n\n    def initialize(secret)\n      @secret = secret\n    end\n\n    def interval\n      30\n    end\n  end\nend\n",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def otp_interval\n    ROTP::TOTP.new(\"JBSWY3DPEHPK3PXP\").interval\n  end\n\n",
        )
}

fn mentions(tree: &std::path::Path, path: &str, needle: &str) -> bool {
    std::fs::read_to_string(tree.join(path)).is_ok_and(|text| text.contains(needle))
}

#[test]
fn the_spinel_tree_compiles_the_gem_behind_a_raising_facade() {
    let (emitted, errors) = overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(mentions(&emitted, "app/models/rotp/totp.rb", "def interval"), "rotp's source is not in the Spinel tree");
    assert!(!mentions(&emitted, "runtime/gem_facades.rb", "GemFacade.fail!(\"ROTP"), "the raising ROTP façade still ships");
    let makefile = std::fs::read_to_string(emitted.join("Makefile")).expect("Makefile");
    assert!(!makefile.contains("--int-overflow"), "{makefile}");
}

#[test]
fn the_ruby_tree_keeps_the_real_gem() {
    let (emitted, errors) = overlay().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(!emitted.join("app/models/rotp/totp.rb").exists(), "rotp is compiled into the Ruby tree");
}

#[test]
fn spinel_int_overflow_reaches_every_compile_the_makefile_runs() {
    let (emitted, errors) =
        overlay().write("roundhouse.yml", "spinel:\n  int_overflow: promote\n").emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let makefile = std::fs::read_to_string(emitted.join("Makefile")).expect("Makefile");
    assert!(makefile.contains("build -- --int-overflow=promote\n"), "{makefile}");
    assert!(makefile.contains("$(SPINEL_TEST_FLAGS) --int-overflow=promote $< -o $@"), "{makefile}");

    let tree: std::collections::HashMap<std::path::PathBuf, Vec<u8>> = [
        ("roundhouse.yml", "spinel:\n  int_overflow: bignum\n"),
        ("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n"),
    ]
    .into_iter()
    .map(|(path, text)| (std::path::PathBuf::from(path), text.as_bytes().to_vec()))
    .collect();
    match roundhouse::ingest::ingest_app_from_tree(tree) {
        Err(roundhouse::ingest::IngestError::Parse { message, .. }) => {
            assert!(message.contains("spinel.int_overflow must be raise, wrap or promote"), "{message}")
        }
        other => panic!("expected a configuration error, got {:?}", other.map(|_| "an app")),
    }
}
