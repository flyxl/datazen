//! `ExecutionSettingsReader` 迁移垫片（D3，2026-10-26）。
//!
//! 执行设置读取端口与 `ExecutionPolicy` 的唯一定义自 D3 起在
//! `datazen-modules::execution::application::settings_reader`。本模块只再
//! 导出，旧 import 路径不变；D8 退役本 crate 时一并删除。

pub use datazen_modules::execution::application::{ExecutionPolicy, ExecutionSettingsReader};
