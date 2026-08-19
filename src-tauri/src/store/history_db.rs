//! Local SQLite persistence for SQL query history and workflow execution history.

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
                let has_connection_id_col = conn
                    .prepare("SELECT connection_id FROM query_history LIMIT 0")
                    .is_ok();

                if has_connection_id_col {
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
            Ok(())
        })
    }

    fn migrate_legacy_json(&self, data_dir: &Path) -> Result<(), HistoryDbError> {
        migrate_queries_json(self, data_dir)?;
        migrate_workflow_json_dir(self, data_dir)?;
        Ok(())
    }

    // ── Query history ─────────────────────────────────────────────────────

    pub fn add_query_history(&self, entry: QueryHistoryEntry) -> Result<(), HistoryDbError> {
        self.with_conn(|conn| {
            let dominated: Option<String> = match conn.query_row(
                "SELECT sql FROM query_history WHERE config_id = ?1 ORDER BY executed_at DESC LIMIT 1",
                params![entry.config_id],
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
                        SELECT id FROM query_history WHERE config_id = ?6 ORDER BY executed_at DESC LIMIT 1
                     )",
                    params![
                        entry.executed_at.to_rfc3339(),
                        entry.execution_time_ms as i64,
                        entry.rows_affected.map(|v| v as i64),
                        entry.success as i32,
                        entry.error_message,
                        entry.config_id,
                    ],
                )?;
            } else {
                conn.execute(
                    "INSERT INTO query_history (
                        id, config_id, database, sql, executed_at,
                        execution_time_ms, rows_affected, success, error_message
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        entry.id,
                        entry.config_id,
                        entry.database,
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
        config_id: Option<&str>,
    ) -> Result<Vec<QueryHistoryEntry>, HistoryDbError> {
        self.with_conn(|conn| {
            if let Some(cid) = config_id {
                let mut stmt = conn.prepare(
                    "SELECT id, config_id, database, sql, executed_at,
                            execution_time_ms, rows_affected, success, error_message
                     FROM query_history
                     WHERE config_id = ?1
                     ORDER BY executed_at DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![cid, limit as i64], map_query_row)?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(HistoryDbError::from)
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, config_id, database, sql, executed_at,
                            execution_time_ms, rows_affected, success, error_message
                     FROM query_history
                     ORDER BY executed_at DESC
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit as i64], map_query_row)?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(HistoryDbError::from)
            }
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
                "INSERT INTO favorite_queries (id, config_id, title, sql, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    fav.id,
                    fav.config_id,
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
        config_id: Option<&str>,
    ) -> Result<Vec<FavoriteQuery>, HistoryDbError> {
        self.with_conn(|conn| {
            if let Some(cid) = config_id {
                let mut stmt = conn.prepare(
                    "SELECT id, config_id, title, sql, created_at
                     FROM favorite_queries
                     WHERE config_id = ?1
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map(params![cid], map_favorite_row)?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(HistoryDbError::from)
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, config_id, title, sql, created_at
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

fn map_query_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueryHistoryEntry> {
    let executed_at: String = row.get(4)?;
    let executed_at = DateTime::parse_from_rfc3339(&executed_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let rows_affected: Option<i64> = row.get(6)?;
    Ok(QueryHistoryEntry {
        id: row.get(0)?,
        config_id: row.get(1)?,
        database: row.get(2)?,
        sql: row.get(3)?,
        executed_at,
        execution_time_ms: row.get::<_, i64>(5)? as u64,
        rows_affected: rows_affected.map(|v| v as u64),
        success: row.get::<_, i32>(7)? != 0,
        error_message: row.get(8)?,
    })
}

fn map_favorite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FavoriteQuery> {
    let created_at: String = row.get(4)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    Ok(FavoriteQuery {
        id: row.get(0)?,
        config_id: row.get(1)?,
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
                    id, config_id, database, sql, executed_at,
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
            config_id: "cfg1".into(),
            database: "app".into(),
            sql: sql.into(),
            executed_at: Utc::now() - Duration::days(days_ago),
            execution_time_ms: 10,
            rows_affected: Some(1),
            success: true,
            error_message: None,
        }
    }

    fn sample_query_for_config(sql: &str, config_id: &str) -> QueryHistoryEntry {
        QueryHistoryEntry {
            id: Uuid::new_v4().to_string(),
            config_id: config_id.into(),
            database: "app".into(),
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
        let loaded = db.get_query_history(10, None).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].sql, "SELECT 1");
        assert!(!dir.path().join("history/queries.json").exists());
        assert!(dir.path().join("history/queries.json.migrated").is_file());

        // Re-open must not duplicate.
        let db2 = HistoryDb::open(dir.path()).unwrap();
        assert_eq!(db2.get_query_history(10, None).unwrap().len(), 1);
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
    fn query_history_config_id_filter() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();
        db.add_query_history(sample_query_for_config("SELECT 1", "cfg-a"))
            .unwrap();
        db.add_query_history(sample_query_for_config("SELECT 2", "cfg-b"))
            .unwrap();
        db.add_query_history(sample_query_for_config("SELECT 3", "cfg-a"))
            .unwrap();

        let all = db.get_query_history(10, None).unwrap();
        assert_eq!(all.len(), 3);

        let a_only = db.get_query_history(10, Some("cfg-a")).unwrap();
        assert_eq!(a_only.len(), 2);
        assert!(a_only.iter().all(|e| e.config_id == "cfg-a"));

        let b_only = db.get_query_history(10, Some("cfg-b")).unwrap();
        assert_eq!(b_only.len(), 1);
        assert_eq!(b_only[0].config_id, "cfg-b");
    }

    #[test]
    fn query_history_dedup_scoped_by_config_id() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();
        db.add_query_history(sample_query_for_config("SELECT 1", "cfg-a"))
            .unwrap();
        db.add_query_history(sample_query_for_config("SELECT 1", "cfg-b"))
            .unwrap();
        let all = db.get_query_history(10, None).unwrap();
        assert_eq!(
            all.len(),
            2,
            "same SQL on different configs should not dedup"
        );
    }

    #[test]
    fn favorite_queries_crud() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();

        assert!(db.get_favorite_queries(None).unwrap().is_empty());

        let fav = FavoriteQuery {
            id: "fav1".into(),
            config_id: "cfg-a".into(),
            title: "My query".into(),
            sql: "SELECT 1".into(),
            created_at: Utc::now(),
        };
        db.add_favorite_query(fav).unwrap();

        let fav2 = FavoriteQuery {
            id: "fav2".into(),
            config_id: "cfg-b".into(),
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
        let loaded = db2.get_query_history(10, None).unwrap();
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
        assert_eq!(db.get_query_history(10, None).unwrap().len(), 1);
        assert_eq!(db.get_query_history(10, None).unwrap()[0].sql, "recent");
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
        assert!(db.get_query_history(10, None).unwrap().is_empty());
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
        let history = db.get_query_history(10, None).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].execution_time_ms, 99);
    }
}
