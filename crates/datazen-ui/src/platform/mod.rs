//! Platform ports: the web-shaped tables exporter.
//!
//! The `platform` directory holds Rune ports of
//! `packages/frontend/src/platform/`; today it is only the exporter
//! ([`blob_tables_exporter`], emission in [`blob_tables_emission`]), the
//! planner mirror ([`export_plan`]) it plans through, the
//! provided-rows formatter mirror ([`export_format`]) its `provided`
//! branch formats through, and the form-factor capability tables
//! ([`capabilities`]).

pub(crate) mod blob_tables_emission;
pub(crate) mod blob_tables_exporter;
pub(crate) mod capabilities;
pub(crate) mod export_format;
pub(crate) mod export_plan;
