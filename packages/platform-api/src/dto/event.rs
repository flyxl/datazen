//! 事件契约迁移垫片（D3，2026-10-26）。
//!
//! `EventEnvelope` / `ArchivedEnvelope` / `EventSequence` 与连接事件载荷
//! （`ConnectionEvent` / `ResultChunkEvent`）的唯一定义在
//! `datazen-modules::execution::application::events`。本模块只再导出，
//! 旧 import 路径不变；D8 退役本 crate 时一并删除。
//!
//! §4.4：事件归档只写显式白名单投影；`EventEnvelope.sessionHandle` 永不落盘。

pub use datazen_modules::execution::application::{
    ArchivedEnvelope, ConnectionEvent, ConnectionEventEnvelope, EventEnvelope, EventSequence,
    ResultChunkEvent,
};
