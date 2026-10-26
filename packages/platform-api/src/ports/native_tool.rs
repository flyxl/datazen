//! 原生工具端口词汇迁移垫片（D5，2026-10-26）。
//!
//! NativeToolSelection 等词汇的唯一定义自 D5 起在
//! `datazen-modules::jobs::domain::native_tool`（jobs::domain::owner 的
//! verify_native_selection 与 runtime 的 native_tool 模块消费）。
//! 本模块只再导出，旧路径不变；D8 退役本 crate 时删除。

pub use datazen_modules::jobs::domain::native_tool::{
    NativeCleanup, NativeTermination, NativeToolLimits, NativeToolPort, NativeToolReport,
    NativeToolResolution, NativeToolSelection,
};
