//! Platform ports: the web-shaped tables exporter.
//!
//! The `platform` directory holds Rune ports of
//! `packages/frontend/src/platform/`; today it is only the exporter
//! ([`blob_tables_exporter`], emission in [`blob_tables_emission`]), the
//! planner mirror ([`export_plan`]) it plans through, the
//! provided-rows formatter mirror ([`export_format`]) its `provided`
//! branch formats through, the form-factor capability tables
//! ([`capabilities`]), and the compile-time form determination
//! ([`env`]).

pub mod blob_tables_emission;
pub mod blob_tables_exporter;
pub mod capabilities;
// T129: the platform port contracts (updater wire types, window routing,
// the exporter port, the binding registry with §7.4 semantics).
pub mod contracts;
pub mod env;
pub mod export_format;
pub mod export_plan;
