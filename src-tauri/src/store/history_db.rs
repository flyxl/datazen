//! Local SQLite persistence for SQL query history and workflow execution history.
//!
//! ## Security note — plaintext SQL
//!
//! `{appData}/history.sqlite` stores executed SQL, database/schema context, and
//! error messages **in plaintext** (not encrypted like `connections.json` or
//! `ai_config.enc`). Query text may contain literals, identifiers, or fragments
//! that embed credentials or other sensitive data. Anyone with filesystem access
//! to the app data directory (backups, sync folders, shared profiles) can read
//! this file. Future hardening may add encryption or redaction; until then treat
//! `history.sqlite` like a sensitive audit log and avoid shipping it off-device.
//!
//! ## Layout
//!
//! This file holds the shared vocabulary — the types the rest of the app names,
//! the connection guard, and the retention policy. The data domains live
//! beside it, one file each, so that adding a domain does not mean growing this
//! one:
//!
//! | module | owns |
//! |---|---|
//! | [`query_history`] | `query_history`: append, filter, page, delete |
//! | [`workflow_history`] | `workflow_history`: record and replay |
//! | [`legacy_favorites`] | the retired `favorite_queries` table (read-only) |
//! | [`migration_run`] | migration run records and their filters |
//! | [`schema`] | table creation, the v1→v4 migration ring, JSON import |
//!
//! Retention ([`HistoryDb::purge`]) stays here because it spans two of them and
//! has no home in either.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};

use super::models::QueryHistoryEntry;
use crate::workflow::workflows::WorkflowExecutionResult;

pub const MAX_QUERY_HISTORY: usize = 1000;
pub const MAX_WORKFLOW_HISTORY: usize = 100;
pub const MAX_MIGRATION_RUN_HISTORY: usize = 1000;

pub use datazen_platform_api::dto::migration::{MigrationRunRecord, MigrationRunFilter, MigrationRunPage, MigrationProfileRef};

/// Name of the retired favorites table. Never created again; only read.
pub const LEGACY_FAVORITES_TABLE: &str = "favorite_queries";

/// Where those rows go once they have been exported to `.sql` files. Keeping
/// them (instead of dropping) is what makes the migration reversible.
pub const LEGACY_FAVORITES_ARCHIVE_TABLE: &str = "favorite_queries_legacy_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    #[serde(alias = "skillId")]
    pub workflow_id: String,
    #[serde(alias = "skillName")]
    pub workflow_name: String,
    pub variables: serde_json::Value,
    pub result: WorkflowExecutionResult,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryListItem {
    pub id: String,
    #[serde(alias = "skillId")]
    pub workflow_id: String,
    #[serde(alias = "skillName")]
    pub workflow_name: String,
    pub success: bool,
    pub total_time_ms: u64,
    pub created_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryDbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryScope {
    Query,
    Workflow,
    All,
}

impl HistoryScope {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "query" => Some(Self::Query),
            "workflow" => Some(Self::Workflow),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

/// Sort order for the paged query-history read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryOrder {
    /// Newest first (the historical default).
    #[default]
    Recent,
    /// Oldest first.
    Oldest,
    /// Slowest first — surfaces the statements that actually hurt.
    Slowest,
}

impl HistoryOrder {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "recent" => Some(Self::Recent),
            "oldest" => Some(Self::Oldest),
            "slowest" => Some(Self::Slowest),
            _ => None,
        }
    }

    /// The exact `ORDER BY` tail, so every page query orders the same way and
    /// "slowest first" cannot silently disagree with its own label.
    pub(super) fn order_by(self) -> &'static str {
        match self {
            Self::Recent => "executed_at DESC, id DESC",
            Self::Oldest => "executed_at ASC, id ASC",
            Self::Slowest => "execution_time_ms DESC, executed_at DESC",
        }
    }
}

/// Filters for the paged query-history read.
///
/// `search` is applied **before** `limit` so a caller never has to widen the
/// page to find a row that exists. `since`/`until` are RFC3339 instants.
#[derive(Debug, Clone)]
pub struct QueryHistoryFilter<'a> {
    pub limit: usize,
    pub connection_id: Option<&'a str>,
    pub database: Option<&'a str>,
    pub schema: Option<&'a str>,
    pub search: Option<&'a str>,
    pub since: Option<&'a str>,
    pub until: Option<&'a str>,
    pub order: HistoryOrder,
}

/// Page size used when a caller does not name one.
///
/// Hand-written rather than `#[derive(Default)]` on purpose: a derived default
/// would be `limit: 0`, and `LIMIT 0` returns an empty page that reads as "you
/// have no history" instead of "you asked for nothing". Every `..Default` call
/// site would silently render an empty list.
pub const DEFAULT_HISTORY_PAGE_SIZE: usize = 200;

impl Default for QueryHistoryFilter<'_> {
    fn default() -> Self {
        Self {
            limit: DEFAULT_HISTORY_PAGE_SIZE,
            connection_id: None,
            database: None,
            schema: None,
            search: None,
            since: None,
            until: None,
            order: HistoryOrder::Recent,
        }
    }
}

/// One page of query history plus the number of rows the filter matched *before*
/// the page was cut.
///
/// Without `total` a truncated page is indistinguishable from a complete one, so
/// the UI cannot tell "you searched and found nothing" from "the row you are
/// looking for fell outside the 200 most recent".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryPage {
    pub entries: Vec<QueryHistoryEntry>,
    /// Rows matching the filter with `limit` ignored.
    pub total: u64,
}

pub struct HistoryDb {
    #[allow(dead_code)] // exposed via `db_path()` for the upcoming cleanup/purge flows
    db_path: PathBuf,
    conn: Mutex<Connection>,
}

mod legacy_favorites;
mod migration_run;
mod query_history;
mod schema;
mod workflow_history;

#[cfg(test)]
#[path = "history_db/retention_tests.rs"]
mod retention_tests;

#[cfg(test)]
#[path = "history_db/fixtures.rs"]
mod fixtures;

#[cfg(test)]
#[path = "history_db_page_tests.rs"]
mod page_tests;

impl HistoryDb {
    pub fn open(data_dir: &Path) -> Result<Arc<Self>, HistoryDbError> {
        let db_path = data_dir.join("history.sqlite");
        let conn = open_connection(&db_path)?;
        let db = Arc::new(Self {
            db_path,
            conn: Mutex::new(conn),
        });
        db.init_schema()?;
        db.run_migrations()?;
        db.migrate_legacy_json(data_dir)?;
        Ok(db)
    }

    /// The single place a borrow of the connection is taken in production.
    ///
    /// Everything downstream goes through the typed methods in the domain
    /// modules, so schema shape stays an implementation detail of this subtree.
    pub(super) fn with_conn<T, F>(&self, f: F) -> Result<T, HistoryDbError>
    where
        F: FnOnce(&Connection) -> Result<T, HistoryDbError>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|e| HistoryDbError::Other(format!("history db lock poisoned: {e}")))?;
        f(&conn)
    }

    /// Run `f` against the raw SQLite connection.
    ///
    /// Test-only: production code goes through the typed methods, so that
    /// schema shape stays an implementation detail of this module.
    #[cfg(test)]
    pub(crate) fn with_raw_conn<T, F>(&self, f: F) -> T
    where
        F: FnOnce(&Connection) -> T,
    {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&conn)
    }

    /// Delete rows in `scope`. `retain_days = None` removes all rows in scope; otherwise
    /// deletes rows older than the cutoff (UTC).
    pub fn purge(
        &self,
        scope: HistoryScope,
        retain_days: Option<u32>,
    ) -> Result<u64, HistoryDbError> {
        self.with_conn(|conn| {
            let mut total = 0u64;
            let cutoff =
                retain_days.map(|days| (Utc::now() - Duration::days(days as i64)).to_rfc3339());

            if matches!(scope, HistoryScope::Query | HistoryScope::All) {
                total += purge_table(conn, "query_history", "executed_at", cutoff.as_deref())?;
            }
            if matches!(scope, HistoryScope::Workflow | HistoryScope::All) {
                total += purge_table(conn, "workflow_history", "created_at", cutoff.as_deref())?;
            }
            Ok(total)
        })
    }

    #[cfg(test)]
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

/// Whether `name` exists as a table. Used to keep migrations tolerant of
/// databases that predate a given table.
pub(super) fn table_exists(conn: &Connection, name: &str) -> Result<bool, HistoryDbError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn open_connection(db_path: &Path) -> Result<Connection, HistoryDbError> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match Connection::open(db_path) {
        Ok(conn) => {
            let _: i32 = conn.query_row("SELECT 1", [], |row| row.get(0))?;
            let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
            Ok(conn)
        }
        Err(e) => {
            tracing::warn!(
                path = %db_path.display(),
                error = %e,
                "history.sqlite open failed; recreating empty database"
            );
            let _ = std::fs::remove_file(db_path);
            let conn = Connection::open(db_path)?;
            let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
            Ok(conn)
        }
    }
}

fn purge_table(
    conn: &Connection,
    table: &str,
    time_col: &str,
    cutoff: Option<&str>,
) -> Result<u64, HistoryDbError> {
    let deleted = if let Some(cutoff) = cutoff {
        let sql = format!("DELETE FROM {table} WHERE {time_col} < ?1");
        conn.execute(&sql, params![cutoff])?
    } else {
        let sql = format!("DELETE FROM {table}");
        conn.execute(&sql, [])?
    };
    Ok(deleted as u64)
}

/// Storage-layer note: since schema v4 the SQLite columns use the unified
/// `connection_id` name (persisted config connection id), matching the struct
/// fields — no legacy-name adapter is needed.
pub(super) fn rename_aside(from: &Path, to: &Path) -> Result<(), HistoryDbError> {
    if to.exists() {
        return Ok(());
    }
    if let Err(e) = std::fs::rename(from, to) {
        tracing::warn!(
            from = %from.display(),
            to = %to.display(),
            error = %e,
            "Failed to rename legacy history aside; leaving source in place"
        );
    }
    Ok(())
}
