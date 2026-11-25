//! Port of `packages/frontend/src/commands/history.ts` (T314).
//!
//! W8 后 (post-W8) the types' single source is backend-client (the
//! domain vocabulary); the React file is a thin re-export shell over
//! the domain clients, and this port is the same three shapers:
//!
//! - `purgeHistory` rides the query-assets domain client (the
//!   configuration-class token): the purge request plus the
//!   `saveUserAsset` submission token the host mints first (the
//!   `ai::workflow` token face — [`AiSubmissionTokenVm`] — is the
//!   shared mint; the desktop `dashboard.rs` port used the same shape
//!   for its authorized writes).
//! - `listMigrationRuns` / `getMigrationRun` ride the sync domain
//!   client (contract §6.13 remark / §6.16): plain reads, no token,
//!   wire names verbatim.
//!
//! The re-exported vocabulary (`HistoryPurgeScope`, the
//! `MigrationRun*` family) mirrors `backend-client`'s
//! `types/queryAssets.ts` / `types/dataSync.ts` field for field —
//! `crates/datazen/src/history` already owns the semantics; these are
//! the UI-side wire shapes, camelCase exactly as the transport speaks.
//!
//! Known deltas: none behavioural — every method name and payload key
//! is the React source's verbatim; `retainDays: null` is an absent key
//! (JS drops `undefined`, and the domain client spreads only when
//! non-null).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::commands::ai::workflow::{AiSubmissionTokenVm, with_submission_token};

/// `HistoryPurgeScope`: which history a purge touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryPurgeScope {
    Query,
    Workflow,
    All,
}

/// `MigrationOperation`: the three migration operations that share the
/// v12 run history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MigrationOperation {
    SchemaDiff,
    DataSync,
    DataTransfer,
}

/// `MigrationRunStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MigrationRunStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

/// `MigrationRunFilter`: absent fields fall back to the backend
/// defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRunFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<MigrationOperation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<MigrationRunStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
}

/// `MigrationRunRecord`: one row of the v12 migration-run history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRunRecord {
    pub id: String,
    pub operation: MigrationOperation,
    pub status: MigrationRunStatus,
    pub outcome: String,
    pub phase: String,
    pub profile_id: Option<String>,
    pub profile_revision: Option<String>,
    pub source_connection_id: Option<String>,
    pub target_connection_id: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub selected_count: u64,
    pub committed_count: u64,
    pub failed_count: u64,
    pub conflict_count: u64,
    pub cancelled: bool,
    pub rollback_outcome: String,
    pub error_summary: Option<String>,
}

/// `MigrationRunPage`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRunPage {
    pub items: Vec<MigrationRunRecord>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}

/// The history domain's wire names (the transport call each shaper
/// rides; `queryAssetsClient` / `dataSyncClient`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryMethod {
    /// The query-assets purge (token-carrying write face).
    PurgeHistory,
    /// The sync domain's run-history read face.
    ListMigrationRuns,
    GetMigrationRun,
}

impl HistoryMethod {
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::PurgeHistory => "purgeHistory",
            Self::ListMigrationRuns => "listMigrationRuns",
            Self::GetMigrationRun => "getMigrationRun",
        }
    }
}

/// `historyCommands.purgeHistory({ scope, retainDays })`: bulk
/// retention policy over the query-assets client. `retainDays` absent
/// (`None`) purges the whole scope regardless of age; the host mints
/// the `saveUserAsset` token first and passes it here. The receipt is
/// the number of rows removed.
pub fn purge_history_request(
    scope: HistoryPurgeScope,
    retain_days: Option<u64>,
    token: &AiSubmissionTokenVm,
) -> (HistoryMethod, Value) {
    // React: `...(args.retainDays != null ? { retainDays } : {})` — a
    // null retainDays is an absent key, not a null value.
    let mut request = match retain_days {
        Some(retain_days) => json!({ "scope": scope, "retainDays": retain_days }),
        None => json!({ "scope": scope }),
    };
    request = with_submission_token(request, token);
    (HistoryMethod::PurgeHistory, request)
}

/// `historyCommands.listMigrationRuns(filter, offset = 0, limit = 25)`:
/// the run-history page read (the defaults are the wrapper's own —
/// callers that omit them get exactly these).
pub fn list_migration_runs_request(
    filter: &MigrationRunFilter,
    offset: u64,
    limit: u64,
) -> (HistoryMethod, Value) {
    (
        HistoryMethod::ListMigrationRuns,
        json!({ "filter": filter, "offset": offset, "limit": limit }),
    )
}

/// `historyCommands.getMigrationRun(runId)`: one run; a miss and a
/// cross-organization read are one indistinguishable 404 (the domain
/// contract's own wording).
pub fn get_migration_run_request(run_id: &str) -> (HistoryMethod, Value) {
    (HistoryMethod::GetMigrationRun, json!({ "runId": run_id }))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn token() -> AiSubmissionTokenVm {
        AiSubmissionTokenVm {
            idempotency_key: "key-1".to_owned(),
            expires_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn purge_merges_the_token_and_drops_a_null_retain_days() {
        let (method, request) =
            purge_history_request(HistoryPurgeScope::Query, None, &token());
        assert_eq!(method, HistoryMethod::PurgeHistory);
        assert_eq!(method.wire_name(), "purgeHistory");
        assert_eq!(request["scope"], json!("query"));
        assert_eq!(request["submissionToken"]["idempotencyKey"], json!("key-1"));
        // A null retainDays is an absent key, not a null value.
        assert!(request.as_object().is_some_and(|map| !map.contains_key("retainDays")));

        let (_, request) =
            purge_history_request(HistoryPurgeScope::All, Some(30), &token());
        assert_eq!(request["scope"], json!("all"));
        assert_eq!(request["retainDays"], json!(30));
        assert_eq!(request["submissionToken"]["expiresAt"], json!("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn the_purge_scope_vocabulary_is_the_lowercase_wire() {
        assert_eq!(serde_json::to_value(HistoryPurgeScope::Query).unwrap_or_default(), json!("query"));
        assert_eq!(
            serde_json::to_value(HistoryPurgeScope::Workflow).unwrap_or_default(),
            json!("workflow")
        );
        assert_eq!(serde_json::to_value(HistoryPurgeScope::All).unwrap_or_default(), json!("all"));
    }

    #[test]
    fn the_list_read_carries_the_filter_and_the_wrapper_defaults() {
        let filter = MigrationRunFilter {
            operation: Some(MigrationOperation::DataSync),
            status: Some(MigrationRunStatus::Failed),
            profile_id: None,
            connection_id: Some("conn-9".to_owned()),
        };
        let (method, request) = list_migration_runs_request(&filter, 0, 25);
        assert_eq!(method.wire_name(), "listMigrationRuns");
        assert_eq!(request["offset"], json!(0));
        assert_eq!(request["limit"], json!(25), "the wrapper's own defaults");
        assert_eq!(request["filter"]["operation"], json!("dataSync"));
        assert_eq!(request["filter"]["status"], json!("failed"));
        assert_eq!(request["filter"]["connectionId"], json!("conn-9"));
        // Absent filter halves are absent keys.
        assert!(
            request["filter"].as_object().is_some_and(|map| !map.contains_key("profileId"))
        );
        // An explicit page rides along verbatim.
        let (_, request) = list_migration_runs_request(&MigrationRunFilter::default(), 50, 10);
        assert_eq!(request["offset"], json!(50));
        assert_eq!(request["limit"], json!(10));
    }

    #[test]
    fn the_get_read_takes_the_run_id() {
        let (method, request) = get_migration_run_request("run-7");
        assert_eq!(method.wire_name(), "getMigrationRun");
        assert_eq!(request, json!({ "runId": "run-7" }));
    }

    #[test]
    fn the_run_vocabulary_round_trips_the_wire() {
        // The camelCase spellings the transport speaks.
        assert_eq!(
            serde_json::to_value(MigrationOperation::SchemaDiff).unwrap_or_default(),
            json!("schemaDiff")
        );
        assert_eq!(
            serde_json::to_value(MigrationOperation::DataTransfer).unwrap_or_default(),
            json!("dataTransfer")
        );
        assert_eq!(
            serde_json::to_value(MigrationRunStatus::Interrupted).unwrap_or_default(),
            json!("interrupted")
        );
        // One record decodes field for field (camelCase on the wire).
        let record: MigrationRunRecord = serde_json::from_value(json!({
            "id": "run-7",
            "operation": "schemaDiff",
            "status": "completed",
            "outcome": "applied",
            "phase": "deploy",
            "profileId": null,
            "profileRevision": null,
            "sourceConnectionId": "conn-a",
            "targetConnectionId": "conn-b",
            "startedAt": "2026-11-20T10:00:00Z",
            "finishedAt": "2026-11-20T10:01:00Z",
            "selectedCount": 4,
            "committedCount": 4,
            "failedCount": 0,
            "conflictCount": 0,
            "cancelled": false,
            "rollbackOutcome": "none",
            "errorSummary": null,
        }))
        .expect("the record decodes");
        assert_eq!(record.operation, MigrationOperation::SchemaDiff);
        assert_eq!(record.status, MigrationRunStatus::Completed);
        assert_eq!(record.selected_count, 4);
        // And the page shape wraps them.
        let page: MigrationRunPage = serde_json::from_value(json!({
            "items": [serde_json::to_value(&record).unwrap_or_default()],
            "total": 1,
            "offset": 0,
            "limit": 25,
        }))
        .expect("the page decodes");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "run-7");
    }
}
