//! Runtime UI/theme extension base: registry over installed wapp packages.
//!
//! Wapps live in `{appData}/wapps/{id}/` (folder name == `manifest.id`).
//! [`WappManager`] loads every valid package at startup, tracks enabled
//! state via a `.enabled` marker file, and is shared through `AppState`.
//!
//! The **kernel** — manifest schema and rules 1–7, ZIP and directory install,
//! per-wapp key-value storage, the installed-app registry, URI grammar and
//! routing decisions — lives in `packages/feature-core` as the
//! `feature_core::workspace_apps` module. Call sites reference that module
//! directly; this module no longer re-exports shared-package names.
//!
//! What stays here is **form-specific**: [`protocol`], the `datazen://` bridge
//! that speaks Tauri IPC. [`WappManager`] is a kernel type now, reached via
//! `datazen_feature_core::workspace_apps::manager::WappManager`.
pub mod protocol;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod protocol_security_tests;

#[allow(unused_imports)]
pub use protocol::{handle_datazen_request, parse_datazen_uri, WAPPS_OPEN_PAGE_EVENT};
