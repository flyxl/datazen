//! 迁移运行历史词汇（原 `datazen::platform_api::dto::migration`，逐字迁移）。
//!
//! `MigrationRunRecord` 同时被 Data Sync / Data Transfer / Schema Diff 三类
//! 运行写入（`operation` 区分），是三件套共享的运行历史形状；归位 history
//! domain 后 platform-api 原路径改再导出垫片（D8 退役）。

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

#[cfg(test)]
mod tests {
    use super::*;

    /// wire 形状钉住：`deny_unknown_fields` 与 camelCase 都是既有 IPC 契约。
    #[test]
    fn migration_run_vocabulary_keeps_wire_shape() {
        let record = MigrationRunRecord {
            id: "r1".into(),
            operation: "data_sync".into(),
            status: "completed".into(),
            outcome: "ok".into(),
            phase: "execute".into(),
            profile_id: Some("p1".into()),
            profile_revision: Some("3".into()),
            source_connection_id: Some("c1".into()),
            target_connection_id: None,
            started_at: "2026-10-26T00:00:00Z".into(),
            finished_at: None,
            selected_count: 10,
            committed_count: 10,
            failed_count: 0,
            conflict_count: 0,
            cancelled: false,
            rollback_outcome: "notRequired".into(),
            error_summary: None,
        };
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["profileRevision"], "3");
        assert_eq!(json["committedCount"], 10);

        let r#ref = MigrationProfileRef {
            id: "p1".into(),
            revision: "3".into(),
        };
        let json = serde_json::to_value(&r#ref).unwrap();
        assert!(serde_json::from_value::<MigrationProfileRef>(json).is_ok());
        let extra = serde_json::json!({"id": "p1", "revision": "3", "extra": 1});
        assert!(serde_json::from_value::<MigrationProfileRef>(extra).is_err());
    }
}
