//! Driver registry — 把 `DatabaseType` 解析为具体 `DatabaseDriver`。
//!
//! 实现自 `src-tauri/src/db/registry.rs` 迁入；宿主侧只保留 re-export 门面
//! （`src-tauri/src/db/mod.rs`），使 `crate::db::registry::*` 既有调用点零改动。
//!
//! 驱动通过 `inventory` 工厂在链接期注册，本 crate 本身不链接任何驱动 crate：
//! 哪个构建里有哪些驱动，取决于最终二进制的链接闭包。

pub mod registry;

pub use registry::{init_drivers, DriverCapabilities, DriverRegistry, ResourceProviderLookup};
