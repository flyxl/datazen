//! Platform ports: the web-shaped tables exporter.
//!
//! The `platform` directory holds Rune ports of
//! `packages/frontend/src/platform/`; today it is only this module and
//! the planner mirror ([`export_plan`]) it plans through.

pub(crate) mod blob_tables_exporter;
pub(crate) mod export_plan;
