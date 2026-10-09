//! SchemaDiffJobHandler 接入 JobRuntime（P5 Wave-1 `p5-schema-diff` 轨）。
//!
//! 三件套的领域包不直接引用 Tauri；JobHandler 协议与 runtime 契约见
//! `packages/runtime/src/job/`。
//!
//! * [`plan`]：§2.2 冻结计划元数据、版本守卫、PlanStore（planId 一次消费）。
//! * [`backend`]：物理资源接缝（prepare/verify_authorization/fingerprint/deploy/read_only_verify）。
//! * [`handler`]：`SchemaDiffHandler` 实现 `JobHandler`（validatePlan/runStage/verifyRecovery）。
//! * [`recovery`]：§7 故障窗口的纯判定（`decide_recovery`）。

pub mod backend;
pub mod handler;
pub mod plan;
pub mod recovery;

pub use backend::{
    ApplyRequest, PrepareRequest, PreparedPlan, ReadOnlyVerdict, SchemaDiffJobBackend,
};
pub use handler::{HandlerRole, SchemaDiffHandler};
pub use plan::{
    fnv1a64_hex, PlanStore, RecoveryPolicy, SchemaDiffFrozenPlan, SchemaDiffPlanError, StoredPlan,
    SCHEMA_DIFF_HANDLER_VERSION, SUPPORTED_PLAN_MAJOR,
};
pub use recovery::{
    decide_recovery, EVIDENCE_BOUNDARY_VERIFIED, EVIDENCE_CAPABILITY_CHANGED,
    EVIDENCE_COMMIT_ACK_LOST, EVIDENCE_DDL_RESPONSE_LOST, EVIDENCE_VERSIONS_CHANGED,
};
