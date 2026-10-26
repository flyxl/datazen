//! Job 端口迁移垫片（D5，2026-10-26）。
//!
//! `JobRepository` 的唯一定义自 D5 起在 `datazen-modules::jobs::domain::repository`
//!（§4.1 仓储条款：仓储 trait 在 domain，实现在宿主）。本模块只再导出，
//! `datazen_platform_api::ports::job::*` 既有路径不变；D8 退役本 crate 时删除。
//!
//! 刻意用**命名**再导出而非 glob：`ports/inventory` 的自检测试从源码文本
//! 抽取 `pub use datazen_modules::…` 的名字来核对 PORT_TRAITS（与 budget 垫片
//! 同一口径）。

pub use datazen_modules::jobs::domain::repository::{
    needs_reclaim, stage_is_finished, JobRepository,
};
