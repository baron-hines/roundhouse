//! `config/cable.yml`'s production adapter, for broadcasts between
//! processes.
//!
//! Rails loads the file with `config_for(:cable)` (ERB, then YAML, the
//! environment's section) and, for `adapter: redis`, hands the rest to
//! `Redis.new`: `url:` names the server, and with neither `url:` nor
//! `REDIS_URL` redis-rb connects to localhost:6379. Every broadcast is a
//! `PUBLISH` on `"#{channel_prefix}:#{broadcasting}"`
//! (`ActionCable::SubscriptionAdapter::ChannelPrefix`), or on the bare
//! broadcasting when there is no prefix.
//!
//! The URL is read in the forms the corpus writes it: a literal,
//! `<%= ENV["NAME"] %>`, `<%= ENV.fetch("NAME") %>` and
//! `<%= ENV.fetch("NAME", "default") %>` or `{ "default" }`. Any other
//! ERB in the URL is a warning, and the URL falls back to `REDIS_URL`
//! then localhost, as redis-rb's would without one. An emitted tree runs
//! in production, so only that section is read.

use std::path::Path;

use serde_yaml_ng::Value;

use crate::app::{CableRedisConfig, RedisSetting};
use crate::vfs::Vfs;

/// The production adapter's name (`None` without the file or the
/// section), the Redis settings when that adapter is `redis`, and a
/// warning for anything in them that isn't read.
pub fn read<V: Vfs + ?Sized>(vfs: &V, dir: &Path) -> (Option<String>, Option<CableRedisConfig>, Vec<String>) {
    let Ok(bytes) = vfs.read(&dir.join("config/cable.yml")) else { return (None, None, Vec::new()) };
    parse(&String::from_utf8_lossy(&bytes))
}

pub fn parse(text: &str) -> (Option<String>, Option<CableRedisConfig>, Vec<String>) {
    let mut warnings = Vec::new();
    let (text, tags) = mark_erb(text);
    let Ok(mut doc) = serde_yaml_ng::from_str::<Value>(&text) else {
        warnings.push("config/cable.yml is not read: it is not YAML once its ERB is set aside".to_string());
        return (None, None, warnings);
    };
    if doc.apply_merge().is_err() {
        return (None, None, warnings);
    }
    let Some(Value::Mapping(production)) = doc.get("production").cloned() else { return (None, None, warnings) };
    let adapter = match production.get("adapter") {
        Some(Value::String(a)) => a.clone(),
        _ => return (None, None, warnings),
    };
    if adapter != "redis" {
        return (Some(adapter), None, warnings);
    }
    let fallback = RedisSetting::EnvFetch { name: "REDIS_URL".into(), default: Some("redis://localhost:6379".into()) };
    let url = match production.get("url") {
        None => fallback,
        Some(Value::String(raw)) => match erb_index(raw) {
            None => RedisSetting::Literal(raw.clone()),
            Some(i) => match env_setting(&tags[i]) {
                Some(setting) if raw == &marker(i) => setting,
                _ => {
                    warnings.push(format!(
                        "config/cable.yml's production `url:` (`{}`) is not read; broadcasts go to REDIS_URL, or localhost",
                        unmark(raw, &tags)
                    ));
                    fallback
                }
            },
        },
        Some(other) => {
            warnings.push(format!("config/cable.yml's production `url:` ({other:?}) is not a String, so it is not read"));
            fallback
        }
    };
    let channel_prefix = match production.get("channel_prefix") {
        None => String::new(),
        Some(Value::String(p)) if erb_index(p).is_none() => p.clone(),
        Some(_) => {
            warnings.push("config/cable.yml's production `channel_prefix:` is not a literal String, so broadcasts go out unprefixed".to_string());
            String::new()
        }
    };
    (Some(adapter), Some(CableRedisConfig { url, channel_prefix }), warnings)
}

fn marker(i: usize) -> String {
    format!("__roundhouse_erb_{i}__")
}

fn erb_index(s: &str) -> Option<usize> {
    let start = s.find("__roundhouse_erb_")? + "__roundhouse_erb_".len();
    let end = s[start..].find("__")? + start;
    s[start..end].parse().ok()
}

fn unmark(s: &str, tags: &[String]) -> String {
    let mut out = s.to_string();
    for (i, tag) in tags.iter().enumerate() {
        out = out.replace(&marker(i), &format!("<%= {tag} %>"));
    }
    out
}

/// `ENV["NAME"]`, `ENV.fetch("NAME")`, `ENV.fetch("NAME", "d")` and
/// `ENV.fetch("NAME") { "d" }`, with either quote.
fn env_setting(expr: &str) -> Option<RedisSetting> {
    let e = expr.trim();
    let string = |s: &str| -> Option<(String, usize)> {
        let s_trim = s.trim_start();
        let lead = s.len() - s_trim.len();
        let q = s_trim.chars().next()?;
        if q != '"' && q != '\'' {
            return None;
        }
        let end = s_trim[1..].find(q)? + 1;
        Some((s_trim[1..end].to_string(), lead + end + 1))
    };
    if let Some(rest) = e.strip_prefix("ENV[") {
        let (name, used) = string(rest)?;
        return (rest[used..].trim() == "]").then_some(RedisSetting::Env(name));
    }
    let rest = e.strip_prefix("ENV.fetch(")?;
    let (name, used) = string(rest)?;
    let rest = rest[used..].trim_start();
    if let Some(after) = rest.strip_prefix(')') {
        let after = after.trim();
        if after.is_empty() {
            return Some(RedisSetting::EnvFetch { name, default: None });
        }
        let inner = after.strip_prefix('{')?.strip_suffix('}')?;
        let (default, used) = string(inner)?;
        return inner[used..].trim().is_empty().then_some(RedisSetting::EnvFetch { name, default: Some(default) });
    }
    let rest = rest.strip_prefix(',')?;
    let (default, used) = string(rest)?;
    (rest[used..].trim() == ")").then_some(RedisSetting::EnvFetch { name, default: Some(default) })
}

/// Each `<%= ... %>` becomes a numbered marker and each other tag
/// nothing; the output tags' Ruby, by marker number.
fn mark_erb(text: &str) -> (String, Vec<String>) {
    let mut out = String::with_capacity(text.len());
    let mut tags = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<%") {
        out.push_str(&rest[..start]);
        let tag = &rest[start..];
        let Some(end) = tag.find("%>") else {
            rest = "";
            break;
        };
        if let Some(body) = tag[..end].strip_prefix("<%=") {
            out.push_str(&marker(tags.len()));
            tags.push(body.trim().trim_end_matches('-').trim().to_string());
        }
        rest = &tag[end + 2..];
    }
    out.push_str(rest);
    (out, tags)
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::app::RedisSetting;

    #[test]
    fn campfire_shape() {
        let yml = "development:\n  adapter: async\n\nproduction:\n  adapter: redis\n  url: <%= ENV.fetch(\"REDIS_URL\") { \"redis://localhost:6379/1\" } %>\n  channel_prefix: campfire_production\n";
        let (adapter, redis, w) = parse(yml);
        assert!(w.is_empty(), "{w:#?}");
        assert_eq!(adapter.as_deref(), Some("redis"));
        let redis = redis.unwrap();
        assert_eq!(
            redis.url,
            RedisSetting::EnvFetch { name: "REDIS_URL".into(), default: Some("redis://localhost:6379/1".into()) }
        );
        assert_eq!(redis.channel_prefix, "campfire_production");
    }

    #[test]
    fn literal_bracket_and_absent_urls() {
        let (_, r, _) = parse("production:\n  adapter: redis\n  url: redis://cache:6379/2\n");
        assert_eq!(r.unwrap().url, RedisSetting::Literal("redis://cache:6379/2".into()));
        let (_, r, _) = parse("production:\n  adapter: redis\n  url: <%= ENV['CABLE_URL'] %>\n");
        assert_eq!(r.unwrap().url, RedisSetting::Env("CABLE_URL".into()));
        let (_, r, _) = parse("production:\n  adapter: redis\n  url: <%= ENV.fetch(\"CABLE_URL\", \"redis://x\") %>\n");
        assert_eq!(r.unwrap().url, RedisSetting::EnvFetch { name: "CABLE_URL".into(), default: Some("redis://x".into()) });
        let (_, r, w) = parse("production:\n  adapter: redis\n");
        assert!(w.is_empty());
        let r = r.unwrap();
        assert_eq!(r.url, RedisSetting::EnvFetch { name: "REDIS_URL".into(), default: Some("redis://localhost:6379".into()) });
        assert_eq!(r.channel_prefix, "");
    }

    #[test]
    fn other_adapters_and_unread_erb() {
        let (adapter, r, _) = parse("production:\n  adapter: solid_cable\n");
        assert_eq!(adapter.as_deref(), Some("solid_cable"));
        assert!(r.is_none());
        let (_, r, w) = parse("production:\n  adapter: redis\n  url: <%= Rails.application.credentials.redis_url %>\n");
        assert_eq!(w.len(), 1, "{w:#?}");
        assert!(w[0].contains("credentials.redis_url"), "{w:#?}");
        assert_eq!(r.unwrap().url, RedisSetting::EnvFetch { name: "REDIS_URL".into(), default: Some("redis://localhost:6379".into()) });
    }

    #[test]
    fn a_merged_default_section() {
        let yml = "default: &default\n  adapter: redis\n  url: redis://shared:6379\nproduction:\n  <<: *default\n  channel_prefix: app\n";
        let (adapter, r, _) = parse(yml);
        assert_eq!(adapter.as_deref(), Some("redis"));
        let r = r.unwrap();
        assert_eq!(r.url, RedisSetting::Literal("redis://shared:6379".into()));
        assert_eq!(r.channel_prefix, "app");
    }
}
