//! 迁移端点身份迁移垫片（D5，2026-10-26）。
//!
//! `EndpointIdentity` 的唯一定义自 D5 起在 `datazen-modules::shared::migration`
//!（data_sync 的 plan_store、feature-core 的 migration_endpoint 消费）。
//! 本模块只再导出，旧路径不变；D8 退役本 crate 时删除。

pub use datazen_modules::shared::migration::EndpointIdentity;
