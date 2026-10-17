use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRunRecord {
    pub id: String,
    pub operation: String,
    pub status: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRunFilter {
    pub operation: Option<String>,
    pub status: Option<String>,
    pub profile_id: Option<String>,
    pub connection_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRunPage {
    pub items: Vec<MigrationRunRecord>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationProfileRef {
    pub id: String,
    pub revision: String,
}
