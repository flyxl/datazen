//! 冻结计划正文：prepare/apply 的 IPC 输入体。
//!
//! 运行时只从 [`FrozenPlan`] 读版本字段与 consumedPlanId/selectionRevision；正文字段
//! 由 host 在构造 [`crate::job::DataSyncHandler`] 时解析并一并冻结，避免客户端把执行
//! SQL 当作恢复资格（§2.2：计划只含可持久化来源，runtimeBinding 保留内存）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::filter::SyncSourceFilter;
use crate::model::{Endpoint, SyncOptions, TableMapping};

/// dataSyncPrepare 的 Job kind。
pub const PREPARE_KIND: &str = "dataSyncPrepare";
/// dataSyncApply 的 Job kind。
pub const APPLY_KIND: &str = "dataSyncApply";

/// handler/plan/checkpoint 三个版本字段都冻结为 1；未知 major 拒绝。
pub const HANDLER_VERSION: u64 = 1;
pub const PLAN_VERSION: u64 = 1;
pub const CHECKPOINT_VERSION: u64 = 1;

/// prepare 受理体：稳定源/目标端点、结构映射、选项与源过滤。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareSpec {
    pub source: Endpoint,
    pub target: Endpoint,
    pub mappings: Vec<TableMapping>,
    pub options: SyncOptions,
    #[serde(default)]
    pub filters: HashMap<String, SyncSourceFilter>,
    /// 可选：由 host 指定 plan_id；缺省时 handler 生成
    #[serde(default)]
    pub plan_id: Option<String>,
}

/// apply 受理体：planId + selectionRevision + 选项。真正的执行 SQL 只从
/// 服务器持有的 ChangeSet Artifact 派生，客户端只提供确认句。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplySpec {
    pub plan_id: String,
    pub selection_revision: u64,
    pub options: SyncOptions,
}

fn versioned(extra: serde_json::Value) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("planVersion".into(), serde_json::Value::from(PLAN_VERSION));
    map.insert(
        "handlerVersion".into(),
        serde_json::Value::from(HANDLER_VERSION),
    );
    map.insert(
        "checkpointVersion".into(),
        serde_json::Value::from(CHECKPOINT_VERSION),
    );
    if let serde_json::Value::Object(obj) = extra {
        for (k, v) in obj {
            map.insert(k, v);
        }
    }
    serde_json::Value::Object(map)
}

/// 生成 prepare Job 的 JobDefinition.payload。
pub fn prepare_payload(spec: &PrepareSpec) -> serde_json::Value {
    versioned(serde_json::to_value(spec).unwrap_or(serde_json::Value::Null))
}

/// 生成 apply Job 的 JobDefinition.payload（含 consumedPlanId/selectionRevision）。
pub fn apply_payload(spec: &ApplySpec) -> serde_json::Value {
    let mut value = versioned(serde_json::to_value(spec).unwrap_or(serde_json::Value::Null));
    if let serde_json::Value::Object(map) = &mut value {
        map.insert(
            "consumedPlanId".into(),
            serde_json::Value::from(spec.plan_id.clone()),
        );
        map.insert(
            "selectionRevision".into(),
            serde_json::Value::from(spec.selection_revision),
        );
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_payload_has_versions_and_no_consumed_plan_id() {
        let spec = PrepareSpec {
            source: Endpoint::new("conn-a", "db_a", None),
            target: Endpoint::new("conn-b", "db_b", None),
            mappings: vec![TableMapping::auto("users")],
            options: SyncOptions::default(),
            filters: HashMap::new(),
            plan_id: None,
        };
        let payload = prepare_payload(&spec);
        assert_eq!(payload["planVersion"], serde_json::json!(1));
        assert_eq!(payload["handlerVersion"], serde_json::json!(1));
        assert_eq!(payload["checkpointVersion"], serde_json::json!(1));
        assert!(payload.get("consumedPlanId").is_none());
        assert_eq!(payload["source"]["database"], "db_a");
    }

    #[test]
    fn apply_payload_carries_consumed_plan_and_revision() {
        let spec = ApplySpec {
            plan_id: "plan-7".into(),
            selection_revision: 3,
            options: SyncOptions::default(),
        };
        let payload = apply_payload(&spec);
        assert_eq!(payload["consumedPlanId"], "plan-7");
        assert_eq!(payload["selectionRevision"], 3);
    }
}
