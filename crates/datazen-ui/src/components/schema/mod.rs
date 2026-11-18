//! Schema UI components (the table-schema diff panel).
//!
//! No production consumer yet; the schema-diff view track will wire them in.
//! Public(crate) re-exports are kept for that upcoming integration.

#[allow(unused_imports)] // consumers will be added by the schema-diff view wiring
pub(crate) use schema_diff_panel::{
    format_schema_diff_text, formatSchemaDiffText, ChangedColumnDiffVm, CheckConstraintDiffVm,
    ColumnDiffVm, SchemaDiffPanel, TableOptionsDiffVm, TableOptionsSnapshotVm, TableSchemaDiffVm,
};

pub(crate) mod schema_diff_panel;
