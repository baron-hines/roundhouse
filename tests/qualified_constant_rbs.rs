//! Keep inferred full names unambiguous in RBS without rewriting declared relative types.
use std::collections::HashMap;
use std::path::PathBuf;

#[path = "support/scratch.rs"]
mod scratch;

const SOURCE: &str = r#"class Token
  def label = "root"
end
module Transport
  class Token
    def label = "nested"
  end
  module Factory
    def self.inferred = ::Token.new
    def self.items = [::Token.new]
    def self.maybe(flag) = flag ? ::Token.new : nil
    def self.either(flag) = flag ? ::Token.new : Token.new
    def self.pair = [::Token.new, 1]
    def self.kind = ::Token
    def self.own = Token.new
    def self.identity(value) = value
    def self.from_caller = identity(::Token.new)

    #: () -> Token
    def self.relative = Token.new
    #: (Integer) -> Token?
    def self.relative_optional(n) = n > 0 ? Token.new : nil
    #: () -> Array[Token]
    def self.relative_items = [Token.new]
    #: () -> Hash[Symbol, Token]
    def self.relative_hash = {token: Token.new}
    #: () -> { token: Token }
    def self.relative_record = {token: Token.new}
    #: (Token) -> String
    def self.relative_param(value) = value.label

    def self.run
      [
        inferred.label, items.first.label, maybe(true).label, either(true).label, either(false).label,
        pair[0].label, kind.new.label, own.label, identity(inferred).label, from_caller.label,
        relative.label, relative_optional(1).label, relative_items.first.label,
        relative_record[:token].label, relative_param(Token.new),
      ].join(",")
    end
  end
end
"#;

const SCRIPT: &str = "puts Transport::Factory.run\n";

const EXPECTED: &str = "root,root,root,root,nested,root,root,nested,root,root,\
    nested,nested,nested,nested,nested\n";

const SCOPE_NAMESAKES: &str = r#"module Capabilities
  class Refund
    def label = "root"
  end
end
module Billing
  module Capabilities
    class Refund
      def label = "nested"
    end
  end
end
"#;

fn scope_sidecar(declaration: &str, extra: &[(&str, &str)], stem: &str) -> String {
    sidecar(&format!("{SCOPE_NAMESAKES}{declaration}"), extra, stem)
}

#[test]
fn scope_qualified_reopening_keeps_mains_binding() {
    let rbs = scope_sidecar(
        "class Billing::Capabilities::Charge\n  #: () -> Capabilities::Refund\n  def self.build = ::Capabilities::Refund.new\nend\n",
        &[], "charge.rbs",
    );
    assert_lines(&rbs, &["def self.build: () -> ::Capabilities::Refund"]);
}

#[test]
fn scope_qualified_module_keeps_mains_binding() {
    let rbs = scope_sidecar(
        "module Billing::Capabilities::Factory\n  #: () -> Capabilities::Refund\n  def self.build = Capabilities::Refund.new\nend\n",
        &[], "factory.rbs",
    );
    assert_lines(&rbs, &["def self.build: () -> ::Capabilities::Refund"]);
}

#[test]
fn scope_sidecar_scope_change_keeps_mains_binding() {
    let rbs = scope_sidecar(
        "class Billing::Capabilities::Charge\n  def self.build = ::Capabilities::Refund.new\nend\n",
        &[("sig/charge.rbs", "class Billing::Capabilities::Charge\n  def self.build: () -> Capabilities::Refund\nend\n")],
        "charge.rbs",
    );
    assert_lines(&rbs, &["def self.build: () -> ::Capabilities::Refund"]);
}

#[test]
fn scope_sorbet_scope_change_keeps_mains_binding() {
    let rbs = scope_sidecar(
        "class Billing::Capabilities::Charge\n  extend T::Sig\n  sig { returns(Capabilities::Refund) }\n  def self.build = Capabilities::Refund.new\nend\n",
        &[], "charge.rbs",
    );
    assert_lines(&rbs, &["def self.build: () -> ::Capabilities::Refund"]);
}

#[test]
fn scope_nested_relative_control_keeps_its_improvement() {
    let rbs = scope_sidecar(
        "module Billing\n  module Capabilities\n    class Charge\n      #: () -> Capabilities::Refund\n      def self.build = Capabilities::Refund.new\n    end\n  end\nend\n",
        &[], "charge.rbs",
    );
    assert_lines(&rbs, &["def self.build: () -> Capabilities::Refund"]);
}

#[test]
fn scope_compact_ancestor_changes_both_ruby_and_sidecar_nesting() {
    let ruby = "module Billing::Capabilities\n  class Charge\n    #: () -> Capabilities::Refund\n    def self.build = ::Capabilities::Refund.new\n  end\nend\n";
    let rbs = scope_sidecar(ruby, &[], "charge.rbs");
    assert_lines(&rbs, &["def self.build: () -> ::Capabilities::Refund"]);
    let signature = "module Billing::Capabilities\n  class Charge\n    def self.build: () -> Capabilities::Refund\n  end\nend\n";
    let rbs = scope_sidecar(ruby, &[("sig/charge.rbs", signature)], "charge.rbs");
    assert_lines(&rbs, &["def self.build: () -> ::Capabilities::Refund"]);
}

#[test]
fn scope_written_root_survives_a_compact_declaration() {
    let rbs = scope_sidecar(
        "class Billing::Capabilities::Charge\n  #: () -> ::Capabilities::Refund\n  def self.build = ::Capabilities::Refund.new\nend\n",
        &[], "charge.rbs",
    );
    assert_lines(&rbs, &["def self.build: () -> ::Capabilities::Refund"]);
}

#[test]
fn scope_reopenings_keep_each_methods_nesting_in_both_file_orders() {
    let compact = "class Billing::Capabilities::Charge\n  #: () -> Capabilities::Refund\n  def self.rooted = ::Capabilities::Refund.new\nend\n";
    let nested = "module Billing\n  module Capabilities\n    class Charge\n      #: () -> Capabilities::Refund\n      def self.relative = Capabilities::Refund.new\n    end\n  end\nend\n";
    for (first, last) in [(compact, nested), (nested, compact)] {
        let rbs = sidecar(SCOPE_NAMESAKES, &[("app/services/a.rb", first), ("app/services/z.rb", last)], "charge.rbs");
        assert_lines(&rbs, &["def self.rooted: () -> ::Capabilities::Refund", "def self.relative: () -> Capabilities::Refund"]);
    }
}

fn tree(source: &str, extra: &[(&str, &str)]) -> HashMap<PathBuf, Vec<u8>> {
    [
        ("db/schema.rb", "ActiveRecord::Schema.define do\nend\n"),
        ("app/services/transport.rb", source),
    ]
    .iter()
    .chain(extra)
    .map(|(p, s)| (PathBuf::from(p), s.as_bytes().to_vec()))
    .collect()
}

fn app(source: &str, extra: &[(&str, &str)]) -> roundhouse::App {
    let mut app = roundhouse::ingest::ingest_app_from_tree(tree(source, extra)).unwrap();
    roundhouse::session::analyze_and_lower(&mut app);
    app
}

fn sidecar(source: &str, extra: &[(&str, &str)], stem: &str) -> String {
    roundhouse::emit::ruby::emit_library(&app(source, extra))
        .into_iter()
        .find(|f| f.path.ends_with(stem))
        .unwrap_or_else(|| panic!("no sidecar ends with {stem}"))
        .content
}

fn assert_lines(rbs: &str, expected: &[&str]) {
    for line in expected {
        assert!(rbs.contains(line), "missing `{line}`\n{rbs}");
    }
}

/// An inferred type is the full name, so a namesake around it must not
/// capture it, in any position a name can appear.
#[test]
fn an_inferred_root_name_is_rooted_in_every_position() {
    assert_lines(
        &sidecar(SOURCE, &[], "factory.rbs"),
        &[
            "def self.inferred: () -> ::Token",
            "def self.items: () -> Array[::Token]",
            "-> ::Token?",
            "(::Token | Transport::Token)",
            "def self.pair: () -> [::Token, Integer]",
            "def self.kind: () -> singleton(::Token)",
            "(::Token value) -> ::Token",
        ],
    );
}

/// A written `Token` means what Ruby finds from there: the nearer class.
#[test]
fn a_written_relative_name_still_binds_to_its_namesake() {
    let rbs = sidecar(SOURCE, &[], "factory.rbs");
    assert_lines(
        &rbs,
        &[
            "def self.own: () -> Transport::Token",
            "def self.relative: () -> Token",
            "(Integer n) -> Token?",
            "def self.relative_items: () -> Array[Token]",
            "def self.relative_hash: () -> Hash[Symbol, Token]",
            "def self.relative_record: () -> { token: Token }",
            "def self.relative_param: (Token value) -> String",
        ],
    );
}

/// A nested class may share its name with one of Ruby's own. A written
/// `Data` inside `Transport` still means `Transport::Data`, never `::Data`.
#[test]
fn a_relative_name_ruby_also_defines_binds_to_its_namesake() {
    let names = [
        "Data",
        "Numeric",
        "Complex",
        "Binding",
        "ObjectSpace",
        "Signal",
        "Singleton",
        "UnboundMethod",
        "Module",
        "Comparable",
        "Rational",
        "GC",
        "Forwardable",
    ];
    let wrong: Vec<String> = names.iter().filter_map(|name| {
        let source = format!(
            "module Transport\n  class {name}\n    def label = \"nested\"\n  end\n  module Factory\n    #: () -> {name}\n    def self.build = {name}.new\n  end\nend\n"
        );
        let rbs = sidecar(&source, &[], "factory.rbs");
        let line = rbs.lines().find(|l| l.contains("def self.build")).unwrap_or_default().trim().to_string();
        (line != format!("def self.build: () -> {name}")).then_some(line)
    }).collect();
    assert!(
        wrong.is_empty(),
        "{} of {} wrong: {wrong:#?}",
        wrong.len(),
        names.len()
    );
}

/// A namespace-only root cannot change a relative module annotation.
#[test]
fn a_relative_mixin_beside_a_namespace_only_root_binds_to_its_namesake() {
    let source = |root: &str| {
        format!(
            "{root}module ReviewScope\n  module Tag\n    def label = \"nested-marker\"\n  end\n  class Payload\n    include Tag\n  end\n  module Factory\n    #: () -> Tag\n    def self.build = Payload.new\n  end\nend\n"
        )
    };
    let namespace_only = sidecar(
        &source("module Tag\n  class Item\n  end\nend\n"),
        &[],
        "factory.rbs",
    );
    assert!(
        namespace_only.contains("def self.build: () -> Tag"),
        "{namespace_only}"
    );
    let no_root = sidecar(&source(""), &[], "factory.rbs");
    assert!(no_root.contains("def self.build: () -> Tag"), "{no_root}");
    let registered_root = sidecar(
        &source("module Tag\n  def self.marker = 1\nend\n"),
        &[],
        "factory.rbs",
    );
    assert!(
        registered_root.contains("def self.build: () -> Tag"),
        "{registered_root}"
    );
}

/// Analysis leaves namespace-only names relative at declaration, caller and cast sites.
#[test]
fn a_namespace_only_namesake_captures_a_relative_name_everywhere() {
    let source = r#"module Tag
  def self.marker = 1
end
module ReviewScope
  module Tag
    class Item
    end
  end
  class Payload
    include Tag
  end
  class Holder
    #: () -> Tag
    def declared = Payload.new
    def forwarded = declared
    def cast
      value = Payload.new #: Tag
      value
    end
  end
end
"#;
    assert_lines(
        &sidecar(source, &[], "holder.rbs"),
        &[
            "def declared: () -> Tag",
            "def forwarded: () -> Tag",
            "def cast: () -> Tag",
        ],
    );
}

/// Nothing around the name collides: the author's spelling is the output.
#[test]
fn an_unambiguous_relative_name_keeps_the_authors_spelling() {
    let source = r#"module Billing
  module Inner
    class Refund
    end
  end
  module Account
    class Charge
      #: () -> Inner::Refund
      def self.build = Inner::Refund.new
    end
    class Other
    end
    class Sibling
      #: () -> Other
      def self.build = Other.new
    end
  end
  class Parent
  end
  module Deep
    module Er
      #: () -> Parent
      def self.build = Parent.new
    end
  end
end
"#;
    assert!(sidecar(source, &[], "charge.rbs").contains("def self.build: () -> Inner::Refund"));
    assert!(sidecar(source, &[], "sibling.rbs").contains("def self.build: () -> Other"));
    assert!(sidecar(source, &[], "er.rbs").contains("def self.build: () -> Parent"));
}

/// The head is an enclosing namespace, and the name is under it, not at
/// the root: `::Capabilities::Refund` names nothing.
#[test]
fn a_relative_name_headed_by_an_enclosing_namespace_stays_under_it() {
    let source = r#"module Billing
  module Capabilities
    class Refund
    end
    class Charge
      #: () -> Capabilities::Refund
      def self.build = Capabilities::Refund.new
    end
  end
end
"#;
    let rbs = sidecar(source, &[], "charge.rbs");
    assert!(
        rbs.contains("def self.build: () -> Capabilities::Refund"),
        "{rbs}"
    );
}

#[test]
fn app_rbs_and_sorbet_keep_their_relative_names() {
    let source = r#"module Billing
  module Capabilities
    class Refund
    end
    class Charge
      extend T::Sig
      sig { returns(Capabilities::Refund) }
      def self.sorbet = Capabilities::Refund.new
      def self.sidecar = Capabilities::Refund.new
    end
  end
end
"#;
    let declared = "module Billing\n  module Capabilities\n    class Charge\n      def self.sidecar: () -> Capabilities::Refund\n    end\n  end\nend\n";
    assert_lines(
        &sidecar(source, &[("sig/billing.rbs", declared)], "charge.rbs"),
        &[
            "def self.sorbet: () -> Capabilities::Refund",
            "def self.sidecar: () -> Capabilities::Refund",
        ],
    );
}

/// A declared attribute reads from its class's scope like any signature.
#[test]
fn a_current_attribute_binds_like_a_signature() {
    let source = r#"class Token
end
module Transport
  class Token
  end
  class Current < ActiveSupport::CurrentAttributes
    attribute :tenant #: Token
  end
end
"#;
    assert_lines(
        &sidecar(source, &[], "current.rbs"),
        &[
            "def tenant: () -> Token?",
            "def tenant=: (Token? value) -> Token?",
        ],
    );
}

/// A nearer declaration captures Ruby's own classes the same way.
#[test]
fn a_shadowed_stdlib_class_is_rooted() {
    let source = r#"module Transport
  class Set
  end
  module Factory
    def self.build = ::Set.new
  end
end
"#;
    let rbs = sidecar(source, &[], "factory.rbs");
    assert!(rbs.contains("def self.build: () -> ::Set"), "{rbs}");
}

/// A superclass binds from outside the class, a mixin from inside it.
#[test]
fn superclass_and_mixin_names_use_their_declaration_scope() {
    let source = r#"class Token
end
module Shared
  def label = "root"
end
module Transport
  class Token
  end
  module Shared
    def label = "nested"
  end
  class Child < ::Token
    include ::Shared
  end
  class Near < Token
    include Shared
  end
end
"#;
    let child = sidecar(source, &[], "child.rbs");
    assert!(child.contains("class Child < ::Token"), "{child}");
    assert!(child.contains("include ::Shared"), "{child}");
    let near = sidecar(source, &[], "near.rbs");
    assert!(near.contains("class Near < Transport::Token"), "{near}");
    assert!(near.contains("include Transport::Shared"), "{near}");
}

/// The first segment decides: `::Domain::Token` is rooted because
/// `Transport::Domain` exists, whether or not it holds a `Token`.
#[test]
fn a_shadowed_namespace_prefix_roots_the_whole_path() {
    let source = r#"module Domain
  class Token
  end
  class Only
  end
end
module Transport
  module Domain
    def self.marker = 1
    class Token
    end
  end
  module Factory
    def self.root = ::Domain::Token.new
    def self.only = ::Domain::Only.new
    def self.own = Transport::Domain::Token.new
  end
end
"#;
    assert_lines(
        &sidecar(source, &[], "factory.rbs"),
        &[
            "def self.root: () -> ::Domain::Token",
            "def self.only: () -> ::Domain::Only",
            "def self.own: () -> Transport::Domain::Token",
        ],
    );
}

#[test]
fn nothing_around_the_name_leaves_it_bare() {
    let source = "class Solo\n  def self.make = new\n  def copy = Solo.new\nend\nclass Plain\nend\nmodule Quiet\n  module Factory\n    def self.build = ::Plain.new\n  end\nend\n";
    assert!(sidecar(source, &[], "solo.rbs").contains("def copy: () -> Solo"));
    assert!(sidecar(source, &[], "factory.rbs").contains("def self.build: () -> Plain"));
}

#[test]
fn declaration_order_does_not_change_the_answer() {
    let nested = SOURCE.find("module Transport").unwrap();
    let reversed = format!("{}\n{}", &SOURCE[nested..], &SOURCE[..nested]);
    assert_eq!(
        sidecar(SOURCE, &[], "factory.rbs"),
        sidecar(&reversed, &[], "factory.rbs")
    );
}

/// The declaration retains its spelling; callers receive analysis's resolved name.
#[test]
fn callers_of_declared_methods_use_analysis_names() {
    let source = r#"class Token
end
module Transport
  class Token
  end
  module Factory
    #: () -> Token
    def self.relative = Token.new
    #: () -> ::Token
    def self.rooted = ::Token.new
    def self.relative_caller = relative
    def self.rooted_caller = rooted
  end
end
"#;
    assert_lines(
        &sidecar(source, &[], "factory.rbs"),
        &[
            "def self.relative: () -> Token",
            "def self.rooted: () -> ::Token",
            "def self.relative_caller: () -> Transport::Token",
            "def self.rooted_caller: () -> ::Token",
        ],
    );
}

#[test]
fn an_inferred_path_can_repeat_its_enclosing_name() {
    let source = "class Token\n  def label = \"outer\"\n  class Token\n    def label = \"inner\"\n    def self.build = ::Token::Token.new\n  end\n  def self.root = ::Token.new\n  def self.inner = Token.new\nend\n";
    let outer = sidecar(source, &[], "app/models/token.rbs");
    assert_lines(
        &outer,
        &[
            "def self.root: () -> ::Token",
            "def self.inner: () -> ::Token::Token",
        ],
    );
}

/// Resolving a declared name in the shared model used to remove this import.
#[test]
fn typescript_keeps_the_import_for_a_written_relative_type() {
    let source = r#"class Token
  def label = "root"
end
module Scope
  class Token
    def label = "nested"
  end
  module Factory
    #: () -> Token
    def self.build = Token.new
  end
end
"#;
    let files = roundhouse::emit::typescript::emit(&app(source, &[]));
    let factory = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("factory.ts"))
        .unwrap();
    assert!(
        factory.content.contains("import { Token }"),
        "{}",
        factory.content
    );
    assert!(
        factory.content.contains("build(): Token"),
        "{}",
        factory.content
    );
}

#[test]
fn reopened_declarations_keep_their_own_written_names_in_both_orders() {
    let declaration = "module Transport\n  module Factory\n    #: () -> Token\n    def self.relative = Token.new\n  end\nend\n";
    let classes = "class Token\nend\nmodule Transport\n  class Token\n  end\n  module Factory\n    def self.inferred = ::Token.new\n  end\nend\n";
    for (first, second) in [(declaration, classes), (classes, declaration)] {
        let files = roundhouse::emit::ruby::emit_library(&app(
            first,
            &[("app/services/reopened.rb", second)],
        ));
        let sidecars = files
            .iter()
            .filter(|f| f.path.extension().is_some_and(|e| e == "rbs"))
            .map(|f| f.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert_lines(
            &sidecars,
            &[
                "def self.relative: () -> Token",
                "def self.inferred: () -> ::Token",
            ],
        );
    }
}

fn run_program(native: bool, source: &str, extra: &[(&str, &str)], script_body: &str, expected: &str) {
    let dir = scratch::dir("qualified_constant_rbs");
    let mut script = String::new();
    for file in roundhouse::emit::ruby::emit_library(&app(source, extra)) {
        let path = dir.join(&file.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &file.content).unwrap();
        if path.extension().is_some_and(|e| e == "rb") {
            script.push_str(&format!(
                "require_relative {:?}\n",
                file.path.to_string_lossy()
            ));
        }
    }
    script.push_str(script_body);
    std::fs::write(dir.join("contract.rb"), script).unwrap();
    let output = if native {
        let compiler = std::env::var("SPINEL").unwrap_or_else(|_| "spinel".into());
        let built = std::process::Command::new(compiler)
            .args(["--rbs", "sig", "contract.rb", "-o", "contract"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        std::process::Command::new(dir.join("contract"))
            .current_dir(&dir)
            .output()
            .unwrap()
    } else {
        std::process::Command::new("ruby")
            .arg("contract.rb")
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
    std::fs::remove_dir_all(dir).unwrap();
}

fn run(native: bool) {
    run_program(native, SOURCE, &[], SCRIPT, EXPECTED);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn scope_changes_seeded_native_agrees() {
    // Explicit roots in compact Ruby bodies isolate RBS binding from the
    // existing lowering of relative body constants in qualified declarations.
    for (declaration, signature, expected) in [
        ("class Billing::Capabilities::Charge\n  #: () -> Capabilities::Refund\n  def self.build = ::Capabilities::Refund.new\nend\n", None, "root\n"),
        ("module Billing::Capabilities::Charge\n  #: () -> Capabilities::Refund\n  def self.build = ::Capabilities::Refund.new\nend\n", None, "root\n"),
        ("class Billing::Capabilities::Charge\n  def self.build = ::Capabilities::Refund.new\nend\n", Some("class Billing::Capabilities::Charge\n  def self.build: () -> Capabilities::Refund\nend\n"), "root\n"),
        ("class Billing::Capabilities::Charge\n  extend T::Sig\n  sig { returns(Capabilities::Refund) }\n  def self.build = ::Capabilities::Refund.new\nend\n", None, "root\n"),
        ("module Billing\n  module Capabilities\n    class Charge\n      #: () -> Capabilities::Refund\n      def self.build = Capabilities::Refund.new\n    end\n  end\nend\n", None, "nested\n"),
    ] {
        let source = format!("{SCOPE_NAMESAKES}{declaration}");
        let script = "puts Billing::Capabilities::Charge.build.label\n";
        let oracle = format!("module T; module Sig; def sig(&); end; end; end\n{source}\n{script}");
        let output = std::process::Command::new("ruby").args(["-e", &oracle]).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        let extra: Vec<_> = signature.map(|sig| ("sig/charge.rbs", sig)).into_iter().collect();
        run_program(false, &source, &extra, script, expected);
        run_program(true, &source, &extra, script, expected);
    }
}

/// The literal expectation is what Ruby itself says of the original source.
#[test]
fn the_expectation_is_what_ruby_says() {
    let output = std::process::Command::new("ruby")
        .args(["-e", &format!("{SOURCE}\n{SCRIPT}")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
}

#[test]
fn the_emitted_ruby_agrees() {
    run(false);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn the_seeded_native_build_agrees() {
    run(true);
}

#[test]
fn app_rbs_prefix_keeps_existing_meaning() {
    let source = r#"module ScopePath
  module ScopePath
    class Token
      def label = "nested"
    end
    module ScopePath
      module ScopePath
        class Token
          def label = "captured"
        end
      end
    end
    module Factory
      def self.build = Token.new
    end
  end
end
"#;
    let signature = "module ScopePath\n  module ScopePath\n    module Factory\n      def self.build: () -> Token\n    end\n  end\nend\n";
    let rbs = sidecar(source, &[("sig/example.rbs", signature)], "factory.rbs");
    assert!(rbs.contains("def self.build: () -> ::ScopePath::ScopePath::Token"), "{rbs}");
}

#[test]
fn written_qualified_root_keeps_existing_meaning() {
    let source = r#"module Capabilities
  class Refund
    def label = "root"
  end
end
module Billing
  module Capabilities
    class Refund
      def label = "nested"
    end
    class Charge
      #: () -> ::Capabilities::Refund
      def self.build = ::Capabilities::Refund.new
    end
  end
end
"#;
    let rbs = sidecar(source, &[], "charge.rbs");
    assert!(rbs.contains("def self.build: () -> ::Capabilities::Refund"), "{rbs}");
}

#[test]
fn every_reader_keeps_a_written_root() {
    let source = r#"class Token
  def label = "root"
end
module Transport
  class Token
    def label = "nested"
  end
  module Factory
    extend T::Sig
    #: (::Token) -> Array[::Token]
    def self.inline(value) = [value]
    sig { returns(::Token) }
    def self.sorbet = ::Token.new
    def self.sidecar = ::Token.new
  end
end
"#;
    let declared = "module Transport\n  module Factory\n    def self.sidecar: () -> ::Token\n  end\nend\n";
    assert_lines(
        &sidecar(source, &[("sig/transport.rbs", declared)], "factory.rbs"),
        &[
            "def self.inline: (::Token value) -> Array[::Token]",
            "def self.sorbet: () -> ::Token",
            "def self.sidecar: () -> ::Token",
        ],
    );
}

mod reopening_provenance {
use std::collections::HashMap;
use std::path::PathBuf;

const TYPES: &str = r#"
module Capabilities
  class Refund
    def label = "root"
  end
end
module Billing
  module Capabilities
    class Refund
      def label = "nested"
    end
  end
end
"#;
const NESTED: &str = r#"
module Billing
  module Capabilities
    class Charge
      #: () -> Capabilities::Refund
      def self.build = ::Billing::Capabilities::Refund.new
    end
  end
end
"#;
const COMPACT: &str = r#"
class Billing::Capabilities::Charge
  #: () -> Capabilities::Refund
  def self.build = ::Capabilities::Refund.new
end
"#;

fn emitted(files: &[(&str, String)]) -> String {
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    tree.insert("db/schema.rb".into(), b"ActiveRecord::Schema.define do\nend\n".to_vec());
    tree.insert("config/routes.rb".into(), b"Rails.application.routes.draw do\nend\n".to_vec());
    for (path, source) in files {
        tree.insert(PathBuf::from(path), source.as_bytes().to_vec());
    }
    let mut app = roundhouse::ingest::ingest_app_from_tree(tree).unwrap();
    roundhouse::session::analyze_and_lower(&mut app);
    roundhouse::emit::ruby::emit_library(&app).into_iter()
        .find(|file| file.path.ends_with("billing/capabilities/charge.rbs"))
        .unwrap().content
}

#[test]
fn unannotated_reopening_keeps_the_final_return_binding() {
    let replacement = COMPACT.replace("  #: () -> Capabilities::Refund\n", "");
    let rbs = emitted(&[("app/services/example.rb", format!("{TYPES}{NESTED}{replacement}"))]);
    assert!(rbs.contains("def self.build: () -> ::Capabilities::Refund"), "{rbs}");
}

#[test]
fn later_file_keeps_the_final_return_binding() {
    let rbs = emitted(&[
        ("app/services/example.rb", TYPES.into()),
        ("app/services/first.rb", NESTED.into()),
        ("app/services/last.rb", COMPACT.into()),
    ]);
    assert!(rbs.contains("def self.build: () -> ::Capabilities::Refund"), "{rbs}");
}

#[test]
fn annotated_reopening_in_one_file_is_the_control() {
    let rbs = emitted(&[("app/services/example.rb", format!("{TYPES}{NESTED}{COMPACT}"))]);
    assert!(rbs.contains("def self.build: () -> ::Capabilities::Refund"), "{rbs}");
}

fn sorbet(source: &str) -> String {
    source.replace("#: () -> Capabilities::Refund", "extend T::Sig; sig { returns(Capabilities::Refund) }")
}

#[test]
fn replacement_metadata_follows_the_final_definition_in_both_orders() {
    let other = NESTED.replace("self.build", "self.other");
    for (first, last, nested_last) in [(NESTED, COMPACT, false), (COMPACT, NESTED, true)] {
        for reader in ["inline", "sorbet"] {
            for final_signature in ["accepted", "absent", "unread"] {
                let mut last = last.to_string();
                if final_signature == "absent" {
                    last = last.replace("#: () -> Capabilities::Refund", "");
                } else if final_signature == "unread" {
                    last = last.replace("#: () -> Capabilities::Refund", "#: () ->");
                }
                let first = format!("{other}{first}");
                let (first, last) = if reader == "sorbet" {
                    (sorbet(&first), sorbet(&last).replace("#: () ->", "extend T::Sig; sig { returns }"))
                } else { (first, last) };
                for split in [false, true] {
                    let files = if split {
                        vec![("app/services/example.rb", TYPES.to_string()),
                            ("app/services/first.rb", first.clone()), ("app/services/last.rb", last.clone())]
                    } else { vec![("app/services/example.rb", format!("{TYPES}{first}{last}"))] };
                    let rbs = emitted(&files);
                    let expected = if final_signature == "accepted" && nested_last {
                        "def self.build: () -> Capabilities::Refund"
                    } else { "def self.build: () -> ::Capabilities::Refund" };
                    assert!(rbs.contains(expected), "{reader} {final_signature} split={split}: {rbs}");
                    assert!(rbs.contains("def self.other: () -> Capabilities::Refund"), "{rbs}");
                }
            }
        }
    }
}

#[test]
fn sidecar_replacements_keep_the_final_signature_and_override_ruby_metadata() {
    let nested = NESTED.replace("#: () -> Capabilities::Refund\n      def self.build = ::Billing::Capabilities::Refund.new",
        "def self.build: () -> Capabilities::Refund");
    let compact = COMPACT.replace("#: () -> Capabilities::Refund\n  def self.build = ::Capabilities::Refund.new",
        "def self.build: () -> Capabilities::Refund");
    for (first, last, ruby, nested_last) in [(&nested, &compact, COMPACT, false), (&compact, &nested, NESTED, true)] {
        for split in [false, true] {
            // An unannotated Ruby replacement must not erase a sidecar contract.
            let ruby = format!("{TYPES}{ruby}{}", ruby.replace("#: () -> Capabilities::Refund", ""));
            let mut files = vec![("app/services/example.rb", ruby)];
            if split {
                files.extend([("sig/first.rbs", first.clone()), ("sig/last.rbs", last.clone())]);
            } else { files.push(("sig/charge.rbs", format!("{first}{last}"))); }
            let rbs = emitted(&files);
            let expected = if nested_last { "def self.build: () -> Capabilities::Refund" }
                else { "def self.build: () -> ::Capabilities::Refund" };
            assert!(rbs.contains(expected), "split={split}: {rbs}");
        }
    }
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn replacement_contracts_agree_with_ruby_and_seeded_native() {
    for reader in ["inline", "sorbet"] {
        for signed in [false, true] {
            let last = if signed { COMPACT.to_string() }
                else { COMPACT.replace("#: () -> Capabilities::Refund", "") };
            let (first, last) = if reader == "sorbet" { (sorbet(NESTED), sorbet(&last)) }
                else { (NESTED.to_string(), last) };
            let script = "puts Billing::Capabilities::Charge.build.label\n";
            let original = format!("module T; module Sig; def sig(&); end; end; end\n{TYPES}{first}{last}{script}");
            let output = std::process::Command::new("ruby").args(["-e", &original]).output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(String::from_utf8_lossy(&output.stdout), "root\n");
            let source = format!("{TYPES}{first}");
            let extra = [("app/services/z.rb", last.as_str())];
            for native in [false, true] { super::run_program(native, &source, &extra, script, "root\n"); }
        }
    }
}

#[test]
fn conflicting_cross_file_seeds_fall_back_without_reusing_later_spellings() {
    let first = COMPACT.replace("() -> Capabilities::Refund", "() -> Array[Capabilities::Refund]")
        .replace("= ::Capabilities::Refund.new", "= [::Capabilities::Refund.new]");
    let rbs = emitted(&[("app/services/example.rb", TYPES.into()),
        ("app/services/first.rb", first), ("app/services/last.rb", NESTED.into())]);
    assert!(rbs.contains("def self.build: () -> Array[::Capabilities::Refund]"), "{rbs}");
}

}
