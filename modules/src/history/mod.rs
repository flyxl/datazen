//! History：查询历史与迁移运行历史的领域词汇和仓储端口（D6 第一切片）。
//!
//! 词汇来源（逐字迁移，serde 形状不变）：
//! * `QueryHistoryEntry` / `FavoriteQuery` 相关——原 `src-tauri/src/store/models.rs`；
//! * `MigrationRunRecord` / `MigrationRunFilter` / `MigrationRunPage` /
//!   `MigrationProfileRef`——原 `datazen-platform-api::dto::migration`；
//! * `HistoryScope` / `HistoryOrder` / `QueryHistoryFilter` / `QueryHistoryPage`
//!   与保留常量——原 `src-tauri/src/store/history_db.rs`。
//!
//! 边界（04-repositories §7.1/§7.3）：仓储 trait 由本 domain 拥有，具体
//! SQLite 实现留在宿主（desktop 的 `history.sqlite`；web 形态换服务端实现，
//! 用例代码一行不改）。`workflow_history` 的词汇属 Workflow 功能族
//!（07-stages D6 第 3 项），不在本切片。
//!
//! [`QueryHistoryEntry`] 的明文 SQL 安全口径与桌面侧一致：history 存储按
//! 敏感审计日志对待，本模块不改变其明文现状，只集中词汇与端口。

pub mod application;
pub mod domain;

pub use application::service::HistoryService;
pub use domain::migration_run;
pub use domain::migration_run::{
    MigrationProfileRef, MigrationRunFilter, MigrationRunPage, MigrationRunRecord,
};
pub use domain::query::QueryHistoryEntry;
pub use domain::repository::{HistoryError, MigrationRunRepository, QueryHistoryRepository};
pub use domain::scope::{
    HistoryOrder, HistoryScope, QueryHistoryFilter, QueryHistoryPage, DEFAULT_HISTORY_PAGE_SIZE,
    MAX_MIGRATION_RUN_HISTORY, MAX_QUERY_HISTORY, MAX_WORKFLOW_HISTORY,
};
