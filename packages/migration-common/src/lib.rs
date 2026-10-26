//! 迁移共享纯算法迁移垫片（D5，2026-10-26）。
//!
//! 唯一定义自 D5 起在 `datazen-modules::migration_common`（01 §4.1：
//! 共享纯算法同名内部模块，不反向依赖三件套或 execution）。
//! 本 crate 只再导出，`datazen_migration_common::*` 既有路径不变；
//! D8 退役本 crate 时删除。

pub use datazen_modules::migration_common::{pairing, recordset_bounds};
