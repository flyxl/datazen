//! React `windows/connection/navigator/` ports, one module per React file.
//!
//! The connection navigator host is a later task; these standalone views are
//! host-wireable now and carry per-item `#[allow(dead_code)]` with that removal
//! stage on every reusable item.

pub mod build_flat_rows;
pub mod database_value_search_dialog;
pub mod global_object_search;
pub(crate) mod native_object_search;
pub mod navigator_dialogs;
pub mod navigator_toolbar;
pub mod navigator_tree_row;
pub mod tree_row_aria;
pub mod tree_status_dot;
pub mod types;
pub mod use_connection_drag;
pub mod use_kv_db_counts;
pub mod use_navigator_context_menus;
pub mod use_navigator_db_state;
pub mod utils;
