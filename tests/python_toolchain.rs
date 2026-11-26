//! Python toolchain integration test — Phase 1 forcing function.
//!
//! Generates the emitted Python project and runs `python -m compileall`
//! over the models package to verify syntax. Python's dynamic
//! late-binding means undefined names don't fail compile (they fail at
//! run time), so compileall is the honest pre-test check available
//! without third-party tools.
//!
//! Marked `#[ignore]`. Explicit:
//!
//!     cargo test --test python_toolchain -- --ignored --nocapture

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
#[path = "support/python_map_statements.rs"]
#[allow(dead_code)]
mod python_map_statements;
#[path = "support/nilable_string_view.rs"]
#[allow(dead_code)]
mod nilable_string_view;
#[path = "support/each_receiver_once.rs"]
#[allow(dead_code)]
mod each_receiver_once;
#[path = "support/join_nil.rs"]
#[allow(dead_code)]
mod join_nil;
#[path = "support/i18n_locales_app.rs"]
#[allow(dead_code)]
mod i18n_locales_app;
#[path = "support/record_slice_app.rs"]
#[allow(dead_code)]
mod record_slice_app;

use roundhouse::emit::python;
use roundhouse::ingest::ingest_app;

fn scratch_dir(fixture: &str) -> PathBuf {
    scratch::dir(&format!("roundhouse-python-check-{fixture}"))
}

fn generate_project(fixture_path: &Path, out: &Path) {
    std::fs::create_dir_all(out).expect("create scratch");

    let mut app = ingest_app(fixture_path).expect("ingest");
    // Emit from the post-analyze lowered IR, as `roundhouse --target python` does
    roundhouse::session::analyze_and_lower(&mut app);
    let files = python::emit(&app);

    for file in &files {
        let path = out.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&path, &file.content).expect("write emitted file");
    }
}

fn assert_python_compiles(fixture: &str, scratch: &Path) {
    // Compile-check the whole `app/` package. Phase 4c brought
    // controllers + the http stub under compileall; earlier phases
    // scoped this to `app/models.py`.
    let output = Command::new("python3")
        .arg("-m")
        .arg("compileall")
        .arg("-q")
        .arg("app/")
        .current_dir(scratch)
        .output()
        .expect("run python compileall");

    assert!(
        output.status.success(),
        "python compileall failed on emitted {fixture} at {}:\n\
         \n=== stdout ===\n{}\n\
         \n=== stderr ===\n{}",
        scratch.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
#[ignore]
fn tiny_blog_python_compile_passes() {
    let fixture = Path::new("fixtures/tiny-blog");
    let scratch = scratch_dir("tiny-blog");
    generate_project(fixture, &scratch);
    assert_python_compiles("tiny-blog", &scratch);
}

#[test]
#[ignore]
fn real_blog_python_compile_passes() {
    let fixture = roundhouse::fixtures::real_blog();
    let scratch = scratch_dir("real-blog");
    generate_project(fixture, &scratch);
    assert_python_compiles("real-blog", &scratch);
}

#[test]
#[ignore]
fn real_blog_python_unittest_passes() {
    // Phase 2 forcing function: emit real-blog, run `python -m
    // unittest discover tests`, assert zero failures. Phase-3 tests
    // are @unittest.skip'd.
    let fixture = roundhouse::fixtures::real_blog();
    let scratch = scratch_dir("real-blog");
    generate_project(fixture, &scratch);

    let output = Command::new("python3")
        .arg("-m")
        .arg("unittest")
        .arg("discover")
        .arg("-s")
        .arg("tests")
        .arg("-t")
        .arg(".")
        .current_dir(&scratch)
        .output()
        .expect("run python unittest");

    assert!(
        output.status.success(),
        "python unittest failed on emitted real-blog at {}:\n\
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
    // locale named in an error: Python has no Symbol apart from str, so `"de"` shows as `:de`.
    let (emitted, errors) = i18n_locales_app::probe_app()
        .edit("app/models/article.rb", i18n_locales_app::IN_LOCALE, "")
        .emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("probe.py"), r#"from app.v2.models import Article
from app.i18n_locale import I18n, InvalidLocale

def expect(what, got, want):
    if got != want:
        raise AssertionError(f"{what}: {got!r}")

expect("lookups", Article.lookups(), ["Hello", "Hi <x>", "1 item", "2 items", "Translation missing: en.probe.nope", "Cześć", "fallback 3", "en"])
expect("polish", Article.polish(), ["1 rzecz", "5 rzeczy", "English only", "Cześć", "Translation missing: pl.probe.nope", "Cześć", "Hi Ola", "pl"])
expect("restored", I18n.locale(), "en")
expect("named", [Article.named("pl"), Article.named("en"), Article.named(None)], ["Cześć", "Hello", "Hello"])
expect("dynamic", [Article.probe_key("greet"), Article.only_key("xx")], ["Hello", "Translation missing: en.probe.only_xx"])
try:
    Article.named("de")
    raise AssertionError("no raise")
except InvalidLocale as e:
    expect("named de", str(e), ":de is not a valid locale")
print("locales passed")
"#).expect("write probe");
    let output = Command::new("python3").arg("probe.py").current_dir(&emitted).output().expect("run python3");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("locales passed"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
#[ignore]
fn real_blog_record_slice_runs() {
    // The answers `emit_and_run::record_slice` checks on CRuby.
    let (emitted, errors) = record_slice_app::probe_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::write(emitted.join("probe.py"), r#"from app.v2.models import Article

got = Article.sliced()
if got != ["2", "Sliced", "7", "1", "Other", "3", "Bound", "5", "4", "A comment body."]:
    raise AssertionError(f"sliced: {got!r}")
print("slice passed")
"#).expect("write probe");
    let output = Command::new("python3").arg("probe.py").current_dir(&emitted).output().expect("run python3");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("slice passed"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}

/// A non-block `content_tag` with options renders as Rails 8.1.4 does
/// (#1780): the app's integration test, run by `unittest`. Python's
/// test emit spells `include?` as a missing `include_p`; the probe uses `in`.
#[test]
#[ignore]
fn content_tag_with_options_renders() {
    let (emitted, errors) = content_tag_options::app(content_tag_options::VIEW).emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1862: `link_to_if` / `link_to_unless` evaluate the url when no link
/// is made, and the generated view still builds and renders as Rails
/// (`link_to_if_eval_order::STRICT_VIEW`): the app's integration test, run by `unittest`.
#[test]
#[ignore]
fn link_to_if_evaluates_its_url_and_renders() {
    let (emitted, errors) = content_tag_options::app_rendering(link_to_if_eval_order::STRICT_VIEW, link_to_if_eval_order::STRICT_RENDERS).emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1861: `simple_format` with `sanitize: false` renders as Rails 8.1.4
/// does, multibyte texts included (`simple_format::VIEW`): the app's integration test, run by `unittest`.
#[test]
#[ignore]
fn simple_format_renders() {
    let (emitted, errors) = content_tag_options::app_rendering(simple_format::VIEW, simple_format::RENDERS).emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1865: `link_to` with a url the template computes renders as Rails
/// 8.1.4 does (`link_to_computed_url::VIEW`).
#[test]
#[ignore]
fn link_to_computed_url_renders() {
    let (emitted, errors) = content_tag_options::app_rendering(link_to_computed_url::VIEW, link_to_computed_url::RENDERS).emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1890: a record handed to a route helper inside an output
/// expression renders as Rails 8.1.4 does
/// (`nested_route_helper_record::VIEW`).
#[test]
#[ignore]
fn nested_route_helper_record_renders() {
    let (emitted, errors) = nested_route_helper_record::strict_app().0.emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// A template local assigned from the ivar of its own name renders as
/// Rails 8.1.4 does (`ivar_named_local::VIEW`).
#[test]
#[ignore]
fn ivar_named_local_renders() {
    let (emitted, errors) = ivar_named_local::strict_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1909: a record as an `if` condition renders as Rails 8.1.4 does
/// (`record_if_condition::VIEW`).
#[test]
#[ignore]
fn record_if_condition_renders() {
    let (emitted, errors) = record_if_condition::strict_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1898: a record in a local the template assigns, handed to
/// `link_to`'s url helper, renders as Rails 8.1.4 does
/// (`link_to_template_local::VIEW`).
#[test]
#[ignore]
fn link_to_template_local_renders() {
    let (emitted, errors) = link_to_template_local::strict_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1910: a block with no parameter renders as Rails 8.1.4 does
/// (`each_no_param::VIEW`).
#[test]
#[ignore]
fn each_no_param_renders() {
    let (emitted, errors) = each_no_param::strict_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// An empty `map` block maps each element to nil, as Rails 8.1.4
/// does (`each_no_param::EMPTY_MAP_VIEW`).
#[test]
#[ignore]
fn each_no_param_empty_map_renders() {
    let (emitted, errors) = each_no_param::strict_empty_map_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1910: a `map` block with no parameter whose body is more than one
/// statement is refused, as before, rather than emitted into a
/// comprehension, which takes only an expression
/// (`each_no_param::MAP_STATEMENTS_VIEW`).
#[test]
fn each_no_param_map_statements_are_refused() {
    let (_emitted, errors) = each_no_param::strict_map_statements_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.iter().any(|e| e.contains("map-block not supported (python)")), "{errors:?}");
}

/// #1939: a `map` block whose body is several statements is refused,
/// not written into a comprehension that isn't Python
/// (`python_map_statements::VIEW`).
#[test]
fn python_map_statements_are_refused() {
    let (_emitted, errors) = python_map_statements::strict_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.iter().any(|e| e.contains("map-block not supported (python)")), "{errors:?}");
}

/// #1910: a block whose only parameter is a rest (`|*args|`) is refused,
/// as before, not emitted as a loop or comprehension over `_`
/// (`each_no_param::REST_VIEW`).
#[test]
fn each_no_param_rest_blocks_are_refused() {
    let (_emitted, errors) = each_no_param::strict_rest_app().emit(roundhouse::project::BuildTarget::Python);
    for kind in ["each-block", "map-block"] {
        assert!(errors.iter().any(|e| e.contains(&format!("{kind} not supported (python)"))), "{kind}: {errors:?}");
    }
}

/// #1921: an `unless` with no `else` in a view renders as Rails 8.1.4
/// does (`view_unless_no_else::VIEW`).
#[test]
#[ignore]
fn view_unless_no_else_renders() {
    let (emitted, errors) = view_unless_no_else::strict_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1937: a view reading a nullable string column with `==`, `!=`,
/// `empty?` and `length` renders as Rails 8.1.4 does
/// (`nilable_string_view::VIEW`).
#[test]
#[ignore]
fn nilable_string_view_renders() {
    let (emitted, errors) = nilable_string_view::strict_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1937: `==` and `!=` on a nullable string column evaluate each
/// operand once, left then right, as Rails 8.1.4 does
/// (`nilable_string_view::EFFECT_VIEW`, title present).
#[test]
#[ignore]
fn nilable_string_view_effect_renders() {
    let (emitted, errors) = nilable_string_view::strict_effect_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1937: with the column nil, `==` and `!=` render as Rails 8.1.4
/// does (`nilable_string_view::NIL_VIEW`).
#[test]
#[ignore]
fn nilable_string_view_nil_renders() {
    let (emitted, errors) = nilable_string_view::strict_nil_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1937: the same operands with a nil title: the right operand still
/// runs, as in Rails 8.1.4 (`nilable_string_view::EFFECT_VIEW`).
#[test]
#[ignore]
fn nilable_string_view_nil_effect_renders() {
    let (emitted, errors) = nilable_string_view::strict_nil_effect_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1946: `each` runs a receiver with an effect once, as Rails 8.1.4
/// does (`each_receiver_once::VIEW`).
#[test]
#[ignore]
fn each_receiver_once_renders() {
    let (emitted, errors) = each_receiver_once::strict_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}

/// #1949: `join` reads a nil element as an empty string, as Rails 8.1.4
/// does (`join_nil::VIEW`).
#[test]
#[ignore]
fn join_nil_renders() {
    let (emitted, errors) = join_nil::strict_app().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.is_empty(), "{errors:?}");
    let test = emitted.join("tests/test_probes_controller.py");
    let source = std::fs::read_to_string(&test).expect("test_probes_controller.py");
    std::fs::write(&test, source.replace(".include_p(", ".__contains__(")).expect("write probe");
    let output = Command::new("python3")
        .args(["-m", "unittest", "tests.test_probes_controller"])
        .current_dir(&*emitted)
        .output()
        .expect("run python unittest");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.contains("Ran 1 test"), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
}
