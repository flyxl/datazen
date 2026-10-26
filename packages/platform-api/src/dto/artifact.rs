//! 产物契约迁移垫片（D3，2026-10-26）。
//!
//! 产物域词汇（结果块/摘要/生命周期/元数据/归属）的唯一定义在
//! `datazen-modules::execution::domain::artifact`；`BackupOutput` 接收回调在
//! `datazen-modules::execution::application`。本模块只再导出，旧 import
//! 路径不变；D8 退役本 crate 时一并删除。

pub use datazen_modules::execution::application::BackupOutput;
pub use datazen_modules::execution::domain::{
    AbortReason, ArtifactChunk, ArtifactLifecycle, ArtifactMetadata, ArtifactProvenance,
    ArtifactSummary,
};
