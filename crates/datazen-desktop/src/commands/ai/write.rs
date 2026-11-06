//! Confirmed write proposals (T24).
//!
//! `propose_write` only mints a short-lived exact-text proposal. `apply_write`
//! validates the one-time token plus exact SQL/connection/database, re-runs the
//! SQL guard, refuses production/read-only targets, and only then reaches the
//! shared host's single `execute_target` entry.

use crate::commands::error::CommandError;
use crate::commands::AppState;
use crate::store::{AiWriteAuditRecord, NewAiWriteAuditRecord};
use datazen::ai::application::ports::DatabaseToolHost;
use datazen::ai::application::write_proposal::{
    is_production_connection, sql_hash, ApplyWriteRequest, WriteProposal, WriteProposalRequest,
    WriteProposalStore,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use tauri::{Emitter, State};

static WRITE_PROPOSALS: OnceLock<Arc<WriteProposalStore>> = OnceLock::new();

/// Process-wide proposal registry shared by the agent tool and the IPC apply.
/// Tokens are unguessable UUIDs, so one desktop process can safely share it.
pub(crate) fn write_proposals() -> Arc<WriteProposalStore> {
    WRITE_PROPOSALS
        .get_or_init(|| Arc::new(WriteProposalStore::new()))
        .clone()
}

/// Receipt for one successful apply. `result` is the driver's command output;
/// it can contain RETURNING rows, so the UI only has to show `rowCount`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiApplyWriteReceipt {
    pub proposal_id: String,
    pub connection_id: String,
    pub database: String,
    pub sql_hash: String,
    pub row_count: Option<u64>,
    pub result: serde_json::Value,
}

/// Create a proposal for direct UI/editor callers. The agent path uses the
/// same shared helper through `DatabaseToolHost`.
#[tauri::command]
pub async fn ai_propose_write(
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    sql: String,
    previous_sql: Option<String>,
) -> Result<WriteProposal, CommandError> {
    propose_write_for_state(
        &state,
        &connection_id,
        database.as_deref(),
        &sql,
        previous_sql.as_deref(),
    )
    .await
}

pub(crate) async fn propose_write_for_state(
    state: &AppState,
    connection_id: &str,
    database: Option<&str>,
    sql: &str,
    previous_sql: Option<&str>,
) -> Result<WriteProposal, CommandError> {
    let config = state
        .store
        .platform_profile(connection_id)
        .await
        .filter(|(_, metadata)| metadata.enabled)
        .map(|(config, _)| config)
        .ok_or_else(|| CommandError::NotFound("Connection not found".into()))?;
    let policy = state.store.execution_settings().await;
    let database = database
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| config.database.clone())
        .unwrap_or_default();
    let request = WriteProposalRequest {
        connection_id: connection_id.trim().to_string(),
        database,
        sql: sql.to_string(),
        previous_sql: previous_sql.map(str::to_string),
    };
    let proposal = write_proposals()
        .propose(&request, &config, policy.safe_mode, chrono::Utc::now())
        .map_err(|error| CommandError::Validation(error.to_string()))?;
    publish_proposal(state, &proposal).await;
    Ok(proposal)
}

/// Backend-name alias for hosts/tests that speak the T24 names directly.
#[tauri::command]
pub async fn propose_write(
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    sql: String,
    previous_sql: Option<String>,
) -> Result<WriteProposal, CommandError> {
    propose_write_for_state(
        &state,
        &connection_id,
        database.as_deref(),
        &sql,
        previous_sql.as_deref(),
    )
    .await
}

/// Validate the exact proposal binding and execute once. Production and
/// read-only checks are repeated here immediately before `execute_target`.
#[tauri::command]
pub async fn ai_apply_write(
    state: State<'_, AppState>,
    token: Option<String>,
    proposal_id: Option<String>,
    connection_id: String,
    database: Option<String>,
    sql: String,
) -> Result<AiApplyWriteReceipt, CommandError> {
    apply_write_for_state(
        &state,
        token.or(proposal_id).unwrap_or_default().as_str(),
        &connection_id,
        database.as_deref().unwrap_or(""),
        &sql,
    )
    .await
}

pub(crate) async fn apply_write_for_state(
    state: &AppState,
    token: &str,
    connection_id: &str,
    database: &str,
    sql: &str,
) -> Result<AiApplyWriteReceipt, CommandError> {
    let config = state
        .store
        .platform_profile(connection_id)
        .await
        .filter(|(_, metadata)| metadata.enabled)
        .map(|(config, _)| config)
        .ok_or_else(|| CommandError::NotFound("Connection not found".into()))?;
    let policy = state.store.execution_settings().await;
    let request = ApplyWriteRequest {
        token: token.trim().to_string(),
        connection_id: connection_id.trim().to_string(),
        database: database.trim().to_string(),
        sql: sql.to_string(),
    };
    let applied =
        match write_proposals().apply(&request, &config, policy.safe_mode, chrono::Utc::now()) {
            Ok(applied) => applied,
            Err(error) => {
                audit_denied(state, token, connection_id, database, sql, error.code()).await;
                return Err(CommandError::Validation(error.to_string()));
            }
        };
    let proposal = applied.proposal;
    let mut input = serde_json::json!({ "sql": proposal.sql });
    if !proposal.database.is_empty() {
        input["database"] = serde_json::Value::String(proposal.database.clone());
    }
    if let Some(schema) = config.schema.as_deref().filter(|schema| !schema.is_empty()) {
        input["schema"] = serde_json::Value::String(schema.to_string());
    }
    let result =
        DatabaseToolHost::execute_target(&*state, &proposal.connection_id, "execute", input).await;
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            audit_denied(
                state,
                &proposal.token,
                &proposal.connection_id,
                &proposal.database,
                &proposal.sql,
                "execution_failed",
            )
            .await;
            return Err(CommandError::Internal(error));
        }
    };
    let row_count = affected_rows(&result.data);
    audit_write(
        state,
        NewAiWriteAuditRecord {
            organization_id: String::new(),
            principal_id: String::new(),
            proposal_id: Some(proposal.proposal_id.clone()),
            connection_id: Some(proposal.connection_id.clone()),
            database_name: Some(proposal.database.clone()),
            sql_hash: proposal.sql_hash.clone(),
            decision: "applied".into(),
            row_count,
            reason: None,
        },
    )
    .await;
    Ok(AiApplyWriteReceipt {
        proposal_id: proposal.proposal_id,
        connection_id: proposal.connection_id,
        database: proposal.database,
        sql_hash: proposal.sql_hash,
        row_count,
        result: result.data,
    })
}

/// Backend-name alias for `ai_apply_write`.
#[tauri::command]
pub async fn apply_write(
    state: State<'_, AppState>,
    token: Option<String>,
    proposal_id: Option<String>,
    connection_id: String,
    database: Option<String>,
    sql: String,
) -> Result<AiApplyWriteReceipt, CommandError> {
    apply_write_for_state(
        &state,
        token.or(proposal_id).unwrap_or_default().as_str(),
        &connection_id,
        database.as_deref().unwrap_or(""),
        &sql,
    )
    .await
}

/// User pressed Deny. Removes the proposal and audited the refusal; an
/// already-expired/used token is treated as already resolved.
#[tauri::command]
pub async fn ai_deny_write(
    state: State<'_, AppState>,
    proposal_id: String,
) -> Result<bool, CommandError> {
    let removed = write_proposals()
        .deny(&proposal_id, chrono::Utc::now())
        .map_err(|error| CommandError::Validation(error.to_string()))?;
    if let Some(proposal) = removed {
        audit_write(
            &state,
            NewAiWriteAuditRecord {
                organization_id: String::new(),
                principal_id: String::new(),
                proposal_id: Some(proposal.proposal_id),
                connection_id: Some(proposal.connection_id),
                database_name: Some(proposal.database),
                sql_hash: proposal.sql_hash,
                decision: "denied".into(),
                row_count: None,
                reason: Some("user_denied".into()),
            },
        )
        .await;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub async fn ai_list_write_audit(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<AiWriteAuditRecord>, CommandError> {
    state
        .store
        .ai_write_audit_log()
        .list(limit.unwrap_or(100))
        .await
        .map_err(CommandError::Store)
}

/// Host notification hook used by the shared `propose_write` DB tool.
pub(crate) async fn publish_proposal(state: &AppState, proposal: &WriteProposal) {
    audit_write(
        state,
        NewAiWriteAuditRecord {
            organization_id: String::new(),
            principal_id: String::new(),
            proposal_id: Some(proposal.proposal_id.clone()),
            connection_id: Some(proposal.connection_id.clone()),
            database_name: Some(proposal.database.clone()),
            sql_hash: proposal.sql_hash.clone(),
            decision: "proposal".into(),
            row_count: None,
            reason: if proposal.production_protected {
                Some("production_protected".into())
            } else if !proposal.apply_allowed {
                Some("read_only".into())
            } else {
                None
            },
        },
    )
    .await;
    if let Some(app) = state.app_handle.get() {
        let _ = app.emit("ai:write-proposal-requested", proposal);
    }
}

async fn audit_denied(
    state: &AppState,
    token: &str,
    connection_id: &str,
    database: &str,
    sql: &str,
    reason: &str,
) {
    audit_write(
        state,
        NewAiWriteAuditRecord {
            organization_id: String::new(),
            principal_id: String::new(),
            proposal_id: (!token.trim().is_empty()).then(|| token.trim().to_string()),
            connection_id: (!connection_id.trim().is_empty())
                .then(|| connection_id.trim().to_string()),
            database_name: (!database.trim().is_empty()).then(|| database.trim().to_string()),
            sql_hash: sql_hash(sql),
            decision: "denied".into(),
            row_count: None,
            reason: Some(reason.to_string()),
        },
    )
    .await;
}

async fn audit_write(state: &AppState, mut input: NewAiWriteAuditRecord) {
    let (organization_id, principal_id) = audit_identity(state);
    input.organization_id = organization_id;
    input.principal_id = principal_id;
    let record = crate::store::new_ai_write_audit_record(input);
    if let Err(error) = state.store.ai_write_audit_log().append(&record).await {
        tracing::warn!(error = %error, "failed to append AI write audit record");
        if let Some(app) = state.app_handle.get() {
            let _ = app.emit("ai:write-audit-error", error.to_string());
        }
    }
}

fn audit_identity(state: &AppState) -> (String, String) {
    if let Ok(adapter) = state.platform.require() {
        return (
            adapter.identity().organization_id().to_string(),
            adapter.identity().principal_id().to_string(),
        );
    }
    (
        state.organization_scope.organization_id().to_string(),
        "unknown".to_string(),
    )
}

fn affected_rows(data: &serde_json::Value) -> Option<u64> {
    for key in [
        "rowsAffected",
        "rows_affected",
        "affectedRows",
        "affected_rows",
    ] {
        if let Some(value) = data.get(key).and_then(serde_json::Value::as_u64) {
            return Some(value);
        }
    }
    data.get("rowCount")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            data.as_array()
                .and_then(|rows| rows.first())
                .and_then(|row| row.get("rowsAffected"))
                .and_then(serde_json::Value::as_u64)
        })
}
