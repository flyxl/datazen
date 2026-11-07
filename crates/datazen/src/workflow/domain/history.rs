//! The vocabulary of a workflow run: what a recorded run looks like.
//!
//! The run record is stored whole — the result is kept as it was produced
//! instead of being projected into columns — because what a step did can
//! change shape as the executor grows, and a projection would have to be
//! migrated every time. The persistence of the record is the host's business;
//! the shape of it is not.

use super::model::WorkflowExecutionResult;

/// One recorded workflow run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunRecord {
    pub id: String,
    #[serde(alias = "skillId")]
    pub workflow_id: String,
    #[serde(alias = "skillName")]
    pub workflow_name: String,
    pub variables: serde_json::Value,
    pub result: WorkflowExecutionResult,
    pub created_at: String,
}

/// One recorded workflow run, without the payload.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunListItem {
    pub id: String,
    #[serde(alias = "skillId")]
    pub workflow_id: String,
    #[serde(alias = "skillName")]
    pub workflow_name: String,
    pub success: bool,
    pub total_time_ms: u64,
    pub created_at: String,
}

/// The id a recorded run carries: a local timestamp plus a short nonce.
///
/// The timestamp keeps the history list sortable by eye; the nonce keeps two
/// runs started in the same second apart.
pub fn new_run_id(now: chrono::DateTime<chrono::Local>) -> String {
    format!(
        "{}_{}",
        now.format("%Y%m%dT%H%M%S"),
        &uuid::Uuid::new_v4().to_string()[..8]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_id_is_a_timestamp_plus_a_nonce() {
        let now = chrono::Local::now();
        let id = new_run_id(now);
        let (timestamp, nonce) = id.split_once('_').expect("id has two parts");
        assert_eq!(timestamp.len(), 15);
        assert_eq!(nonce.len(), 8);
        assert!(timestamp.chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(nonce.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn two_runs_in_the_same_second_get_different_ids() {
        let now = chrono::Local::now();
        assert_ne!(new_run_id(now), new_run_id(now));
    }

    #[test]
    fn the_run_record_keeps_the_legacy_skill_aliases() {
        let record = WorkflowRunRecord {
            id: "run-1".into(),
            workflow_id: "workflow-1".into(),
            workflow_name: "Workflow 1".into(),
            variables: serde_json::json!({"uid": "U001"}),
            result: WorkflowExecutionResult {
                success: true,
                final_output: "test output".into(),
                steps: vec![],
                total_time_ms: 42,
                error: None,
            },
            created_at: "2024-01-01T00:00:00+00:00".into(),
        };
        let encoded = serde_json::to_value(&record).expect("encode");
        assert_eq!(encoded["workflowId"], "workflow-1");
        assert_eq!(encoded["workflowName"], "Workflow 1");
        assert_eq!(encoded["createdAt"], "2024-01-01T00:00:00+00:00");

        let decoded: WorkflowRunRecord = serde_json::from_value(serde_json::json!({
            "id": "run-1",
            "skillId": "workflow-1",
            "skillName": "Workflow 1",
            "variables": {},
            "result": record.result,
            "createdAt": "2024-01-01T00:00:00+00:00",
        }))
        .expect("decode");
        assert_eq!(decoded.workflow_id, "workflow-1");
        assert_eq!(decoded.workflow_name, "Workflow 1");
    }
}
