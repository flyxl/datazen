//! Job DTO 家族迁移垫片（D5，2026-10-26）。
//!
//! 落盘词汇（JobRecord/StageRecord/Checkpoint/CommitBoundary/JobClaim/Filter 家族
//! 与读侧视图）的唯一定义自 D5 起在 `datazen-modules::jobs::domain::job`。
//! 本模块只再导出，`datazen_platform_api::dto::job::*` 既有路径不变；
//! D8 退役本 crate 时删除。

pub use datazen_modules::jobs::domain::job::{
    workflow_block_owner, Checkpoint, CommitBoundary, JobClaim, JobDefinition, JobDetails,
    JobDomainResult, JobFilter, JobProgress, JobRecord, JobRecoveryRequest, JobRecoveryResult,
    JobRecoveryTarget, JobRecoveryVerdict, JobRecoveryVerification, JobResultCounter,
    JobResultItem, JobState, JobView, RecoveryFilter, StageRecord,
};
