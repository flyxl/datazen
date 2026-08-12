//! Unified SQLite persistence for workflows, dashboards, widgets, and widget runs.
//!
//! File: `{data_dir}/datazen.sqlite`

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const APP_DB_FILE: &str = "datazen.sqlite";
pub const SCHEMA_VERSION: i32 = 1;
pub const MAX_RUN_ROWS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowVisibility {
    User,
    DashboardHidden,
}

impl WorkflowVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::DashboardHidden => "dashboardHidden",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Self::User),
            "dashboardHidden" => Some(Self::DashboardHidden),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub visibility: WorkflowVisibility,
    pub definition_yaml: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardRecord {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub layout_cols: u32,
    pub layout_row_height: u32,
    pub enabled: bool,
    pub refresh_paused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRecord {
    pub id: String,
    pub dashboard_id: String,
    pub title: String,
    pub workflow_id: String,
    pub view_mode: String,
    pub chart_config_json: Option<String>,
    pub layout_x: u32,
    pub layout_y: u32,
    pub layout_w: u32,
    pub layout_h: u32,
    pub refresh_mode: String,
    pub refresh_sec: Option<u32>,
    pub alert_json: Option<String>,
    pub enabled: bool,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRunRecord {
    pub id: String,
    pub dashboard_id: String,
    pub widget_id: String,
    pub workflow_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: String,
    pub error: Option<String>,
    pub row_count: u32,
    pub columns_json: String,
    pub rows_json: String,
    pub variables_json: Option<String>,
    pub alert_fired: Option<bool>,
    pub alert_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWorkflowRef {
    pub workflow_id: String,
    pub dashboard_id: String,
    pub widget_id: String,
    pub dashboard_name: String,
    pub widget_title: String,
}

#[derive(Debug, Error)]
pub enum AppDbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Workflow in use by dashboards: {0}")]
    WorkflowInUse(String),

    #[error("{0}")]
    Other(String),
}

pub struct AppDb {
    db_path: PathBuf,
    conn: Mutex<Connection>,
}

impl AppDb {
    pub fn open(data_dir: &Path) -> Result<Arc<Self>, AppDbError> {
        let db_path = data_dir.join(APP_DB_FILE);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppDbError::Other(e.to_string()))?;
        }
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        let db = Arc::new(Self {
            db_path,
            conn: Mutex::new(conn),
        });
        db.init_schema()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Arc<Self>, AppDbError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let db = Arc::new(Self {
            db_path: PathBuf::from(":memory:"),
            conn: Mutex::new(conn),
        });
        db.init_schema()?;
        Ok(db)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    fn with_conn<T, F>(&self, f: F) -> Result<T, AppDbError>
    where
        F: FnOnce(&Connection) -> Result<T, AppDbError>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppDbError::Other(format!("app db lock poisoned: {e}")))?;
        f(&conn)
    }

    fn init_schema(&self) -> Result<(), AppDbError> {
        self.with_conn(|conn| {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS schema_migrations (
                  version INTEGER PRIMARY KEY NOT NULL,
                  applied_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS workflows (
                  id TEXT PRIMARY KEY NOT NULL,
                  name TEXT NOT NULL,
                  description TEXT NOT NULL DEFAULT '',
                  visibility TEXT NOT NULL DEFAULT 'user'
                    CHECK (visibility IN ('user', 'dashboardHidden')),
                  definition_yaml TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  created_at TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_workflows_visibility
                  ON workflows(visibility);

                CREATE TABLE IF NOT EXISTS dashboards (
                  id TEXT PRIMARY KEY NOT NULL,
                  name TEXT NOT NULL,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  layout_cols INTEGER NOT NULL DEFAULT 12,
                  layout_row_height INTEGER NOT NULL DEFAULT 80,
                  enabled INTEGER NOT NULL DEFAULT 1,
                  refresh_paused INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS widgets (
                  id TEXT PRIMARY KEY NOT NULL,
                  dashboard_id TEXT NOT NULL REFERENCES dashboards(id) ON DELETE CASCADE,
                  title TEXT NOT NULL,
                  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE RESTRICT,
                  view_mode TEXT NOT NULL CHECK (view_mode IN ('chart', 'table')),
                  chart_config_json TEXT,
                  layout_x INTEGER NOT NULL,
                  layout_y INTEGER NOT NULL,
                  layout_w INTEGER NOT NULL,
                  layout_h INTEGER NOT NULL,
                  refresh_mode TEXT NOT NULL
                    CHECK (refresh_mode IN ('manual', 'onOpen', 'interval')),
                  refresh_sec INTEGER,
                  alert_json TEXT,
                  enabled INTEGER NOT NULL DEFAULT 1,
                  sort_order INTEGER NOT NULL DEFAULT 0,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_widgets_dashboard
                  ON widgets(dashboard_id, sort_order);
                CREATE INDEX IF NOT EXISTS idx_widgets_workflow
                  ON widgets(workflow_id);

                CREATE TABLE IF NOT EXISTS widget_runs (
                  id TEXT PRIMARY KEY NOT NULL,
                  dashboard_id TEXT NOT NULL,
                  widget_id TEXT NOT NULL REFERENCES widgets(id) ON DELETE CASCADE,
                  workflow_id TEXT NOT NULL,
                  started_at TEXT NOT NULL,
                  finished_at TEXT NOT NULL,
                  status TEXT NOT NULL CHECK (status IN ('ok', 'error', 'timeout')),
                  error TEXT,
                  row_count INTEGER NOT NULL DEFAULT 0,
                  columns_json TEXT NOT NULL,
                  rows_json TEXT NOT NULL,
                  variables_json TEXT,
                  alert_fired INTEGER,
                  alert_value REAL
                );
                CREATE INDEX IF NOT EXISTS idx_widget_runs_widget_started
                  ON widget_runs(widget_id, started_at DESC);

                CREATE TABLE IF NOT EXISTS widget_latest_run (
                  widget_id TEXT PRIMARY KEY NOT NULL REFERENCES widgets(id) ON DELETE CASCADE,
                  run_id TEXT NOT NULL REFERENCES widget_runs(id) ON DELETE CASCADE,
                  started_at TEXT NOT NULL,
                  status TEXT NOT NULL
                );
                "#,
            )?;

            let applied: Option<i32> = conn
                .query_row(
                    "SELECT version FROM schema_migrations WHERE version = ?1",
                    params![SCHEMA_VERSION],
                    |row| row.get(0),
                )
                .optional()?;
            if applied.is_none() {
                conn.execute(
                    "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                    params![SCHEMA_VERSION, Utc::now().to_rfc3339()],
                )?;
            }
            Ok(())
        })
    }

    // ── Workflows ─────────────────────────────────────────────────────────

    pub fn upsert_workflow(&self, record: &WorkflowRecord) -> Result<(), AppDbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO workflows
                    (id, name, description, visibility, definition_yaml, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    description = excluded.description,
                    visibility = excluded.visibility,
                    definition_yaml = excluded.definition_yaml,
                    updated_at = excluded.updated_at",
                params![
                    record.id,
                    record.name,
                    record.description,
                    record.visibility.as_str(),
                    record.definition_yaml,
                    record.created_at,
                    record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_workflow(&self, id: &str) -> Result<WorkflowRecord, AppDbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, description, visibility, definition_yaml, created_at, updated_at
                 FROM workflows WHERE id = ?1",
                params![id],
                map_workflow_row,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => AppDbError::NotFound(id.into()),
                other => AppDbError::from(other),
            })
        })
    }

    pub fn list_workflows(
        &self,
        visibility: Option<WorkflowVisibility>,
    ) -> Result<Vec<WorkflowRecord>, AppDbError> {
        self.with_conn(|conn| {
            let mut rows = Vec::new();
            if let Some(v) = visibility {
                let mut stmt = conn.prepare(
                    "SELECT id, name, description, visibility, definition_yaml, created_at, updated_at
                     FROM workflows WHERE visibility = ?1 ORDER BY name COLLATE NOCASE",
                )?;
                let iter = stmt.query_map(params![v.as_str()], map_workflow_row)?;
                for row in iter {
                    rows.push(row?);
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, name, description, visibility, definition_yaml, created_at, updated_at
                     FROM workflows ORDER BY name COLLATE NOCASE",
                )?;
                let iter = stmt.query_map([], map_workflow_row)?;
                for row in iter {
                    rows.push(row?);
                }
            }
            Ok(rows)
        })
    }

    pub fn delete_workflow(&self, id: &str) -> Result<(), AppDbError> {
        let refs = self.find_workflow_refs(id)?;
        if !refs.is_empty() {
            let summary = refs
                .iter()
                .map(|r| format!("{} / {}", r.dashboard_name, r.widget_title))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(AppDbError::WorkflowInUse(summary));
        }
        self.with_conn(|conn| {
            let n = conn.execute("DELETE FROM workflows WHERE id = ?1", params![id])?;
            if n == 0 {
                return Err(AppDbError::NotFound(id.into()));
            }
            Ok(())
        })
    }

    pub fn find_workflow_refs(&self, workflow_id: &str) -> Result<Vec<DashboardWorkflowRef>, AppDbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT w.workflow_id, w.dashboard_id, w.id, d.name, w.title
                 FROM widgets w
                 JOIN dashboards d ON d.id = w.dashboard_id
                 WHERE w.workflow_id = ?1
                 ORDER BY d.name, w.title",
            )?;
            let iter = stmt.query_map(params![workflow_id], |row| {
                Ok(DashboardWorkflowRef {
                    workflow_id: row.get(0)?,
                    dashboard_id: row.get(1)?,
                    widget_id: row.get(2)?,
                    dashboard_name: row.get(3)?,
                    widget_title: row.get(4)?,
                })
            })?;
            let mut refs = Vec::new();
            for row in iter {
                refs.push(row?);
            }
            Ok(refs)
        })
    }

    // ── Dashboards ────────────────────────────────────────────────────────

    pub fn upsert_dashboard(&self, record: &DashboardRecord) -> Result<(), AppDbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO dashboards
                    (id, name, created_at, updated_at, layout_cols, layout_row_height, enabled, refresh_paused)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    updated_at = excluded.updated_at,
                    layout_cols = excluded.layout_cols,
                    layout_row_height = excluded.layout_row_height,
                    enabled = excluded.enabled,
                    refresh_paused = excluded.refresh_paused",
                params![
                    record.id,
                    record.name,
                    record.created_at,
                    record.updated_at,
                    record.layout_cols as i64,
                    record.layout_row_height as i64,
                    record.enabled as i32,
                    record.refresh_paused as i32,
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_dashboard(&self, id: &str) -> Result<DashboardRecord, AppDbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, created_at, updated_at, layout_cols, layout_row_height, enabled, refresh_paused
                 FROM dashboards WHERE id = ?1",
                params![id],
                map_dashboard_row,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => AppDbError::NotFound(id.into()),
                other => AppDbError::from(other),
            })
        })
    }

    pub fn list_dashboards(&self) -> Result<Vec<DashboardRecord>, AppDbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, created_at, updated_at, layout_cols, layout_row_height, enabled, refresh_paused
                 FROM dashboards ORDER BY name COLLATE NOCASE",
            )?;
            let iter = stmt.query_map([], map_dashboard_row)?;
            let mut rows = Vec::new();
            for row in iter {
                rows.push(row?);
            }
            Ok(rows)
        })
    }

    pub fn delete_dashboard(&self, id: &str) -> Result<(), AppDbError> {
        self.with_conn(|conn| {
            let n = conn.execute("DELETE FROM dashboards WHERE id = ?1", params![id])?;
            if n == 0 {
                return Err(AppDbError::NotFound(id.into()));
            }
            Ok(())
        })
    }

    pub fn set_dashboard_refresh_paused(&self, id: &str, paused: bool) -> Result<(), AppDbError> {
        self.with_conn(|conn| {
            let n = conn.execute(
                "UPDATE dashboards SET refresh_paused = ?1, updated_at = ?2 WHERE id = ?3",
                params![paused as i32, Utc::now().to_rfc3339(), id],
            )?;
            if n == 0 {
                return Err(AppDbError::NotFound(id.into()));
            }
            Ok(())
        })
    }

    // ── Widgets ───────────────────────────────────────────────────────────

    pub fn upsert_widget(&self, record: &WidgetRecord) -> Result<(), AppDbError> {
        validate_view_mode(&record.view_mode)?;
        validate_refresh_pair(Some(&record.refresh_mode), record.refresh_sec)?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO widgets (
                    id, dashboard_id, title, workflow_id, view_mode, chart_config_json,
                    layout_x, layout_y, layout_w, layout_h,
                    refresh_mode, refresh_sec, alert_json, enabled, sort_order,
                    created_at, updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, ?8, ?9, ?10,
                    ?11, ?12, ?13, ?14, ?15,
                    ?16, ?17
                 )
                 ON CONFLICT(id) DO UPDATE SET
                    dashboard_id = excluded.dashboard_id,
                    title = excluded.title,
                    workflow_id = excluded.workflow_id,
                    view_mode = excluded.view_mode,
                    chart_config_json = excluded.chart_config_json,
                    layout_x = excluded.layout_x,
                    layout_y = excluded.layout_y,
                    layout_w = excluded.layout_w,
                    layout_h = excluded.layout_h,
                    refresh_mode = excluded.refresh_mode,
                    refresh_sec = excluded.refresh_sec,
                    alert_json = excluded.alert_json,
                    enabled = excluded.enabled,
                    sort_order = excluded.sort_order,
                    updated_at = excluded.updated_at",
                params![
                    record.id,
                    record.dashboard_id,
                    record.title,
                    record.workflow_id,
                    record.view_mode,
                    record.chart_config_json,
                    record.layout_x as i64,
                    record.layout_y as i64,
                    record.layout_w as i64,
                    record.layout_h as i64,
                    record.refresh_mode,
                    record.refresh_sec.map(|v| v as i64),
                    record.alert_json,
                    record.enabled as i32,
                    record.sort_order,
                    record.created_at,
                    record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_widgets(&self, dashboard_id: &str) -> Result<Vec<WidgetRecord>, AppDbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, dashboard_id, title, workflow_id, view_mode, chart_config_json,
                        layout_x, layout_y, layout_w, layout_h,
                        refresh_mode, refresh_sec, alert_json, enabled, sort_order,
                        created_at, updated_at
                 FROM widgets WHERE dashboard_id = ?1 ORDER BY sort_order, title",
            )?;
            let iter = stmt.query_map(params![dashboard_id], map_widget_row)?;
            let mut rows = Vec::new();
            for row in iter {
                rows.push(row?);
            }
            Ok(rows)
        })
    }

    pub fn get_widget(&self, id: &str) -> Result<WidgetRecord, AppDbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, dashboard_id, title, workflow_id, view_mode, chart_config_json,
                        layout_x, layout_y, layout_w, layout_h,
                        refresh_mode, refresh_sec, alert_json, enabled, sort_order,
                        created_at, updated_at
                 FROM widgets WHERE id = ?1",
                params![id],
                map_widget_row,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => AppDbError::NotFound(id.into()),
                other => AppDbError::from(other),
            })
        })
    }

    pub fn delete_widget(&self, id: &str) -> Result<(), AppDbError> {
        self.with_conn(|conn| {
            let n = conn.execute("DELETE FROM widgets WHERE id = ?1", params![id])?;
            if n == 0 {
                return Err(AppDbError::NotFound(id.into()));
            }
            Ok(())
        })
    }

    // ── Widget runs ───────────────────────────────────────────────────────

    pub fn write_run(
        &self,
        mut run: WidgetRunRecord,
        retention_count: u32,
        retention_days: u32,
    ) -> Result<(), AppDbError> {
        // Cap rows in JSON payload
        if let Ok(mut rows) = serde_json::from_str::<Vec<serde_json::Value>>(&run.rows_json) {
            if rows.len() > MAX_RUN_ROWS {
                rows.truncate(MAX_RUN_ROWS);
                run.rows_json = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into());
                run.row_count = run.row_count.min(MAX_RUN_ROWS as u32);
            }
        }

        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO widget_runs (
                    id, dashboard_id, widget_id, workflow_id, started_at, finished_at,
                    status, error, row_count, columns_json, rows_json, variables_json,
                    alert_fired, alert_value
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14
                 )",
                params![
                    run.id,
                    run.dashboard_id,
                    run.widget_id,
                    run.workflow_id,
                    run.started_at,
                    run.finished_at,
                    run.status,
                    run.error,
                    run.row_count as i64,
                    run.columns_json,
                    run.rows_json,
                    run.variables_json,
                    run.alert_fired.map(|v| v as i32),
                    run.alert_value,
                ],
            )?;
            conn.execute(
                "INSERT INTO widget_latest_run (widget_id, run_id, started_at, status)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(widget_id) DO UPDATE SET
                    run_id = excluded.run_id,
                    started_at = excluded.started_at,
                    status = excluded.status",
                params![run.widget_id, run.id, run.started_at, run.status],
            )?;
            Ok(())
        })?;

        self.prune_runs(&run.widget_id, retention_count, retention_days)?;
        Ok(())
    }

    pub fn list_run_index(
        &self,
        widget_id: &str,
        limit: u32,
    ) -> Result<Vec<WidgetRunRecord>, AppDbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, dashboard_id, widget_id, workflow_id, started_at, finished_at,
                        status, error, row_count, columns_json, rows_json, variables_json,
                        alert_fired, alert_value
                 FROM widget_runs
                 WHERE widget_id = ?1
                 ORDER BY started_at DESC
                 LIMIT ?2",
            )?;
            let iter = stmt.query_map(params![widget_id, limit as i64], map_run_row)?;
            let mut rows = Vec::new();
            for row in iter {
                rows.push(row?);
            }
            Ok(rows)
        })
    }

    pub fn get_run(&self, run_id: &str) -> Result<WidgetRunRecord, AppDbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, dashboard_id, widget_id, workflow_id, started_at, finished_at,
                        status, error, row_count, columns_json, rows_json, variables_json,
                        alert_fired, alert_value
                 FROM widget_runs WHERE id = ?1",
                params![run_id],
                map_run_row,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => AppDbError::NotFound(run_id.into()),
                other => AppDbError::from(other),
            })
        })
    }

    fn prune_runs(
        &self,
        widget_id: &str,
        retention_count: u32,
        retention_days: u32,
    ) -> Result<(), AppDbError> {
        self.with_conn(|conn| {
            if retention_count > 0 {
                // SQLite forbids deleting from a table while selecting it unless nested.
                conn.execute(
                    "DELETE FROM widget_runs
                     WHERE widget_id = ?1
                       AND id NOT IN (
                         SELECT id FROM (
                           SELECT id FROM widget_runs
                           WHERE widget_id = ?1
                           ORDER BY started_at DESC
                           LIMIT ?2
                         )
                       )",
                    params![widget_id, retention_count as i64],
                )?;
            }
            if retention_days > 0 {
                let cutoff =
                    (Utc::now() - chrono::Duration::days(retention_days as i64)).to_rfc3339();
                conn.execute(
                    "DELETE FROM widget_runs
                     WHERE widget_id = ?1
                       AND started_at < ?2
                       AND id NOT IN (
                         SELECT run_id FROM widget_latest_run WHERE widget_id = ?1
                       )",
                    params![widget_id, cutoff],
                )?;
            }
            Ok(())
        })
    }
}

fn validate_view_mode(mode: &str) -> Result<(), AppDbError> {
    match mode {
        "chart" | "table" => Ok(()),
        _ => Err(AppDbError::Validation(format!("invalid view_mode: {mode}"))),
    }
}

fn validate_refresh_pair(mode: Option<&str>, refresh_sec: Option<u32>) -> Result<(), AppDbError> {
    let Some(mode) = mode else {
        return Ok(());
    };
    match mode {
        "manual" | "onOpen" => Ok(()),
        "interval" => {
            let sec = refresh_sec.unwrap_or(0);
            if sec < 30 {
                return Err(AppDbError::Validation(
                    "interval refresh_sec must be >= 30".into(),
                ));
            }
            Ok(())
        }
        _ => Err(AppDbError::Validation(format!("invalid refresh_mode: {mode}"))),
    }
}

fn map_workflow_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowRecord> {
    let visibility_raw: String = row.get(3)?;
    let visibility = WorkflowVisibility::parse(&visibility_raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("bad visibility: {visibility_raw}"),
            )),
        )
    })?;
    Ok(WorkflowRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        visibility,
        definition_yaml: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn map_dashboard_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DashboardRecord> {
    Ok(DashboardRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        layout_cols: row.get::<_, i64>(4)? as u32,
        layout_row_height: row.get::<_, i64>(5)? as u32,
        enabled: row.get::<_, i32>(6)? != 0,
        refresh_paused: row.get::<_, i32>(7)? != 0,
    })
}

fn map_widget_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WidgetRecord> {
    Ok(WidgetRecord {
        id: row.get(0)?,
        dashboard_id: row.get(1)?,
        title: row.get(2)?,
        workflow_id: row.get(3)?,
        view_mode: row.get(4)?,
        chart_config_json: row.get(5)?,
        layout_x: row.get::<_, i64>(6)? as u32,
        layout_y: row.get::<_, i64>(7)? as u32,
        layout_w: row.get::<_, i64>(8)? as u32,
        layout_h: row.get::<_, i64>(9)? as u32,
        refresh_mode: row.get(10)?,
        refresh_sec: row
            .get::<_, Option<i64>>(11)?
            .map(|v| v as u32),
        alert_json: row.get(12)?,
        enabled: row.get::<_, i32>(13)? != 0,
        sort_order: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}

fn map_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WidgetRunRecord> {
    Ok(WidgetRunRecord {
        id: row.get(0)?,
        dashboard_id: row.get(1)?,
        widget_id: row.get(2)?,
        workflow_id: row.get(3)?,
        started_at: row.get(4)?,
        finished_at: row.get(5)?,
        status: row.get(6)?,
        error: row.get(7)?,
        row_count: row.get::<_, i64>(8)? as u32,
        columns_json: row.get(9)?,
        rows_json: row.get(10)?,
        variables_json: row.get(11)?,
        alert_fired: row
            .get::<_, Option<i32>>(12)?
            .map(|v| v != 0),
        alert_value: row.get(13)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> String {
        Utc::now().to_rfc3339()
    }

    fn sample_workflow(id: &str, visibility: WorkflowVisibility) -> WorkflowRecord {
        let ts = now();
        WorkflowRecord {
            id: id.into(),
            name: format!("WF {id}"),
            description: "desc".into(),
            visibility,
            definition_yaml: format!("id: {id}\nname: test\nsteps: []\n"),
            created_at: ts.clone(),
            updated_at: ts,
        }
    }

    fn sample_dashboard(id: &str) -> DashboardRecord {
        let ts = now();
        DashboardRecord {
            id: id.into(),
            name: format!("Dash {id}"),
            created_at: ts.clone(),
            updated_at: ts,
            layout_cols: 12,
            layout_row_height: 80,
            enabled: true,
            refresh_paused: false,
        }
    }

    fn sample_widget(id: &str, dashboard_id: &str, workflow_id: &str) -> WidgetRecord {
        let ts = now();
        WidgetRecord {
            id: id.into(),
            dashboard_id: dashboard_id.into(),
            title: format!("Widget {id}"),
            workflow_id: workflow_id.into(),
            view_mode: "chart".into(),
            chart_config_json: None,
            layout_x: 0,
            layout_y: 0,
            layout_w: 6,
            layout_h: 4,
            refresh_mode: "manual".into(),
            refresh_sec: None,
            alert_json: None,
            enabled: true,
            sort_order: 0,
            created_at: ts.clone(),
            updated_at: ts,
        }
    }

    #[test]
    fn open_creates_schema_version() {
        let db = AppDb::open_in_memory().unwrap();
        let version: i32 = db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT version FROM schema_migrations LIMIT 1",
                    [],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn workflow_crud_and_visibility_filter() {
        let db = AppDb::open_in_memory().unwrap();
        db.upsert_workflow(&sample_workflow("u1", WorkflowVisibility::User))
            .unwrap();
        db.upsert_workflow(&sample_workflow(
            "h1",
            WorkflowVisibility::DashboardHidden,
        ))
        .unwrap();

        let users = db.list_workflows(Some(WorkflowVisibility::User)).unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].id, "u1");

        let all = db.list_workflows(None).unwrap();
        assert_eq!(all.len(), 2);

        let got = db.get_workflow("h1").unwrap();
        assert_eq!(got.visibility, WorkflowVisibility::DashboardHidden);
    }

    #[test]
    fn dashboard_widget_cascade_and_refs() {
        let db = AppDb::open_in_memory().unwrap();
        db.upsert_workflow(&sample_workflow("wf1", WorkflowVisibility::User))
            .unwrap();
        db.upsert_dashboard(&sample_dashboard("d1")).unwrap();
        db.upsert_widget(&sample_widget("w1", "d1", "wf1"))
            .unwrap();

        let refs = db.find_workflow_refs("wf1").unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].widget_title, "Widget w1");

        let err = db.delete_workflow("wf1").unwrap_err();
        assert!(matches!(err, AppDbError::WorkflowInUse(_)));

        db.delete_dashboard("d1").unwrap();
        assert!(db.list_widgets("d1").unwrap().is_empty());
        db.delete_workflow("wf1").unwrap();
    }

    #[test]
    fn widget_fk_requires_workflow() {
        let db = AppDb::open_in_memory().unwrap();
        db.upsert_dashboard(&sample_dashboard("d1")).unwrap();
        let err = db
            .upsert_widget(&sample_widget("w1", "d1", "missing"))
            .unwrap_err();
        assert!(matches!(err, AppDbError::Sqlite(_)));
    }

    #[test]
    fn refresh_interval_validation() {
        let db = AppDb::open_in_memory().unwrap();
        db.upsert_workflow(&sample_workflow("wf1", WorkflowVisibility::User))
            .unwrap();
        db.upsert_dashboard(&sample_dashboard("d1")).unwrap();
        let mut w = sample_widget("w1", "d1", "wf1");
        w.refresh_mode = "interval".into();
        w.refresh_sec = Some(10);
        let err = db.upsert_widget(&w).unwrap_err();
        assert!(matches!(err, AppDbError::Validation(_)));

        w.refresh_sec = Some(30);
        db.upsert_widget(&w).unwrap();
    }

    #[test]
    fn write_run_caps_rows_and_updates_latest() {
        let db = AppDb::open_in_memory().unwrap();
        db.upsert_workflow(&sample_workflow("wf1", WorkflowVisibility::User))
            .unwrap();
        db.upsert_dashboard(&sample_dashboard("d1")).unwrap();
        db.upsert_widget(&sample_widget("w1", "d1", "wf1"))
            .unwrap();

        let big_rows: Vec<Vec<i32>> = (0..600).map(|i| vec![i]).collect();
        let run = WidgetRunRecord {
            id: "r1".into(),
            dashboard_id: "d1".into(),
            widget_id: "w1".into(),
            workflow_id: "wf1".into(),
            started_at: now(),
            finished_at: now(),
            status: "ok".into(),
            error: None,
            row_count: 600,
            columns_json: r#"["n"]"#.into(),
            rows_json: serde_json::to_string(&big_rows).unwrap(),
            variables_json: None,
            alert_fired: None,
            alert_value: None,
        };
        db.write_run(run, 200, 30).unwrap();
        let got = db.get_run("r1").unwrap();
        let rows: Vec<serde_json::Value> = serde_json::from_str(&got.rows_json).unwrap();
        assert_eq!(rows.len(), MAX_RUN_ROWS);

        let latest: String = db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT run_id FROM widget_latest_run WHERE widget_id = 'w1'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(latest, "r1");
    }

    #[test]
    fn pause_dashboard_and_list_runs_order() {
        let db = AppDb::open_in_memory().unwrap();
        db.upsert_workflow(&sample_workflow("wf1", WorkflowVisibility::User))
            .unwrap();
        db.upsert_dashboard(&sample_dashboard("d1")).unwrap();
        db.upsert_widget(&sample_widget("w1", "d1", "wf1"))
            .unwrap();
        db.set_dashboard_refresh_paused("d1", true).unwrap();
        assert!(db.get_dashboard("d1").unwrap().refresh_paused);

        for (id, offset_secs) in [("r1", 10i64), ("r2", 20i64)] {
            let started = (Utc::now() - chrono::Duration::seconds(30 - offset_secs)).to_rfc3339();
            db.write_run(
                WidgetRunRecord {
                    id: id.into(),
                    dashboard_id: "d1".into(),
                    widget_id: "w1".into(),
                    workflow_id: "wf1".into(),
                    started_at: started.clone(),
                    finished_at: started,
                    status: "ok".into(),
                    error: None,
                    row_count: 0,
                    columns_json: "[]".into(),
                    rows_json: "[]".into(),
                    variables_json: None,
                    alert_fired: None,
                    alert_value: None,
                },
                200,
                30,
            )
            .unwrap();
        }
        let list = db.list_run_index("w1", 10).unwrap();
        assert_eq!(list[0].id, "r2");
        assert_eq!(list[1].id, "r1");
    }
}
