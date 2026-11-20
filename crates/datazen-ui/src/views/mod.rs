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
pub(crate) mod create_database_dialog;
pub(crate) mod create_schema_dialog;
pub(crate) mod create_user_dialog;
pub(crate) mod content_view_dialogs;
pub(crate) mod content_view_drawers;
pub(crate) mod execute_sql_file_dialog;

mod connections;
mod connections_home;
mod dashboards_workflows;
pub(crate) mod data_sync;
mod data_transfer;
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
