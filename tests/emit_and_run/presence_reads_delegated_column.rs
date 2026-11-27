//! A presence check of a column whose reader a `delegate` replaces reads
//! the delegated method, as ActiveModel's `read_attribute_for_validation`
//! (`send(attr)`) does: the column's nil doesn't fail it, and the
//! delegate's work (here, loading an association) happens during the
//! save. Ignored as a fork spec until the compiler does; the expected
//! values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/tally.rb", r##"class Tally
  def initialize(invoice)
    @invoice = invoice
  end

  def total_paid
    @invoice.payments.sum { |p| p.amount }
  end
end
"##)
        .write("app/models/payment.rb", r##"class Payment < ApplicationRecord
  belongs_to :invoice
end
"##)
        .write("app/models/invoice.rb", r##"class Invoice < ApplicationRecord
  has_many :payments

  validates :total_paid, presence: true

  delegate :total_paid, to: :tally

  def tally
    Tally.new(self)
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    inv = Invoice.new
    check("the column is nil", inv.read_attribute(:total_paid).nil?)
    check("presence reads the delegate", inv.valid?)
    inv.save!
    Payment.create!(invoice: inv, amount: 7)
    inv = Invoice.find(inv.id)
    check("unloaded before the save", !inv.payments.loaded?)
    inv.save!
    check("the save ran the delegate", inv.payments.loaded?)
    check("it answers the payments", inv.total_paid == 7)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "invoices", force: :cascade do |t|
    t.integer "total_paid"
  end
  create_table "payments", force: :cascade do |t|
    t.integer "invoice_id"
    t.integer "amount"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: validations"]
fn presence_reads_a_delegated_column() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: validations"]
fn presence_reads_a_delegated_column_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}
