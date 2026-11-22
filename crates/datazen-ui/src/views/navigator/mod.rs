//! React `windows/connection/navigator/` ports, one module per React file.
//!
//! The connection navigator host is a later task; these standalone views are
//! host-wireable now and carry per-item `#[allow(dead_code)]` with that removal
//! stage on every reusable item.

pub(crate) mod build_flat_rows;
pub(crate) mod database_value_search_dialog;
pub(crate) mod global_object_search;
pub(crate) mod navigator_dialogs;
pub(crate) mod navigator_toolbar;
pub(crate) mod navigator_tree_row;
pub(crate) mod tree_row_aria;
pub(crate) mod tree_status_dot;
pub(crate) mod types;
pub(crate) mod use_connection_drag;
pub(crate) mod use_kv_db_counts;
