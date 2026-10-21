//! Schema creation and the versioned migration ring.
//!
//! Split out of `history_db.rs` so the startup path reads on its own: open a
//! database, make the tables the current version expects, then step anything
//! older forward. Each step is guarded by a probe rather than by assuming a
//! shape, because the oldest supported install is a real starting point here —
//! the v1→v4 ring has to complete for a library that never had a
//! `favorite_queries` table, and failing that aborts startup for the users who
//! need it most.

use super::LEGACY_FAVORITES_TABLE;
use super::{HistoryDb, HistoryDbError};
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::path::Path;

use super::query_history::trim_query_history;
use super::rename_aside;
use super::table_exists;
use super::workflow_history::trim_workflow_history;

/// Legacy JSON entry format (has `connectionId` instead of `configId`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LegacyQueryHistoryEntry {
    id: String,
    connection_id: String,
    database: String,
    sql: String,
    executed_at: chrono::DateTime<chrono::Utc>,
    execution_time_ms: u64,
    rows_affected: Option<u64>,
    success: bool,
    error_message: Option<String>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LegacyWorkflowHistoryEntry {
    pub id: String,
    #[serde(alias = "skillId")]
    pub workflow_id: String,
    #[serde(alias = "skillName")]
    pub workflow_name: String,
    pub variables: serde_json::Value,
    pub result: datazen_feature_core::workflow::model::WorkflowExecutionResult,
    pub created_at: String,
}

/// Whether `table` has a column named `column`.
pub(super) fn has_column(
    conn: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, HistoryDbError> {
    if !table_exists(conn, table)? {
        return Ok(false);
    }
    // A `SELECT <column> FROM <table> LIMIT 0` is the cheapest portable way to
    // ask; table and column names here are compile-time constants.
    let probe = format!("SELECT {column} FROM {table} LIMIT 0");
    Ok(conn.prepare(&probe).is_ok())
}

impl HistoryDb {
    pub(super) fn init_schema(&self) -> Result<(), HistoryDbError> {
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

                CREATE TABLE IF NOT EXISTS migration_run_history (
                    id TEXT PRIMARY KEY NOT NULL,
                    operation TEXT NOT NULL,
                    status TEXT NOT NULL,
                    outcome TEXT NOT NULL,
                    phase TEXT NOT NULL,
                    profile_id TEXT,
                    profile_revision TEXT,
                    source_connection_id TEXT,
                    target_connection_id TEXT,
                    started_at TEXT NOT NULL,
                    finished_at TEXT,
                    selected_count INTEGER NOT NULL DEFAULT 0,
                    committed_count INTEGER NOT NULL DEFAULT 0,
                    failed_count INTEGER NOT NULL DEFAULT 0,
                    conflict_count INTEGER NOT NULL DEFAULT 0,
                    cancelled INTEGER NOT NULL DEFAULT 0,
                    rollback_outcome TEXT NOT NULL DEFAULT 'notRequired',
                    error_summary TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_migration_run_started
                    ON migration_run_history(started_at DESC);
                CREATE INDEX IF NOT EXISTS idx_migration_run_profile
                    ON migration_run_history(profile_id, started_at DESC);
                ",
            )?;
            conn.execute(
                "UPDATE migration_run_history SET status = 'interrupted', outcome = 'unknown', \
                 phase = 'interrupted', finished_at = COALESCE(finished_at, ?1), \
                 rollback_outcome = CASE WHEN rollback_outcome = 'notRequired' THEN 'unknown' ELSE rollback_outcome END \
                 WHERE status = 'running'",
                params![chrono::Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })
    }

    pub(super) fn run_migrations(&self) -> Result<(), HistoryDbError> {
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
                    INSERT OR IGNORE INTO schema_version (version) VALUES (2);
                    ",
                )?;
                // `favorite_queries` used to be created here. It is not
                // created any more: favorites live in files under the
                // favorites root, and an install that still has the table
                // gets it exported and archived instead (see
                // `read_legacy_favorites`). Creating it here would hand every
                // new install a table the app has no writer for.
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
                if table_exists(conn, LEGACY_FAVORITES_TABLE)? {
                    rename_column(conn, LEGACY_FAVORITES_TABLE, "config_id", "connection_id")?;
                }

                // The favorites index is only rebuilt when the table still
                // exists. A database that never had one (every install from
                // now on) would otherwise fail this batch outright, since
                // SQLite cannot index a table that was never created.
                let favorites_index_ddl = if table_exists(conn, LEGACY_FAVORITES_TABLE)? {
                    "CREATE INDEX IF NOT EXISTS idx_favorite_queries_connection_id
                        ON favorite_queries(connection_id);"
                } else {
                    ""
                };
                conn.execute_batch(&format!(
                    "
                    DROP INDEX IF EXISTS idx_query_history_connection_id;
                    DROP INDEX IF EXISTS idx_query_history_config_id;
                    DROP INDEX IF EXISTS idx_query_history_config_db;
                    DROP INDEX IF EXISTS idx_favorite_queries_config_id;
                    DROP INDEX IF EXISTS idx_favorite_queries_connection_id;
                    CREATE INDEX IF NOT EXISTS idx_query_history_connection_id
                        ON query_history(connection_id);
                    CREATE INDEX IF NOT EXISTS idx_query_history_connection_db
                        ON query_history(connection_id, database);
                    {favorites_index_ddl}
                    INSERT OR IGNORE INTO schema_version (version) VALUES (4);
                    "
                ))?;
                tracing::info!("Database schema migrated to version 4");
            }
            Ok(())
        })
    }

    /// Import the pre-SQLite JSON layouts, once each.
    ///
    /// Each importer renames its source aside rather than deleting it, so a
    /// half-finished migration is retried on the next launch instead of being
    /// indistinguishable from a completed one.
    pub(super) fn migrate_legacy_json(&self, data_dir: &Path) -> Result<(), HistoryDbError> {
        migrate_queries_json(self, data_dir)?;
        migrate_workflow_json_dir(self, data_dir)?;
        Ok(())
    }
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
            if !path.extension().is_some_and(|ext| ext == "json") {
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

#[cfg(test)]
#[path = "migration_startpoint_tests.rs"]
mod migration_startpoint_tests;
