//! Data Sync 的 JobHandler 接入（P5 Wave-1）。
//!
//! 冻结语义（§10.1.1 / data-migration-jobs.md §5、§7）：
//!
//! * 准备与应用分别受理：`dataSyncPrepare` 产出 ChangeSet Artifact 与计划摘要，
//!   结束后释放快照/会话资源；`dataSyncApply` 消费 planId + selectionRevision + 确认。
//! * handler 不改 JobState、不自续租、未知提交不自动重跑。
//! * validatePlan 只能做静态检查（trait 为同步方法）；family/结构/PK/重叠等动态门闸
//!   在 run_stage 内先行拒绝——外部输入经 runStage 异步重验。
//! * verifyRecovery 只依据 checkpoint 中的证据字符串与冻结指纹做只读裁决。

pub mod artifact;
pub mod body;
pub mod handler;
pub mod host;

pub use artifact::{ChangeBlock, ChangeSetArtifact, ColumnMeta, RelationIdentity, TableMeta};
pub use body::{apply_payload, prepare_payload, ApplySpec, PrepareSpec};
pub use handler::DataSyncHandler;
pub use host::{DataSyncHost, EndpointSession, KeysetPageSource, TargetExecutor, TransactionScope};
