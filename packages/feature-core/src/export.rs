//! 表导出服务迁移垫片（D6 item 5 切片 a，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::export`（含 format 子模块与
//! FileSink/规划/流式写出/zip 归档）。本模块只再导出，旧路径
//! `datazen_feature_core::export::*` 与 `…::export::format::*` 不变
//! （src-tauri 消费者零改动）；feature-core 退役时（D8）随垫片删除。

pub use datazen_modules::export::format;
pub use datazen_modules::export::*;
