//! Dashboard runs 迁移垫片（D6 item 2，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::dashboard::application::runs`。
//! 本模块只再导出，旧路径 `datazen_feature_core::dashboard::runs::*`
//! 不变（src-tauri 与 feature-core 内部消费者零改动）；feature-core 退役时
//! （D8）一并删除。

pub use datazen_modules::dashboard::application::runs::*;
