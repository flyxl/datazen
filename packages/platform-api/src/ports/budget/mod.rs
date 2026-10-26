//! 预算端口迁移垫片（D3，2026-10-26）。
//!
//! 预算端口家族（`BudgetCoordinator` / `DriverPoolBudgetPort` /
//! `ControlResourcePort` / `ClusterNodeBudgetPort` 与配套词汇、进程内参考账本
//! `pool_ledger`）的唯一定义自 D3 起在
//! `datazen-modules::execution::application::budget`（§5.3：Job 预算协调经注入
//! 契约使用同一账本）。本模块只再导出，旧 import 路径不变；D8 退役本 crate
//! 时一并删除。

pub use datazen_modules::execution::application::budget::*;
