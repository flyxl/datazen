//! View modules.

mod ai_chat;
mod app_root;
mod backup;
mod chrome;
pub(crate) mod shell_menu;
pub mod content_status_bar;
pub mod content_toolbar;
mod connection_page;
mod connection_workspace_home;
pub mod content_view;
pub mod database_object_view;
pub mod document_connection_view;
pub mod export_dialog;
pub mod ddl_view;
pub mod er;
pub mod foreign_keys_view;
pub mod indexes_view;
pub mod create_database_dialog;
pub mod create_schema_dialog;
pub mod create_user_dialog;
pub mod content_view_dialogs;
pub mod content_view_drawers;
pub mod execute_sql_file_dialog;
pub mod home;
pub mod import_dialog;
pub mod navigator;
pub mod object_browser;
pub mod pending_plan_dialog;
pub mod panel_content_renderer;
pub mod panel_tab_bar;
pub mod privilege_view;
pub mod process_list_view;
pub mod query_panel;
pub mod query;
pub mod result_workspace;
pub mod server_status_view;
pub mod set_value_date_dialog;
pub mod structure_column_table;
pub mod table_column_filter;
pub mod structure_index_table;
pub mod structure_plan_preview;
pub mod structure_view;
pub mod table_structure_editor;
pub mod workspace_mode_sidebar;
pub mod use_dirty_index;
pub mod use_named_view_menu;
pub mod table_filter_toolbar;
pub mod table_pending_changes_bar;
pub mod table_foreign_key_navigation;
pub mod table_toolbar;
pub mod table_clipboard_status;
pub mod table_view_clipboard;
pub mod use_table_actions;
pub mod use_connection_tabs;
pub mod use_kv_workspace_slots;
pub mod table_fk_breadcrumb;
pub mod table_fk_error_banner;
pub mod table_load_error_state;
pub mod table_no_primary_key_tip;
pub mod table_read_only_tip;
pub mod table_status_banners;
pub mod table_view;
// T606: pure tableEditing decisions (editability, navigation, candidates, set value).
pub mod table_editing;
pub mod use_kv_slot_actions;
pub mod use_panel_handlers;
pub mod use_connection_context_menu;
// T612: pure useConnectionWorkspaceMeta derivation (contexts, capabilities, toolbar).
mod use_connection_workspace_meta;
pub use use_connection_workspace_meta::*;

mod connections;
mod connections_home;
mod dashboards_workflows;
// Both subtrees are `pub`: seg3's data-sync ports and seg4's data-transfer
// ports are consumed across views and by their test bridges.
pub mod data_sync;
// The windows/workspace subtree ports (T436 lands useTransferJobRun here);
// `data_transfer_attached_jobs` carries the shared `JobState`/`JobProgress`
// vocabulary the schema-diff commands shell (T377) re-uses.
pub mod data_transfer;
mod native_csv_import;
mod native_transfer;
mod main;
// The windows/dashboard subtree ports (T018: AddToDashboardDialog is its
// first); the dashboards page itself stays in dashboards_workflows.rs.
pub mod dashboard;
// The windows/onboarding subtree ports (T021: BrandSidebar is its first).
pub mod onboarding;
// The windows/schema-diff subtree ports (T025: the deploy panel is its first).
pub mod schema_diff;
mod modals;
mod navigation;
mod settings;
mod structure;
mod tools_ops;
mod tools_sync;
mod wapps;
mod workflow;
mod workspace;

pub use ai_chat::*;
pub use app_root::*;
#[allow(unused_imports)] // the Backup window (T026-S2) is the production consumer
pub use backup::*;
pub use chrome::*;
pub use content_status_bar::*;
pub use content_toolbar::*;
pub use connection_page::*;
pub use connection_workspace_home::*;
pub use content_view::*;
pub use content_view_dialogs::*;
pub use connections::*;
pub use connections_home::*;
pub use dashboards_workflows::*;
#[allow(unused_imports)] // the Data Sync window (T270) is the production consumer
pub use data_sync::*;
#[allow(unused_imports)] // the editor shell (T023-S2) is the production consumer
pub use data_transfer::*;
pub use main::*;
pub use modals::*;
pub use navigation::*;
pub use settings::*;
pub use structure::*;
pub use tools_ops::*;
pub use tools_sync::*;
pub use wapps::*;
pub use workflow::*;
pub use workspace::*;

pub(crate) mod native_kv;
