pub mod command_policy;
pub mod db_tools;
pub mod fk_navigation;
pub mod migration_endpoint;
pub mod ports;
pub mod referenced_by;
// D4-e（2026-10-26）：result_edit 写回规则迁入 datazen-modules::table_editing::result_edit，
// 这里按旧模块名再导出（§5.5），`datazen_feature_core::services::result_edit::*`
// 既有路径不变（桌面 result_origin / result_write 与集成测试经此消费）。
pub use datazen_modules::table_editing::result_edit;
pub mod schema_context;
pub mod value_search;

pub use query_executor::filter_sql::{join_filters, validate_filters, FilterRejection};

// D4-b（2026-10-26）：schema 服务迁入 datazen-modules::schema::application，
// 这里按旧模块名再导出（§5.5）。
pub use datazen_modules::schema::application::metadata as schema_metadata;
pub use datazen_modules::schema::application::physical_context as physical_schema_context;
pub use datazen_modules::schema::application::scope as schema_scope;

// D4-c（2026-10-26）：表浏览读链路迁入 datazen-modules::table_browsing，
// 这里按旧模块名再导出（§5.5）。
pub use datazen_modules::table_browsing::browsing as table_browsing;
pub use datazen_modules::table_browsing::executor as query_executor;
pub use datazen_modules::table_browsing::paging as table_paging;
