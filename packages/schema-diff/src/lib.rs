//! Schema Diff 引擎迁移垫片（D5，2026-10-26）。
//!
//! 唯一定义自 D5 起在 `datazen-modules::schema_diff`。本 crate 只再导出，
//! `datazen_schema_diff::*` 既有路径不变（drivers 的方言测试、application
//! 的装配测试、feature-core 经此消费）；D8 退役本 crate 时删除。

pub use datazen_modules::schema_diff::*;
