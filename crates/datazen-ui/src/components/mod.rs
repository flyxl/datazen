//! React component ports that keep the source tree's shape.
//!
//! One module per React file, at the same path: `components/DataTable/Foo.tsx`
//! becomes `components/data_table/foo.rs`. Nothing is re-exported flat; callers
//! name the module, so a port keeps the React file's identity. Registration is
//! one `mod` line per ported file: parallel migration tracks each add their own
//! line to the owning `mod.rs` instead of sharing a file.

pub(crate) mod admin;
pub(crate) mod ai;
pub(crate) mod connection;
pub(crate) mod data_table;
pub(crate) mod chart;
pub(crate) mod ui;
pub(crate) mod db_type_badge;
pub(crate) mod error_boundary;
pub(crate) mod export;
pub(crate) mod filter;
pub(crate) mod filter_bar;
pub(crate) mod filter_editor;
// T140: the lazy-i18n-domain loading skeleton (page / section variants).
pub(crate) mod locale_domain_loading;
// T301: the overlay-window chrome fallback (the registered chrome slot
// over a centered announcing spinner).
pub(crate) mod window_chrome_fallback;
pub(crate) mod history;
pub(crate) mod query;
pub(crate) mod schema;
pub(crate) mod migration;
pub(crate) mod mcp_approval_host;
// T195: the web-shape menu bar (MenuBar.tsx).
pub(crate) mod menu_bar;
pub(crate) mod sql_editor;
// T219: the SQL code block (SqlCodeBlock.tsx).
pub(crate) mod sql_code_block;
