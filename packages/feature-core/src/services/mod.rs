pub mod command_policy;
pub mod db_tools;
pub mod fk_navigation;
pub mod migration_endpoint;
pub mod ports;
pub mod query_executor;
pub mod referenced_by;
pub mod result_edit;
pub mod schema_context;
pub mod table_browsing;
pub mod table_paging;
pub mod value_search;

pub use query_executor::filter_sql::{join_filters, validate_filters, FilterRejection};

// D4-b（2026-10-26）：schema 服务迁入 datazen-modules::schema::application，
// 这里按旧模块名再导出（§5.5）。
pub use datazen_modules::schema::application::physical_context as physical_schema_context;
pub use datazen_modules::schema::application::metadata as schema_metadata;
pub use datazen_modules::schema::application::scope as schema_scope;
