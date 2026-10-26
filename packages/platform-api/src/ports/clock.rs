//! 时基端口迁移垫片（D5，2026-10-26）。
//!
//! `Clock` / `SystemClock` / `FixedClock` 的唯一定义自 D5 起在
//! `datazen-modules::shared::clock`（常驻，无 feature 门；jobs::domain::time 与
//! 迁移三件套消费）。本模块只再导出，旧路径不变；D8 退役本 crate 时删除。

pub use datazen_modules::shared::clock::{Clock, FixedClock, SystemClock};
