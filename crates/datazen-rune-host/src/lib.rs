//! The native desktop host for the shared Rune UI.
//!
//! The demo process (`datazen-rune` in `datazen-ui`) opens the in-memory
//! world. This host injects [`LiveAdapter`], which runs the native use cases
//! against the file repositories and the driver registry.

pub mod backend;
pub mod dashboards;
pub mod favorites;
pub mod history;
mod workflow_run_history;
pub mod jobs;
pub mod live;
pub mod secrets;
pub mod settings;
pub mod snippets;
pub mod store;
pub mod tray;

pub use dashboards::{FileDashboardStore, FileWorkflowStore};
pub use history::{FileMigrationRuns, FileQueryHistory};
pub use jobs::HostJobs;
pub use live::LiveAdapter;
pub use settings::FileSettingsRepository;
pub use store::FileConnectionRepository;

use std::path::PathBuf;

/// Trusted identity for this single-tenant process and its background worker.
fn host_request_context() -> &'static datazen::shared::context::RequestContext {
    use datazen::shared::{
        context::RequestContext,
        id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId},
    };
    static CONTEXT: std::sync::LazyLock<RequestContext> = std::sync::LazyLock::new(|| {
        // scope-gate: rune-composition-root
        RequestContext::new(
            OrganizationId::new("desktop"),
            PrincipalId::new("datazen-native"),
            None,
            ClientInstanceId::new("datazen-native"),
            RequestId::new("datazen-native"),
            None,
        )
    });
    &CONTEXT
}

fn host_connection_scope() -> datazen::connection::ConnectionScope {
    datazen::connection::ConnectionScope::new(host_request_context().organization_id.as_str())
}

/// The connection store's path: `DATAZEN_NATIVE_CONNECTIONS` when set, else
/// `connections.json` under `${XDG_DATA_HOME:-$HOME/.local/share}/datazen-native`.
pub fn default_connections_path() -> PathBuf {
    if let Ok(path) = std::env::var("DATAZEN_NATIVE_CONNECTIONS") {
        return PathBuf::from(path);
    }
    let base = match std::env::var("XDG_DATA_HOME") {
        Ok(xdg) => PathBuf::from(xdg),
        Err(_) => {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local").join("share")
        }
    };
    base.join("datazen-native").join("connections.json")
}
