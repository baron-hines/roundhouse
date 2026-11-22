//! multi_json over its oj adapter, lowered into a Ruby stand-in.
//!
//! An app that picks the oj adapter in an initializer (`MultiJson.use(:oj)`,
//! discourse's `config/initializers/100-oj.rb`) gets oj's compat dump:
//! `MultiJson.dump(obj)` is `Oj.dump(obj, mode: :compat, time_format: :ruby,
//! use_to_json: true)`, whatever `Oj.default_options` sets the mode to. For
//! the values the stand-in writes that is:
//!
//! - a Hash's keys as their `to_s`, Strings and Symbols as JSON strings with
//!   `<>&` and U+2028 raw (no Rails escaping);
//! - an integral Float within 2**63 as `<integer>.0`, `-0.0` as `0.0`, any
//!   other with C's `%.16g` (`0.1 + 0.2` is `0.3`, `1e20` is `1e+20`), and
//!   `NaN` and the infinities written as such;
//! - a Time as its `to_json`, Rails' `xmlschema(3)`;
//! - `pretty: <anything>` (oj reads the key, not its value) as
//!   `JSON.pretty_generate` lays it out.
//!
//! Any other value raises, and the analysis refuses a call whose value is
//! not one of those (`analyze::body`'s `multi_json_dump_refusal`).
//!
//! Without such an initializer multi_json 1.21 picks json_gem (`JSON::Ext::
//! Parser` is always loaded), which this does not model: `MultiJson` stays
//! unknown and its uses stay errors. So does an adapter set anywhere but an
//! initializer's top level, and an `Oj.default_options` that sets more than
//! the mode (`float_precision:`, `escape_mode:`, … reach the dump).
//!
//! `MultiJson.load` (and `parse`, `decode`) is oj's strict load: `JSON.parse`
//! where that answers alike, and otherwise a parser written as oj reads, for
//! the text where they differ (a decimal of more than 16 digits is a
//! BigDecimal, `01` is 1, a sign alone is 0, a lone surrogate raises). Its
//! options are `symbolize_keys:`/`symbolize_names:` alone, and a call under a
//! `rescue` that reads the exception is refused: oj's messages are not the
//! stand-in's. Each other call is a refusal of its own, through
//! `survey::continue_or_fail`.
//!
//! Checked against multi_json 1.21.1 and oj 3.17.6 with ActiveSupport.
//!
//! Ruby, JRuby and Spinel emit this class (`lower::oj` gates both stand-ins).

use crate::App;
use crate::dialect::{LibraryClass, LibraryClassOrigin};

use super::IngestError;
use super::survey;
use crate::ingest::prism_api::PrismLocation;

const RECEIVERS: [&str; 4] = ["MultiJson", "::MultiJson", "MultiJSON", "::MultiJSON"];

pub(super) fn lower_multi_json(
    app: &mut App,
    sources: &[crate::span::SourceFile],
    initializers: &[String],
) -> super::IngestResult<()> {
    if !sources.iter().any(|s| s.text.contains("MultiJson") || s.text.contains("MultiJSON")) {
        return Ok(());
    }
    let Some(lock) = app.gem_lock.as_ref() else { return Ok(()) };
    if !lock.has("multi_json") {
        return Ok(());
    }
    let refuse = |message: String| IngestError::Unsupported { file: "config/initializers".into(), message };
    let mut adapters = Vec::new();
    let mut refusals = Vec::new();
    for text in initializers.iter().filter(|t| t.contains("MultiJson") || t.contains("MultiJSON") || t.contains("Oj")) {
        for call in configuration_calls(text) {
            match call {
                Configuration::Adapter { name, top: true } => adapters.push(name),
                Configuration::Adapter { name, top: false } => refusals.push(refuse(format!(
                    "MultiJson not lowered: adapter `{name}` not chosen at an initializer's top level"
                ))),
                Configuration::Other(what) => refusals.push(refuse(format!("MultiJson not lowered: {what}"))),
            }
        }
    }
    if adapters.is_empty() && refusals.is_empty() {
        return Ok(());
    }
    if lock.version_of("multi_json").is_none_or(|v| !v.starts_with("1.21.")) {
        return survey::continue_or_fail(refuse(format!(
            "MultiJson not lowered: multi_json {} is not the version modeled (1.21)",
            lock.version_of("multi_json").unwrap_or("?")
        )));
    }
    if !lock.has("oj") || adapters.iter().any(|a| a != "oj") || adapters.len() != 1 {
        refusals.push(refuse(format!("MultiJson not lowered: adapter {adapters:?}; only the oj adapter is modeled")));
    }
    if !refusals.is_empty() {
        for err in refusals {
            survey::continue_or_fail(err)?;
        }
        return Ok(());
    }
    for source in sources.iter().filter(|s| s.text.contains("MultiJson") || s.text.contains("MultiJSON")) {
        let initializer = source.path.starts_with("config/initializers/") || source.path.contains("/config/initializers/");
        for reason in unmodeled_calls(&source.text, initializer) {
            survey::continue_or_fail(IngestError::Unsupported {
                file: source.path.clone(),
                message: format!("MultiJson not lowered: {reason}"),
            })?;
        }
    }
    if app.library_classes.iter().any(|lc| lc.name.0.as_str() == "MultiJson") {
        return Ok(());
    }
    app.library_classes.extend(stand_ins());
    Ok(())
}

enum Configuration {
    Adapter { name: String, top: bool },
    Other(String),
}

fn text_of(node: &ruby_prism::Node<'_>) -> String {
    String::from_utf8_lossy(node.location().as_slice()).to_string()
}

fn literal_name(node: &ruby_prism::Node<'_>) -> Option<String> {
    if let Some(sym) = node.as_symbol_node() {
        return Some(String::from_utf8_lossy(sym.unescaped()).to_string());
    }
    node.as_string_node().map(|s| String::from_utf8_lossy(s.unescaped()).to_string())
}

/// What an initializer configures of multi_json and of the oj it dumps with.
fn configuration_calls(text: &str) -> Vec<Configuration> {
    struct Finder {
        // Whole ranges: a modifier (`MultiJson.use(:oj) if …`) starts where its call does.
        top: Vec<(usize, usize)>,
        found: Vec<Configuration>,
    }
    impl<'pr> ruby_prism::Visit<'pr> for Finder {
        fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
            let receiver = node.receiver().map(|r| text_of(&r)).unwrap_or_default();
            let method = String::from_utf8_lossy(node.name().as_slice()).to_string();
            let args: Vec<ruby_prism::Node<'_>> =
                node.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
            if RECEIVERS.contains(&receiver.as_str()) {
                match method.as_str() {
                    "use" | "adapter=" | "engine=" => {
                        let at = (node.location().start_offset(), node.location().end_offset());
                        let name = match args.as_slice() {
                            [arg] if node.block().is_none() => literal_name(arg),
                            _ => None,
                        };
                        self.found.push(match name {
                            Some(name) => Configuration::Adapter { name, top: self.top.contains(&at) },
                            None => Configuration::Other(format!("`{}`", text_of(&node.as_node()))),
                        });
                    }
                    "dump_options=" | "load_options=" | "default_options=" | "generate_options=" | "parse_options=" | "default_adapter=" => {
                        self.found.push(Configuration::Other(format!("`{receiver}.{method}`")));
                    }
                    _ => {}
                }
            } else if matches!(receiver.as_str(), "Oj" | "::Oj") && method == "default_options=" {
                // Only the mode, which the adapter's own `mode: :compat` overrides on every dump.
                let options = args.into_iter().next().and_then(|a| match a.as_call_node() {
                    Some(merge) if String::from_utf8_lossy(merge.name().as_slice()) == "merge"
                        && merge.receiver().is_some_and(|r| matches!(text_of(&r).as_str(), "Oj.default_options" | "::Oj.default_options")) =>
                        // One operand: a second merged after it may set more than the mode.
                        merge.arguments().and_then(|m| {
                            let operands: Vec<ruby_prism::Node<'_>> = m.arguments().iter().collect();
                            if operands.len() == 1 { operands.into_iter().next() } else { None }
                        }),
                    _ => Some(a),
                });
                let only_mode = options.as_ref().is_some_and(|o| {
                    let elements: Vec<ruby_prism::Node<'_>> = if let Some(h) = o.as_keyword_hash_node() {
                        h.elements().iter().collect()
                    } else if let Some(h) = o.as_hash_node() {
                        h.elements().iter().collect()
                    } else {
                        return false;
                    };
                    elements.iter().all(|e| e.as_assoc_node().is_some_and(|p| literal_name(&p.key()).as_deref() == Some("mode")))
                });
                if !only_mode {
                    self.found.push(Configuration::Other(format!("`{}` sets more than the mode", text_of(&node.as_node()))));
                }
            }
            ruby_prism::visit_call_node(self, node);
        }
    }
    let parsed = super::prism::parse_silent(text.as_bytes());
    let root = parsed.node();
    let top: Vec<(usize, usize)> = root
        .as_program_node()
        .map(|p| p.statements().body().iter().map(|s| (s.location().start_offset(), s.location().end_offset())).collect())
        .unwrap_or_default();
    let mut finder = Finder { top, found: Vec::new() };
    ruby_prism::Visit::visit(&mut finder, &root);
    finder.found
}

/// The `MultiJson.*` calls in `text` this does not model, described.
fn unmodeled_calls(text: &str, initializer: bool) -> Vec<String> {
    struct Finder {
        found: Vec<String>,
        initializer: bool,
        /// Inside a `begin` whose `rescue` reads the exception it binds (or `$!`).
        observed: usize,
        /// Every local read in the file, with where; and the methods' ranges, innermost last.
        reads: Vec<(String, usize)>,
        defs: Vec<(usize, usize)>,
    }
    impl<'pr> ruby_prism::Visit<'pr> for Finder {
        fn visit_def_node(&mut self, node: &ruby_prism::DefNode<'pr>) {
            self.defs.push((node.location().start_offset(), node.location().end_offset()));
            ruby_prism::visit_def_node(self, node);
            self.defs.pop();
        }
        fn visit_rescue_modifier_node(&mut self, node: &ruby_prism::RescueModifierNode<'pr>) {
            struct Bang(bool);
            impl<'pr> ruby_prism::Visit<'pr> for Bang {
                fn visit_global_variable_read_node(&mut self, node: &ruby_prism::GlobalVariableReadNode<'pr>) {
                    self.0 |= node.name().as_slice() == b"$!";
                }
            }
            let mut bang = Bang(false);
            ruby_prism::Visit::visit(&mut bang, &node.rescue_expression());
            self.observed += usize::from(bang.0);
            ruby_prism::Visit::visit(self, &node.expression());
            self.observed -= usize::from(bang.0);
            ruby_prism::Visit::visit(self, &node.rescue_expression());
        }
        fn visit_begin_node(&mut self, node: &ruby_prism::BeginNode<'pr>) {
            // The exception a rescue binds is read in its body, or anywhere after it the variable lives.
            let (from, to) = (node.location().start_offset(), self.defs.last().map_or(usize::MAX, |d| d.1));
            let later = |name: &str| self.reads.iter().any(|(n, at)| n == name && *at > from && *at < to);
            let reads = node.rescue_clause().is_some_and(|r| rescue_reads_exception(&r) || rescue_bindings(&r).iter().any(|n| later(n)));
            // An `ensure` reading `$!` sees what the body, a rescue or the else raises on its way out.
            let ensure_reads = node.ensure_clause().and_then(|e| e.statements()).is_some_and(|s| reads_bang(&s.as_node()));
            if let Some(statements) = node.statements() {
                self.observed += usize::from(reads || ensure_reads);
                ruby_prism::Visit::visit_statements_node(self, &statements);
                self.observed -= usize::from(reads || ensure_reads);
            }
            self.observed += usize::from(ensure_reads);
            if let Some(r) = node.rescue_clause() {
                ruby_prism::Visit::visit_rescue_node(self, &r);
            }
            if let Some(e) = node.else_clause() {
                ruby_prism::Visit::visit_else_node(self, &e);
            }
            self.observed -= usize::from(ensure_reads);
            if let Some(e) = node.ensure_clause() {
                ruby_prism::Visit::visit_ensure_node(self, &e);
            }
        }
        fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
            let receiver = node.receiver().map(|r| text_of(&r)).unwrap_or_default();
            if RECEIVERS.contains(&receiver.as_str()) {
                let method = String::from_utf8_lossy(node.name().as_slice()).to_string();
                let args: Vec<ruby_prism::Node<'_>> =
                    node.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
                match method.as_str() {
                    // The initializer's own call, read above; anywhere else it would switch the adapter as the app runs.
                    "use" | "adapter=" | "engine=" if self.initializer => {}
                    // The stand-in is `MultiJson` only: the analysis resolves no constant alias in a stand-in.
                    _ if receiver.trim_start_matches("::") == "MultiJSON" => {
                        self.found.push(format!("`{receiver}.{method}`: only the `MultiJson` spelling is lowered"));
                    }
                    "dump" | "generate" | "encode" => {
                        // The object is the first argument, a braceless Hash included; a second is the options.
                        let options_ok = match args.get(1) {
                            None => true,
                            Some(options) => {
                                let elements: Option<Vec<ruby_prism::Node<'_>>> = options
                                    .as_keyword_hash_node()
                                    .map(|h| h.elements().iter().collect())
                                    .or_else(|| options.as_hash_node().map(|h| h.elements().iter().collect()));
                                elements.is_some_and(|elements| {
                                    elements.iter().all(|e| e.as_assoc_node().is_some_and(|p| literal_name(&p.key()).as_deref() == Some("pretty")))
                                })
                            }
                        };
                        if args.is_empty() || args.len() > 2 || !options_ok || node.block().is_some() {
                            self.found.push(format!("`{}`", text_of(&node.as_node()).lines().next().unwrap_or_default()));
                        }
                    }
                    "load" | "decode" | "parse" => {
                        // The options the stand-in reads: the keys' spelling, as literals.
                        let options_ok = match args.get(1) {
                            None => true,
                            Some(options) => {
                                let elements: Option<Vec<ruby_prism::Node<'_>>> = options
                                    .as_keyword_hash_node()
                                    .map(|h| h.elements().iter().collect())
                                    .or_else(|| options.as_hash_node().map(|h| h.elements().iter().collect()));
                                elements.is_some_and(|elements| {
                                    elements.iter().all(|e| {
                                        e.as_assoc_node().is_some_and(|p| {
                                            matches!(literal_name(&p.key()).as_deref(), Some("symbolize_keys" | "symbolize_names"))
                                                && (p.value().as_true_node().is_some() || p.value().as_false_node().is_some())
                                        })
                                    })
                                })
                            }
                        };
                        if args.is_empty() || args.len() > 2 || !options_ok || node.block().is_some() {
                            self.found.push(format!("`{}`", text_of(&node.as_node()).lines().next().unwrap_or_default()));
                        } else if self.observed > 0 {
                            self.found.push(format!(
                                "`{receiver}.{method}` under a `rescue` that reads the exception: the stand-in's ParseError message is not oj's"
                            ));
                        }
                    }
                    other => self.found.push(format!("`{receiver}.{other}`")),
                }
            }
            ruby_prism::visit_call_node(self, node);
        }
    }
    let parsed = super::prism::parse_silent(text.as_bytes());
    struct Reads(Vec<(String, usize)>);
    impl<'pr> ruby_prism::Visit<'pr> for Reads {
        fn visit_local_variable_read_node(&mut self, node: &ruby_prism::LocalVariableReadNode<'pr>) {
            self.0.push((String::from_utf8_lossy(node.name().as_slice()).to_string(), node.location().start_offset()));
        }
    }
    let parsed_reads = super::prism::parse_silent(text.as_bytes());
    let mut reads = Reads(Vec::new());
    ruby_prism::Visit::visit(&mut reads, &parsed_reads.node());
    let mut finder = Finder { found: Vec::new(), initializer, observed: 0, reads: reads.0, defs: Vec::new() };
    ruby_prism::Visit::visit(&mut finder, &parsed.node());
    finder.found
}

/// The variables a `rescue` chain binds the exception to.
fn rescue_bindings(node: &ruby_prism::RescueNode<'_>) -> Vec<String> {
    let name = |r: &ruby_prism::RescueNode<'_>| {
        r.reference().and_then(|t| t.as_local_variable_target_node().map(|t| String::from_utf8_lossy(t.name().as_slice()).to_string()))
    };
    let mut out: Vec<String> = name(node).into_iter().collect();
    let mut next = node.subsequent();
    while let Some(r) = next {
        out.extend(name(&r));
        next = r.subsequent();
    }
    out
}

/// Whether a `rescue` chain reads the exception: the variable one binds, or `$!`.
fn reads_bang(node: &ruby_prism::Node<'_>) -> bool {
    struct Bang(bool);
    impl<'pr> ruby_prism::Visit<'pr> for Bang {
        fn visit_global_variable_read_node(&mut self, node: &ruby_prism::GlobalVariableReadNode<'pr>) {
            self.0 |= node.name().as_slice() == b"$!";
        }
    }
    let mut bang = Bang(false);
    ruby_prism::Visit::visit(&mut bang, node);
    bang.0
}

fn rescue_reads_exception(node: &ruby_prism::RescueNode<'_>) -> bool {
    struct Reads(Vec<String>, bool);
    impl<'pr> ruby_prism::Visit<'pr> for Reads {
        fn visit_local_variable_read_node(&mut self, node: &ruby_prism::LocalVariableReadNode<'pr>) {
            self.1 |= self.0.contains(&String::from_utf8_lossy(node.name().as_slice()).to_string());
        }
        fn visit_global_variable_read_node(&mut self, node: &ruby_prism::GlobalVariableReadNode<'pr>) {
            self.1 |= node.name().as_slice() == b"$!";
        }
    }
    fn one(r: &ruby_prism::RescueNode<'_>) -> bool {
        let bound: Vec<String> = r
            .reference()
            .and_then(|t| t.as_local_variable_target_node().map(|t| String::from_utf8_lossy(t.name().as_slice()).to_string()))
            .into_iter()
            .collect();
        let mut visitor = Reads(bound, false);
        if let Some(statements) = r.statements() {
            ruby_prism::Visit::visit_statements_node(&mut visitor, &statements);
        }
        visitor.1
    }
    let mut reads = one(node);
    let mut next = node.subsequent();
    while let Some(r) = next {
        reads |= one(&r);
        next = r.subsequent();
    }
    reads
}

/// What the lowered calls reach once multi_json is not there.
fn stand_ins() -> Vec<LibraryClass> {
    const SOURCE: &str = r#"module MultiJson
  # Not `LoadError = ParseError` as the gem has it: the analysis resolves no constant alias in a stand-in. The gem
  # raises only ParseError, which a rescue of either name still catches.
  class DecodeError < StandardError
  end

  class LoadError < DecodeError
  end

  class ParseError < LoadError
  end

  def self.dump(object, options = {})
    options.key?(:pretty) ? MultiJson.pretty(object, "") : MultiJson.compact(object)
  end

  def self.generate(object, options = {})
    options.key?(:pretty) ? MultiJson.pretty(object, "") : MultiJson.compact(object)
  end

  def self.encode(object, options = {})
    options.key?(:pretty) ? MultiJson.pretty(object, "") : MultiJson.compact(object)
  end

  def self.compact(value)
    case value
    when Hash
      parts = []
      value.each { |key, item| parts << JSON.generate(key.to_s) + ":" + MultiJson.compact(item) }
      "{" + parts.join(",") + "}"
    when Array then "[" + value.map { |item| MultiJson.compact(item) }.join(",") + "]"
    when String then JSON.generate(value)
    when Symbol then JSON.generate(value.to_s)
    when Float then MultiJson.float(value)
    when Integer then value.to_s
    when nil then "null"
    when true then "true"
    when false then "false"
    when Time then JSON.generate(ActiveSupport::JSON.time_json(value))
    else raise ArgumentError, "MultiJson.dump: #{value.class} values are not supported yet"
    end
  end

  def self.pretty(value, indent)
    inner = indent + "  "
    case value
    when Hash
      return "{}" if value.empty?
      parts = []
      value.each { |key, item| parts << inner + JSON.generate(key.to_s) + ": " + MultiJson.pretty(item, inner) }
      "{\n" + parts.join(",\n") + "\n" + indent + "}"
    when Array
      return "[]" if value.empty?
      "[\n" + value.map { |item| inner + MultiJson.pretty(item, inner) }.join(",\n") + "\n" + indent + "]"
    else MultiJson.compact(value)
    end
  end

  def self.float(value)
    return "NaN" if value.nan?
    return (value > 0 ? "Infinity" : "-Infinity") if value.infinite?
    return "0.0" if value == 0.0
    # Not `to_i`: 2**63 itself is written whole, and it does not fit an Integer every target has.
    return format("%.1f", value) if value == value.floor && value.abs <= 9223372036854775808.0
    written = format("%.16g", value)
    # oj's own fallback: a 17-character result ending so is a rounding artefact, and Ruby's shortest form is written.
    written.length >= 17 && (written.end_with?("0001") || written.end_with?("9999")) ? value.to_s : written
  end

  def self.use(adapter)
    nil
  end

  def self.load(source, options = {})
    source = source.read if source.respond_to?(:read)
    # Not `to_s`: the adapter's own blank check asks `empty?`, which an Integer answers with NoMethodError.
    return nil if source.nil? || source.empty?
    text = source
    # Bytes, not characters: oj takes a String's invalid UTF-8 as it is.
    bytes = text.b
    return nil if text.empty? || (!text.start_with?("{", "[") && bytes.match?(/\A\s*\z/))
    symbolize = options[:symbolize_names] == true || (!options.key?(:symbolize_names) && options[:symbolize_keys] == true)
    # Not JSON.parse alone: oj's strict mode reads `01` as 1, a decimal of more than 16 digits as a BigDecimal, and
    # refuses a lone surrogate; each sends the text to the parser below, which reads it as oj does.
    unless MultiJson.long_decimal?(bytes) || bytes.match?(/\\u[dD][89a-fA-F]/) || MultiJson.commented?(bytes)
      begin
        return JSON.parse(text, symbolize_names: symbolize, max_nesting: false)
      rescue JSON::ParserError
        nil
      end
    end
    MultiJson::Strict.new(text, symbolize).document
  end

  # A number oj reads as a BigDecimal: a fraction or exponent with more than 16 digits leading zeros aside, or an
  # exponent of 100000 or more (a String's text counts too).
  def self.long_decimal?(text)
    return true if text.match?(/[eE][-+]?0*[1-9]\d{5}/)
    return false unless text.match?(/\d(?:\.?\d){16}/)
    text.scan(/\d+(?:\.\d+)?(?:[eE][-+]?\d+)?/).any? do |number|
      (number.include?(".") || number.include?("e") || number.include?("E")) &&
        number.sub(/[eE].*\z/, "").delete(".").sub(/\A0+/, "").length > 16
    end
  end

  # A comment outside the strings: oj reads one only ahead of a value, where JSON.parse takes one anywhere.
  def self.commented?(text)
    return false unless text.include?("/")
    text.scan(/"(?:[^"\\]|\\.)*"|\/[\/*]/).any? { |token| token.start_with?("/") }
  end

  def self.parse(source, options = {})
    MultiJson.load(source, options)
  end

  def self.decode(source, options = {})
    MultiJson.load(source, options)
  end

  class Strict
    def initialize(text, symbolize)
      @text = text
      @at = 0
      @size = text.bytesize
      @symbolize = symbolize
    end

    def document
      blank
      return nil if @at >= @size
      value = read_value
      # Whitespace alone after the value: oj reads no comment there.
      @at += 1 while @at < @size && space?(byte)
      raise MultiJson::ParseError, "unexpected characters after the JSON document" if @at < @size
      value
    end

    def space?(b)
      b == 32 || b == 9 || b == 10 || b == 13 || b == 12
    end

    def byte(offset = 0)
      @text.getbyte(@at + offset)
    end

    def digit?(b)
      !b.nil? && b >= 48 && b <= 57
    end

    def blank
      while @at < @size
        b = byte
        if space?(b)
          @at += 1
        elsif b == 47 && byte(1) == 47
          @at += 2
          @at += 1 while @at < @size && byte != 10 && byte != 13 && byte != 12 && byte != 0
        elsif b == 47 && byte(1) == 42
          # Not refused unclosed: oj reads it to the end.
          close = @text.byteindex("*/", @at + 2)
          @at = close.nil? ? @size : close + 2
        elsif b == 47
          raise MultiJson::ParseError, "invalid comment format"
        else
          break
        end
      end
    end

    def read_value
      b = byte
      if b == 123 then read_object
      elsif b == 91 then read_array
      elsif b == 34 then read_string
      elsif b == 116 then read_word("true", true)
      elsif b == 102 then read_word("false", false)
      elsif b == 110 then read_word("null", nil)
      elsif b == 45 || digit?(b) then read_number
      else raise MultiJson::ParseError, "unexpected character"
      end
    end

    def read_word(word, value)
      raise MultiJson::ParseError, "not a number or other value" unless @text.byteslice(@at, word.bytesize) == word
      @at += word.bytesize
      value
    end

    def read_number
      start = @at
      @at += 1 if byte == 45
      digits = @at
      @at += 1 while digit?(byte)
      if @at == digits
        # oj reads a sign with no digits after it as 0, unless a fraction or exponent follows.
        raise MultiJson::ParseError, "not a number" if byte == 46 || byte == 101 || byte == 69
        return 0
      end
      decimal = false
      if byte == 46
        decimal = true
        @at += 1
        fraction = @at
        @at += 1 while digit?(byte)
        raise MultiJson::ParseError, "not a number" if @at == fraction
      end
      if byte == 101 || byte == 69
        decimal = true
        @at += 1
        @at += 1 if byte == 43 || byte == 45
        exponent = @at
        @at += 1 while digit?(byte)
        raise MultiJson::ParseError, "not a number" if @at == exponent
      end
      written = @text.byteslice(start, @at - start)
      return Integer(written, 10) unless decimal
      significant = written.sub(/[eE].*\z/, "").delete("-.").sub(/\A0+/, "")
      power = written.include?("e") || written.include?("E") ? written.sub(/\A.*[eE][-+]?/, "").sub(/\A0+/, "") : ""
      significant.length > 16 || power.length > 5 ? BigDecimal(written) : Float(written)
    end

    def hex4(offset)
      digits = @text.byteslice(offset, 4)
      raise MultiJson::ParseError, "invalid hex character" unless digits && digits.match?(/\A\h{4}\z/)
      digits.to_i(16)
    end

    def read_string
      @at += 1
      out = +""
      segment = @at
      while true
        b = byte
        raise MultiJson::ParseError, "quoted string not terminated" if b.nil?
        raise MultiJson::ParseError, "NULL byte in string" if b == 0
        if b == 34
          out << @text.byteslice(segment, @at - segment)
          @at += 1
          return out
        end
        unless b == 92
          @at += 1
          next
        end
        out << @text.byteslice(segment, @at - segment)
        escape = byte(1)
        @at += 2
        if escape == 34 then out << "\""
        elsif escape == 92 then out << "\\"
        elsif escape == 47 then out << "/"
        elsif escape == 98 then out << "\b"
        elsif escape == 102 then out << "\f"
        elsif escape == 110 then out << "\n"
        elsif escape == 114 then out << "\r"
        elsif escape == 116 then out << "\t"
        elsif escape == 117
          code = hex4(@at)
          @at += 4
          # Not checked for a low half: oj takes any `\u` after one from the surrogate range and keeps its low ten bits.
          if code >= 0xD800 && code <= 0xDFFF
            raise MultiJson::ParseError, "invalid escaped character" unless byte == 92 && byte(1) == 117
            low = hex4(@at + 2)
            @at += 6
            code = ((((code - 0xD800) & 0x3FF) << 10) | ((low - 0xDC00) & 0x3FF)) + 0x10000
          end
          out << code.chr(Encoding::UTF_8)
        else
          raise MultiJson::ParseError, "invalid escaped character"
        end
        segment = @at
      end
    end

    def read_object
      @at += 1
      hash = {}
      blank
      if byte == 125
        @at += 1
        return hash
      end
      while true
        blank
        raise MultiJson::ParseError, "expected hash key" unless byte == 34
        key = read_string
        blank
        raise MultiJson::ParseError, "expected colon" unless byte == 58
        @at += 1
        blank
        value = read_value
        hash[@symbolize ? key.to_sym : key] = value
        blank
        b = byte
        @at += 1
        return hash if b == 125
        raise MultiJson::ParseError, "expected comma or hash close" unless b == 44
      end
    end

    def read_array
      @at += 1
      array = []
      blank
      if byte == 93
        @at += 1
        return array
      end
      while true
        blank
        array << read_value
        blank
        b = byte
        @at += 1
        return array if b == 93
        raise MultiJson::ParseError, "expected comma or array close" unless b == 44
      end
    end
  end
end
"#;
    let mut classes =
        crate::ingest::ingest_library_classes(SOURCE.as_bytes(), "<multi_json>").expect("the MultiJson stand-ins parse");
    for c in &mut classes {
        c.origin = Some(LibraryClassOrigin::MultiJson);
    }
    classes
}
