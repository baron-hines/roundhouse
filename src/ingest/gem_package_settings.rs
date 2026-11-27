//! `Rack::MiniProfiler.config.position = "bottom-left"` in a
//! `config/initializers/` file: a literal attribute write on the config
//! receiver of a gem the Spinel tree carries as a package
//! (`crate::gems::GEM_PACKAGES`). The statement is kept verbatim and
//! replayed after the package loads (`project::apply_gem_packages`), where
//! the package defines the receiver.

use ruby_prism::Node;

use crate::app::GemPackageSetting;

pub(crate) fn extract(source: &[u8], file: &str) -> Vec<GemPackageSetting> {
    let result = super::prism::parse(source, file);
    let root = result.node();
    let Some(program) = root.as_program_node() else { return Vec::new() };
    super::app::initializer_statements(&program).iter().filter_map(|stmt| setting(stmt, file)).collect()
}

fn setting(stmt: &Node<'_>, file: &str) -> Option<GemPackageSetting> {
    let call = stmt.as_call_node()?;
    let name = super::util::constant_id_str(&call.name());
    if !name.ends_with('=') || matches!(name, "==" | "!=" | "===" | "<=" | ">=") {
        return None;
    }
    let receiver = call.receiver()?;
    let receiver = String::from_utf8_lossy(receiver.location().as_slice()).trim_start_matches("::").to_string();
    let package = crate::gems::GEM_PACKAGES.iter().find(|p| p.config_receiver == Some(receiver.as_str()))?;
    let args: Vec<Node<'_>> = call.arguments()?.arguments().iter().collect();
    let [value] = args.as_slice() else { return None };
    // Not any expression: a literal cannot reach anything the Spinel tree lacks, and the package's own types decide whether it fits.
    if !literal(value) {
        return None;
    }
    let loc = stmt.location();
    let (start, end) = super::prism_api::span_offsets(&loc);
    Some(GemPackageSetting {
        gem: package.gem.to_string(),
        setter: name.trim_end_matches('=').to_string(),
        statement: String::from_utf8_lossy(loc.as_slice()).into_owned(),
        span: crate::span::Span { file: super::sources::file_id(file), start, end },
    })
}

impl GemPackageSetting {
    pub(crate) fn carried(&self) -> bool {
        crate::gems::GEM_PACKAGES.iter().any(|p| p.gem == self.gem && p.config_setters.contains(&self.setter.as_str()))
    }
}

fn literal(node: &Node<'_>) -> bool {
    if let Some(array) = node.as_array_node() {
        return array.elements().iter().all(|e| literal(&e));
    }
    node.as_string_node().is_some()
        || node.as_symbol_node().is_some()
        || node.as_true_node().is_some()
        || node.as_false_node().is_some()
        || node.as_integer_node().is_some()
        || node.as_float_node().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statements(src: &str) -> Vec<String> {
        extract(src.as_bytes(), "<test>").into_iter().map(|s| s.statement).collect()
    }

    #[test]
    fn literal_writes_on_a_package_config_are_kept_verbatim() {
        let src = "Rack::MiniProfiler.config.disable_caching = false\nRack::MiniProfiler.config.position = \"bottom-left\"\n";
        assert_eq!(
            statements(src),
            ["Rack::MiniProfiler.config.disable_caching = false", "Rack::MiniProfiler.config.position = \"bottom-left\""]
        );
        assert_eq!(extract(src.as_bytes(), "<test>")[0].gem, "rack-mini-profiler");
    }

    #[test]
    fn a_setting_the_package_has_no_writer_for_is_not_carried() {
        let found = extract(b"Rack::MiniProfiler.config.skip_schema_queries = true\n", "<test>");
        assert_eq!(found[0].setter, "skip_schema_queries");
        assert!(!found[0].carried());
        assert!(extract(b"Rack::MiniProfiler.config.position = \"bottom-left\"\n", "<test>")[0].carried());
    }

    #[test]
    fn a_computed_value_or_another_receiver_is_not_a_setting() {
        assert!(statements("Rack::MiniProfiler.config.position = ENV[\"POS\"]\n").is_empty());
        assert!(statements("Rack::MiniProfiler.config.position = \"#{side}-left\"\n").is_empty());
        assert!(statements("Other.config.position = \"bottom-left\"\n").is_empty());
        assert!(statements("Rack::MiniProfiler.config.skip_paths << \"/x\"\n").is_empty());
    }
}
