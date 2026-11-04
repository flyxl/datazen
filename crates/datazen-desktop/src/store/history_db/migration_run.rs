//! Persistent history of Schema Diff, Data Sync, and Data Transfer runs.

use super::{
    HistoryDb, HistoryDbError, MigrationRunFilter, MigrationRunPage, MigrationRunRecord,
    MAX_MIGRATION_RUN_HISTORY,
};
use rusqlite::params;

impl HistoryDb {
    pub fn save_migration_run(&self, run: &MigrationRunRecord) -> Result<(), HistoryDbError> {
        validate_migration_run(run)?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO migration_run_history (
                    id, operation, status, outcome, phase, profile_id, profile_revision,
                    source_connection_id, target_connection_id, started_at, finished_at,
                    selected_count, committed_count, failed_count, conflict_count, cancelled,
                    rollback_outcome, error_summary
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
                 ON CONFLICT(id) DO UPDATE SET
                    status=excluded.status, outcome=excluded.outcome, phase=excluded.phase,
                    profile_id=excluded.profile_id, profile_revision=excluded.profile_revision,
                    source_connection_id=excluded.source_connection_id,
                    target_connection_id=excluded.target_connection_id,
                    finished_at=excluded.finished_at, selected_count=excluded.selected_count,
                    committed_count=excluded.committed_count, failed_count=excluded.failed_count,
                    conflict_count=excluded.conflict_count, cancelled=excluded.cancelled,
                    rollback_outcome=excluded.rollback_outcome, error_summary=excluded.error_summary",
                params![run.id, run.operation, run.status, run.outcome, run.phase,
                    run.profile_id, run.profile_revision, run.source_connection_id,
                    run.target_connection_id, run.started_at, run.finished_at,
                    run.selected_count as i64, run.committed_count as i64,
                    run.failed_count as i64, run.conflict_count as i64,
                    run.cancelled as i32, run.rollback_outcome, run.error_summary],
            )?;
            conn.execute(
                "DELETE FROM migration_run_history WHERE id IN (
                    SELECT id FROM migration_run_history ORDER BY started_at DESC LIMIT -1 OFFSET ?1
                 )",
                params![MAX_MIGRATION_RUN_HISTORY as i64],
            )?;
            Ok(())
        })
    }

    pub fn get_migration_run(
        &self,
        id: &str,
    ) -> Result<Option<MigrationRunRecord>, HistoryDbError> {
        self.with_conn(|conn| {
            match conn.query_row(
                "SELECT id,operation,status,outcome,phase,profile_id,profile_revision,
             source_connection_id,target_connection_id,started_at,finished_at,selected_count,
             committed_count,failed_count,conflict_count,cancelled,rollback_outcome,error_summary
             FROM migration_run_history WHERE id=?1",
                params![id],
                map_migration_run_row,
            ) {
                Ok(run) => Ok(Some(run)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(error) => Err(error.into()),
            }
        })
    }

    pub fn list_migration_runs(
        &self,
        filter: &MigrationRunFilter,
        offset: u64,
        limit: u64,
    ) -> Result<MigrationRunPage, HistoryDbError> {
        let limit = limit.clamp(1, 100);
        self.with_conn(|conn| {
            let mut clauses = Vec::new();
            let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            for (column, value) in [("operation", filter.operation.as_ref()), ("status", filter.status.as_ref()), ("profile_id", filter.profile_id.as_ref())] {
                if let Some(value) = value.filter(|value| !value.is_empty()) {
                    clauses.push(format!("{column}=?{}", values.len() + 1));
                    values.push(Box::new(value.clone()));
                }
            }
            if let Some(value) = filter.connection_id.as_ref().filter(|value| !value.is_empty()) {
                clauses.push(format!("(source_connection_id=?{} OR target_connection_id=?{})", values.len() + 1, values.len() + 1));
                values.push(Box::new(value.clone()));
            }
            let where_sql = if clauses.is_empty() { String::new() } else { format!(" WHERE {}", clauses.join(" AND ")) };
            let count_sql = format!("SELECT COUNT(*) FROM migration_run_history{where_sql}");
            let total: i64 = conn.query_row(&count_sql, rusqlite::params_from_iter(values.iter().map(|v| v.as_ref())), |row| row.get(0))?;
            let sql = format!("SELECT id,operation,status,outcome,phase,profile_id,profile_revision,
                source_connection_id,target_connection_id,started_at,finished_at,selected_count,
                committed_count,failed_count,conflict_count,cancelled,rollback_outcome,error_summary
                FROM migration_run_history{where_sql} ORDER BY started_at DESC LIMIT ?{} OFFSET ?{}", values.len()+1, values.len()+2);
            values.push(Box::new(limit as i64));
            values.push(Box::new(offset as i64));
            let mut stmt = conn.prepare(&sql)?;
            let items = stmt.query_map(rusqlite::params_from_iter(values.iter().map(|v| v.as_ref())), map_migration_run_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(MigrationRunPage { items, total: total as u64, offset, limit })
        })
    }
}

fn map_migration_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MigrationRunRecord> {
    Ok(MigrationRunRecord {
        id: row.get(0)?,
        operation: row.get(1)?,
        status: row.get(2)?,
        outcome: row.get(3)?,
        phase: row.get(4)?,
        profile_id: row.get(5)?,
        profile_revision: row.get(6)?,
        source_connection_id: row.get(7)?,
        target_connection_id: row.get(8)?,
        started_at: row.get(9)?,
        finished_at: row.get(10)?,
        selected_count: row.get::<_, i64>(11)? as u64,
        committed_count: row.get::<_, i64>(12)? as u64,
        failed_count: row.get::<_, i64>(13)? as u64,
        conflict_count: row.get::<_, i64>(14)? as u64,
        cancelled: row.get::<_, i32>(15)? != 0,
        rollback_outcome: row.get(16)?,
        error_summary: row.get(17)?,
    })
}

fn validate_migration_run(run: &MigrationRunRecord) -> Result<(), HistoryDbError> {
    const OPERATIONS: &[&str] = &["schemaDiff", "dataSync", "dataTransfer"];
    const STATUSES: &[&str] = &["running", "completed", "failed", "cancelled", "interrupted"];
    if !OPERATIONS.contains(&run.operation.as_str()) || !STATUSES.contains(&run.status.as_str()) {
        return Err(HistoryDbError::Other(
            "invalid migration run operation or status".into(),
        ));
    }
    if run.id.trim().is_empty() || run.started_at.trim().is_empty() {
        return Err(HistoryDbError::Other(
            "migration run id and start time are required".into(),
        ));
    }
    if run
        .error_summary
        .as_ref()
        .is_some_and(|summary| summary.len() > 500)
    {
        return Err(HistoryDbError::Other(
            "migration run error summary is too long".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn migration_run(id: &str, status: &str) -> MigrationRunRecord {
        MigrationRunRecord {
            id: id.into(),
            operation: "dataSync".into(),
            status: status.into(),
            outcome: if status == "running" {
                "pending"
            } else {
                "success"
            }
            .into(),
            phase: "execute".into(),
            profile_id: Some("profile-1".into()),
            profile_revision: Some("2026-09-21T00:00:00Z".into()),
            source_connection_id: Some("source".into()),
            target_connection_id: Some("target".into()),
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            selected_count: 4,
            committed_count: 0,
            failed_count: 0,
            conflict_count: 0,
            cancelled: false,
            rollback_outcome: "notRequired".into(),
            error_summary: None,
        }
    }

    #[test]
    fn migration_runs_page_filter_and_recover_running_without_sensitive_columns() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = HistoryDb::open(dir.path()).unwrap();
            db.save_migration_run(&migration_run("run-1", "running"))
                .unwrap();
            let page = db
                .list_migration_runs(
                    &MigrationRunFilter {
                        operation: Some("dataSync".into()),
                        ..Default::default()
                    },
                    0,
                    10,
                )
                .unwrap();
            assert_eq!(page.total, 1);
            assert_eq!(
                page.items[0].profile_revision.as_deref(),
                Some("2026-09-21T00:00:00Z")
            );
            let columns: Vec<String> = {
                let conn = db.conn.lock().unwrap();
                let mut stmt = conn
                    .prepare("PRAGMA table_info(migration_run_history)")
                    .unwrap();
                stmt.query_map([], |row| row.get(1))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect()
            };
            for forbidden in [
                "sql",
                "payload",
                "credentials",
                "db_session_id",
                "file_token",
            ] {
                assert!(!columns.iter().any(|column| column.contains(forbidden)));
            }
        }
        let reopened = HistoryDb::open(dir.path()).unwrap();
        let recovered = reopened.get_migration_run("run-1").unwrap().unwrap();
        assert_eq!(recovered.status, "interrupted");
        assert_eq!(recovered.outcome, "unknown");
        assert_eq!(recovered.rollback_outcome, "unknown");
    }

    #[test]
    fn test_tester_migration_run_upsert_persists_resolved_connections_and_filters_them() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path()).unwrap();
        let mut run = migration_run("run-resolved", "running");
        run.source_connection_id = None;
        run.target_connection_id = None;
        db.save_migration_run(&run).unwrap();

        run.status = "completed".into();
        run.outcome = "success".into();
        run.source_connection_id = Some("source-resolved".into());
        run.target_connection_id = Some("target-resolved".into());
        db.save_migration_run(&run).unwrap();

        let stored = db.get_migration_run("run-resolved").unwrap().unwrap();
        assert_eq!(
            stored.source_connection_id.as_deref(),
            Some("source-resolved")
        );
        assert_eq!(
            stored.target_connection_id.as_deref(),
            Some("target-resolved")
        );

        for connection_id in ["source-resolved", "target-resolved"] {
            let page = db
                .list_migration_runs(
                    &MigrationRunFilter {
                        connection_id: Some(connection_id.into()),
                        ..Default::default()
                    },
                    0,
                    25,
                )
                .unwrap();
            assert_eq!(
                page.total, 1,
                "history must be filterable by {connection_id}"
            );
            assert_eq!(page.items[0].id, "run-resolved");
        }
    }
}
