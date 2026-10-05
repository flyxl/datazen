//! P5 DataTransferHandler：JobRuntime 接入（prepare/apply 分离）。
//!
//! - `validate_plan`：冻结语义复验（版本/指纹/能力），不通过必须 Err（§10.1.1）。
//! - `run_stage(prepare)`：源一致性证据（稳定完整 key + driver 快照证明或等价证据，
//!   缺则 recoveryPolicy=forbidAutoResume）、小规模采样产出映射/类型损失清单 Artifact。
//! - `run_stage(apply)`：有界管道（固定源 reader → 有界 typed row batch → IR 转换 →
//!   有界参数批次 → 固定目标 writer/transaction → commit→checkpoint），
//!   缓冲按 §6.2 计入解码行/转换副本/待发送参数，慢目标源暂停，单值超限明确失败；
//!   SQL 文件输出走 writer finalize 完整性、无目标连接。
//! - `verify_recovery`：§7 决策表（commit 应答丢失→读目标标识补边界；源变化→PlanStale；
//!   缺完整 typed row 摘要/外部不可变版本或等价 driver 证明→禁止自动续写）。

mod checkpoint;
mod handler;
mod pipeline;
mod plan;
mod recovery;
mod sqlfile;

pub use checkpoint::{commit_boundary, versioned_checkpoint};
pub use handler::{DataTransferHandler, TransferEndpoints, TransferStageProfile};
pub use pipeline::{PipelineBudget, PIPELINE_INITIAL_BYTES};
pub use plan::{
    validate_frozen_plan, TransferFreezeBody, CHECKPOINT_VERSION, HANDLER_VERSION, PLAN_VERSION,
};
pub use recovery::verify_checkpoint;

#[cfg(test)]
mod tests;
