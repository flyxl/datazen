//! View modules.

mod ai_chat;
mod app_root;
mod backup;
mod chrome;
pub(crate) mod content_status_bar;
pub(crate) mod content_toolbar;
mod connection_page;
mod connection_workspace_home;
pub(crate) mod content_view;
pub(crate) mod database_object_view;
pub(crate) mod document_connection_view;
pub(crate) mod export_dialog;
pub(crate) mod ddl_view;
pub(crate) mod er;
pub(crate) mod foreign_keys_view;
pub(crate) mod indexes_view;
pub(crate) mod create_database_dialog;
pub(crate) mod create_schema_dialog;
pub(crate) mod create_user_dialog;
pub(crate) mod content_view_dialogs;
pub(crate) mod content_view_drawers;
pub(crate) mod execute_sql_file_dialog;
pub(crate) mod home;
pub(crate) mod import_dialog;
pub(crate) mod navigator;
pub(crate) mod object_browser;
pub(crate) mod pending_plan_dialog;
pub(crate) mod panel_content_renderer;
pub(crate) mod panel_tab_bar;
pub(crate) mod privilege_view;
pub(crate) mod process_list_view;
pub(crate) mod query_panel;
pub(crate) mod query;
pub(crate) mod result_workspace;
pub(crate) mod server_status_view;
pub(crate) mod set_value_date_dialog;
pub(crate) mod structure_column_table;
pub(crate) mod table_column_filter;
pub(crate) mod structure_index_table;
pub(crate) mod structure_plan_preview;
pub(crate) mod structure_view;
pub(crate) mod table_structure_editor;
pub(crate) mod workspace_mode_sidebar;
pub(crate) mod use_dirty_index;
pub(crate) mod use_named_view_menu;
pub(crate) mod table_filter_toolbar;
pub(crate) mod table_pending_changes_bar;
pub(crate) mod table_foreign_key_navigation;
pub(crate) mod table_toolbar;
pub(crate) mod table_clipboard_status;
pub(crate) mod table_view_clipboard;
pub(crate) mod use_table_actions;
pub(crate) mod use_connection_tabs;
pub(crate) mod use_kv_workspace_slots;
pub(crate) mod table_fk_breadcrumb;
pub(crate) mod table_fk_error_banner;
pub(crate) mod table_load_error_state;
pub(crate) mod table_no_primary_key_tip;
pub(crate) mod table_read_only_tip;
pub(crate) mod table_status_banners;
pub(crate) mod table_view;
// T606: pure tableEditing decisions (editability, navigation, candidates, set value).
pub(crate) mod table_editing;
pub(crate) mod use_kv_slot_actions;
pub(crate) mod use_panel_handlers;
pub(crate) mod use_connection_context_menu;
// T612: pure useConnectionWorkspaceMeta derivation (contexts, capabilities, toolbar).
mod use_connection_workspace_meta;
pub(crate) use use_connection_workspace_meta::*;

mod connections;
mod connections_home;
mod dashboards_workflows;
// Both subtrees are `pub(crate)`: seg3's data-sync ports and seg4's data-transfer
// ports are consumed across views and by their test bridges.
pub(crate) mod data_sync;
// The windows/workspace subtree ports (T436 lands useTransferJobRun here);
// `data_transfer_attached_jobs` carries the shared `JobState`/`JobProgress`
// vocabulary the schema-diff commands shell (T377) re-uses.
pub(crate) mod data_transfer;
mod main;
// The windows/dashboard subtree ports (T018: AddToDashboardDialog is its
// first); the dashboards page itself stays in dashboards_workflows.rs.
pub(crate) mod dashboard;
// The windows/onboarding subtree ports (T021: BrandSidebar is its first).
pub(crate) mod onboarding;
// The windows/schema-diff subtree ports (T025: the deploy panel is its first).
pub(crate) mod schema_diff;
mod modals;
mod navigation;
mod settings;
mod structure;
mod tools_ops;
mod tools_sync;
mod wapps;
mod workflow;
mod workspace;

pub(crate) use ai_chat::*;
pub(crate) use app_root::*;
#[allow(unused_imports)] // the Backup window (T026-S2) is the production consumer
pub(crate) use backup::*;
pub(crate) use chrome::*;
pub(crate) use content_status_bar::*;
pub(crate) use content_toolbar::*;
pub(crate) use connection_page::*;
pub(crate) use connection_workspace_home::*;
pub(crate) use content_view::*;
pub(crate) use content_view_dialogs::*;
pub(crate) use connections::*;
pub(crate) use connections_home::*;
pub(crate) use dashboards_workflows::*;
#[allow(unused_imports)] // the Data Sync window (T270) is the production consumer
pub(crate) use data_sync::*;
#[allow(unused_imports)] // the editor shell (T023-S2) is the production consumer
pub(crate) use data_transfer::*;
pub(crate) use main::*;
pub(crate) use modals::*;
pub(crate) use navigation::*;
pub(crate) use settings::*;
pub(crate) use structure::*;
pub(crate) use tools_ops::*;
pub(crate) use tools_sync::*;
pub(crate) use wapps::*;
pub(crate) use workflow::*;
pub(crate) use workspace::*;
