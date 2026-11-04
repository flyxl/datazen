//! SchemaDiffJobHandler 接入 JobRuntime（P5 Wave-1 `p5-schema-diff` 轨）。
//!
//! 三件套的领域包不直接引用 Tauri；JobHandler 协议与 runtime 契约见
//! `crates/datazen/src/jobs/`。
//!
//! * [`plan`]：§2.2 冻结计划元数据、版本守卫、PlanRepository（planId 一次消费）。
//! * [`backend`]：物理资源接缝（prepare/verify_authorization/fingerprint/deploy/read_only_verify）。
//! * [`handler`]：`SchemaDiffHandler` 实现 `JobHandler`（validatePlan/runStage/verifyRecovery）。
//! * [`recovery`]：§7 故障窗口的纯判定（`decide_recovery`）。
//! * [`recovery_targets`]：恢复目标的纯投影（线 id 编解码 / 证据记录 / 表目标派生）。

pub mod backend;
pub mod handler;
pub mod plan;
pub mod recovery;
// B2 schema_diff 域刀：宿主 job/recovery.rs 的纯投影函数下沉（线 id/证据/表目标）。
pub mod recovery_targets;

pub use backend::{
    ApplyRequest, PrepareRequest, PreparedPlan, ReadOnlyVerdict, SchemaDiffJobBackend,
};
pub use handler::{HandlerRole, SchemaDiffHandler};
pub use plan::{
    fnv1a64_hex, PlanRepository, RecoveryPolicy, SchemaDiffFrozenPlan, SchemaDiffPlanError,
    StoredPlan, SCHEMA_DIFF_HANDLER_VERSION, SUPPORTED_PLAN_MAJOR,
};
pub use recovery::{
    decide_recovery, EVIDENCE_BOUNDARY_VERIFIED, EVIDENCE_CAPABILITY_CHANGED,
    EVIDENCE_COMMIT_ACK_LOST, EVIDENCE_DDL_RESPONSE_LOST, EVIDENCE_VERSIONS_CHANGED,
};
pub use recovery_targets::{
    decode_target_id, encode_target_id, record_target_objects, table_target,
    target_ids_from_evidence, targets_from_evidence,
};
