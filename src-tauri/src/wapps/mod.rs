//! Runtime UI/theme extension base: registry over installed wapp packages.
//!
//! Wapps live in `{appData}/wapps/{id}/` (folder name == `manifest.id`).
//! [`WappManager`] loads every valid package at startup, tracks enabled
//! state via a `.enabled` marker file, and is shared through `AppState`.
//!
//! Wave-2 track B1 split this module along a form-neutral / form-specific
//! seam. The **kernel** — manifest schema and rules 1–7, ZIP and directory
//! install, per-wapp key-value storage, zip entry-path guards — moved to
//! `packages/workspace-apps`; it is re-exported verbatim below, so every
//! `crate::wapps::*` call site in the host is unchanged. What stays here is
//! **form-specific**: [`protocol`] (the `datazen://` bridge, which speaks
//! Tauri IPC) and [`WappManager`] (the startup registry that owns the host's
//! `{appData}/wapps` root).
pub mod protocol;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod protocol_security_tests;

// Kernel modules, re-exported under their previous paths (`wapps::install`,
// `wapps::manifest`, `wapps::storage`) plus flat manifest/storage helpers.
#[allow(unused_imports)]
pub use datazen_workspace_apps::manifest::{
    allowed_wapp_file_ext, is_valid_wapp_id, parse_manifest, validate_manifest, validate_wapp_dir,
    Contributions, PageContribution, Permission, ThemeContribution, WappManifest, MAX_WAPP_FILES,
    MAX_WAPP_UNCOMPRESSED,
};
#[allow(unused_imports)]
pub use datazen_workspace_apps::storage::{storage_get, storage_remove, storage_set};
#[allow(unused_imports)]
pub use datazen_workspace_apps::{install, manifest, storage, WAPP_API_VERSION};
#[allow(unused_imports)]
pub use protocol::{handle_datazen_request, parse_datazen_uri, WAPPS_OPEN_PAGE_EVENT};

pub use datazen_workspace_apps::manager::{LoadedWapp, WappManager};
