//! 文件产物作业迁移垫片（D6 item 5 切片 a，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::export::file_job`。本模块只再导出，
//! 旧路径 `datazen_feature_core::file_job::*` 不变（src-tauri 的
//! file_jobs/export/backup 命令零改动）；行为用例见
//! `tests/file_artifact_job.rs`（持有 datazen-runtime dev-dep，垫片保留先例）。

pub use datazen_modules::export::file_job::*;
