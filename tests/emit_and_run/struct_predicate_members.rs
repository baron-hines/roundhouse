//! maybe's `Account::OpeningBalanceManager::Result = Struct.new(:success?,
//! :changes_made?, :error, keyword_init: true)`: a predicate member is
//! Struct's reader of that name, and the keyword constructor takes it as
//! `success?:`, refusing an unknown keyword as Struct does.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        "app/models/balance_result.rb",
        r#"class BalanceResult
  Result = Struct.new(:success?, :changes_made?, :error, keyword_init: true)
  Pair = Struct.new(:valid?, :count)

  def self.ok = Result.new(success?: true, changes_made?: false, error: nil)
  def self.failed(message) = Result.new(success?: false, changes_made?: false, error: message)
  def self.pair = Pair.new(true, 3)

  def self.describe
    r = ok
    f = failed("too late")
    p = pair
    keyed = Result.new(**{ "success?" => true, "error" => "s" })
    later_string = Result.new(**{ success?: false, "success?" => true })
    later_symbol = Result.new(**{ "success?" => true, success?: false })
    odd = begin
      Result.new(**{ 100 => true })
      "accepted"
    rescue ArgumentError
      "argument error"
    end
    unknown = begin
      Result.new(success?: true, typo: 1)
      "accepted"
    rescue ArgumentError => e
      e.message
    end
    [r.success?, r.changes_made?, r.error, f.success?, f.error, p.valid?, p.count, unknown, keyed.success?, keyed.error, later_string.success?, later_symbol.success?, odd]
  end
end
"#,
    )
}

const ASSERTIONS: &str = r#"got = BalanceResult.describe
want = [true, false, nil, false, "too late", true, 3, "unknown keywords: typo", true, "s", true, false, "argument error"]
raise got.inspect unless got == want
puts "struct predicate members passed"
"#;

#[test]
fn a_structs_predicate_members_read_and_construct() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("struct predicate members passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_structs_predicate_members_read_and_construct_on_spinel() {
    let run = app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("struct predicate members passed"), "{}", run.stdout);
}

/// A member named as another's held slot (`:__q_success` beside `:success?`) is not given the shared slot: the
/// struct stays Ruby's own, and each member keeps its value.
#[test]
fn a_member_named_as_anothers_slot_keeps_its_own_value() {
    let run = emit_and_run::real_blog()
        .write(
            "app/models/clash_result.rb",
            "class ClashResult\n  Result = Struct.new(:success?, :__q_success, keyword_init: true)\n\n  def self.both\n    r = Result.new(success?: true, __q_success: false)\n    [r.success?, r.__q_success]\n  end\nend\n",
        )
        .run_ruby("got = ClashResult.both\nraise got.inspect unless got == [true, false]\nputs :ok\n");
    run.assert_passes();
}
