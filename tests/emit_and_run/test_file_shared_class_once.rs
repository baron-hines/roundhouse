//! A test file's non-test classes run their bodies, once however many of
//! its test classes a process loads, as Ruby runs the file once: a
//! catalog class beside two test classes in one file declares its one
//! kind once, so each test sees `[:oak]`. The emitted tree writes each
//! test class to its own file, so this runs both of them in one process,
//! as a suite run does; Spinel builds each test file as a program of its
//! own, so there is no Spinel variant. Ignored as a fork spec until the
//! compiler does; the test passes unchanged in the generated blog on
//! Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/shelf_catalog_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        FILE,
        r##"require "test_helper"

class ShelfCatalog
  def self.kinds
    @kinds ||= []
  end

  def self.kind(name)
    kinds << name
  end

  kind :oak
end

class ShelfCatalogTest < ActiveSupport::TestCase
  test "the catalog declared its kind once" do
    assert_equal [:oak], ShelfCatalog.kinds
  end
end

class ShelfCatalogKindTest < ActiveSupport::TestCase
  test "the catalog declared its kind once here too" do
    assert_equal [:oak], ShelfCatalog.kinds
  end
end
"##,
    )
}

#[test]
#[ignore = "fork spec: test framework"]
fn a_test_files_classes_are_defined_once_per_process() {
    app().run_tests(&[FILE, "test/models/shelf_catalog_kind_test.rb"]).assert_passes();
}
