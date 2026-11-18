//! `config/database.yml`, read when Roundhouse builds the app: each
//! environment's database configurations (`database_configurations`, see
//! [`crate::database`]) and the primary's SQLite `pragmas:`
//! (`database_pragmas`).
//!
//! ## Configurations
//!
//! As Rails' `ActiveRecord::DatabaseConfigurations` builds them: each
//! top-level key is an environment; an environment whose values are all
//! mappings names its configurations (three-tier, `production: {
//! primary: ..., cache: ... }`), any other is the one configuration
//! `primary` (two-tier). The primary is the configuration named
//! `primary`, else the environment's first. YAML anchors and merge keys
//! are applied. Only the keys that decide identity are read: `adapter`
//! (or the scheme of a `url`, then a literal `adapter` beside a computed
//! `url`), `replica`, `database_tasks`,
//! `migrations_paths` and `schema_search_path`.
//!
//! Resolution is at build time, so ERB is not run. An output tag
//! (`<%= ... %>`) in a connection value (a password, a pool size) does
//! not matter here. In one of the keys above it leaves that
//! configuration out, with a warning naming the key; a control tag
//! (`<% if ... %>`), which can change which configurations exist, leaves
//! them all out. `DATABASE_URL`, which Rails merges at boot, is not read.
//!
//! ## Pragmas
//!
//! Rails' SQLite adapter (activerecord 8.1,
//! `SQLite3Adapter#configure_connection`) runs, on every connection it
//! opens, its `DEFAULT_PRAGMAS` merged with the config's own:
//!
//! ```ruby
//! pragmas = @config.fetch(:pragmas, {}).stringify_keys
//! DEFAULT_PRAGMAS.merge(pragmas).each do |pragma, value|
//!   if ::SQLite3::Pragmas.method_defined?("#{pragma}=")
//!     @raw_connection.public_send("#{pragma}=", value)
//!   else
//!     warn "Unknown SQLite pragma: #{pragma}"
//!   end
//! end
//! ```
//!
//! Each Db shim pins its own defaults (journal_mode, synchronous,
//! foreign_keys, ...) when it opens a connection; this lifts the app's
//! additions so the shim can run them after its own, as the merge lets
//! the app's value win. Campfire sets `wal_autocheckpoint: 0` under its
//! `default:` anchor so commits never checkpoint on the writer.
//!
//! Only the environment's `primary` database is read (a two-tier
//! `test: { primary: ... }` or the flat `test: { adapter: ... }`): the
//! Db shims hold one database. A value is spelled into SQL as Rails'
//! pragma setters do for the shapes taken here: an integer, a bare
//! word, or a boolean (`ON`/`OFF`). Any other value (ERB, a quoted
//! string with spaces, a nested map) is left out rather than guessed at.

use std::path::{Path, PathBuf};

use crate::database::{Database, DatabaseAdapter, DatabaseRefusal};
use crate::span::{FileId, Span};
use crate::vfs::Vfs;

/// What an ERB output tag reads as before the YAML is parsed: a plain
/// scalar that is valid in any value position and in a quoted string, so
/// a value it appears in is known to be computed.
const ERB_VALUE: &str = "__roundhouse_erb_value__";

/// The configurations of every environment, and the refusals for what
/// cannot be resolved at build time.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Configurations {
    pub databases: Vec<Database>,
    pub refusals: Vec<DatabaseRefusal>,
    /// Some `url:` is computed. Its configuration is still read from the
    /// keys beside it, but Rails takes its adapter from the URL at boot.
    pub computed_url: bool,
    /// Some configuration's `pragmas:` may set `case_sensitive_like`: a key
    /// names it (to any value, as a string or a symbol) or is computed, or
    /// `pragmas:` itself is computed or not a mapping. Rails runs a
    /// configuration's pragmas on each connection it opens, and this one
    /// changes what `LIKE` matches.
    pub case_sensitive_like: bool,
}

/// `config/database.yml`'s configurations, resolved at build time; empty
/// when the file is absent. `file` locates refusals in it.
pub(crate) fn database_configurations(text: &str, file: FileId) -> Configurations {
    use serde_yaml_ng::Value;
    let mut out = Configurations::default();
    let at = |start: usize, len: usize| Span { file, start: start as u32, end: (start + len) as u32 };
    let refuse = |out: &mut Configurations, span: Span, detail: String| {
        out.refusals.push(DatabaseRefusal { span, construct: "database_configurations".to_string(), detail, warning: true });
    };
    if let Some(start) = control_tag(text) {
        refuse(
            &mut out,
            at(start, 2),
            "config/database.yml runs ERB control flow (`<% ... %>`), which can change which configurations exist; \
             build-time resolution reads only literal YAML, with output tags (`<%= ... %>`) in connection values"
                .to_string(),
        );
        return out;
    }
    let yaml = erb_as_values(text);
    let mut doc = match serde_yaml_ng::from_str::<Value>(&yaml) {
        Ok(doc) => doc,
        Err(error) => {
            refuse(&mut out, at(0, 0), format!("config/database.yml does not parse as YAML: {error}"));
            return out;
        }
    };
    if let Err(error) = doc.apply_merge() {
        refuse(&mut out, at(0, 0), format!("config/database.yml has a merge key that does not apply: {error}"));
        return out;
    }
    let Value::Mapping(envs) = doc else { return out };
    for (env, config) in envs {
        let Value::String(env) = env else { continue };
        // A string is the configuration's URL (Rails'
        // `build_db_config_from_string`), read as `url:` would be.
        let config = match config {
            Value::Mapping(config) => config,
            Value::String(url) => serde_yaml_ng::Mapping::from_iter([(Value::String("url".to_string()), Value::String(url))]),
            _ => continue,
        };
        let env_at = key_offset(text, &env, 0, true).unwrap_or(0);
        let three_tier = !config.is_empty() && config.values().all(|value| matches!(value, Value::Mapping(_)));
        let named: Vec<(String, serde_yaml_ng::Mapping)> =
            if three_tier {
                config
                    .into_iter()
                    .filter_map(|(name, value)| match (name, value) {
                        (Value::String(name), Value::Mapping(hash)) => Some((name, hash)),
                        _ => None,
                    })
                    .collect()
            } else {
                vec![("primary".to_string(), config)]
            };
        let primary = named.iter().position(|(name, _)| name == "primary").unwrap_or(0);
        for (index, (name, hash)) in named.into_iter().enumerate() {
            // A two-tier environment's one configuration is the environment.
            let span = match three_tier.then(|| key_offset(text, &name, env_at + 1, false)).flatten() {
                Some(start) => at(start, name.len()),
                None => at(env_at, env.len()),
            };
            let label = format!("`{env}.{name}`");
            let scalar = |key: &str| hash.get(key).cloned();
            let other_pragma = |key: &Value| matches!(key, Value::String(key) if !key.contains(ERB_VALUE) && !key.trim_start_matches(':').eq_ignore_ascii_case("case_sensitive_like"));
            // ERB runs before YAML is parsed, so a computed key may be `pragmas`.
            let computed_key = hash.keys().any(|key| matches!(key, Value::String(key) if key.contains(ERB_VALUE)));
            out.case_sensitive_like |= computed_key || match scalar("pragmas").or_else(|| scalar(":pragmas")) {
                None => false,
                Some(Value::Mapping(pragmas)) => !pragmas.keys().all(other_pragma),
                Some(_) => true,
            };
            let computed = |value: &Value| match value {
                Value::String(text) => text.contains(ERB_VALUE),
                Value::Sequence(items) => items.iter().any(|item| matches!(item, Value::String(text) if text.contains(ERB_VALUE))),
                _ => false,
            };
            let mut unresolved = Vec::new();
            for key in ["adapter", "replica", "database_tasks", "migrations_paths", "schema_search_path"] {
                if scalar(key).is_some_and(|value| computed(&value)) {
                    unresolved.push(key);
                }
            }
            // A `url:` wins over the keys beside it (Rails' `UrlConfig`
            // merges the URL's hash over the configuration's).
            let adapter = match (scalar("url"), scalar("adapter")) {
                // The rest of the URL isn't known until boot. A scheme written out still wins, and without
                // one a literal `adapter:` beside it is what the file declares.
                (Some(Value::String(url)), adapter) if url.contains(ERB_VALUE) => {
                    out.computed_url = true;
                    // ERB where the query could name the adapter leaves it unknown.
                    let query_computed = DatabaseAdapter::query_pairs(&url).any(|(key, value)| key.contains(ERB_VALUE) || (key == "adapter" && value.contains(ERB_VALUE)));
                    let declared = if query_computed {
                        None
                    } else {
                        DatabaseAdapter::from_url(&url).or(match adapter {
                            Some(Value::String(adapter)) if !adapter.contains(ERB_VALUE) => Some(DatabaseAdapter::parse(&adapter)),
                            _ => None,
                        })
                    };
                    if declared.is_none() {
                        unresolved.push("url");
                    }
                    declared
                }
                (Some(Value::String(url)), adapter) => DatabaseAdapter::from_url(&url).or(match adapter {
                    Some(Value::String(adapter)) => Some(DatabaseAdapter::parse(&adapter)),
                    _ => None,
                }),
                (_, Some(Value::String(adapter))) => Some(DatabaseAdapter::parse(&adapter)),
                _ => None,
            };
            if !unresolved.is_empty() {
                let keys = unresolved.iter().map(|key| format!("`{key}`")).collect::<Vec<_>>().join(", ");
                refuse(
                    &mut out,
                    span,
                    format!("{label} computes {keys} with ERB; build-time resolution of config/database.yml reads these keys only as literals"),
                );
                continue;
            }
            let Some(adapter) = adapter else {
                refuse(&mut out, span, format!("{label} names no adapter (no `adapter:` and no `url:` with a scheme)"));
                continue;
            };
            let replica = matches!(scalar("replica"), Some(Value::Bool(true)));
            let database_tasks = !replica && !matches!(scalar("database_tasks"), Some(Value::Bool(false)));
            let migrations_paths = match scalar("migrations_paths") {
                Some(Value::String(path)) => vec![path],
                Some(Value::Sequence(paths)) => paths
                    .into_iter()
                    .filter_map(|path| match path {
                        Value::String(path) => Some(path),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let schema_search_path = match scalar("schema_search_path") {
                Some(Value::String(path)) => Some(path),
                _ => None,
            };
            out.databases.push(Database {
                env: env.clone(),
                name,
                adapter,
                primary: index == primary,
                replica,
                database_tasks,
                migrations_paths,
                schema_search_path,
            });
        }
    }
    out
}

/// The first ERB control tag: `<%` not followed by `=` (output) or `#`
/// (a comment). `<%%` is a literal `<%`.
fn control_tag(text: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = text[from..].find("<%").map(|i| i + from) {
        match text.as_bytes().get(i + 2) {
            Some(b'=' | b'#' | b'%') => from = i + 2,
            _ => return Some(i),
        }
    }
    None
}

/// The text with each ERB output tag read as [`ERB_VALUE`] and each
/// comment tag removed, ready for the YAML parser.
fn erb_as_values(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<%") {
        out.push_str(&rest[..start]);
        let tag = &rest[start..];
        if tag.starts_with("<%%") {
            out.push_str("<%");
            rest = &tag[3..];
            continue;
        }
        let Some(end) = tag.find("%>") else {
            rest = "";
            break;
        };
        if tag.starts_with("<%=") {
            out.push_str(ERB_VALUE);
        }
        rest = &tag[end + 2..];
    }
    out.push_str(rest);
    out
}

/// Where `key:` starts a line (unindented when `top_level`), at or after
/// byte `from`.
fn key_offset(text: &str, key: &str, from: usize, top_level: bool) -> Option<usize> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        let start = offset + indent;
        if start >= from && (!top_level || indent == 0) {
            let rest = line.trim_start();
            let quoted = |q: char| rest.strip_prefix(q).and_then(|r| r.strip_prefix(key)).and_then(|r| r.strip_prefix(q));
            let rest = quoted('"').or_else(|| quoted('\'')).or_else(|| rest.strip_prefix(key));
            if rest.is_some_and(|r| r.trim_start().starts_with(':')) {
                return Some(start);
            }
        }
        offset += line.len();
    }
    None
}

/// Where `schema_dump` first appears as a word (no letter, digit or `_`
/// beside it) on a line that isn't a comment line. A line starting with `#`
/// can't hold a key; anything else counts, an inline comment and a value
/// included, so a file whose `schema_dump` the parser can't read refuses.
fn mentions_schema_dump(text: &str) -> Option<usize> {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    text.match_indices("schema_dump").map(|(at, _)| at).find(|&at| {
        let line = &text[text[..at].rfind('\n').map_or(0, |n| n + 1)..at];
        !line.trim_start().starts_with('#') && !word(text[..at].chars().next_back()) && !word(text[at + "schema_dump".len()..].chars().next())
    })
}

/// A file whose test primary the parser can't establish (`reason`): any
/// mention of `schema_dump` refuses the file it may name, and a file with
/// none keeps main's behavior.
fn unresolved(text: &str, reason: &str) -> NamedSchema {
    let mut out = NamedSchema::default();
    if let Some(at) = mentions_schema_dump(text) {
        out.primary = Some(Err(()));
        let reason: String = shown(reason.lines().next().unwrap_or_default()).chars().take(120).collect();
        let detail = format!("config/database.yml couldn't be read to check `test.primary`'s `schema_dump` ({reason}), so the file it names isn't read");
        out.refusals.push(DatabaseRefusal { span: Span { file: FileId(0), start: at as u32, end: at as u32 }, construct: "schema_dump".to_string(), detail, warning: false });
    }
    out
}

/// The test environment's `schema_dump:`, read only in the one form this
/// supports: a plain or quoted YAML string written directly in the
/// primary configuration, naming a relative `.rb` or `.sql` file under
/// `db/` (Rails joins the name to `db/`). `false`, null or an unquoted
/// YAML 1.1 boolean names no file, and the schema is found as before.
/// Any other `schema_dump:` there is refused by name, as is a
/// `schema_dump` or `schema_format` in a literal `url:`'s query, which
/// Rails merges over the keys beside it.
#[derive(Default)]
pub(crate) struct NamedSchema {
    /// `Some(Ok((path, sql)))` reads that file; `Some(Err(()))` was refused.
    pub primary: Option<Result<(PathBuf, bool), ()>>,
    /// At their place in `config/database.yml`, filed by [`NamedSchema::refusals`].
    refusals: Vec<DatabaseRefusal>,
}

impl NamedSchema {
    /// The refusals and warnings, filed in `file`. A file that is `joined`
    /// (solid_queue's, whose tables ingest adds to the primary's) is not
    /// reported.
    pub(crate) fn refusals(&self, file: FileId, joined: Option<&str>) -> Vec<DatabaseRefusal> {
        let joined = joined.map(|path| format!(" {path} "));
        self.refusals
            .iter()
            .filter(|r| !r.warning || joined.as_deref().is_none_or(|path| !r.detail.contains(path)))
            .map(|r| DatabaseRefusal { span: Span { file, ..r.span }, ..r.clone() })
            .collect()
    }
}

pub(crate) fn named_schema<V: Vfs + ?Sized>(vfs: &V, dir: &Path) -> NamedSchema {
    use serde_yaml_ng::Value;
    let mut out = NamedSchema::default();
    let Ok(text) = vfs.read_to_string(&dir.join("config/database.yml")) else { return out };
    // Each ERB output tag gets its own placeholder: two computed keys would otherwise be one
    // duplicate key, the YAML would not parse, and a named file would be dropped unseen.
    let distinct = erb_as_values(&text).split(ERB_VALUE).enumerate().fold(String::new(), |mut yaml, (i, part)| {
        if i > 0 {
            yaml.push_str(&format!("{ERB_VALUE}{i}_"));
        }
        yaml.push_str(part);
        yaml
    });
    // Unparsed, or with a merge that doesn't apply: the parser's own reason
    // ("duplicate entry ...", "unknown anchor ..."), placeholders shown as ERB.
    let mut doc = match serde_yaml_ng::from_str::<Value>(&distinct) {
        Ok(doc) => doc,
        Err(error) => return unresolved(&text, &error.to_string()),
    };
    if let Err(error) = doc.apply_merge() {
        return unresolved(&text, &error.to_string());
    }
    // A key computed by ERB (`<%= "schema_dump" %>:`) could be the setting itself.
    fn computed_key(value: &Value) -> bool {
        match value {
            Value::Mapping(m) => m.iter().any(|(k, v)| k.as_str().is_some_and(|k| k.contains(ERB_VALUE)) || computed_key(v)),
            Value::Sequence(items) => items.iter().any(computed_key),
            _ => false,
        }
    }
    // Checked once the test primary's own `schema_dump` names a file.
    let computed = computed_key(&doc);
    let Value::Mapping(mut envs) = doc else { return out };
    // Without a literal `test`, a computed environment key may be it.
    if !envs.contains_key("test") && envs.keys().any(|k| k.as_str().is_some_and(|k| k.contains(ERB_VALUE))) {
        return unresolved(&text, "an environment's key is computed with ERB, and there is no literal `test`");
    }
    // An environment given as a URL string is its one configuration's `url:`.
    let env = match envs.remove("test") {
        Some(Value::Mapping(env)) => env,
        Some(Value::String(url)) => serde_yaml_ng::Mapping::from_iter([(Value::String("url".to_string()), Value::String(url))]),
        _ => return out,
    };
    // Not located in the source: no line is read as written directly, so a named file is refused.
    let env_at = key_offset(&text, "test", 0, true);
    let at = |start: usize| Span { file: FileId(0), start: start as u32, end: start as u32 };
    let three_tier = !env.is_empty() && env.values().all(|v| matches!(v, Value::Mapping(_)));
    let names: Vec<String> = env.keys().filter_map(|k| k.as_str().map(str::to_string)).collect();
    // Rails' primary is `primary`, else the first configuration. A configuration named by a non-String key can't be named here.
    let primary_name = if !three_tier || names.iter().any(|n| n == "primary") {
        "primary".to_string()
    } else if let Some(first) = names.first().filter(|_| names.len() == env.len()) {
        first.clone()
    } else {
        if env.values().any(|config| matches!(config, Value::Mapping(c) if c.keys().any(|k| k.as_str().is_some_and(|k| k.trim_start_matches(':') == "schema_dump")))) {
            out.primary = Some(Err(()));
            let detail = "`test.primary`'s `schema_dump` is not supported: the test environment names a configuration with a key that isn't a string, so its primary can't be named".to_string();
            out.refusals.push(DatabaseRefusal { span: at(env_at.unwrap_or(0)), construct: "schema_dump".to_string(), detail, warning: false });
        }
        return out;
    };
    // The lines written directly in a configuration's block: `(offset, key, value)`.
    let block = |start: Option<usize>| -> Vec<(usize, String, String)> {
        let Some(start) = start else { return Vec::new() };
        let line_end = text[start..].find('\n').map_or(text.len(), |i| start + i + 1);
        // The key's own indentation, from the physical line start: a sibling ends the block.
        let own = start - text[..start].rfind('\n').map_or(0, |i| i + 1);
        let mut lines = Vec::new();
        let mut child = None;
        let mut offset = line_end;
        for line in text[line_end..].split_inclusive('\n') {
            let indent = line.len() - line.trim_start().len();
            let body = line.trim();
            if !body.is_empty() && !body.starts_with('#') {
                if indent <= own {
                    break;
                }
                // A quoted key keeps its quotes (`':schema_dump'`), so it matches neither spelling.
                let quote = body.chars().next().filter(|c| *c == '\'' || *c == '"');
                let pair = match (quote, body.strip_prefix(':')) {
                    (Some(q), _) => body[1..].find(q).and_then(|end| body[end + 2..].strip_prefix(':').map(|v| (body[..end + 2].to_string(), v))),
                    (None, Some(rest)) => rest.split_once(':').map(|(k, v)| (format!(":{k}"), v)),
                    (None, None) => body.split_once(':').map(|(k, v)| (k.to_string(), v)),
                };
                if *child.get_or_insert(indent) == indent && let Some((key, value)) = pair {
                    lines.push((offset + indent, key, value.split(" #").next().unwrap_or_default().trim().to_string()));
                }
            }
            offset += line.len();
        }
        lines
    };
    // A configuration's key among the environment block's direct children.
    let child_key = |name: &str| -> Option<usize> {
        let env_lines = block(env_at);
        env_lines.iter().find(|(_, key, _)| key.trim_matches(|c| c == '"' || c == '\'') == name).map(|(offset, _, _)| *offset)
    };
    let configs: Vec<(String, Value, Option<usize>)> = if three_tier {
        names.iter().map(|n| (n.clone(), env[n.as_str()].clone(), child_key(n))).collect()
    } else {
        vec![(primary_name.clone(), Value::Mapping(env.clone()), env_at)]
    };
    for (name, config, start) in configs {
        let Value::Mapping(config) = config else { continue };
        let lines = block(start);
        let start = start.unwrap_or(0);
        let label = format!("`test.{name}`");
        // A quoted String key (`'schema_dump':`) is that key; `':schema_dump'` stays apart from the Symbol key.
        let unquoted = |k: &str| ['\'', '"'].iter().find_map(|q| k.strip_prefix(*q)?.strip_suffix(*q)).map(str::to_string);
        let line_of = |key: &str| lines.iter().find(|(_, k, _)| k == key || !key.starts_with(':') && unquoted(k).as_deref() == Some(key));
        let mut refuse = |offset: usize, detail: String, warning: bool| out.refusals.push(DatabaseRefusal { span: at(offset), construct: "schema_dump".to_string(), detail, warning });
        if name != primary_name {
            // Not read: the runtime opens only the primary. Reported when the file exists.
            let skip = matches!(config.get("replica"), Some(Value::Bool(true))) || matches!(config.get("database_tasks"), Some(Value::Bool(false)));
            let file = match config.get("schema_dump") {
                Some(Value::String(file)) if !file.contains(ERB_VALUE) => vec![format!("db/{file}")],
                Some(_) => Vec::new(),
                None => vec![format!("db/{name}_schema.rb"), format!("db/{name}_structure.sql")],
            };
            if let Some(file) = file.into_iter().find(|f| !skip && vfs.exists(&dir.join(f))) {
                refuse(start, format!("{label}'s schema file {file} is not read: the emitted runtime opens only the primary database, so this database's tables are not carried, and a model routed to it is refused"), true);
            }
            continue;
        }
        let unsupported = |written: &str, why: &str| format!("{label}'s `schema_dump: {written}` is not supported: schema selection reads only a plain string naming a relative .rb or .sql file under db/ ({why})");
        // A literal URL's query wins over the keys beside it: the last value of a key, unless blank.
        // Every key that reads as `url` in some spelling (`:url`, quoted, any case), after merges.
        // A named file is read only beside one plain literal one; each one's query is checked.
        let urls: Vec<&Value> = config.iter().filter(|(k, _)| k.as_str().is_some_and(|k| k.trim_start_matches(':').eq_ignore_ascii_case("url"))).map(|(_, v)| v).collect();
        let url_unread = urls.len() > 1 || urls.iter().any(|url| !matches!(url, Value::String(url) if !url.contains(ERB_VALUE)));
        for url in urls.iter().filter_map(|url| url.as_str()) {
            let query = url.split('#').next().unwrap_or_default().split_once('?').map_or("", |(_, q)| q);
            for key in ["schema_dump", "schema_format"] {
                let value = query.split('&').filter_map(|pair| { let (k, v) = pair.split_once('=').unwrap_or((pair, "")); (k == key).then_some(v) }).last();
                if value.is_some_and(|v| !v.is_empty()) {
                    out.primary = Some(Err(()));
                    refuse(line_of("url").or(line_of(":url")).map_or(start, |l| l.0), format!("{label}'s `url:` sets `{key}` in its query, which schema selection does not read"), false);
                }
            }
        }
        if let Some((offset, _, written)) = line_of(":schema_dump") {
            out.primary = Some(Err(()));
            refuse(*offset, unsupported(written, "it is written as the Symbol key `:schema_dump:`"), false);
        } else if config.contains_key(":schema_dump")
            && (lines.iter().any(|(_, k, _)| k == "<<") || !lines.iter().any(|(_, k, _)| k == "':schema_dump'" || k == "\":schema_dump\""))
        {
            // Only a String key written directly, with nothing merged beside it, is the key Rails ignores.
            out.primary = Some(Err(()));
            let written = shown(serde_yaml_ng::to_string(&config[":schema_dump"]).unwrap_or_default().trim());
            refuse(start, unsupported(&written, "it is the Symbol key `:schema_dump:`, here through a merge"), false);
        }
        let Some(value) = config.get("schema_dump") else { continue };
        if out.primary.is_some() {
            continue;
        }
        let direct = line_of("schema_dump");
        let written = direct.map_or_else(|| shown(serde_yaml_ng::to_string(value).unwrap_or_default().trim()), |l| l.2.replace(ERB_VALUE, "<%= ... %>"));
        let offset = direct.map_or(start, |l| l.0);
        let token = direct.map(|l| l.2.as_str()).unwrap_or_default();
        let plain = token.trim_matches(|c| c == '\'' || c == '"');
        let quoted = token.len() >= 2 && (token.starts_with('\'') && token.ends_with('\'') || token.starts_with('"') && token.ends_with('"'));
        // `false`, null or an unquoted YAML 1.1 boolean: no file, so the schema is found as before.
        let boolean = |name: &str| !quoted && name == token && ["yes", "no", "on", "off", "true", "false"].iter().any(|w| name.eq_ignore_ascii_case(w));
        if matches!(value, Value::Null | Value::Bool(_)) || matches!(value, Value::String(name) if boolean(name)) {
            continue;
        }
        out.primary = Some(Err(()));
        // ERB control flow can decide whether this configuration, or this key, exists at all.
        if control_tag(&text).is_some() {
            refuse(offset, unsupported(&written, "config/database.yml runs ERB control flow (`<% ... %>`), which can decide whether it applies"), false);
            continue;
        }
        if computed {
            refuse(offset, unsupported(&written, "config/database.yml computes a key with ERB, which could be the setting itself"), false);
            continue;
        }
        // Psych applies a merge where it is written, over the keys before it, and puts the
        // configurations it brings first; serde_yaml_ng keeps the written keys and their order.
        // They agree on a merge written first in the primary's block, not on one between configurations.
        if lines.iter().skip(1).any(|(_, key, _)| key == "<<") || three_tier && block(env_at).iter().any(|(_, key, _)| key == "<<") {
            refuse(offset, unsupported(&written, "a merge key (`<<:`) comes after other keys or brings configurations, and Rails applies it where it is written"), false);
            continue;
        }
        let why = match value {
            _ if direct.is_none() => Some("it is not written directly in the configuration: an anchor merge or flow mapping"),
            Value::String(name) if name.contains(ERB_VALUE) => Some("it is computed with ERB"),
            Value::String(name) if name != plain || token.starts_with(['&', '*', '!', '|', '>', '{', '[']) => Some("it is an anchor, alias, tag or block value, or a quoted name holding ` #`"),
            Value::String(name) if name.starts_with('/') || name.split('/').any(|part| part == "..") => Some("it is not a relative path inside db/"),
            Value::String(name) if !name.ends_with(".rb") && !name.ends_with(".sql") => Some("it does not end in .rb or .sql"),
            Value::String(_) => None,
            _ => Some("it is a value that names no file"),
        };
        if let Some(why) = why {
            refuse(offset, unsupported(&written, why), false);
            continue;
        }
        let name = plain;
        let path = format!("db/{name}");
        // The format Rails reads it in: a literal `schema_format:` written directly here,
        // else `:ruby` unless the app's config mentions `schema_format` at all.
        let format_lines: Vec<&(usize, String, String)> = lines.iter().filter(|(_, k, _)| k.trim_start_matches(':').trim_matches(|c| c == '"' || c == '\'') == "schema_format").collect();
        let format = match (format_lines.as_slice(), config.get("schema_format").or(config.get(":schema_format"))) {
            ([], None) => config_mentions(vfs, dir, |text| text.contains("schema_format")).map_or(Ok(".rb"), |file| Err(format!("{file} mentions `schema_format`, which can change the format Rails reads it in"))),
            ([(_, key, value)], Some(Value::String(parsed))) if key == "schema_format" && value.trim_matches(|c| c == '"' || c == '\'') == parsed => match parsed.trim_start_matches(':') {
                "ruby" => Ok(".rb"),
                "sql" => Ok(".sql"),
                _ => Err(format!("`schema_format: {value}` is not a literal `ruby` or `sql`")),
            },
            _ => Err("its `schema_format` is not a literal `ruby` or `sql` written directly in the configuration".to_string()),
        };
        if url_unread {
            refuse(offset, unsupported(&written, "the configuration's `url:` is computed, not a string, or written under more than one spelling, so its query could name another file"), false);
        } else if let Some(setter) = config_mentions(vfs, dir, mentions_paths) {
            refuse(offset, format!("{label}'s `schema_dump: {written}` is not supported: {setter} mentions `.paths`, `paths[` or `paths.add`, which can move the db directory Rails reads it from"), false);
        } else if let Err(why) = &format {
            refuse(offset, unsupported(&written, why), false);
        } else if format.as_deref() != Ok(&name[name.rfind('.').unwrap_or(0)..]) {
            refuse(offset, unsupported(&written, "its extension is not the schema format Rails reads it in (`.rb` for ruby, `.sql` for sql)"), false);
        } else if !vfs.exists(&dir.join(&path)) {
            refuse(offset, format!("{label} names its schema file {path} with `schema_dump: {written}`, which does not exist"), false);
        } else {
            out.primary = Some(Ok((dir.join(path), name.ends_with(".sql"))));
        }
    }
    out
}

/// A value as written, with each numbered ERB placeholder shown as `<%= ... %>`.
fn shown(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(at) = rest.find(ERB_VALUE) {
        out.push_str(&rest[..at]);
        out.push_str("<%= ... %>");
        let after = rest[at + ERB_VALUE.len()..].trim_start_matches(|c: char| c.is_ascii_digit());
        rest = after.strip_prefix('_').unwrap_or(after);
    }
    out.push_str(rest);
    out
}

/// The first Ruby file under `config/` whose text matches, as a path from
/// the app root. Text only, comments included: a match only ever refuses.
fn config_mentions<V: Vfs + ?Sized>(vfs: &V, dir: &Path, matches: impl Fn(&str) -> bool) -> Option<String> {
    let config = dir.join("config");
    let files = if vfs.is_dir(&config) { super::app::read_rb_files(vfs, &config).unwrap_or_default() } else { Vec::new() };
    files.into_iter().find(|file| vfs.read_to_string(file).is_ok_and(|text| matches(&text))).map(|file| file.strip_prefix(dir).unwrap_or(&file).display().to_string())
}

/// `.paths`, `paths[` or `paths.add` anywhere, on any receiver and in
/// comments too: Rails resolves `schema_dump:` in `paths["db"]`, which
/// any of them may change, through an alias or a computed key. Only
/// `assets.paths`, Sprockets' and Propshaft's load path (named in the
/// initializer `rails new` writes), is not Rails' `paths`.
fn mentions_paths(text: &str) -> bool {
    text.match_indices("paths").any(|(at, _)| {
        let before = &text[..at];
        let after = &text[at + "paths".len()..];
        let dotted = before.ends_with('.');
        let assets = before.strip_suffix('.').is_some_and(|b| b.ends_with("assets"));
        !assets && (dotted || after.starts_with('[') || after.starts_with(".add"))
    })
}

/// `(environment, ["PRAGMA name = value", ...])`, for each environment
/// whose primary config names pragmas. Empty when the file is absent or
/// names none.
pub fn database_pragmas(vfs: &dyn Vfs, dir: &Path) -> Vec<(String, Vec<String>)> {
    let path = dir.join("config/database.yml");
    let Ok(bytes) = vfs.read(&path) else { return Vec::new() };
    let Ok(text) = String::from_utf8(bytes) else { return Vec::new() };
    parse(&text)
}

fn parse(text: &str) -> Vec<(String, Vec<String>)> {
    use serde_yaml_ng::Value;
    // ERB is evaluated before the YAML is read. Values it produces are
    // not ours to know, so each tag reads as an empty scalar; a pragma
    // whose value was ERB is then dropped by `sql_value`.
    let text = strip_erb(text);
    let Ok(mut doc) = serde_yaml_ng::from_str::<Value>(&text) else { return Vec::new() };
    if doc.apply_merge().is_err() {
        return Vec::new();
    }
    let Value::Mapping(envs) = doc else { return Vec::new() };
    let mut out = Vec::new();
    for (env, config) in envs {
        let Value::String(env) = env else { continue };
        let Value::Mapping(config) = config else { continue };
        // Flat (`adapter:` at the top) or one level of named databases.
        let primary = if config.contains_key("adapter") || config.contains_key("database") {
            Some(config)
        } else {
            match config.get("primary") {
                Some(Value::Mapping(m)) => Some(m.clone()),
                _ => None,
            }
        };
        let Some(primary) = primary else { continue };
        let Some(Value::Mapping(pragmas)) = primary.get("pragmas") else { continue };
        let mut statements = Vec::new();
        for (name, value) in pragmas {
            let Value::String(name) = name else { continue };
            if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.is_empty() {
                continue;
            }
            let Some(value) = sql_value(value) else { continue };
            statements.push(format!("PRAGMA {name} = {value}"));
        }
        if !statements.is_empty() {
            out.push((env.clone(), statements));
        }
    }
    out.sort();
    out
}

fn sql_value(value: &serde_yaml_ng::Value) -> Option<String> {
    use serde_yaml_ng::Value;
    match value {
        Value::Number(n) if n.is_i64() || n.is_u64() => Some(n.to_string()),
        Value::Bool(true) => Some("ON".to_string()),
        Value::Bool(false) => Some("OFF".to_string()),
        Value::String(s) if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => {
            Some(s.clone())
        }
        _ => None,
    }
}

fn strip_erb(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<%") {
        out.push_str(&rest[..start]);
        match rest[start..].find("%>") {
            Some(end) => {
                out.push_str("\"\"");
                rest = &rest[start + end + 2..];
            }
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::{database_configurations, parse};
    use crate::database::DatabaseAdapter;
    use crate::span::FileId;

    /// `(env, name, adapter, primary, replica, database_tasks)` of each configuration.
    fn shape(yml: &str) -> Vec<(String, String, String, bool, bool, bool)> {
        database_configurations(yml, FileId(1))
            .databases
            .into_iter()
            .map(|d| (d.env, d.name, d.adapter.name().to_string(), d.primary, d.replica, d.database_tasks))
            .collect()
    }

    fn row(env: &str, name: &str, adapter: &str, primary: bool, replica: bool, tasks: bool) -> (String, String, String, bool, bool, bool) {
        (env.to_string(), name.to_string(), adapter.to_string(), primary, replica, tasks)
    }

    /// The file `rails new` writes for SQLite: two-tier development and
    /// test, and a three-tier production whose solid_* databases keep
    /// their own migrations.
    #[test]
    fn the_generated_sqlite_file_reads_as_rails_builds_it() {
        let yml = r#"
default: &default
  adapter: sqlite3
  pool: <%= ENV.fetch("RAILS_MAX_THREADS") { 5 } %>
  timeout: 5000

development:
  <<: *default
  database: storage/development.sqlite3

test:
  <<: *default
  database: storage/test.sqlite3

production:
  primary:
    <<: *default
    database: storage/production.sqlite3
  cache:
    <<: *default
    database: storage/production_cache.sqlite3
    migrations_paths: db/cache_migrate
"#;
        let configurations = database_configurations(yml, FileId(1));
        assert!(configurations.refusals.is_empty(), "{:?}", configurations.refusals);
        assert_eq!(
            shape(yml),
            vec![
                row("default", "primary", "sqlite3", true, false, true),
                row("development", "primary", "sqlite3", true, false, true),
                row("test", "primary", "sqlite3", true, false, true),
                row("production", "primary", "sqlite3", true, false, true),
                row("production", "cache", "sqlite3", false, false, true),
            ]
        );
        let cache = configurations.databases.iter().find(|d| d.name == "cache").unwrap();
        assert_eq!(cache.migrations_paths, vec!["db/cache_migrate".to_string()]);
    }

    /// Without one named `primary`, the environment's first configuration
    /// is the primary; a replica runs no database tasks.
    #[test]
    fn the_first_configuration_is_the_primary_and_a_replica_runs_no_tasks() {
        let yml = "production:\n  main:\n    adapter: postgresql\n  main_replica:\n    adapter: postgresql\n    replica: true\n  reports:\n    adapter: postgresql\n    database_tasks: false\n    schema_search_path: reporting\n";
        assert_eq!(
            shape(yml),
            vec![
                row("production", "main", "postgresql", true, false, true),
                row("production", "main_replica", "postgresql", false, true, false),
                row("production", "reports", "postgresql", false, false, false),
            ]
        );
        let reports = &database_configurations(yml, FileId(1)).databases[2];
        assert_eq!(reports.schema_search_path.as_deref(), Some("reporting"));
    }

    #[test]
    fn a_literal_url_names_the_adapter_by_its_scheme() {
        let yml = "production:\n  url: postgres://db.example.test/app\ntest:\n  adapter: sqlite3\n  url: postgresql://db.example.test/app_test\n";
        let databases = database_configurations(yml, FileId(1)).databases;
        assert_eq!(databases[0].adapter, DatabaseAdapter::Postgresql);
        // The URL wins over an `adapter:` beside it, as in Rails' `UrlConfig`.
        assert_eq!(databases[1].adapter, DatabaseAdapter::Postgresql);
    }

    /// An identity key computed by ERB leaves its configuration out, with
    /// a warning at the configuration's name naming the key; ERB in a
    /// connection value does not matter.
    #[test]
    fn erb_in_an_identity_key_leaves_the_configuration_out_with_a_warning() {
        let yml = "test:\n  primary:\n    adapter: sqlite3\n    database: <%= ENV[\"DB\"] %>\n  events:\n    adapter: <%= ENV[\"EVENTS_ADAPTER\"] %>\n  queue:\n    url: <%= ENV[\"QUEUE_URL\"] %>\n";
        let configurations = database_configurations(yml, FileId(1));
        assert_eq!(configurations.databases.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["primary"]);
        let [events, queue] = configurations.refusals.as_slice() else { panic!("{:?}", configurations.refusals) };
        assert!(events.warning && events.detail.contains("`test.events`") && events.detail.contains("`adapter`"), "{events:?}");
        assert_eq!(&yml[events.span.start as usize..events.span.end as usize], "events");
        assert!(queue.detail.contains("`test.queue`") && queue.detail.contains("`url`"), "{queue:?}");
        assert_eq!(&yml[queue.span.start as usize..queue.span.end as usize], "queue");
    }

    /// An environment that is only a URL string is that URL's
    /// configuration, as Rails builds it; a computed one is a warning.
    /// `url: <%= ENV["DATABASE_URL"] %>` beside `adapter: postgresql`: the URL isn't known at build time, and the
    /// literal adapter is what the file says it runs on.
    #[test]
    fn a_literal_adapter_beside_a_computed_url_names_the_adapter() {
        let yml = "default: &default\n  adapter: postgresql\n  url: <%= ENV[\"DATABASE_URL\"] %>\ntest:\n  <<: *default\nproduction:\n  primary:\n    <<: *default\n";
        let read = database_configurations(yml, FileId(1));
        assert!(read.refusals.is_empty(), "{:?}", read.refusals);
        assert_eq!(shape(yml), [row("default", "primary", "postgresql", true, false, true), row("test", "primary", "postgresql", true, false, true), row("production", "primary", "postgresql", true, false, true)]);
    }

    /// The scheme is written out, and a URL's scheme wins over the keys beside it.
    #[test]
    fn a_literal_scheme_in_a_computed_url_names_the_adapter() {
        for yml in [
            "test:\n  adapter: sqlite3\n  url: postgresql://<%= ENV[\"DB_HOST\"] %>/app\n",
            "test:\n  url: postgres://<%= ENV[\"DB_HOST\"] %>/app\n",
        ] {
            let read = database_configurations(yml, FileId(1));
            assert!(read.refusals.is_empty(), "{yml}: {:?}", read.refusals);
            assert_eq!(shape(yml), [row("test", "primary", "postgresql", true, false, true)], "{yml}");
        }
    }

    /// A hierarchical URL's query names the adapter over its scheme, as in Rails' `ConnectionUrlResolver`;
    /// an opaque URL's scheme stays.
    #[test]
    fn a_query_adapter_wins_over_a_hierarchical_scheme() {
        for yml in [
            "test:\n  url: postgresql://db.example.test/app?pool=5&adapter=postgis\n",
            "test:\n  url: postgresql://<%= ENV[\"DB_HOST\"] %>/app?adapter=postgis\n",
        ] {
            let read = database_configurations(yml, FileId(1));
            assert!(read.refusals.is_empty(), "{yml}: {:?}", read.refusals);
            assert_eq!(read.databases[0].adapter, DatabaseAdapter::Postgis, "{yml}");
        }
        let opaque = database_configurations("test:\n  url: sqlite3:db/app.sqlite3?adapter=postgis\n", FileId(1));
        assert_eq!(opaque.databases[0].adapter, DatabaseAdapter::Sqlite3);
    }

    /// ERB where a query could name the adapter leaves it unknown; ERB in another key's value doesn't.
    #[test]
    fn erb_where_a_query_could_name_the_adapter_stays_out() {
        for yml in [
            "test:\n  adapter: sqlite3\n  url: postgresql://db.example.test/app?adapter=<%= ENV[\"A\"] %>\n",
            "test:\n  url: postgresql://db.example.test/app?<%= ENV[\"Q\"] %>\n",
        ] {
            let read = database_configurations(yml, FileId(1));
            assert!(read.databases.is_empty(), "{yml}: {:?}", read.databases);
            assert_eq!(read.refusals.len(), 1, "{yml}");
        }
        let pool = database_configurations("test:\n  url: postgresql://db.example.test/app?pool=<%= ENV[\"P\"] %>\n", FileId(1));
        assert!(pool.refusals.is_empty(), "{:?}", pool.refusals);
        assert_eq!(pool.databases[0].adapter, DatabaseAdapter::Postgresql);
    }

    /// Only the scheme's own characters: a tag in the scheme leaves it to the literal adapter.
    #[test]
    fn a_computed_scheme_falls_back_to_the_literal_adapter() {
        let yml = "test:\n  adapter: postgresql\n  url: <%= ENV[\"DB_SCHEME\"] %>://db.example.test/app\n";
        assert_eq!(shape(yml), [row("test", "primary", "postgresql", true, false, true)]);
    }

    #[test]
    fn a_computed_url_with_no_literal_adapter_stays_out() {
        for (yml, keys) in [("test:\n  url: <%= ENV[\"DATABASE_URL\"] %>\n", vec!["`url`"]), ("test:\n  adapter: <%= ENV[\"ADAPTER\"] %>\n  url: <%= ENV[\"DATABASE_URL\"] %>\n", vec!["`adapter`", "`url`"])] {
            let read = database_configurations(yml, FileId(1));
            assert!(read.databases.is_empty(), "{yml}");
            assert!(read.refusals.iter().any(|r| keys.iter().all(|key| r.detail.contains(key))), "{yml}: {:?}", read.refusals);
        }
    }

    #[test]
    fn an_environment_given_as_a_url_string_is_its_configuration() {
        let yml = "production: postgres://db.example.test/app\nstaging: <%= ENV[\"DATABASE_URL\"] %>\n";
        let configurations = database_configurations(yml, FileId(1));
        assert_eq!(shape(yml), vec![row("production", "primary", "postgresql", true, false, true)]);
        let [staging] = configurations.refusals.as_slice() else { panic!("{:?}", configurations.refusals) };
        assert!(staging.warning && staging.detail.contains("`staging.primary`") && staging.detail.contains("`url`"), "{staging:?}");
    }

    #[test]
    fn erb_control_flow_leaves_every_configuration_out() {
        let yml = "test:\n<% if ENV[\"PG\"] %>\n  adapter: postgresql\n<% else %>\n  adapter: sqlite3\n<% end %>\n";
        let configurations = database_configurations(yml, FileId(1));
        assert!(configurations.databases.is_empty());
        let [refusal] = configurations.refusals.as_slice() else { panic!("{:?}", configurations.refusals) };
        assert!(refusal.warning && refusal.detail.contains("control flow"), "{refusal:?}");
        assert_eq!(refusal.span.start, 6);
    }

    #[test]
    fn a_configuration_naming_no_adapter_is_left_out_with_a_warning() {
        let configurations = database_configurations("test:\n  database: tmp/test.sqlite3\n", FileId(1));
        assert!(configurations.databases.is_empty());
        assert!(configurations.refusals[0].detail.contains("names no adapter"), "{:?}", configurations.refusals);
    }

    #[test]
    fn campfire_shape_reads_the_anchor_into_each_environment() {
        let yml = r#"
default: &default
  adapter: sqlite3
  pool: <%= ENV.fetch("RAILS_MAX_THREADS") { 10 } %>
  timeout: 5000
  pragmas:
    wal_autocheckpoint: 0

development:
  primary:
    <<: *default
    database: storage/db/development.sqlite3

test:
  <<: *default
  database: storage/db/test.sqlite3
"#;
        let got = parse(yml);
        assert_eq!(
            got,
            vec![
                ("default".to_string(), vec!["PRAGMA wal_autocheckpoint = 0".to_string()]),
                ("development".to_string(), vec!["PRAGMA wal_autocheckpoint = 0".to_string()]),
                ("test".to_string(), vec!["PRAGMA wal_autocheckpoint = 0".to_string()]),
            ]
        );
    }

    #[test]
    fn values_it_cannot_spell_are_left_out() {
        let yml = "test:\n  adapter: sqlite3\n  pragmas:\n    journal_mode: wal\n    foreign_keys: true\n    mmap_size: <%= 1 %>\n    cache_size: \"a b\"\n";
        assert_eq!(
            parse(yml),
            vec![(
                "test".to_string(),
                vec!["PRAGMA journal_mode = wal".to_string(), "PRAGMA foreign_keys = ON".to_string()]
            )]
        );
    }
}
