//! Desktop adapter for the shared connection import feature.
//!
//! The format parsers (DataZen / DBX / DBeaver / Navicat / DataGrip /
//! TablePlus) live in [`datazen_feature_core::connection_import`], next to the
//! import/export feature they belong to. The desktop host keeps only what is
//! environment-specific: the Tauri commands, the native file dialog and the
//! store merge.

pub(crate) mod ipc;
