//! Dashboards and the widgets on them.

use super::{AppDb, AppDbError};
use chrono::Utc;
pub use datazen::dashboard::records::{DashboardRecord, WidgetRecord};
pub use datazen::dashboard::types::DashboardWorkflowRef;
use rusqlite::params;

impl AppDb {
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
            conn.execute(
                "DELETE FROM widget_latest_run WHERE widget_id = ?1",
                params![id],
            )?;
            conn.execute("DELETE FROM widget_runs WHERE widget_id = ?1", params![id])?;
            let n = conn.execute("DELETE FROM widgets WHERE id = ?1", params![id])?;
            if n == 0 {
                return Err(AppDbError::NotFound(id.into()));
            }
            Ok(())
        })
    }

    // ── Widget runs ───────────────────────────────────────────────────────
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
        _ => Err(AppDbError::Validation(format!(
            "invalid refresh_mode: {mode}"
        ))),
    }
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
        refresh_sec: row.get::<_, Option<i64>>(11)?.map(|v| v as u32),
        alert_json: row.get(12)?,
        enabled: row.get::<_, i32>(13)? != 0,
        sort_order: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}
