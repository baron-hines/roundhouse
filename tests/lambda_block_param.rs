//! A stabby lambda's named block parameter (`->(x, &block) { … }`, view_component's slot lambdas): the lambda is
//! called with a block, and the body calls or tests it. Ingest refused the lambda ("block parameter capture"); it now
//! keeps the parameter as a `define_method` block's is.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        "app/models/wrapper.rb",
        "class Wrapper\n  def self.wrap\n    ->(label, &block) { block ? \"[\" + block.call(label) + \"]\" : label }\n  end\n\n  def self.report\n    w = wrap\n    [w.call(\"a\") { |l| l.upcase }, w.call(\"b\")].join(\",\")\n  end\nend\n",
    )
}

#[test]
fn a_lambda_keeps_its_block_parameter_on_ruby() {
    let run = app().run_ruby("puts Wrapper.report\n");
    run.assert_passes();
    assert_eq!(run.stdout, "[A],b\n");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_lambda_keeps_its_block_parameter_on_spinel() {
    let run = app().run_spinel_slim(&[], "puts Wrapper.report\n");
    run.assert_passes();
    assert_eq!(run.stdout, "[A],b\n");
}
