//! `ExecutionTargetResolver` 迁移垫片（D3，2026-10-26）。
//!
//! 目标解析端口与 `TargetResolution` 的唯一定义自 D3 起在
//! `datazen-modules::execution::application::target_resolver`。本模块只再
//! 导出，旧 import 路径不变；D8 退役本 crate 时一并删除。

pub use datazen_modules::execution::application::{ExecutionTargetResolver, TargetResolution};
