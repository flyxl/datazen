//! Recording and reading `workflow_history`.
//!
//! Workflow runs are stored whole: the result is serialized as-is rather than
//! projected into columns, because what a step did can change shape as the
//! executor grows and a projection here would have to be migrated every time.

use super::{HistoryDb, HistoryDbError, MAX_WORKFLOW_HISTORY};
use datazen_feature_core::workflow::history::{WorkflowRunListItem, WorkflowRunRecord};
use datazen_feature_core::workflow::model::WorkflowExecutionResult;
use rusqlite::params;

impl HistoryDb {
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
    ) -> Result<Vec<WorkflowRunListItem>, HistoryDbError> {
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
    ) -> Result<Option<WorkflowRunRecord>, HistoryDbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, workflow_id, workflow_name, variables_json, result_json, created_at
                 FROM workflow_history WHERE id = ?1",
            )?;
            let mut rows = stmt.query_map(params![history_id], |row| {
                let result_json: String = row.get(4)?;
                let result: WorkflowExecutionResult = serde_json::from_str(&result_json)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                Ok(WorkflowRunRecord {
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
}

pub(super) fn map_workflow_list_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<WorkflowRunListItem> {
    let result_json: String = row.get(3)?;
    let result: WorkflowExecutionResult = serde_json::from_str(&result_json)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    Ok(WorkflowRunListItem {
        id: row.get(0)?,
        workflow_id: row.get(1)?,
        workflow_name: row.get(2)?,
        success: result.success,
        total_time_ms: result.total_time_ms,
        created_at: row.get(4)?,
    })
}

pub(super) fn trim_workflow_history(conn: &rusqlite::Connection) -> Result<(), HistoryDbError> {
    conn.execute(
        "DELETE FROM workflow_history WHERE id NOT IN (
            SELECT id FROM workflow_history ORDER BY created_at DESC LIMIT ?1
         )",
        params![MAX_WORKFLOW_HISTORY as i64],
    )?;
    Ok(())
}
