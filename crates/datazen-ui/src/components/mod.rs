//! React component ports that keep the source tree's shape.
//!
//! One module per React file, at the same path: `components/DataTable/Foo.tsx`
//! becomes `components/data_table/foo.rs`. Nothing is re-exported flat; callers
//! name the module, so a port keeps the React file's identity. Registration is
//! one `mod` line per ported file: parallel migration tracks each add their own
//! line to the owning `mod.rs` instead of sharing a file.

pub mod admin;
pub mod ai;
pub mod connection;
pub mod data_table;
pub mod chart;
pub mod ui;
pub mod db_type_badge;
pub mod error_boundary;
pub mod export;
pub mod filter;
pub mod filter_bar;
pub mod filter_editor;
// T140: the lazy-i18n-domain loading skeleton (page / section variants).
pub mod locale_domain_loading;
// T301: the overlay-window chrome fallback (the registered chrome slot
// over a centered announcing spinner).
pub mod window_chrome_fallback;
pub mod history;
pub mod query;
pub mod schema;
pub mod migration;
pub mod mcp_approval_host;
// T195: the web-shape menu bar (MenuBar.tsx).
pub mod menu_bar;
pub mod sql_editor;
pub mod status_bar;
pub mod themed_icon;
// T282: the theme toggle (the chrome's theme picker).
pub mod theme_toggle;
// T219: the SQL code block (SqlCodeBlock.tsx).
pub mod sql_code_block;
// T241: the Settings page's static SQL syntax preview
// (SqlSyntaxPreview.tsx). The other T241 half, the top-level
// `SqlEditor.tsx` barrel, is the re-export surface `sql_editor`'s own
// `mod.rs` already owns (SqlEditorComponent / SqlEditorProps /
// SqlSchema / DroppedTablePayload / resolve_cm_dialect, with the
// `SqlEditorHandle` surface as the state's methods) — no second source
// of truth, the barrel's whole contract.
pub mod sql_syntax_preview;
