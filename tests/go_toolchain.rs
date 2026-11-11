//! Go toolchain integration test — Phase 1 forcing function.
//!
//! Generates the emitted Go project for a fixture into a scratch dir
//! and runs `go vet ./app` against it. Go's `vet` parses + type-checks
//! the package without linking — closest equivalent to `cargo check` /
//! `crystal build --no-codegen` / `tsc --noEmit`.
//!
//! Scoped to the `./app` package, which as of Phase 4c contains
//! controllers, http runtime, and models all flat in one package.
//!
//! Marked `#[ignore]`; run with:
//!
//!     cargo test --test go_toolchain -- --ignored --nocapture

#[path = "support/scratch.rs"]
mod scratch;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;
#[path = "support/content_tag_options.rs"]
#[allow(dead_code)]
mod content_tag_options;
#[path = "support/link_to_if_eval_order.rs"]
#[allow(dead_code)]
mod link_to_if_eval_order;
#[path = "support/simple_format.rs"]
mod simple_format;
#[path = "support/link_to_computed_url.rs"]
mod link_to_computed_url;
#[path = "support/nested_route_helper_record.rs"]
#[allow(dead_code)]
mod nested_route_helper_record;
#[path = "support/ivar_named_local.rs"]
#[allow(dead_code)]
mod ivar_named_local;
#[path = "support/record_if_condition.rs"]
#[allow(dead_code)]
mod record_if_condition;
#[path = "support/link_to_template_local.rs"]
#[allow(dead_code)]
mod link_to_template_local;
#[path = "support/each_no_param.rs"]
#[allow(dead_code)]
mod each_no_param;
#[path = "support/view_unless_no_else.rs"]
#[allow(dead_code)]
mod view_unless_no_else;
#[path = "support/nilable_string_view.rs"]
#[allow(dead_code)]
mod nilable_string_view;
#[path = "support/go_nil_narrowed.rs"]
#[allow(dead_code)]
mod go_nil_narrowed;
#[path = "support/go_nil_literal_left.rs"]
#[allow(dead_code)]
mod go_nil_literal_left;
#[path = "support/each_receiver_once.rs"]
#[allow(dead_code)]
mod each_receiver_once;
#[path = "support/join_nil.rs"]
#[allow(dead_code)]
mod join_nil;
#[path = "support/go_view_predicate.rs"]
mod go_view_predicate;
#[path = "support/go_test_literal_receiver.rs"]
mod go_test_literal_receiver;
#[path = "support/go_action_controller_api.rs"]
mod go_action_controller_api;
#[path = "support/go_conditional_operand.rs"]
mod go_conditional_operand;
#[path = "support/i18n_locales_app.rs"]
#[allow(dead_code)]
mod i18n_locales_app;
#[path = "support/record_slice_app.rs"]
#[allow(dead_code)]
mod record_slice_app;

use roundhouse::emit::go;
use roundhouse::ingest::ingest_app;

#[test]
fn real_blog_runtime_calls_use_qualified_class_names() {
    let mut app = ingest_app(roundhouse::fixtures::real_blog()).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let files = go::emit(&app);
    for (path, calls) in [
        ("app/v2/flash.go", vec!["NewActionDispatchFlashNow(self)", "NewActionDispatchFlash()"]),
        ("app/v2/router.go", vec!["return NewActionDispatchRouterMatchResult("]),
        ("app/v2/session.go", vec![
            "NewActionDispatchSession(self.ToH())",
            "NewActionDispatchSession()",
            "ActionDispatchSession_cookie_decode(fmt.Sprintf(\"%v\", parts[0]))",
            "ActionDispatchSession_cookie_decode(fmt.Sprintf(\"%v\", parts[1]))",
            "ActionDispatchSession_cookie_encode(k)",
            "ActionDispatchSession_cookie_encode(fmt.Sprintf(\"%v\", self.OpGet(k)))",
        ]),
    ] {
        let file = files.iter().find(|file| file.path == Path::new(path)).expect("runtime file");
        for call in calls {
            assert!(file.content.contains(call), "{path} must call {call}");
        }
    }
}

fn scratch_dir(fixture: &str) -> PathBuf {
    scratch::dir(&format!("roundhouse-go-check-{fixture}"))
}

fn generate_project(fixture_path: &Path, out: &Path) {
    std::fs::create_dir_all(out).expect("create scratch");

    let mut app = ingest_app(fixture_path).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let files = go::emit(&app);

    for file in &files {
        let path = out.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&path, &file.content).expect("write emitted file");
    }
}

fn go_mod_tidy(scratch: &Path) {
    let tidy = Command::new("go")
        .arg("mod")
        .arg("tidy")
        .current_dir(scratch)
        .output()
        .expect("run go mod tidy");
    assert!(
        tidy.status.success(),
        "go mod tidy failed at {}:\n\
         \n=== stdout ===\n{}\n\
         \n=== stderr ===\n{}",
        scratch.display(),
        String::from_utf8_lossy(&tidy.stdout),
        String::from_utf8_lossy(&tidy.stderr),
    );
}

fn assert_go_passes(fixture: &str, scratch: &Path) {
    go_mod_tidy(scratch);
    let output = Command::new("go")
        .arg("vet")
        .arg("./app/v2/")
        .current_dir(scratch)
        .output()
        .expect("run go vet");

    assert!(
        output.status.success(),
        "go vet failed on emitted {fixture} project at {}:\n\
         \n=== stdout ===\n{}\n\
         \n=== stderr ===\n{}",
        scratch.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
#[ignore]
fn real_blog_go_vet_passes() {
    let fixture = roundhouse::fixtures::real_blog();
    let scratch = scratch_dir("real-blog-vet");
    generate_project(fixture, &scratch);
    assert_go_passes("real-blog", &scratch);
}

#[test]
#[ignore]
fn real_blog_go_test_passes() {
    // Phase 2 forcing function: emit real-blog and run `go test ./...`,
    // the same discovery the emitted README documents. Asserts zero
    // failures across the fixture, model, and controller tests.
    // `./app/v2/` alone would stay green if a later package grew tests.
    let fixture = roundhouse::fixtures::real_blog();
    let scratch = scratch_dir("real-blog-test");
    generate_project(fixture, &scratch);
    go_mod_tidy(&scratch);

    let output = Command::new("go")
        .arg("test")
        .arg("./...")
        .current_dir(&scratch)
        .output()
        .expect("run go test");

    assert!(
        output.status.success(),
        "go test failed on emitted real-blog at {}:\n\
         \n=== stdout ===\n{}\n\
         \n=== stderr ===\n{}",
        scratch.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
#[ignore]
fn real_blog_i18n_locales_run() {
    // The answers `emit_and_run::i18n_locales` checks on CRuby, but for a String
    // locale named in an error: Go has no Symbol apart from string, so `"de"` shows as `:de`.
    let (emitted, errors) = i18n_locales_app::probe_app()
        .edit("app/models/article.rb", i18n_locales_app::IN_LOCALE, "")
        .emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("app/v2/i18n_probe_test.go"), r#"package v2

import (
	"reflect"
	"testing"
)

func TestI18nLocalesProbe(t *testing.T) {
	expect := func(what string, got, want interface{}) {
		if !reflect.DeepEqual(got, want) {
			t.Fatalf("%s: %#v", what, got)
		}
	}
	expect("lookups", Article_lookups(), []string{"Hello", "Hi <x>", "1 item", "2 items", "Translation missing: en.probe.nope", "Cześć", "fallback 3", "en"})
	expect("polish", Article_polish(), []string{"1 rzecz", "5 rzeczy", "English only", "Cześć", "Translation missing: pl.probe.nope", "Cześć", "Hi Ola", "pl"})
	expect("restored", I18n_locale(), "en")
	expect("named", []string{Article_named("pl"), Article_named("en"), Article_named(nil)}, []string{"Cześć", "Hello", "Hello"})
	expect("dynamic", []string{Article_probe_key("greet"), Article_only_key("xx")}, []string{"Hello", "Translation missing: en.probe.only_xx"})
	func() {
		defer func() {
			e, ok := recover().(*I18nInvalidLocale)
			if !ok {
				t.Fatalf("named de: %#v", e)
			}
			expect("named de", e.Message, ":de is not a valid locale")
		}()
		Article_named("de")
	}()
}
"#).expect("write probe");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-run", "TestI18nLocalesProbe", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore]
fn real_blog_record_slice_runs() {
    // The answers `emit_and_run::record_slice` checks on CRuby.
    let (emitted, errors) = record_slice_app::probe_app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("app/v2/record_slice_probe_test.go"), r#"package v2

import (
	"reflect"
	"testing"
)

func TestRecordSliceProbe(t *testing.T) {
	if got := Article_sliced(); !reflect.DeepEqual(got, []string{"2", "Sliced", "7", "1", "Other", "3", "Bound", "5", "4", "A comment body."}) {
		t.Fatalf("sliced: %#v", got)
	}
}
"#).expect("write probe");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-run", "TestRecordSliceProbe", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore]
fn constructor_optionals_of_mixed_types_build_and_keep_their_defaults() {
    // The router's constructors have String optionals, then Integer ones;
    // an omitted `guard` is -1, no guard, not Go's zero.
    let (emitted, errors) = emit_and_run::real_blog().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("app/v2/ctor_optionals_test.go"), r#"package v2

import "testing"

func TestCtorOptionals(t *testing.T) {
	r := NewActionDispatchRouterRoute("GET", "/x", "XController", "show")
	if r.Guard != -1 || r.ReqFormat != "" {
		t.Fatalf("omitted: guard %d format %q", r.Guard, r.ReqFormat)
	}
	g := NewActionDispatchRouterRoute("GET", "/x", "XController", "show", "json", "", "", 3)
	if g.Guard != 3 || g.ReqFormat != "json" {
		t.Fatalf("passed: guard %d format %q", g.Guard, g.ReqFormat)
	}
	m := NewActionDispatchRouterMatchResult("C", "a", map[string]string{})
	if m.Index != 0 || m.Guard != -1 {
		t.Fatalf("result omitted: index %d guard %d", m.Index, m.Guard)
	}
	f := NewActionDispatchRouterMatchResult("C", "a", map[string]string{}, "json", int64(2), int64(5))
	if f.Index != 2 || f.Guard != 5 || f.ReqFormat != "json" {
		t.Fatalf("result passed: index %d guard %d format %q", f.Index, f.Guard, f.ReqFormat)
	}
	hit := ActionDispatchRouter_match("GET", "/x", []*ActionDispatchRouterRoute{r})
	if hit == nil || hit.Guard != -1 {
		t.Fatalf("a matched route with no guard: %#v", hit)
	}
	n := NewActionDispatchRouterMatchResult("C", "a", map[string]string{}, nil, 2, 5)
	if n.ReqFormat != "" || n.Index != 2 || n.Guard != 5 {
		t.Fatalf("an explicit nil and bare integers: format %q index %d guard %d", n.ReqFormat, n.Index, n.Guard)
	}
}
"#).expect("write probe");
    go_mod_tidy(&emitted);
    for args in [vec!["build", "./..."], vec!["test", "-count=1", "-run", "TestCtorOptionals", "./app/v2/"]] {
        let output = Command::new("go").args(&args).current_dir(&*emitted).output().expect("run go");
        assert!(
            output.status.success(),
            "go {args:?}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore]
fn a_float_optional_of_mixed_types_takes_a_bare_integer() {
    // A bare integer literal boxes as Go's `int`.
    let (emitted, errors) = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def scored(label = \"a\", ratio = 1.5, count = 7)\n    ratio\n  end\n\n  def scored_from_ruby\n    scored(\"x\", 2, 3)\n  end\n\n  def counted(label = \"a\", count = 7)\n    count\n  end\n\n  def counted_from_ruby\n    counted(\"x\", 3)\n  end\n\n  def counted_nil_label\n    counted(nil, 4)\n  end\n\n",
        )
        .write("sig/article_scored.rbs", "class Article\n  def scored: (?String label, ?Float ratio, ?Integer count) -> Float\n  def scored_from_ruby: () -> Float\n  def counted: (?String? label, ?Integer count) -> Integer\n  def counted_from_ruby: () -> Integer\n  def counted_nil_label: () -> Integer\nend\n")
        .emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("app/v2/float_optional_test.go"), r#"package v2

import "testing"

func TestFloatOptional(t *testing.T) {
	a := &Article{}
	if got := a.Scored(); got != 1.5 {
		t.Fatalf("omitted: %v", got)
	}
	if got := a.Scored("x", 2.5); got != 2.5 {
		t.Fatalf("a float: %v", got)
	}
	if got := a.Scored("x", 2); got != 2.0 {
		t.Fatalf("a bare integer: %v", got)
	}
	if got := a.Scored("x", int64(3)); got != 3.0 {
		t.Fatalf("an int64: %v", got)
	}
	if got := a.ScoredFromRuby(); got != 2.0 {
		t.Fatalf("a float from a Ruby call: %v", got)
	}
	if got := a.CountedFromRuby(); got != 3 {
		t.Fatalf("an integer from a Ruby call: %v", got)
	}
	if got := a.CountedNilLabel(); got != 4 {
		t.Fatalf("a nil from a Ruby call: %v", got)
	}
}
"#).expect("write probe");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-run", "TestFloatOptional", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore]
fn nilable_numeric_optionals_narrow_after_a_nil_guard() {
    // `return … if x.nil?` narrows with a plain assertion, so the boxed value must be the type it asserts.
    let (emitted, errors) = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def nil_ratio(label = \"a\", ratio = 1)\n    return 0.5 if ratio.nil?\n    ratio + 1.5\n  end\n\n  def nil_count(label = \"a\", count = 2)\n    return 0 if count.nil?\n    count + 1\n  end\n\n  def ratio_from_ruby\n    nil_ratio(\"x\", 2)\n  end\n\n  def count_from_ruby\n    nil_count(\"x\", 3)\n  end\n\n",
        )
        .write("sig/article_nilable.rbs", "class Article\n  def nil_ratio: (?String label, ?Float? ratio) -> Float\n  def nil_count: (?String label, ?Integer? count) -> Integer\n  def ratio_from_ruby: () -> Float\n  def count_from_ruby: () -> Integer\nend\n")
        .emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("app/v2/nilable_optional_test.go"), r#"package v2

import "testing"

func checkFloat(t *testing.T, name string, want float64, f func() float64) {
	defer func() {
		if r := recover(); r != nil {
			t.Errorf("%s: panic: %v", name, r)
		}
	}()
	if got := f(); got != want {
		t.Errorf("%s: %v (want %v)", name, got, want)
	}
}

func checkInt(t *testing.T, name string, want int64, f func() int64) {
	defer func() {
		if r := recover(); r != nil {
			t.Errorf("%s: panic: %v", name, r)
		}
	}()
	if got := f(); got != want {
		t.Errorf("%s: %v (want %v)", name, got, want)
	}
}

func TestNilableOptionals(t *testing.T) {
	a := &Article{}
	checkFloat(t, "float omitted", 2.5, func() float64 { return a.NilRatio() })
	checkFloat(t, "float nil", 0.5, func() float64 { return a.NilRatio("x", nil) })
	checkFloat(t, "float bare int", 3.5, func() float64 { return a.NilRatio("x", 2) })
	checkFloat(t, "float int64", 3.5, func() float64 { return a.NilRatio("x", int64(2)) })
	checkFloat(t, "float float", 4.0, func() float64 { return a.NilRatio("x", 2.5) })
	checkFloat(t, "float from ruby", 3.5, func() float64 { return a.RatioFromRuby() })
	checkInt(t, "int omitted", 3, func() int64 { return a.NilCount() })
	checkInt(t, "int nil", 0, func() int64 { return a.NilCount("x", nil) })
	checkInt(t, "int bare int", 4, func() int64 { return a.NilCount("x", 3) })
	checkInt(t, "int int64", 4, func() int64 { return a.NilCount("x", int64(3)) })
	checkInt(t, "int from ruby", 4, func() int64 { return a.CountFromRuby() })
}
"#).expect("write probe");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-run", "TestNilableOptionals", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore]
fn real_blog_flash_each_yields() {
    // A yielding runtime method takes the block as a Go function.
    let (emitted, errors) = emit_and_run::real_blog().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("app/v2/flash_each_test.go"), r#"package v2

import (
	"reflect"
	"testing"
)

func TestFlashEachYieldsPopulatedFields(t *testing.T) {
	flash := NewActionDispatchFlash(map[string]string{"alert": "Oops"})
	var got []string
	flash.Each(func(k string, v string) { got = append(got, k+"="+v) })
	if !reflect.DeepEqual(got, []string{"alert=Oops"}) {
		t.Fatalf("%#v", got)
	}
}
"#).expect("write test");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-run", "TestFlashEachYieldsPopulatedFields", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// #1714: a model test calling `Article.create!` and `article.persisted?`.
fn bang_test_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        "test/models/bang_test.rb",
        "require \"test_helper\"\n\nclass BangTest < ActiveSupport::TestCase\n  test \"create! saves a record\" do\n    article = Article.create!(title: \"Bang\", body: \"A body long enough to pass.\")\n    assert article.persisted?\n    assert_not article.new_record?\n    draft = Article.create(title: \"\", body: \"\")\n    assert_not draft.persisted?\n  end\nend\n",
    )
}

/// The Go test calls what the Go emit defines: a class method by its
/// model function (`Article_create_bang`, `Article_create`), a
/// predicate with `Pred` (`NewRecordPred()`), and a predicate the
/// runtime's `ActiveRecord::Base` reads straight off an ivar
/// (`persisted?`) by the field the library emit reads (`Persisted`).
#[test]
fn a_model_test_calls_bang_and_predicate_methods_by_their_go_names() {
    let (emitted, errors) = bang_test_app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    let test = std::fs::read_to_string(emitted.join("app/v2/bang_test.go")).expect("bang_test.go");
    for want in ["Article_create_bang(map", "Article_create(map", ".Persisted)", ".NewRecordPred()"] {
        assert!(test.contains(want), "missing {want}:\n{test}");
    }
    assert!(!test.contains("!(map") && !test.contains("?()"), "{test}");
}

#[test]
#[ignore]
fn a_model_test_with_bang_and_predicate_calls_runs() {
    let (emitted, errors) = bang_test_app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-v", "-run", "TestCreateSavesARecord", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("--- PASS: TestCreateSavesARecord"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}

/// A model that defines `persisted?` itself: the library emit writes it as
/// `PersistedPred()`, and the test calls that, not the field.
fn persisted_override_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :body, presence: true, length: { minimum: 10 }\n",
            "  validates :body, presence: true, length: { minimum: 10 }\n\n  def persisted?\n    false\n  end\n",
        )
        .write(
            "test/models/persisted_override_test.rb",
            "require \"test_helper\"\n\nclass PersistedOverrideTest < ActiveSupport::TestCase\n  test \"the override answers\" do\n    article = Article.create!(title: \"Bang\", body: \"A body long enough to pass.\")\n    assert_not article.persisted?\n  end\nend\n",
        )
}

#[test]
fn a_model_test_calls_a_models_own_persisted_override() {
    let (emitted, errors) = persisted_override_app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    let test = std::fs::read_to_string(emitted.join("app/v2/persisted_override_test.go")).expect("persisted_override_test.go");
    assert!(test.contains("article.PersistedPred()") && !test.contains("article.Persisted)"), "{test}");
}

#[test]
#[ignore]
fn a_model_test_with_a_persisted_override_runs() {
    let (emitted, errors) = persisted_override_app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-v", "-run", "TestTheOverrideAnswers", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("--- PASS: TestTheOverrideAnswers"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}

/// A non-block `content_tag` with options renders as Rails 8.1.4 does
/// (#1780): the app's integration test, run by `go test`. Go's
/// test emit has no `assert_includes`; the probe defines it.
#[test]
#[ignore]
fn content_tag_with_options_renders() {
    assert_content_tag_test_passes(content_tag_options::app(content_tag_options::VIEW));
}

/// #1771's `content_tag … do … end` with options alone: the view's options
/// Hash literal reaches `content_tag_html` as the `map[string]interface{}`
/// it takes, where it was a `map[string]string` that didn't compile.
#[test]
#[ignore]
fn content_tag_block_with_options_renders() {
    assert_content_tag_test_passes(content_tag_options::app_rendering(
        "<%= content_tag :div, class: \"x\" do %>b<% end %>\n",
        &["<div class=\"x\">b</div>"],
    ));
}

fn assert_content_tag_test_passes(app: emit_and_run::Overlay) {
    let (emitted, errors) = app.emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(
        emitted.join("app/v2/assert_includes_test.go"),
        "package v2\n\nimport \"strings\"\n\nfunc assert_includes(body string, want string) {\n\tif !strings.Contains(body, want) {\n\t\tpanic(\"missing \" + want + \" in \" + body)\n\t}\n}\n",
    )
    .expect("write probe");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-v", "-run", "TestContentTagRenders", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("--- PASS: TestContentTagRenders"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}

/// #1802: a view's call to a model's own predicate builds and renders.
#[test]
#[ignore]
fn a_views_model_predicate_call_runs() {
    let (emitted, errors) = go_view_predicate::app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("app/v2/assert_includes_test.go"), go_view_predicate::GO_ASSERT_INCLUDES).expect("write probe");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-v", "-run", "TestTheIndexRenders", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("--- PASS: TestTheIndexRenders"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}

/// A view's 0-arg reads of a model's own methods build and render: a
/// trivial `attr_accessor` reader, an `Article | nil` receiver, a
/// namespaced model, and a method whose omitted optional params take
/// their literal Ruby defaults.
#[test]
#[ignore]
fn a_views_model_method_shapes_run() {
    let (emitted, errors) = go_view_predicate::shapes_app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("app/v2/assert_includes_test.go"), go_view_predicate::GO_ASSERT_INCLUDES).expect("write probe");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-v", "-run", "TestTheIndexRenders", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("--- PASS: TestTheIndexRenders"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}

/// #1803: an assertion on a method of a new record parses and runs.
#[test]
#[ignore]
fn an_assertion_on_a_new_records_method_runs() {
    let (emitted, errors) = go_test_literal_receiver::app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-v", "-run", "TestANewArticleIsRecentAndNotFeatured", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("--- PASS: TestANewArticleIsRecentAndNotFeatured"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}

/// #1804: a controller under `ActionController::API` builds and answers.
#[test]
#[ignore]
fn an_api_controller_runs() {
    let (emitted, errors) = go_action_controller_api::app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-v", "-run", "TestAnAPIControllerAnswers", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("--- PASS: TestAnAPIControllerAnswers"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}

/// #1804: tiny-api (`ApplicationController < ActionController::API`) on
/// Go. Nothing else builds it for Go. Its controllers embed the runtime's
/// controller base now; the package still has other gaps (a uuid key typed
/// as `int64`, `render json:` in a helper), so this pins only that
/// `ActionControllerAPI` is no longer named.
#[test]
#[ignore]
fn tiny_api_names_no_action_controller_api_type() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tiny-api");
    let scratch = scratch_dir("tiny-api");
    generate_project(&fixture, &scratch);
    go_mod_tidy(&scratch);
    let vet = Command::new("go").args(["vet", "./..."]).current_dir(&scratch).output().expect("run go vet");
    let stderr = String::from_utf8_lossy(&vet.stderr);
    assert!(!stderr.contains("ActionControllerAPI"), "{stderr}");
    let controller = std::fs::read_to_string(scratch.join("app/v2/application_controller.go")).unwrap();
    assert!(controller.contains("*ActionControllerBase"), "{controller}");
}

/// #1696: the issue's reduction parses: its conditional operand of `&&`
/// is a func literal, not an `if` in expression position.
#[test]
#[ignore]
fn a_conditional_operand_of_and_parses() {
    let (emitted, errors) = go_conditional_operand::issue_app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    let output = Command::new("gofmt").args(["-e", "-l", "app/v2/article.go"]).current_dir(&*emitted).output().expect("run gofmt");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

/// A conditional operand of `&&` runs, answering as Ruby does.
#[test]
#[ignore]
fn a_conditional_operand_of_and_runs() {
    let (emitted, errors) = go_conditional_operand::note_app()
        .remove("test/models/note_headline_test.rb")
        .emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("app/v2/note_headline_probe_test.go"), go_conditional_operand::GO_PROBE).expect("write probe");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-v", "-run", "TestNoteHeadlineProbe", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("--- PASS: TestNoteHeadlineProbe"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}

/// A conditional of non-nil scalars on the left of `||` is its own value,
/// as in Ruby, where 0 and "" are truthy.
#[test]
#[ignore]
fn a_conditional_scalar_left_of_or_is_kept() {
    let (emitted, errors) = go_conditional_operand::or_app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("app/v2/note_or_probe_test.go"), go_conditional_operand::GO_OR_PROBE).expect("write probe");
    go_mod_tidy(&emitted);
    let output = Command::new("go")
        .args(["test", "-count=1", "-v", "-run", "TestNoteOrProbe", "./app/v2/"])
        .current_dir(&*emitted)
        .output()
        .expect("run go test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("--- PASS: TestNoteOrProbe"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}

/// `link_to_unless`'s block form with a name that is an expression, not a
/// call (`"a" + "b"`), in a block without a parameter and in one whose
/// parameter the block doesn't read: the no-link branch still evaluates
/// the name, and the generated view builds.
#[test]
#[ignore]
fn a_link_to_unless_block_over_an_expression_name_builds() {
    let (emitted, errors) = emit_and_run::real_blog()
        .edit(
            "app/views/articles/index.html.erb",
            "<% content_for :title, \"Articles\" %>\n",
            "<% content_for :title, \"Articles\" %>\n<% t = @articles.nil? %>\n<%= link_to_unless !t, \"a\" + \"b\", \"/x\" do %>none<% end %>\n<%= link_to_unless !t, \"c\" + \"d\", \"/x\" do |name| %>none<% end %>\n",
        )
        .emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    assert_go_passes("real-blog with link_to_unless blocks", &emitted);
}

/// #1862: `link_to_if` / `link_to_unless` evaluate the url when no link
/// is made, and the generated view still builds and renders as Rails
/// (`link_to_if_eval_order::STRICT_VIEW`): the app's integration test, run by `go test`.
#[test]
#[ignore]
fn link_to_if_evaluates_its_url_and_renders() {
    assert_content_tag_test_passes(content_tag_options::app_rendering(link_to_if_eval_order::STRICT_VIEW, link_to_if_eval_order::STRICT_RENDERS));
}

/// #1861: `simple_format` with `sanitize: false` renders as Rails 8.1.4
/// does, multibyte texts included (`simple_format::VIEW`): the app's integration test, run by `go test`.
#[test]
#[ignore]
fn simple_format_renders() {
    assert_content_tag_test_passes(content_tag_options::app_rendering(simple_format::VIEW, simple_format::RENDERS));
}

/// #1865: `link_to` with a url the template computes renders as Rails
/// 8.1.4 does (`link_to_computed_url::VIEW`).
#[test]
#[ignore]
fn link_to_computed_url_renders() {
    assert_content_tag_test_passes(content_tag_options::app_rendering(link_to_computed_url::VIEW, link_to_computed_url::RENDERS));
}

/// #1890: a record handed to a route helper inside an output
/// expression renders as Rails 8.1.4 does
/// (`nested_route_helper_record::VIEW`).
#[test]
#[ignore]
fn nested_route_helper_record_renders() {
    assert_content_tag_test_passes(nested_route_helper_record::strict_app().0);
}

/// A template local assigned from the ivar of its own name renders as
/// Rails 8.1.4 does (`ivar_named_local::VIEW`).
#[test]
#[ignore]
fn ivar_named_local_renders() {
    assert_content_tag_test_passes(ivar_named_local::strict_app());
}

/// #1909: a record as an `if` condition renders as Rails 8.1.4 does
/// (`record_if_condition::VIEW`).
#[test]
#[ignore]
fn record_if_condition_renders() {
    assert_content_tag_test_passes(record_if_condition::strict_app());
}

/// #1909: an assignment as the `if` condition, the record read in the
/// branch, renders as Rails 8.1.4 does (`record_if_condition::ASSIGN_VIEW`).
#[test]
#[ignore]
fn record_if_assignment_renders() {
    assert_content_tag_test_passes(record_if_condition::strict_assign_app());
}

/// #1898: a record in a local the template assigns, handed to
/// `link_to`'s url helper, renders as Rails 8.1.4 does
/// (`link_to_template_local::VIEW`).
#[test]
#[ignore]
fn link_to_template_local_renders() {
    assert_content_tag_test_passes(link_to_template_local::strict_app());
}

/// #1910: a block with no parameter renders as Rails 8.1.4 does
/// (`each_no_param::VIEW`).
#[test]
#[ignore]
fn each_no_param_renders() {
    assert_content_tag_test_passes(each_no_param::strict_app());
}

/// An empty `map` block maps each element to nil, as Rails 8.1.4
/// does (`each_no_param::EMPTY_MAP_VIEW`).
#[test]
#[ignore]
fn each_no_param_empty_map_renders() {
    assert_content_tag_test_passes(each_no_param::strict_empty_map_app());
}

/// #1910: a block whose only parameter is a rest (`|*args|`) is refused,
/// not emitted as a loop that binds nothing (`each_no_param::REST_VIEW`).
#[test]
fn each_no_param_rest_blocks_are_refused() {
    let (_emitted, errors) = each_no_param::strict_rest_app().emit(roundhouse::project::BuildTarget::Go);
    for method in ["each", "map"] {
        assert!(
            errors.iter().any(|e| e.contains(&format!("{method} not supported")) && e.contains("a block with only a rest")),
            "{method}: {errors:?}"
        );
    }
}

/// #1921: an `unless` with no `else` in a view renders as Rails 8.1.4
/// does (`view_unless_no_else::VIEW`).
#[test]
#[ignore]
fn view_unless_no_else_renders() {
    assert_content_tag_test_passes(view_unless_no_else::strict_app());
}

/// #1937: a view reading a nullable string column with `==`, `!=`,
/// `empty?` and `length` renders as Rails 8.1.4 does
/// (`nilable_string_view::VIEW`).
#[test]
#[ignore]
fn nilable_string_view_renders() {
    assert_content_tag_test_passes(nilable_string_view::strict_app());
}

/// #1937: `==` and `!=` on a nullable string column evaluate each
/// operand once, left then right, as Rails 8.1.4 does
/// (`nilable_string_view::EFFECT_VIEW`, title present).
#[test]
#[ignore]
fn nilable_string_view_effect_renders() {
    assert_content_tag_test_passes(nilable_string_view::strict_effect_app());
}

/// #1937: with the column nil, `==` and `!=` render as Rails 8.1.4
/// does (`nilable_string_view::NIL_VIEW`).
#[test]
#[ignore]
fn nilable_string_view_nil_renders() {
    assert_content_tag_test_passes(nilable_string_view::strict_nil_app());
}

/// #1937: the same operands with a nil title: the right operand still
/// runs, as in Rails 8.1.4 (`nilable_string_view::EFFECT_VIEW`).
#[test]
#[ignore]
fn nilable_string_view_nil_effect_renders() {
    assert_content_tag_test_passes(nilable_string_view::strict_nil_effect_app());
}

/// #1946: `each` runs a receiver with an effect once, as Rails 8.1.4
/// does (`each_receiver_once::VIEW`).
#[test]
#[ignore]
fn each_receiver_once_renders() {
    assert_content_tag_test_passes(each_receiver_once::strict_app());
}

/// #1949: `join` reads a nil element as an empty string, as Rails 8.1.4
/// does (`join_nil::GO_VIEW`).
#[test]
#[ignore]
fn join_nil_renders() {
    assert_content_tag_test_passes(join_nil::strict_go_app());
}

/// #1949: a nil column beside an Integer joins as an empty string, as
/// Rails 8.1.4 does (`join_nil::MIXED_VIEW`).
#[test]
#[ignore]
fn join_nil_mixed_nil_renders() {
    assert_content_tag_test_passes(join_nil::strict_mixed_nil_app());
}

/// #1949: a present column beside an Integer joins as its text, as
/// Rails 8.1.4 does (`join_nil::MIXED_VIEW`).
#[test]
#[ignore]
fn join_nil_mixed_renders() {
    assert_content_tag_test_passes(join_nil::strict_mixed_app());
}

/// #1977: a String on the left of a nil column renders as Rails 8.1.4
/// does (`go_nil_literal_left::VIEW`).
#[test]
#[ignore]
fn go_nil_literal_left_nil_renders() {
    assert_content_tag_test_passes(go_nil_literal_left::strict_nil_app());
}

/// #1977: a String on the left of a present column renders as Rails
/// 8.1.4 does (`go_nil_literal_left::VIEW`).
#[test]
#[ignore]
fn go_nil_literal_left_renders() {
    assert_content_tag_test_passes(go_nil_literal_left::strict_app());
}

/// A nullable column compared after a nil check, on either side, renders as
/// Rails 8.1.4 does (`go_nil_narrowed::VIEW`).
#[test]
#[ignore]
fn go_nil_narrowed_renders() {
    assert_content_tag_test_passes(go_nil_narrowed::strict_app());
}

/// The nil check and the comparison in a controller and a model method,
/// where a local's reader narrows, render as Rails 8.1.4 does.
#[test]
#[ignore]
fn go_nil_narrowed_code_renders() {
    assert_content_tag_test_passes(go_nil_narrowed::strict_code_app());
}

/// Two nullable columns compare as Rails 8.1.4 does: nil equals only nil.
#[test]
#[ignore]
fn go_nil_column_pairs_render() {
    for app in go_nil_narrowed::strict_pair_apps() {
        assert_content_tag_test_passes(app);
    }
}

/// `join` runs a receiver with an effect once, as Rails 8.1.4 does
/// (`each_receiver_once::JOIN_VIEW`).
#[test]
#[ignore]
fn join_receiver_once_renders() {
    assert_content_tag_test_passes(each_receiver_once::strict_join_app());
}
