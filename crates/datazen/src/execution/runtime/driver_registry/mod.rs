//! Driver registry — 把 `DatabaseType` 解析为具体 `DatabaseDriver`。
//!
//! 实现自 `src-tauri/src/db/registry.rs` 迁入；宿主侧只保留 re-export 门面
//! （`src-tauri/src/db/mod.rs`），使 `crate::db::registry::*` 既有调用点零改动。
//!
//! 驱动通过 `inventory` 工厂在链接期注册，本 crate 本身不链接任何驱动 crate：
//! 哪个构建里有哪些驱动，取决于最终二进制的链接闭包。
//!
//! 静态注入形态（P2 驱动注入静态化）：[`DriverRegistry::with_factories`]
//! 由宿主在构造期显式递交工厂清单——登记一次成型，构造后不再回落链接期
//! 扫描，登记缺失一律显式失败。懒加载与链接闭包依赖只属于懒形态
//! （`new` / `init_drivers`），是宿主现行接线的过渡态。

pub mod registry;

pub use registry::{init_drivers, DriverCapabilities, DriverRegistry, ResourceProviderLookup};
