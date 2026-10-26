//! D5（2026-10-26）：Job 运行时的唯一定义在 `datazen-modules::jobs`
//!（domain/application/engine 三分）。本模块只再导出，保持
//! `datazen_kernel::job::*` 既有路径（含 `job::runtime::JobRuntime` 等深路径）
//! 不变；D8 退役本 crate 时一并删除（§5.5 单向委托）。

pub use datazen_modules::jobs::*;
