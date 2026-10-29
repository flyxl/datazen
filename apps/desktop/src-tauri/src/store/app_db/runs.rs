//! Widget run records: the result rows a widget produced, and the cap that
//! keeps one widget from filling the database.

use super::{AppDb, AppDbError};
use chrono::Utc;
pub use datazen::dashboard::records::WidgetRunRecord;
use rusqlite::params;

/// Rows kept per widget. The shared dashboard core declares a constant of the
/// same name and value; the two must agree, or the dashboard truncates to one
/// number while the database stores another.
pub const MAX_RUN_ROWS: usize = datazen::dashboard::execute::MAX_RUN_ROWS;

impl AppDb {
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
        alert_fired: row.get::<_, Option<i32>>(12)?.map(|v| v != 0),
        alert_value: row.get(13)?,
    })
}
