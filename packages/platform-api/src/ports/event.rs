//! `EventSink` 迁移垫片（D3，2026-10-26）。
//!
//! 事件发布端口与 `EventSubscription` 的唯一定义自 D3 起在
//! `datazen-modules::execution::application::event_sink`。本模块只再导出，
//! 旧 import 路径不变；D8 退役本 crate 时一并删除。

pub use datazen_modules::execution::application::{EventSink, EventSubscription};
