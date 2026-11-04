//! Monitor scheduling, grants and delivery rules.
//!
//! The module owns the form-neutral monitor invariants. Desktop supplies the
//! dashboard, workflow, persistence and notification adapters through the
//! application ports; no Tauri or SQLite type crosses this boundary.

pub mod domain;

#[cfg(feature = "dashboard")]
pub mod application;
