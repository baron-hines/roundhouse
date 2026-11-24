//! A gem's load-time branch on the Ruby that runs it, settled where every Ruby the tree runs on answers it the same.
//!
//! `def alphanumeric … end if RUBY_VERSION < '3.3'` (securerandom), `if RUBY_ENGINE != 'jruby'` (ostruct),
//! `if defined?(JRUBY_VERSION)`: the emitted trees run on Ruby 4.0 or later, CRuby's (`RUBY_ENGINE` "ruby") and
//! Spinel's ("spinel"), so a comparison with an older version or another engine has one answer. The branch that runs
//! is kept and the rest blanked, newlines kept, so every offset and line stays where the source has it: the ingest
//! then reads plain declarations where it refused the conditional (a visibility change or a constant in a branch).
//! A condition any of those Rubies may answer differently (`RUBY_VERSION >= "4.1"`, `RUBY_ENGINE == "ruby"`) is
//! left as written.

use ruby_prism::Node;

use super::prism_api::PrismLocation;

/// The oldest Ruby the trees run on.
const MINIMUM: [u64; 3] = [4, 0, 0];

/// Engines neither tree is.
const OTHER_ENGINES: &[&str] = &["jruby", "truffleruby", "rbx", "mruby", "opal", "maglev", "ironruby", "macruby"];

/// A core class's `method_defined?` that CRuby 4.0 and Spinel answer alike (`stdlib_constant_execution`'s
/// `core_feature_probes_answer_as_settled` runs each on both): redis-client's `if String.method_defined?(:byteindex)`.
/// Not `Process.respond_to?(:fork)`, which Spinel answers false.
pub(crate) const CORE_PROBES: &[(&str, &str, bool)] = &[
    ("String", "byteindex", true),
    ("IO", "wait_readable", true),
    ("IO", "wait_writable", true),
];

/// A core class's `respond_to?` Spinel answers, for a gem only the Spinel tree compiles (`settled_for`).
const SPINEL_PROBES: &[(&str, &str, bool)] = &[("Process", "fork", false)];

/// Constants only another engine defines.
const OTHER_ENGINE_CONSTANTS: &[&str] = &["JRUBY_VERSION", "TruffleRuby", "Truffle", "Rubinius", "MRUBY_VERSION", "Java"];

/// `text` with each settled load-time branch replaced by the arm that runs, or None when nothing settles.
#[cfg(test)]
pub(super) fn settled(text: &str) -> Option<String> {
    settled_for(text, false)
}

/// `settled`, and for a gem only the Spinel tree compiles (`spinel_only`), a probe Spinel alone answers too:
/// connection_pool's `if Process.respond_to?(:fork)`, false there, where CRuby answers true.
pub(super) fn settled_for(text: &str, spinel_only: bool) -> Option<String> {
    if !(text.contains("RUBY_VERSION")
        || text.contains("RUBY_ENGINE")
        || OTHER_ENGINE_CONSTANTS.iter().any(|c| text.contains(c))
        || text.contains(".method_defined?(")
        || (spinel_only && text.contains(".respond_to?(")))
    {
        return None;
    }
    // Not a file that defines one of these itself: its reads in a nesting that holds the definition are its own.
    if shadows(text) {
        return None;
    }
    let result = ruby_prism::parse(text.as_bytes());
    if result.errors().next().is_some() {
        return None;
    }
    let program = result.node();
    let program = program.as_program_node()?;
    let mut blanks: Vec<(usize, usize)> = Vec::new();
    for stmt in program.statements().body().iter() {
        statement(&stmt, &mut blanks, spinel_only);
    }
    if blanks.is_empty() {
        return None;
    }
    let mut bytes = text.as_bytes().to_vec();
    for (from, to) in blanks {
        for b in &mut bytes[from..to] {
            if *b != b'\n' {
                *b = b' ';
            }
        }
    }
    String::from_utf8(bytes).ok()
}

/// A statement where a class, module or file body runs: a settled branch is blanked but for its arm, whose own
/// statements are walked in turn, as is a class or module body.
fn statement(stmt: &Node<'_>, blanks: &mut Vec<(usize, usize)>, spinel_only: bool) {
    if let Some(class) = stmt.as_class_node() {
        body(class.body(), blanks, spinel_only);
    } else if let Some(module) = stmt.as_module_node() {
        body(module.body(), blanks, spinel_only);
    } else if let Some(singleton) = stmt.as_singleton_class_node() {
        body(singleton.body(), blanks, spinel_only);
    } else if let Some(arm) = settled_arm(stmt, spinel_only) {
        let (start, end) = (stmt.location().start_offset(), stmt.location().end_offset());
        // Not a branch with a heredoc: its body lies past the node's end, which the blanking does not follow.
        if has_heredoc(stmt) {
            return;
        }
        // Nor one whose arm left out assigns a local: Ruby binds it all the same, to nil, for what follows.
        let (kept_from, kept_to) = arm.as_ref().map_or((end, end), |k| (k.location().start_offset(), k.location().end_offset()));
        if assigns_local_outside(stmt, kept_from, kept_to) {
            return;
        }
        match arm {
            Some(kept) => {
                let (from, to) = (kept.location().start_offset(), kept.location().end_offset());
                blanks.push((start, from));
                blanks.push((to, end));
                for inner in kept.body().iter() {
                    statement(&inner, blanks, spinel_only);
                }
            }
            None => blanks.push((start, end)),
        }
    }
}

fn body(node: Option<Node<'_>>, blanks: &mut Vec<(usize, usize)>, spinel_only: bool) {
    let Some(node) = node else { return };
    if let Some(statements) = node.as_statements_node() {
        for stmt in statements.body().iter() {
            statement(&stmt, blanks, spinel_only);
        }
    }
}

/// For an `if`/`unless` (or its modifier form) the condition settles: the statements of the arm that runs, or None
/// for an arm with none. Not settled: an `elsif` before the arm that runs whose own condition does not.
fn settled_arm<'pr>(stmt: &Node<'pr>, spinel_only: bool) -> Option<Option<ruby_prism::StatementsNode<'pr>>> {
    if let Some(branch) = stmt.as_if_node() {
        // Not a ternary: it is an expression, not a branch of declarations.
        branch.if_keyword_loc()?;
        if truth(&branch.predicate(), spinel_only)? {
            return Some(branch.statements());
        }
        return match branch.subsequent() {
            None => Some(None),
            Some(next) => {
                if let Some(otherwise) = next.as_else_node() {
                    Some(otherwise.statements())
                } else {
                    settled_arm(&next, spinel_only)
                }
            }
        };
    }
    if let Some(branch) = stmt.as_unless_node() {
        if truth(&branch.predicate(), spinel_only)? {
            return Some(branch.else_clause().and_then(|e| e.statements()));
        }
        return Some(branch.statements());
    }
    None
}

/// What every Ruby the trees run on answers for `cond`, or None.
fn truth(cond: &Node<'_>, spinel_only: bool) -> Option<bool> {
    if let Some(paren) = cond.as_parentheses_node() {
        let inner = paren.body()?;
        let statements = inner.as_statements_node()?;
        let only: Vec<_> = statements.body().iter().collect();
        let [only] = only.as_slice() else { return None };
        return truth(only, spinel_only);
    }
    // Not on the right alone: a left Ruby evaluates first may do something (`require("x") && RUBY_ENGINE == …`).
    if let Some(and) = cond.as_and_node() {
        return match truth(&and.left(), spinel_only)? {
            false => Some(false),
            true => truth(&and.right(), spinel_only),
        };
    }
    if let Some(or) = cond.as_or_node() {
        return match truth(&or.left(), spinel_only)? {
            true => Some(true),
            false => truth(&or.right(), spinel_only),
        };
    }
    if let Some(defined) = cond.as_defined_node() {
        let name = constant_name(&defined.value())?;
        if OTHER_ENGINE_CONSTANTS.contains(&name.as_str()) {
            return Some(false);
        }
        return matches!(name.as_str(), "RUBY_VERSION" | "RUBY_ENGINE" | "RUBY_ENGINE_VERSION").then_some(true);
    }
    let call = cond.as_call_node()?;
    let method = String::from_utf8_lossy(call.name().as_slice()).into_owned();
    let args: Vec<_> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
    let receiver = call.receiver();
    if method == "!" && args.is_empty() {
        return truth(&receiver?, spinel_only).map(|t| !t);
    }
    let [arg] = args.as_slice() else { return None };
    let receiver = receiver?;
    if spinel_only
        && method == "respond_to?"
        && let Some(class) = constant_name(&receiver)
        && let Some(symbol) = arg.as_symbol_node()
    {
        let name = String::from_utf8_lossy(symbol.unescaped()).into_owned();
        return SPINEL_PROBES.iter().find(|(c, m, _)| *c == class.trim_start_matches("::") && *m == name).map(|(_, _, answer)| *answer);
    }
    if method == "method_defined?"
        && let Some(class) = constant_name(&receiver)
        && let Some(symbol) = arg.as_symbol_node()
    {
        let name = String::from_utf8_lossy(symbol.unescaped()).into_owned();
        return CORE_PROBES.iter().find(|(c, m, _)| *c == class.trim_start_matches("::") && *m == name).map(|(_, _, answer)| *answer);
    }
    if engine_operand(&receiver).is_some() {
        let other = string(arg)?;
        if !OTHER_ENGINES.contains(&other.as_str()) {
            return None;
        }
        return match method.as_str() {
            "==" => Some(false),
            "!=" => Some(true),
            _ => None,
        };
    }
    let than = version_operand(arg)?;
    if !version_operand_is_ruby(&receiver) {
        return None;
    }
    // Only an older version: any Ruby from MINIMUM on is above it, compared as a String (`"4.0.0" > "3.3"`) or as
    // a Gem::Version alike, which a version of a major past 9 would not keep.
    if !(than.first().is_some_and(|major| (1..=3).contains(major)) && than.as_slice() < MINIMUM.as_slice()) {
        return None;
    }
    match method.as_str() {
        "<" | "<=" | "==" => Some(false),
        ">" | ">=" | "!=" => Some(true),
        _ => None,
    }
}

/// Does the file assign one of the names the settling reads, or open a class or module of one?
fn shadows(text: &str) -> bool {
    let names = ["RUBY_VERSION", "RUBY_ENGINE", "RUBY_ENGINE_VERSION"]
        .into_iter()
        .chain(OTHER_ENGINE_CONSTANTS.iter().copied())
        .chain(CORE_PROBES.iter().map(|(class, _, _)| *class));
    names.into_iter().any(|name| {
        text.match_indices(name).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + name.len()..].trim_start_matches([' ', '\t']);
            let word = !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':')
                && !text[at + name.len()..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_');
            let assigned = after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=~");
            let opened = text[..at].trim_end_matches([' ', '\t']).ends_with("class") || text[..at].trim_end_matches([' ', '\t']).ends_with("module");
            word && (assigned || opened)
        })
    })
}

/// Does `stmt` hold a heredoc? Not any `<<`: a `class << self` or an `x << y` in an arm is no heredoc.
fn has_heredoc(stmt: &Node<'_>) -> bool {
    struct Find(bool);
    impl<'pr> ruby_prism::Visit<'pr> for Find {
        fn visit_string_node(&mut self, node: &ruby_prism::StringNode<'pr>) {
            self.0 |= node.opening_loc().is_some_and(|o| o.as_slice().starts_with(b"<<"));
        }
        fn visit_interpolated_string_node(&mut self, node: &ruby_prism::InterpolatedStringNode<'pr>) {
            self.0 |= node.opening_loc().is_some_and(|o| o.as_slice().starts_with(b"<<"));
            ruby_prism::visit_interpolated_string_node(self, node);
        }
        fn visit_x_string_node(&mut self, node: &ruby_prism::XStringNode<'pr>) {
            self.0 |= node.opening_loc().as_slice().starts_with(b"<<");
        }
        fn visit_interpolated_x_string_node(&mut self, node: &ruby_prism::InterpolatedXStringNode<'pr>) {
            self.0 |= node.opening_loc().as_slice().starts_with(b"<<");
            ruby_prism::visit_interpolated_x_string_node(self, node);
        }
    }
    let mut find = Find(false);
    ruby_prism::Visit::visit(&mut find, stmt);
    find.0
}

/// Does `stmt` assign a local outside `[from, to)`, the arm it keeps?
fn assigns_local_outside(stmt: &Node<'_>, from: usize, to: usize) -> bool {
    struct Find {
        from: usize,
        to: usize,
        found: bool,
    }
    impl Find {
        fn at(&mut self, start: usize) {
            if start < self.from || start >= self.to {
                self.found = true;
            }
        }
    }
    impl<'pr> ruby_prism::Visit<'pr> for Find {
        fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'pr>) {
            self.at(node.location().start_offset());
            ruby_prism::visit_local_variable_write_node(self, node);
        }
        fn visit_local_variable_target_node(&mut self, node: &ruby_prism::LocalVariableTargetNode<'pr>) {
            self.at(node.location().start_offset());
        }
        fn visit_local_variable_operator_write_node(&mut self, node: &ruby_prism::LocalVariableOperatorWriteNode<'pr>) {
            self.at(node.location().start_offset());
            ruby_prism::visit_local_variable_operator_write_node(self, node);
        }
        fn visit_local_variable_or_write_node(&mut self, node: &ruby_prism::LocalVariableOrWriteNode<'pr>) {
            self.at(node.location().start_offset());
            ruby_prism::visit_local_variable_or_write_node(self, node);
        }
        fn visit_local_variable_and_write_node(&mut self, node: &ruby_prism::LocalVariableAndWriteNode<'pr>) {
            self.at(node.location().start_offset());
            ruby_prism::visit_local_variable_and_write_node(self, node);
        }
        // A def or a block binds its own locals.
        fn visit_def_node(&mut self, _: &ruby_prism::DefNode<'pr>) {}
        fn visit_block_node(&mut self, _: &ruby_prism::BlockNode<'pr>) {}
        fn visit_lambda_node(&mut self, _: &ruby_prism::LambdaNode<'pr>) {}
        fn visit_class_node(&mut self, _: &ruby_prism::ClassNode<'pr>) {}
        fn visit_module_node(&mut self, _: &ruby_prism::ModuleNode<'pr>) {}
    }
    let mut find = Find { from, to, found: false };
    ruby_prism::Visit::visit(&mut find, stmt);
    find.found
}

fn constant_name(node: &Node<'_>) -> Option<String> {
    if let Some(read) = node.as_constant_read_node() {
        return Some(String::from_utf8_lossy(read.name().as_slice()).into_owned());
    }
    let path = node.as_constant_path_node()?;
    if path.parent().is_some() {
        return None;
    }
    Some(String::from_utf8_lossy(path.name()?.as_slice()).into_owned())
}

fn string(node: &Node<'_>) -> Option<String> {
    node.as_string_node().map(|s| String::from_utf8_lossy(s.unescaped()).into_owned())
}

/// `RUBY_ENGINE`, read bare or `::RUBY_ENGINE`.
fn engine_operand(node: &Node<'_>) -> Option<()> {
    (constant_name(node)? == "RUBY_ENGINE").then_some(())
}

/// `RUBY_VERSION`, or `Gem::Version.new(RUBY_VERSION)`.
fn version_operand_is_ruby(node: &Node<'_>) -> bool {
    if constant_name(node).as_deref() == Some("RUBY_VERSION") {
        return true;
    }
    gem_version_of(node).is_some_and(|inner| constant_name(&inner).as_deref() == Some("RUBY_VERSION"))
}

/// A version written as a String, or `Gem::Version.new("…")`: its numeric segments.
fn version_operand(node: &Node<'_>) -> Option<Vec<u64>> {
    let text = match string(node) {
        Some(text) => text,
        None => string(&gem_version_of(node)?)?,
    };
    let segments: Option<Vec<u64>> = text.split('.').map(|s| s.parse().ok()).collect();
    segments.filter(|s| !s.is_empty() && s.len() <= 4)
}

/// The argument of `Gem::Version.new(x)`.
fn gem_version_of<'pr>(node: &Node<'pr>) -> Option<Node<'pr>> {
    let call = node.as_call_node()?;
    if call.name().as_slice() != b"new" {
        return None;
    }
    let receiver = call.receiver()?;
    let path = receiver.as_constant_path_node()?;
    if path.name()?.as_slice() != b"Version" || path.parent().and_then(|p| constant_name(&p)).as_deref() != Some("Gem") {
        return None;
    }
    let mut args = call.arguments()?.arguments().iter();
    let only = args.next()?;
    args.next().is_none().then_some(only)
}

#[cfg(test)]
mod tests {
    use super::settled;

    #[test]
    fn an_older_version_or_another_engine_settles_and_keeps_every_offset() {
        let src = "class A\n  def x = 1\n  def y = 2 if RUBY_VERSION < '3.3'\n  if RUBY_ENGINE != 'jruby'\n    private :x\n  end\n  if defined?(JRUBY_VERSION)\n    Z = 1\n  else\n    Z = 2\n  end\nend\n";
        let out = settled(src).expect("settles");
        assert_eq!(out.len(), src.len());
        assert_eq!(out.lines().count(), src.lines().count());
        assert!(!out.contains("def y") && !out.contains("RUBY_ENGINE") && !out.contains("Z = 1"), "{out}");
        assert!(out.contains("private :x") && out.contains("Z = 2"), "{out}");
    }

    /// redis-client's `if String.method_defined?(:byteindex)`: a core method CRuby 4.0 and Spinel both have. A probe
    /// they answer differently (`Process.respond_to?(:fork)`) or one the table does not list is left.
    #[test]
    fn a_core_method_probe_both_rubies_answer_alike_settles() {
        let src = "class B
  if String.method_defined?(:byteindex)
    ENCODING = 1
  else
    ENCODING = 2
  end
end
require \"io/wait\" unless IO.method_defined?(:wait_readable) && ::IO.method_defined?(:wait_writable)
";
        let out = settled(src).expect("settles");
        assert!(out.contains("ENCODING = 1") && !out.contains("ENCODING = 2") && !out.contains("io/wait"), "{out}");
        for left in [
            "class C
  if Process.respond_to?(:fork)
    X = 1
  end
end
",
            "class C
  if String.method_defined?(:unknown_one)
    X = 1
  end
end
",
            "module M
  class String
  end
  if String.method_defined?(:byteindex)
    X = 1
  end
end
",
        ] {
            assert_eq!(settled(left), None, "{left}");
        }
    }

    /// connection_pool's `if Process.respond_to?(:fork)`: Spinel answers false, CRuby true. Only for a gem the Spinel
    /// tree alone compiles does it settle, to Spinel's answer.
    #[test]
    fn a_probe_spinel_alone_answers_settles_only_for_a_spinel_only_gem() {
        let src = "class Pool
  if Process.respond_to?(:fork)
    INSTANCES = {}

    def self.after_fork
      INSTANCES
    end
  end
end
";
        assert_eq!(super::settled_for(src, false), None);
        let out = super::settled_for(src, true).expect("settles for Spinel");
        assert!(!out.contains("INSTANCES") && !out.contains("after_fork"), "{out}");
    }

    /// redis-client's `PIDCache`: the arm left out holds a `class << self`, which is no heredoc.
    #[test]
    fn an_arm_with_a_singleton_class_is_not_taken_for_a_heredoc() {
        let src = "module PIDCache\n  if !Process.respond_to?(:fork)\n    @pid = Process.pid\n  elsif Process.respond_to?(:_fork)\n    class << self\n      attr_reader :pid\n    end\n  end\nend\n";
        let out = super::settled_for(src, true).expect("settles");
        assert!(out.contains("@pid = Process.pid") && !out.contains("class << self"), "{out}");
    }

    #[test]
    fn a_condition_some_ruby_answers_otherwise_is_left() {
        for src in [
            "def x = 1 if RUBY_VERSION >= '4.1'\n",
            "def x = 1 if RUBY_ENGINE == 'ruby'\n",
            "def x = 1 if RUBY_VERSION < '10'\n",
            "if RUBY_VERSION < '3.0'\n  A = 1\nelsif FOO\n  A = 2\nend\n",
        ] {
            assert_eq!(settled(src), None, "{src}");
        }
    }

    #[test]
    fn a_shadowed_name_a_local_a_heredoc_or_an_effect_before_it_is_left() {
        for src in [
            "module M\n  RUBY_ENGINE = 'jruby'\n  if RUBY_ENGINE == 'jruby'\n    A = 1\n  end\nend\n",
            "adapter = :java if RUBY_ENGINE == 'jruby'\nR = adapter\n",
            "if RUBY_ENGINE != 'jruby'\n  T = <<~X\n    hi\n  X\nend\n",
            "if require('json') && RUBY_ENGINE == 'jruby'\n  A = 1\nend\n",
        ] {
            assert_eq!(settled(src), None, "{src}");
        }
    }

    #[test]
    fn a_gem_version_comparison_settles_too() {
        let src = "if Gem::Version.new(RUBY_VERSION) >= Gem::Version.new('2.7.0')\n  A = 1\nelse\n  A = 2\nend\n";
        let out = settled(src).expect("settles");
        assert!(out.contains("A = 1") && !out.contains("A = 2"), "{out}");
    }
}
