//! A class a test file's top level builds with `Struct.new` or
//! `Data.define` is defined when the file loads, as Ruby defines it, and
//! a test class nested in a module reads it by its top-level name. The
//! emitted tree names the nested test class's file
//! `test/models/billing_receipt_test.rb`. Ignored as a fork spec until the
//! compiler does; the test passes unchanged in the generated blog on
//! Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/receipt_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        FILE,
        r##"require "test_helper"

FakeReceipt = Struct.new(:number)
FakeStamp = Data.define(:label)

module Billing
  class ReceiptTest < ActiveSupport::TestCase
    test "the file's factory-built classes are defined" do
      assert_equal 7, FakeReceipt.new(7).number
      assert_equal "FakeReceipt", FakeReceipt.name
      assert_equal "paid", FakeStamp.new(label: "paid").label
    end
  end
end
"##,
    )
}

#[test]
#[ignore = "fork spec: test framework"]
fn a_test_files_struct_and_data_classes_are_defined() {
    app().run_test("test/models/billing_receipt_test.rb").assert_passes();
}

#[test]
#[ignore = "fork spec: test framework"]
fn a_test_files_struct_and_data_classes_are_defined_on_spinel() {
    app().run_spin_test("test/billing_receipt_test.rb").assert_passes();
}
