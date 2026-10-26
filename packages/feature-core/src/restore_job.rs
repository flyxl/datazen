//! 恢复 apply-job 迁移垫片（D6 item 5 切片 b，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::backup::restore_job`。本模块只
//! 再导出，旧路径 `datazen_feature_core::restore_job::*` 不变（src-tauri
//! 与 restore_gateway 消费者零改动）；行为用例见 `tests/restore_apply_job.rs`
//! （持有 runtime/kernel 引擎 dev-dep，垫片保留先例）。

pub use datazen_modules::backup::restore_job::*;
