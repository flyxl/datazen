//! Read-only projection of a frozen `ChangeSetArtifact` back into the
//! review/execution shapes the Data Sync window already consumes.
//!
//! The Artifact is the single source of truth after `dataSyncPrepare`
//! terminates. The legacy comparison store is rebuilt *from* the Artifact —
//! never from client rows — so preview, paging and selection validation keep
//! one contract (§5.1).

use crate::data_sync::job::artifact::{ChangeBlock, ChangeSetArtifact, TableMeta};
use crate::data_sync::{ComparisonResult, RowChange, TableResult};

/// Rebuild the server-owned comparison from the frozen blocks.
///
/// `blocks` is the only row source; `table_meta` carries the per-table
/// columns/PK baseline that the review panel and the statement generator need.
pub(crate) fn comparison_from_artifact(artifact: &ChangeSetArtifact) -> ComparisonResult {
    let mut tables = Vec::with_capacity(artifact.table_meta.len());
    for meta in &artifact.table_meta {
        let source_table = source_table_of(artifact, meta);
        let rows = artifact
            .blocks
            .iter()
            .filter(|block| block.relation == meta.relation)
            .map(row_change)
            .collect();
        tables.push(table_result(
            source_table,
            meta.relation.table.clone(),
            rows,
            meta,
        ));
    }
    ComparisonResult { tables }
}

fn row_change(block: &ChangeBlock) -> RowChange {
    RowChange {
        operation: block.operation,
        key: block.key.clone(),
        source_row: block.after.clone(),
        target_row: block.before.clone(),
        changed_columns: block.changed_columns.clone(),
        selected: true,
    }
}

fn table_result(
    source_table: String,
    target_table: String,
    rows: Vec<RowChange>,
    meta: &TableMeta,
) -> TableResult {
    let mut result = TableResult::matched(source_table, target_table, rows);
    result.columns = meta
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect();
    result.column_types = meta
        .columns
        .iter()
        .map(|column| column.data_type.clone())
        .collect();
    result.primary_keys = meta.pk_columns.clone();
    result.unchanged_count = meta.unchanged_count;
    result.source_filter = meta.source_filter.clone();
    result
}

/// Blocks are keyed by the *target* relation, so the source table name has to
/// come from the frozen mapping list (source and target names may differ).
fn source_table_of(artifact: &ChangeSetArtifact, meta: &TableMeta) -> String {
    artifact
        .mappings
        .iter()
        .find(|mapping| mapping.target_table == meta.relation.table)
        .map(|mapping| mapping.source_table.clone())
        .or_else(|| {
            artifact
                .table_meta
                .iter()
                .find(|candidate| candidate.relation == meta.relation)
                .map(|candidate| candidate.relation.table.clone())
        })
        .unwrap_or_else(|| meta.relation.table.clone())
}
