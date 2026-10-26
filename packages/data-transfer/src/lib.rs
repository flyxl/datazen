//! Data Transfer 引擎迁移垫片（D5，2026-10-26）。
//!
//! 唯一定义自 D5 起在 `datazen-modules::data_transfer`。本 crate 只再导出，
//! `datazen_data_transfer::*` 既有路径不变（src-tauri / feature-core /
//! sqlite、rqlite、turso 三驱动的 dev-deps 经此消费）；D8 退役本 crate 时删除。

pub use datazen_modules::data_transfer::*;
