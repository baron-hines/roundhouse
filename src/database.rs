//! Database identity: which databases each environment of an app has,
//! and which database each model runs on.
//!
//! The first is `config/database.yml`, resolved when Roundhouse builds
//! the app (`ingest::database_yml::database_configurations`), the way
//! Rails' `ActiveRecord::DatabaseConfigurations` reads it: the
//! environments, each one's configurations, which is the primary, which
//! are replicas, and the adapter each names. Only the keys that decide
//! identity are read, a SQLite configuration's file among them;
//! connection values (host, credentials, pool sizes) are not, so none of
//! them reaches the IR.
//!
//! The second is read from source, never from configuration: a
//! `connects_to` in an abstract class, inherited by its subclasses
//! (`ingest::database_ownership`). One shape is carried, on the Spinel
//! target's SQLite runtime ([`DatabaseRoutes`]). Every other routing
//! shape is a named refusal (`DatabaseRefusal`),
//! reported by `analyze::database_identity`, so an app that needs a
//! database the runtime does not open is reported unsupported instead of
//! silently running on the primary.
//!
//! An app without `config/database.yml` and without routing has none of
//! this, and its IR serializes as before: every field is skipped when
//! empty.

use serde::{Deserialize, Serialize};

use crate::ident::{ClassId, Symbol};
use crate::span::Span;

/// The adapter a configuration names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseAdapter {
    Sqlite3,
    Postgresql,
    /// PostGIS is its own adapter (spatial types, its own OIDs), not
    /// postgresql: a postgresql runtime does not cover it.
    Postgis,
    /// `mysql2` or `trilogy`, as written.
    Mysql(String),
    /// Any other adapter name, as written.
    Other(String),
}

impl DatabaseAdapter {
    /// The adapter for a configuration's `adapter:` value.
    pub fn parse(adapter: &str) -> Self {
        match adapter {
            "sqlite3" => Self::Sqlite3,
            "postgresql" => Self::Postgresql,
            "postgis" => Self::Postgis,
            "mysql2" | "trilogy" => Self::Mysql(adapter.to_string()),
            other => Self::Other(other.to_string()),
        }
    }

    /// The adapter a `url:` names by its scheme, as Rails'
    /// `ConnectionUrlResolver` maps it (`postgres` is `postgresql`).
    pub fn from_url(url: &str) -> Option<Self> {
        let query_adapter = Self::query_pairs(url).filter(|(key, _)| *key == "adapter").last();
        if let Some((_, adapter)) = query_adapter {
            return Some(Self::parse(adapter));
        }
        let (scheme, _) = url.split_once(':')?;
        if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)) {
            return None;
        }
        Some(match scheme {
            "postgres" => Self::Postgresql,
            scheme => Self::parse(&scheme.replace('-', "_")),
        })
    }

    /// The `key=value` pairs of a hierarchical URL's query, where Rails' `ConnectionUrlResolver`
    /// lets them win over the URL's own parts. An opaque URL (`sqlite3:db/x`) keeps its scheme.
    pub fn query_pairs(url: &str) -> impl Iterator<Item = (&str, &str)> {
        let query = url.split_once(':').map(|(_, rest)| rest).filter(|rest| rest.starts_with('/')).and_then(|rest| rest.split('#').next()).and_then(|rest| rest.split_once('?')).map(|(_, query)| query);
        query.into_iter().flat_map(|query| query.split('&')).map(|pair| pair.split_once('=').unwrap_or((pair, "")))
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Sqlite3 => "sqlite3",
            Self::Postgresql => "postgresql",
            Self::Postgis => "postgis",
            Self::Mysql(name) | Self::Other(name) => name,
        }
    }
}

/// One configuration of one environment of `config/database.yml`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Database {
    /// The environment (`development`, `test`, `production`, ...).
    pub env: String,
    /// The configuration's name: `primary` for a two-tier environment.
    pub name: String,
    pub adapter: DatabaseAdapter,
    /// Rails' primary: the configuration named `primary`, else the
    /// environment's first.
    pub primary: bool,
    pub replica: bool,
    /// Whether Rails' database tasks (create, migrate, schema dump) run
    /// for it: `database_tasks:`, default true, never for a replica.
    pub database_tasks: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub migrations_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_search_path: Option<String>,
    /// A SQLite configuration's literal `database:`, the file it opens,
    /// relative to the app's root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// A literal `schema_dump:` file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_dump: Option<String>,
}

impl Database {
    /// Whether the Spinel SQLite runtime can open this configuration as
    /// the pool of a role: a SQLite file named literally, and for the
    /// writing role the primary or one that names the app's own schema
    /// file (Rails' default for another database is a file of its own).
    pub fn routable(&self, writing: bool, schema: &str) -> bool {
        let file = self.database.as_deref().is_some_and(|f| !f.is_empty() && f != ":memory:" && !f.starts_with("file:"));
        let shared_schema = self.schema_dump.as_ref().is_some_and(|name| {
            std::path::Path::new("db").join(name) == std::path::Path::new(schema)
        });
        let own_tables = !self.replica && self.database_tasks && (self.primary || shared_schema);
        // Not a replica primary: its pool is the one `Db.configure` opened, which carries no replica flag.
        self.adapter == DatabaseAdapter::Sqlite3 && file && (!writing || own_tables) && !(self.primary && self.replica)
    }
}

/// The pools of an app whose one connection class routes every model,
/// which the Spinel SQLite runtime opens (`ingest::database_ownership`,
/// `project::apply_database_routes`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatabaseRoutes {
    /// The abstract class that declares them.
    pub owner: ClassId,
    /// Its default shard: the first `shards:` names, `default` for
    /// `database:`.
    pub default_shard: String,
    /// The loaded Ruby schema, relative to the app root.
    pub schema_file: String,
    /// `(shard, role, configuration name)`, in declaration order.
    pub routes: Vec<(String, String, String)>,
}

/// The database a model runs on, read from source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDatabase {
    /// No `connects_to` or routing override on the model or an ancestor.
    Primary,
    /// A literal `connects_to database: { writing:, reading: }`, on the
    /// model or the nearest ancestor that declares one.
    Named {
        writing: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reading: Option<String>,
        declared_by: ClassId,
        span: Span,
    },
    /// The app defines the class-side connection lookup itself
    /// (`def self.connection_pool`, also from a concern), so which
    /// database a query reaches is app code, not configuration.
    AppDefined { method: Symbol, defined_by: ClassId, span: Span },
    /// A routing declaration Roundhouse cannot resolve (non-literal,
    /// conditional, `shards:`, on a concrete class, ...).
    Unresolved { reason: String, span: Span },
}

/// A database-identity fact the emitted runtime does not carry: reported
/// at `span` by `analyze::database_identity`, as an error unless
/// `warning`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatabaseRefusal {
    pub span: Span,
    /// Stable construct name (`database_configurations`, `connects_to`,
    /// `connected_to`, `connection_override`, `establish_connection`).
    pub construct: String,
    pub detail: String,
    /// A configuration build-time resolution cannot read is a warning: the
    /// emitted runtime opens the primary without reading
    /// `config/database.yml`, so it matters only once a model routes to
    /// that configuration, and that routing is the error.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub warning: bool,
}
