//! Table structure editor IPC — delegates to driver `structure_capabilities` /
//! `plan_structure_changes` (no host-side capability registry).

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{StructureCapabilities, StructureChangePlan, StructureChangeRequest};
use std::time::Instant;
use tauri::State;

#[tauri::command]
pub async fn get_structure_capabilities(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<StructureCapabilities, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, "get_structure_capabilities");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("get_structure_capabilities")?;

    let caps = driver
        .structure_capabilities(&handle)
        .await
        .cmd_err("get_structure_capabilities")?;
    tracing::info!(
        %db_session_id,
        dialect = %caps.dialect_id,
        ms = start.elapsed().as_millis() as u64,
        "get_structure_capabilities OK"
    );
    Ok(caps)
}

#[tauri::command]
pub async fn plan_table_structure_changes(
    state: State<'_, AppState>,
    db_session_id: String,
    request: StructureChangeRequest,
    database: Option<String>,
) -> Result<StructureChangePlan, CommandError> {
    plan_table_structure_changes_impl(&state, db_session_id, request, database).await
}

pub(crate) async fn plan_table_structure_changes_impl(
    state: &AppState,
    db_session_id: String,
    request: StructureChangeRequest,
    database: Option<String>,
) -> Result<StructureChangePlan, CommandError> {
    let start = Instant::now();
    tracing::info!(
        %db_session_id,
        mode = ?request.mode,
        schema = ?request.schema,
        table = %request.table,
        "plan_table_structure_changes"
    );
    // F1: optional explicit database pin — same `ensure_session_database`
    // mechanism as query/stream/get_table_data, so cross-library DDL planning
    // and the subsequent unqualified statement execution land on the panel's
    // target database instead of the session's previous active one.
    super::query::ensure_session_database(
        state,
        &db_session_id,
        database.as_deref(),
        "plan_table_structure_changes",
    )
    .await?;
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("plan_table_structure_changes")?;

    let plan = driver
        .plan_structure_changes(&handle, &request)
        .await
        .cmd_err("plan_table_structure_changes")?;
    tracing::info!(
        %db_session_id,
        statements = plan.statements.len(),
        ms = start.elapsed().as_millis() as u64,
        "plan_table_structure_changes OK"
    );
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{StructureChangeMode, StructureColumnDraft};
    use crate::testing::app_state::TestAppState;

    fn create_request(table: &str) -> StructureChangeRequest {
        StructureChangeRequest {
            mode: StructureChangeMode::Create,
            schema: None,
            table: table.into(),
            original_columns: vec![],
            current_columns: vec![StructureColumnDraft {
                id: "c1".into(),
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
                is_unique: false,
            }],
            original_indexes: vec![],
            current_indexes: vec![],
        }
    }

    #[tokio::test]
    async fn plan_switches_session_database_when_pinned_differs() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("struct-pin-db").await;
        // Sample config pins database = "app"; the editor targets another one.
        let plan = plan_table_structure_changes_impl(
            &test.state,
            conn_id.clone(),
            create_request("t_f1"),
            Some("analytics".into()),
        )
        .await
        .unwrap();
        assert!(plan.statements.is_empty());
        assert_eq!(
            test.mock.use_database_calls(),
            vec!["analytics".to_string()]
        );
        let config = test
            .state
            .connection_manager
            .get_session_config(&conn_id)
            .await
            .unwrap();
        assert_eq!(config.database.as_deref(), Some("analytics"));
    }

    #[tokio::test]
    async fn plan_without_pin_keeps_active_database() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("struct-no-pin").await;
        plan_table_structure_changes_impl(
            &test.state,
            conn_id.clone(),
            create_request("t_f1"),
            None,
        )
        .await
        .unwrap();
        assert!(test.mock.use_database_calls().is_empty());
        let config = test
            .state
            .connection_manager
            .get_session_config(&conn_id)
            .await
            .unwrap();
        assert_eq!(config.database.as_deref(), Some("app"));
    }
}
