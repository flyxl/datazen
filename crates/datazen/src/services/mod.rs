//! Shared services that need driver-facing host ports.
//!
//! These services were the last substantive part of the former modules
//! crate. Their rules live here; Desktop only supplies the port implementations.

pub mod fk_navigation;
// W8 刀2 刻痕修复（第四处，设计档 §2.6 之外的实施级发现）：本模块消费
// `crate::migration::schema_diff::normalize_dialect`，而 `migration` 模块门
// （lib.rs）独立于 `services`——门不挂回依赖方时，`services` 单独开门编不过
// （桌面全 feature 恒真，零行为变化；server 开 services 不被动拖入 schema_diff）。
#[cfg(feature = "schema_diff")]
pub mod migration_endpoint;
pub mod ports;
pub mod referenced_by;
pub mod value_search;

// W8 刀2 刻痕修复：这三条 re-export 的唯一消费者是桌面（ai + services 同
// 开，门恒真）；`services` 不再硬编码 `ai`（Cargo.toml），故挂回自己的门。
#[cfg(feature = "ai")]
pub mod command_policy {
    pub use crate::execution::application::command_policy::*;
}
#[cfg(feature = "ai")]
pub mod db_tools {
    pub use crate::ai::application::db_tools::*;
}
#[cfg(feature = "ai")]
pub mod schema_context {
    pub use crate::ai::application::schema_context::*;
}

pub use crate::schema::application::metadata as schema_metadata;
pub use crate::schema::application::physical_context as physical_schema_context;
pub use crate::schema::application::scope as schema_scope;
pub use crate::table_browsing::browsing as table_browsing;
pub use crate::table_browsing::executor as query_executor;
pub use crate::table_browsing::paging as table_paging;
pub use crate::table_editing::result_edit;

pub use query_executor::filter_sql::{join_filters, validate_filters, FilterRejection};
