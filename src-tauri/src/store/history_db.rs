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

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::models::{FavoriteQuery, QueryHistoryEntry};
use crate::workflow::workflows::WorkflowExecutionResult;

pub const MAX_QUERY_HISTORY: usize = 1000;
pub const MAX_WORKFLOW_HISTORY: usize = 100;

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

pub struct HistoryDb {
    #[allow(dead_code)] // exposed via `db_path()` for the upcoming cleanup/purge flows
    db_path: PathBuf,
    conn: Mutex<Connection>,
}

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

    fn with_conn<T, F>(&self, f: F) -> Result<T, HistoryDbError>
    where
        F: FnOnce(&Connection) -> Result<T, HistoryDbError>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|e| HistoryDbError::Other(format!("history db lock poisoned: {e}")))?;
        f(&conn)
    }

    fn init_schema(&self) -> Result<(), HistoryDbError> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS query_history (
                    id TEXT PRIMARY KEY NOT NULL,
                    connection_id TEXT NOT NULL,
                    database TEXT NOT NULL,
                    sql TEXT NOT NULL,
                    executed_at TEXT NOT NULL,
                    execution_time_ms INTEGER NOT NULL,
                    rows_affected INTEGER,
                    success INTEGER NOT NULL,
                    error_message TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_query_history_executed_at
                    ON query_history(executed_at DESC);

                CREATE TABLE IF NOT EXISTS workflow_history (
                    id TEXT PRIMARY KEY NOT NULL,
                    workflow_id TEXT NOT NULL,
                    workflow_name TEXT NOT NULL,
                    variables_json TEXT NOT NULL,
                    result_json TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_workflow_history_created_at
                    ON workflow_history(created_at DESC);
                CREATE INDEX IF NOT EXISTS idx_workflow_history_workflow_id
                    ON workflow_history(workflow_id);
                ",
            )?;
            Ok(())
        })
    }

    fn run_migrations(&self) -> Result<(), HistoryDbError> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);",
            )?;
            let version: i32 = conn.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get(0),
            )?;

            if version < 2 {
                // Historical v1 → v2 step: v1 stored the column as
                // `connection_id`; it was renamed to `config_id` (data cleared).
                let has_legacy_connection_id_col = conn
                    .prepare("SELECT connection_id FROM query_history LIMIT 0")
                    .is_ok();

                if has_legacy_connection_id_col {
                    conn.execute_batch(
                        "
                        DELETE FROM query_history;
                        ALTER TABLE query_history RENAME COLUMN connection_id TO config_id;
                        ",
                    )?;
                    tracing::info!(
                        "Migrated query_history: connection_id → config_id (cleared old data)"
                    );
                }

                conn.execute_batch(
                    "
                    CREATE INDEX IF NOT EXISTS idx_query_history_config_id
                        ON query_history(config_id);
                    CREATE TABLE IF NOT EXISTS favorite_queries (
                        id TEXT PRIMARY KEY NOT NULL,
                        config_id TEXT NOT NULL,
                        title TEXT NOT NULL,
                        sql TEXT NOT NULL,
                        created_at TEXT NOT NULL
                    );
                    CREATE INDEX IF NOT EXISTS idx_favorite_queries_config_id
                        ON favorite_queries(config_id);
                    INSERT OR IGNORE INTO schema_version (version) VALUES (2);
                    ",
                )?;
                tracing::info!("Database schema migrated to version 2");
            }

            if version < 3 {
                let has_schema_col = conn
                    .prepare("SELECT schema FROM query_history LIMIT 0")
                    .is_ok();

                if !has_schema_col {
                    conn.execute_batch("ALTER TABLE query_history ADD COLUMN schema TEXT;")?;
                    tracing::info!("Added query_history.schema column");
                }

                conn.execute_batch(
                    "
                    CREATE INDEX IF NOT EXISTS idx_query_history_config_db
                        ON query_history(config_id, database);
                    INSERT OR IGNORE INTO schema_version (version) VALUES (3);
                    ",
                )?;
                tracing::info!("Database schema migrated to version 3");
            }

            if version < 4 {
                // v4: align physical column names with the ID terminology —
                // the persisted config connection id is now called
                // `connection_id` everywhere (struct fields, IPC, storage).
                let rename_column = |conn: &rusqlite::Connection,
                                     table: &str,
                                     from: &str,
                                     to: &str|
                 -> Result<(), HistoryDbError> {
                    let probe = format!("SELECT {from} FROM {table} LIMIT 0");
                    if conn.prepare(&probe).is_ok() {
                        conn.execute_batch(&format!(
                            "ALTER TABLE {table} RENAME COLUMN {from} TO {to};"
                        ))?;
                        tracing::info!("Migrated {table}: {from} → {to}");
                    }
                    Ok(())
                };
                rename_column(conn, "query_history", "config_id", "connection_id")?;
                rename_column(conn, "favorite_queries", "config_id", "connection_id")?;

                conn.execute_batch(
                    "
                    DROP INDEX IF EXISTS idx_query_history_connection_id;
                    DROP INDEX IF EXISTS idx_query_history_config_id;
                    DROP INDEX IF EXISTS idx_query_history_config_db;
                    DROP INDEX IF EXISTS idx_favorite_queries_config_id;
                    CREATE INDEX IF NOT EXISTS idx_query_history_connection_id
                        ON query_history(connection_id);
                    CREATE INDEX IF NOT EXISTS idx_query_history_connection_db
                        ON query_history(connection_id, database);
                    CREATE INDEX IF NOT EXISTS idx_favorite_queries_connection_id
                        ON favorite_queries(connection_id);
                    INSERT OR IGNORE INTO schema_version (version) VALUES (4);
                    ",
                )?;
                tracing::info!("Database schema migrated to version 4");
            }
            Ok(())
        })
    }

    fn migrate_legacy_json(&self, data_dir: &Path) -> Result<(), HistoryDbError> {
        migrate_queries_json(self, data_dir)?;
        migrate_workflow_json_dir(self, data_dir)?;
        Ok(())
    }

    // ── Query history ─────────────────────────────────────────────────────

    /// Append one query outcome to `history.sqlite` (plaintext `sql` column).
    ///
    /// See the module-level security note: history is not encrypted at rest.
    pub fn add_query_history(&self, entry: QueryHistoryEntry) -> Result<(), HistoryDbError> {
        self.with_conn(|conn| {
            let dominated: Option<String> = match conn.query_row(
                "SELECT sql FROM query_history WHERE connection_id = ?1 AND database = ?2 AND schema IS ?3 ORDER BY executed_at DESC LIMIT 1",
                params![entry.connection_id, entry.database, entry.schema],
                |row| row.get(0),
            ) {
                Ok(v) => Some(v),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(HistoryDbError::from(e)),
            };

            if dominated
                .as_deref()
                .map(|sql| sql.trim() == entry.sql.trim())
                .unwrap_or(false)
            {
                conn.execute(
                    "UPDATE query_history SET
                        executed_at = ?1,
                        execution_time_ms = ?2,
                        rows_affected = ?3,
                        success = ?4,
                        error_message = ?5
                     WHERE id = (
                        SELECT id FROM query_history WHERE connection_id = ?6 AND database = ?7 AND schema IS ?8 ORDER BY executed_at DESC LIMIT 1
                     )",
                    params![
                        entry.executed_at.to_rfc3339(),
                        entry.execution_time_ms as i64,
                        entry.rows_affected.map(|v| v as i64),
                        entry.success as i32,
                        entry.error_message,
                        entry.connection_id,
                        entry.database,
                        entry.schema,
                    ],
                )?;
            } else {
                conn.execute(
                    "INSERT INTO query_history (
                        id, connection_id, database, schema, sql, executed_at,
                        execution_time_ms, rows_affected, success, error_message
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        entry.id,
                        entry.connection_id,
                        entry.database,
                        entry.schema,
                        entry.sql,
                        entry.executed_at.to_rfc3339(),
                        entry.execution_time_ms as i64,
                        entry.rows_affected.map(|v| v as i64),
                        entry.success as i32,
                        entry.error_message,
                    ],
                )?;
            }

            trim_query_history(conn)?;
            Ok(())
        })
    }

    pub fn get_query_history(
        &self,
        limit: usize,
        connection_id: Option<&str>,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Result<Vec<QueryHistoryEntry>, HistoryDbError> {
        self.with_conn(|conn| {
            let mut where_clauses: Vec<String> = Vec::new();
            let mut filter_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            if let Some(cid) = connection_id {
                where_clauses.push(format!("connection_id = ?{}", filter_params.len() + 1));
                filter_params.push(Box::new(cid.to_string()));
            }
            if let Some(db) = database {
                where_clauses.push(format!("database = ?{}", filter_params.len() + 1));
                filter_params.push(Box::new(db.to_string()));
            }
            if let Some(s) = schema {
                // Empty string means "rows with no schema" (NULL); else exact match.
                if s.is_empty() {
                    where_clauses.push("schema IS NULL".to_string());
                } else {
                    where_clauses.push(format!("schema = ?{}", filter_params.len() + 1));
                    filter_params.push(Box::new(s.to_string()));
                }
            }
            let where_sql = if where_clauses.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", where_clauses.join(" AND "))
            };
            let sql = format!(
                "SELECT id, connection_id, database, schema, sql, executed_at, \
                 execution_time_ms, rows_affected, success, error_message \
                 FROM query_history {} ORDER BY executed_at DESC LIMIT ?{}",
                where_sql,
                filter_params.len() + 1,
            );
            let mut stmt = conn.prepare(&sql)?;
            filter_params.push(Box::new(limit as i64));
            let rows = stmt.query_map(
                rusqlite::params_from_iter(filter_params.iter().map(|p| p.as_ref())),
                map_query_row,
            )?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(HistoryDbError::from)
        })
    }

    pub fn clear_query_history(&self) -> Result<(), HistoryDbError> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM query_history", [])?;
            Ok(())
        })
    }

    // ── Favorite queries ──────────────────────────────────────────────────

    pub fn add_favorite_query(&self, fav: FavoriteQuery) -> Result<(), HistoryDbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO favorite_queries (id, connection_id, title, sql, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    fav.id,
                    fav.connection_id,
                    fav.title,
                    fav.sql,
                    fav.created_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_favorite_queries(
        &self,
        connection_id: Option<&str>,
    ) -> Result<Vec<FavoriteQuery>, HistoryDbError> {
        self.with_conn(|conn| {
            if let Some(cid) = connection_id {
                let mut stmt = conn.prepare(
                    "SELECT id, connection_id, title, sql, created_at
                     FROM favorite_queries
                     WHERE connection_id = ?1
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map(params![cid], map_favorite_row)?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(HistoryDbError::from)
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, connection_id, title, sql, created_at
                     FROM favorite_queries
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map([], map_favorite_row)?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(HistoryDbError::from)
            }
        })
    }

    pub fn delete_favorite_query(&self, id: &str) -> Result<(), HistoryDbError> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM favorite_queries WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    // ── Workflow history ──────────────────────────────────────────────────

    pub fn record_workflow(
        &self,
        id: &str,
        workflow_id: &str,
        workflow_name: &str,
        variables: &serde_json::Value,
        result: &WorkflowExecutionResult,
        created_at: &str,
    ) -> Result<(), HistoryDbError> {
        let variables_json = serde_json::to_string(variables)?;
        let result_json = serde_json::to_string(result)?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO workflow_history (
                    id, workflow_id, workflow_name, variables_json, result_json, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    workflow_id,
                    workflow_name,
                    variables_json,
                    result_json,
                    created_at,
                ],
            )?;
            trim_workflow_history(conn)?;
            Ok(())
        })
    }

    pub fn list_workflow_history(
        &self,
        workflow_id: Option<&str>,
    ) -> Result<Vec<HistoryListItem>, HistoryDbError> {
        self.with_conn(|conn| {
            let mut items = Vec::new();
            if let Some(wid) = workflow_id {
                let mut stmt = conn.prepare(
                    "SELECT id, workflow_id, workflow_name, result_json, created_at
                     FROM workflow_history
                     WHERE workflow_id = ?1
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map(params![wid], map_workflow_list_row)?;
                for row in rows {
                    items.push(row?);
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, workflow_id, workflow_name, result_json, created_at
                     FROM workflow_history
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map([], map_workflow_list_row)?;
                for row in rows {
                    items.push(row?);
                }
            }
            Ok(items)
        })
    }

    pub fn get_workflow_history(
        &self,
        history_id: &str,
    ) -> Result<Option<HistoryEntry>, HistoryDbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, workflow_id, workflow_name, variables_json, result_json, created_at
                 FROM workflow_history WHERE id = ?1",
            )?;
            let mut rows = stmt.query_map(params![history_id], |row| {
                let result_json: String = row.get(4)?;
                let result: WorkflowExecutionResult = serde_json::from_str(&result_json)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    workflow_id: row.get(1)?,
                    workflow_name: row.get(2)?,
                    variables: serde_json::from_str(&row.get::<_, String>(3)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    result,
                    created_at: row.get(5)?,
                })
            })?;
            match rows.next() {
                Some(Ok(entry)) => Ok(Some(entry)),
                Some(Err(e)) => Err(HistoryDbError::from(e)),
                None => Ok(None),
            }
        })
    }

    pub fn clear_workflow_history(
        &self,
        workflow_id: Option<&str>,
    ) -> Result<usize, HistoryDbError> {
        self.with_conn(|conn| {
            let deleted = if let Some(wid) = workflow_id {
                conn.execute(
                    "DELETE FROM workflow_history WHERE workflow_id = ?1",
                    params![wid],
                )?
            } else {
                conn.execute("DELETE FROM workflow_history", [])?
            };
            Ok(deleted)
        })
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

fn trim_query_history(conn: &Connection) -> Result<(), HistoryDbError> {
    conn.execute(
        "DELETE FROM query_history WHERE id NOT IN (
            SELECT id FROM query_history ORDER BY executed_at DESC LIMIT ?1
         )",
        params![MAX_QUERY_HISTORY as i64],
    )?;
    Ok(())
}

fn trim_workflow_history(conn: &Connection) -> Result<(), HistoryDbError> {
    conn.execute(
        "DELETE FROM workflow_history WHERE id NOT IN (
            SELECT id FROM workflow_history ORDER BY created_at DESC LIMIT ?1
         )",
        params![MAX_WORKFLOW_HISTORY as i64],
    )?;
    Ok(())
}

// Storage-layer note: since schema v4 the SQLite columns use the unified
// `connection_id` name (persisted config connection id), matching the struct
// fields — no legacy-name adapter is needed.

fn map_query_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueryHistoryEntry> {
    let executed_at: String = row.get(5)?;
    let executed_at = DateTime::parse_from_rfc3339(&executed_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let rows_affected: Option<i64> = row.get(7)?;
    Ok(QueryHistoryEntry {
        id: row.get(0)?,
        connection_id: row.get(1)?,
        database: row.get(2)?,
        schema: row.get(3)?,
        sql: row.get(4)?,
        executed_at,
        execution_time_ms: row.get::<_, i64>(6)? as u64,
        rows_affected: rows_affected.map(|v| v as u64),
        success: row.get::<_, i32>(8)? != 0,
        error_message: row.get(9)?,
    })
}

fn map_favorite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FavoriteQuery> {
    let created_at: String = row.get(4)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    Ok(FavoriteQuery {
        id: row.get(0)?,
        connection_id: row.get(1)?,
        title: row.get(2)?,
        sql: row.get(3)?,
        created_at,
    })
}

fn map_workflow_list_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryListItem> {
    let result_json: String = row.get(3)?;
    let result: WorkflowExecutionResult = serde_json::from_str(&result_json)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    Ok(HistoryListItem {
        id: row.get(0)?,
        workflow_id: row.get(1)?,
        workflow_name: row.get(2)?,
        success: result.success,
        total_time_ms: result.total_time_ms,
        created_at: row.get(4)?,
    })
}

/// Legacy JSON entry format (has `connectionId` instead of `configId`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyQueryHistoryEntry {
    id: String,
    connection_id: String,
    database: String,
    sql: String,
    executed_at: DateTime<Utc>,
    execution_time_ms: u64,
    rows_affected: Option<u64>,
    success: bool,
    error_message: Option<String>,
}

fn migrate_queries_json(db: &HistoryDb, data_dir: &Path) -> Result<(), HistoryDbError> {
    let json_path = data_dir.join("history/queries.json");
    let migrated_path = data_dir.join("history/queries.json.migrated");
    if !json_path.is_file() || migrated_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&json_path)?;
    let entries: Vec<LegacyQueryHistoryEntry> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(path = %json_path.display(), error = %e, "Skipping invalid queries.json during migration");
            rename_aside(&json_path, &migrated_path)?;
            return Ok(());
        }
    };

    db.with_conn(|conn| {
        for entry in entries {
            let _ = conn.execute(
                "INSERT OR IGNORE INTO query_history (
                    id, connection_id, database, sql, executed_at,
                    execution_time_ms, rows_affected, success, error_message
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    entry.id,
                    entry.connection_id,
                    entry.database,
                    entry.sql,
                    entry.executed_at.to_rfc3339(),
                    entry.execution_time_ms as i64,
                    entry.rows_affected.map(|v| v as i64),
                    entry.success as i32,
                    entry.error_message,
                ],
            );
        }
        trim_query_history(conn)?;
        Ok(())
    })?;

    rename_aside(&json_path, &migrated_path)?;
    tracing::info!(
        from = %json_path.display(),
        "Migrated query history JSON → history.sqlite"
    );
    Ok(())
}

fn migrate_workflow_json_dir(db: &HistoryDb, data_dir: &Path) -> Result<(), HistoryDbError> {
    // Legacy skill_history → workflow_history directory rename.
    let workflow_dir = data_dir.join("workflow_history");
    if !workflow_dir.exists() {
        let legacy = data_dir.join("skill_history");
        if legacy.is_dir() {
            if let Err(e) = std::fs::rename(&legacy, &workflow_dir) {
                tracing::warn!(
                    from = %legacy.display(),
                    to = %workflow_dir.display(),
                    error = %e,
                    "Failed to rename skill_history → workflow_history"
                );
            } else {
                tracing::info!(to = %workflow_dir.display(), "Migrated skill_history → workflow_history");
            }
        }
    }

    let migrated_dir = data_dir.join("workflow_history.migrated");
    if !workflow_dir.is_dir() || migrated_dir.exists() {
        return Ok(());
    }

    let imported = db.with_conn(|conn| {
        let mut count = 0usize;
        for entry in std::fs::read_dir(&workflow_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.extension().map_or(false, |ext| ext == "json") {
                continue;
            }
            let content = std::fs::read_to_string(&path)?;
            let he: LegacyWorkflowHistoryEntry = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Skipping invalid workflow history JSON");
                    continue;
                }
            };
            let variables_json = serde_json::to_string(&he.variables)?;
            let result_json = serde_json::to_string(&he.result)?;
            conn.execute(
                "INSERT OR IGNORE INTO workflow_history (
                    id, workflow_id, workflow_name, variables_json, result_json, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    he.id,
                    he.workflow_id,
                    he.workflow_name,
                    variables_json,
                    result_json,
                    he.created_at,
                ],
            )?;
            count += 1;
        }
        trim_workflow_history(conn)?;
        Ok(count)
    })?;

    if imported > 0 || workflow_dir.read_dir()?.next().is_some() {
        rename_aside(&workflow_dir, &migrated_dir)?;
        tracing::info!(
            count = imported,
            from = %workflow_dir.display(),
            "Migrated workflow history JSON → history.sqlite"
        );
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacyWorkflowHistoryEntry {
    id: String,
    #[serde(alias = "skillId")]
    workflow_id: String,
    #[serde(alias = "skillName")]
    workflow_name: String,
    variables: serde_json::Value,
    result: WorkflowExecutionResult,
    created_at: String,
}

fn rename_aside(from: &Path, to: &Path) -> Result<(), HistoryDbError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::workflows::{StepExecutionResult, StepStatus};
    use chrono::Utc;
    use uuid::Uuid;

    fn sample_query(sql: &str, days_ago: i64) -> QueryHistoryEntry {
        QueryHistoryEntry {
            id: Uuid::new_v4().to_string(),
            connection_id: "cfg1".into(),
            database: "app".into(),
            schema: None,
            sql: sql.into(),
            executed_at: Utc::now() - Duration::days(days_ago),
            execution_time_ms: 10,
            rows_affected: Some(1),
            success: true,
            error_message: None,
        }
    }

    fn sample_query_for_config(sql: &str, connection_id: &str) -> QueryHistoryEntry {
        QueryHistoryEntry {
            id: Uuid::new_v4().to_string(),
            connection_id: connection_id.into(),
            database: "app".into(),
            schema: None,
            sql: sql.into(),
            executed_at: Utc::now(),
            execution_time_ms: 10,
            rows_affected: Some(1),
            success: true,
            error_message: None,
        }
    }

    fn make_test_result(success: bool) -> WorkflowExecutionResult {
        WorkflowExecutionResult {
            success,
            final_output: "test".into(),
            steps: vec![StepExecutionResult {
                step_id: "s1".into(),
                step_type: "query".into(),
                status: StepStatus::Success,
                result: Some(serde_json::json!({})),
                execution_time_ms: 10,
                error: None,
                connection_name: None,
                sql_executed: Some("SELECT 1".into()),
            }],
            total_time_ms: 42,
            error: None,
        }
    }

    #[test]
    fn migrates_queries_json_once() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("history")).unwrap();
        let legacy = serde_json::json!([{
            "id": "test-id",
            "connectionId": "c1",
            "database": "app",
            "sql": "SELECT 1",
            "executedAt": chrono::Utc::now().to_rfc3339(),
            "executionTimeMs": 10,
            "rowsAffected": 1,
            "success": true
        }]);
        std::fs::write(
            dir.path().join("history/queries.json"),
            serde_json::to_string_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let db = HistoryDb::open(dir.path()).unwrap();
        let loaded = db.get_query_history(10, None, None, None).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].sql, "SELECT 1");
        assert!(!dir.path().join("history/queries.json").exists());
        assert!(dir.path().join("history/queries.json.migrated").is_file());

        // Re-open must not duplicate.
        let db2 = HistoryDb::open(dir.path()).unwrap();
        assert_eq!(
            db2.get_query_history(10, None, None, None).unwrap().len(),
            1
        );
        let _ = db;
        let _ = db2;
    }

    #[test]
    fn migrates_workflow_json_dir_once() {
        let dir = tempfile::tempdir().unwrap();
        let wf_dir = dir.path().join("workflow_history");
        std::fs::create_dir_all(&wf_dir).unwrap();
        let entry = LegacyWorkflowHistoryEntry {
            id: "h1".into(),
            workflow_id: "wf1".into(),
            workflow_name: "WF".into(),
            variables: serde_json::json!({}),
            result: make_test_result(true),
            created_at: Utc::now().to_rfc3339(),
        };
        std::fs::write(
            wf_dir.join("h1.json"),
            serde_json::to_string_pretty(&entry).unwrap(),
        )
        .unwrap();

        let db = HistoryDb::open(dir.path()).unwrap();
        let list = db.list_workflow_history(None).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].workflow_id, "wf1");
        assert!(!wf_dir.is_dir());
        assert!(dir.path().join("workflow_history.migrated").is_dir());
    }

    #[test]
    fn query_history_connection_id_filter() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();
        db.add_query_history(sample_query_for_config("SELECT 1", "cfg-a"))
            .unwrap();
        db.add_query_history(sample_query_for_config("SELECT 2", "cfg-b"))
            .unwrap();
        db.add_query_history(sample_query_for_config("SELECT 3", "cfg-a"))
            .unwrap();

        let all = db.get_query_history(10, None, None, None).unwrap();
        assert_eq!(all.len(), 3);

        let a_only = db.get_query_history(10, Some("cfg-a"), None, None).unwrap();
        assert_eq!(a_only.len(), 2);
        assert!(a_only.iter().all(|e| e.connection_id == "cfg-a"));

        let b_only = db.get_query_history(10, Some("cfg-b"), None, None).unwrap();
        assert_eq!(b_only.len(), 1);
        assert_eq!(b_only[0].connection_id, "cfg-b");
    }

    #[test]
    fn query_history_dedup_scoped_by_connection_id() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();
        db.add_query_history(sample_query_for_config("SELECT 1", "cfg-a"))
            .unwrap();
        db.add_query_history(sample_query_for_config("SELECT 1", "cfg-b"))
            .unwrap();
        let all = db.get_query_history(10, None, None, None).unwrap();
        assert_eq!(
            all.len(),
            2,
            "same SQL on different configs should not dedup"
        );
    }

    #[test]
    fn query_history_database_filter() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();

        let mut app = sample_query("SELECT 1", 0);
        app.database = "app_db".into();
        db.add_query_history(app.clone()).unwrap();

        let mut analytics = sample_query("SELECT 2", 0);
        analytics.database = "analytics".into();
        analytics.schema = Some("public".into());
        db.add_query_history(analytics).unwrap();

        let legacy = sample_query("SELECT 3", 0);
        let legacy_db = legacy.database.clone();
        db.add_query_history(legacy).unwrap();

        let app_only = db
            .get_query_history(10, None, Some("app_db"), None)
            .unwrap();
        assert_eq!(app_only.len(), 1);
        assert_eq!(app_only[0].database, "app_db");

        // Legacy rows record an empty database string; filtering by "" finds them.
        let legacy_rows = db
            .get_query_history(10, None, Some(&legacy_db), None)
            .unwrap();
        assert_eq!(legacy_rows.len(), 1);

        let none = db
            .get_query_history(10, None, Some("missing"), None)
            .unwrap();
        assert!(none.is_empty());

        // Unfiltered still returns everything.
        assert_eq!(db.get_query_history(10, None, None, None).unwrap().len(), 3);
    }

    #[test]
    fn query_history_schema_roundtrip_and_filter() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();

        let mut with_schema = sample_query("SELECT 1", 0);
        with_schema.database = "analytics".into();
        with_schema.schema = Some("sales".into());
        db.add_query_history(with_schema).unwrap();

        let mut without_schema = sample_query("SELECT 2", 0);
        without_schema.database = "analytics".into();
        without_schema.schema = None;
        db.add_query_history(without_schema).unwrap();

        let sales = db
            .get_query_history(10, None, Some("analytics"), Some("sales"))
            .unwrap();
        assert_eq!(sales.len(), 1);
        assert_eq!(sales[0].schema.as_deref(), Some("sales"));

        // NULL-safe: NULL schema rows only match when the filter asks for NULL
        // (empty string is the sentinel for "no schema").
        let null_schema = db
            .get_query_history(10, None, Some("analytics"), Some(""))
            .unwrap();
        assert_eq!(null_schema.len(), 1);

        let all = db
            .get_query_history(10, None, Some("analytics"), None)
            .unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn query_history_dedup_scoped_by_database() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();

        let mut a = sample_query("SELECT 1", 0);
        a.connection_id = "cfg1".into();
        a.database = "app_db".into();
        db.add_query_history(a).unwrap();

        let mut b = sample_query("SELECT 1", 0);
        b.connection_id = "cfg1".into();
        b.database = "other_db".into();
        db.add_query_history(b).unwrap();

        let all = db.get_query_history(10, None, None, None).unwrap();
        assert_eq!(
            all.len(),
            2,
            "same SQL on different databases of one config should not dedup"
        );
    }

    #[test]
    fn favorite_queries_crud() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();

        assert!(db.get_favorite_queries(None).unwrap().is_empty());

        let fav = FavoriteQuery {
            id: "fav1".into(),
            connection_id: "cfg-a".into(),
            title: "My query".into(),
            sql: "SELECT 1".into(),
            created_at: Utc::now(),
        };
        db.add_favorite_query(fav).unwrap();

        let fav2 = FavoriteQuery {
            id: "fav2".into(),
            connection_id: "cfg-b".into(),
            title: "Other".into(),
            sql: "SELECT 2".into(),
            created_at: Utc::now(),
        };
        db.add_favorite_query(fav2).unwrap();

        let all = db.get_favorite_queries(None).unwrap();
        assert_eq!(all.len(), 2);

        let a_only = db.get_favorite_queries(Some("cfg-a")).unwrap();
        assert_eq!(a_only.len(), 1);
        assert_eq!(a_only[0].title, "My query");

        db.delete_favorite_query("fav1").unwrap();
        assert_eq!(db.get_favorite_queries(None).unwrap().len(), 1);
    }

    #[test]
    fn schema_version_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();
        drop(db);
        let db2 = HistoryDb::open(dir.path()).unwrap();
        db2.add_query_history(sample_query("SELECT 1", 0)).unwrap();
        let loaded = db2.get_query_history(10, None, None, None).unwrap();
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn purge_retains_recent_rows_only() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();

        db.add_query_history(sample_query("old", 40)).unwrap();
        db.add_query_history(sample_query("recent", 1)).unwrap();
        db.record_workflow(
            "w-old",
            "wf",
            "WF",
            &serde_json::json!({}),
            &make_test_result(true),
            &(Utc::now() - Duration::days(40)).to_rfc3339(),
        )
        .unwrap();
        db.record_workflow(
            "w-new",
            "wf",
            "WF",
            &serde_json::json!({}),
            &make_test_result(true),
            &(Utc::now() - Duration::days(1)).to_rfc3339(),
        )
        .unwrap();

        let deleted = db.purge(HistoryScope::All, Some(30)).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(db.get_query_history(10, None, None, None).unwrap().len(), 1);
        assert_eq!(
            db.get_query_history(10, None, None, None).unwrap()[0].sql,
            "recent"
        );
        assert_eq!(db.list_workflow_history(None).unwrap().len(), 1);
        assert_eq!(db.list_workflow_history(None).unwrap()[0].id, "w-new");
    }

    #[test]
    fn purge_clear_all_empties_scope() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();
        db.add_query_history(sample_query("q", 0)).unwrap();
        db.record_workflow(
            "w1",
            "wf",
            "WF",
            &serde_json::json!({}),
            &make_test_result(true),
            &Utc::now().to_rfc3339(),
        )
        .unwrap();

        assert_eq!(db.purge(HistoryScope::Query, None).unwrap(), 1);
        assert!(db
            .get_query_history(10, None, None, None)
            .unwrap()
            .is_empty());
        assert_eq!(db.list_workflow_history(None).unwrap().len(), 1);

        assert_eq!(db.purge(HistoryScope::Workflow, None).unwrap(), 1);
        assert!(db.list_workflow_history(None).unwrap().is_empty());
    }

    #[test]
    fn query_history_dedup_updates_latest() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();
        let mut e1 = sample_query("SELECT 1", 0);
        db.add_query_history(e1.clone()).unwrap();
        e1.execution_time_ms = 99;
        db.add_query_history(e1).unwrap();
        let history = db.get_query_history(10, None, None, None).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].execution_time_ms, 99);
    }
}

/// Legacy-column helpers shared by the v2/v3 upgrade regression tests.
#[cfg(test)]
mod migration_startpoint_tests {
    use super::*;

    /// Builds a raw SQLite file in the *pre-v4* physical shape (columns named
    /// `config_id`, indexes on `config_id`) so that opening the store must run
    /// the guarded RENAME migration without touching product code paths.
    /// `with_schema_column` models a v3 library when true, a v2 one otherwise.
    fn create_legacy_db(dir: &Path, with_schema_column: bool) -> Connection {
        let conn = Connection::open(dir.join("history.sqlite")).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version (version) VALUES (2);
             CREATE TABLE query_history (
                 id TEXT PRIMARY KEY NOT NULL,
                 config_id TEXT NOT NULL,
                 database TEXT NOT NULL,
                 sql TEXT NOT NULL,
                 executed_at TEXT NOT NULL,
                 execution_time_ms INTEGER NOT NULL,
                 rows_affected INTEGER,
                 success INTEGER NOT NULL,
                 error_message TEXT
             );
             CREATE INDEX idx_query_history_config_id ON query_history(config_id);
             CREATE TABLE favorite_queries (
                 id TEXT PRIMARY KEY NOT NULL,
                 config_id TEXT NOT NULL,
                 title TEXT NOT NULL,
                 sql TEXT NOT NULL,
                 created_at TEXT NOT NULL
             );
             CREATE INDEX idx_favorite_queries_config_id ON favorite_queries(config_id);",
        )
        .unwrap();
        if with_schema_column {
            conn.execute_batch(
                "ALTER TABLE query_history ADD COLUMN schema TEXT;
                 CREATE INDEX idx_query_history_config_db ON query_history(config_id, database);
                 INSERT INTO schema_version (version) VALUES (3);",
            )
            .unwrap();
        }
        conn
    }

    fn column_names(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        let rows = stmt.query_map([], |row| row.get::<_, String>(1)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    #[test]
    fn legacy_v3_database_with_rows_migrates_to_connection_id_preserving_data() {
        let dir = tempfile::tempdir().unwrap();
        {
            let conn = create_legacy_db(&dir.path().to_path_buf(), true);
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO query_history (id, config_id, database, sql, executed_at,
                     execution_time_ms, rows_affected, success, error_message, schema)
                 VALUES ('h1', 'cfg-legacy', 'app', 'SELECT 1', ?1, 10, 1, 1, NULL, NULL),
                        ('h2', 'cfg-legacy', 'app', 'SELECT 2', ?1, 20, 0, 1, NULL, 'public')",
                [now.clone()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO favorite_queries (id, config_id, title, sql, created_at)
                 VALUES ('f1', 'cfg-legacy', 'My Fav', 'SELECT 3', ?1)",
                [&now],
            )
            .unwrap();
        }

        let db = HistoryDb::open(dir.path()).unwrap();

        // Physical columns renamed on both tables.
        let db_conn = db.conn.lock().unwrap();
        let qh_cols = column_names(&db_conn, "query_history");
        assert!(qh_cols.contains(&"connection_id".to_string()));
        assert!(!qh_cols.contains(&"config_id".to_string()));
        let fq_cols = column_names(&db_conn, "favorite_queries");
        assert!(fq_cols.contains(&"connection_id".to_string()));
        assert!(!fq_cols.contains(&"config_id".to_string()));

        // New indexes exist; every legacy config_id index is gone.
        let indexes: Vec<String> = {
            let mut stmt = db_conn
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type='index'
                     AND tbl_name IN ('query_history','favorite_queries')",
                )
                .unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        assert!(indexes.contains(&"idx_query_history_connection_id".to_string()));
        assert!(indexes.contains(&"idx_query_history_connection_db".to_string()));
        assert!(indexes.contains(&"idx_favorite_queries_connection_id".to_string()));
        for stale in [
            "idx_query_history_config_id",
            "idx_query_history_config_db",
            "idx_favorite_queries_config_id",
        ] {
            assert!(
                !indexes.contains(&stale.to_string()),
                "legacy index {stale} must be dropped"
            );
        }

        // Schema version stamped to 4.
        let version: i32 = db_conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, 4);

        // Data rows survive the rename and remain filterable by connection.
        drop(db_conn);
        let history = db
            .get_query_history(10, Some("cfg-legacy"), None, None)
            .unwrap();
        assert_eq!(history.len(), 2, "history rows must be preserved");
        assert!(history.iter().any(|e| e.sql == "SELECT 1"));
        assert!(history.iter().any(|e| e.sql == "SELECT 2"));
        let favs = db.get_favorite_queries(Some("cfg-legacy")).unwrap();
        assert_eq!(favs.len(), 1, "favorite row must be preserved");
        assert_eq!(favs[0].title, "My Fav");
        assert_eq!(favs[0].sql, "SELECT 3");
    }

    #[test]
    fn empty_v2_database_migrates_cleanly_through_the_full_ring() {
        // v2 starting point: pre-rename columns, no `schema` column yet, no
        // data rows. The v1→v2 guard must not fire (no connection_id column),
        // and the ring must land on v4 with final-state naming.
        let dir = tempfile::tempdir().unwrap();
        {
            let _conn = create_legacy_db(&dir.path().to_path_buf(), false);
        }

        let db = HistoryDb::open(dir.path()).unwrap();

        let db_conn = db.conn.lock().unwrap();
        let qh_cols = column_names(&db_conn, "query_history");
        assert!(qh_cols.contains(&"connection_id".to_string()));
        assert!(qh_cols.contains(&"schema".to_string()));
        assert!(!qh_cols.contains(&"config_id".to_string()));
        let version: i32 = db_conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, 4);
        drop(db_conn);

        assert!(db
            .get_query_history(10, None, None, None)
            .unwrap()
            .is_empty());
        assert!(db.get_favorite_queries(None).unwrap().is_empty());
    }
}
