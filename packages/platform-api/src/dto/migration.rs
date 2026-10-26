//! 迁移运行历史词汇迁移垫片（D6 第一切片，2026-10-26）。
//!
//! 唯一定义自 D6 起在 `datazen-modules::history::domain::migration_run`。
//! 本模块只再导出，旧路径 `datazen_platform_api::dto::migration::*` 不变；
//! D8 退役本 crate 时删除。

pub use datazen_modules::history::migration_run::{
    MigrationProfileRef, MigrationRunFilter, MigrationRunPage, MigrationRunRecord,
};
