//! The native desktop host for the shared Rune UI.
//!
//! The demo process (`datazen-rune` in `datazen-ui`) opens the in-memory
//! world. This host is the live one: its adapter owns real use cases, and
//! its repositories read and write real files. N2 wires the connection
//! store; the data paths follow on the same seam.

pub mod backend;
pub mod dashboards;
pub mod favorites;
pub mod history;
pub mod jobs;
pub mod live;
pub mod settings;
pub mod snippets;
pub mod tray;
pub mod store;

pub use dashboards::{FileDashboardStore, FileWorkflowStore};
pub use history::{FileMigrationRuns, FileQueryHistory};
pub use jobs::HostJobs;
pub use live::LiveAdapter;
pub use settings::FileSettingsRepository;
pub use store::FileConnectionRepository;

use std::path::PathBuf;

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
