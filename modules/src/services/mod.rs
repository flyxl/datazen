//! Shared services that need driver-facing host ports.
//!
//! These services were the last substantive part of the former modules
//! crate. Their rules live here; Desktop only supplies the port implementations.

pub mod fk_navigation;
pub mod migration_endpoint;
pub mod ports;
pub mod referenced_by;
pub mod value_search;

pub mod command_policy {
    pub use crate::mcp::application::command_policy::*;
}
pub mod db_tools {
    pub use crate::ai::application::db_tools::*;
}
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
