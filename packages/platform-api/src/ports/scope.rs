//! 组织范围端口迁移垫片（D5，2026-10-26）。
//!
//! `OrganizationScope` 的唯一定义自 D5 起在 `datazen-modules::shared::scope`
//!（schema_diff 的 Profile 仓储、data-transfer 的 plan_store 等消费）。
//! 本模块只再导出，旧路径不变；D8 退役本 crate 时删除。

pub use datazen_modules::shared::scope::OrganizationScope;
