//! Data Sync utilities, ported from React
//! `packages/frontend/src/windows/data-sync/utils.ts` (tasklist T500).
//!
//! `SyncState` itself already lives in the compare step's port (T024,
//! [`crate::views::data_sync::data_sync_compare_step::SyncState`]); this
//! file carries the three helpers that remained: the schema set of a
//! table list, the default-schema pick, and the reviewable-state predicate.
//!
//! Type mirrors: the tables come as [`TableInfoVm`] — the schema-diff
//! table-names port's field-for-field mirror of `TableInfo`
//! (`name`, `schema: string | null`, `tableType`, `rowCount`), the same
//! shape the sync window's table listing produces.

use crate::views::data_sync::data_sync_compare_step::SyncState;
use crate::views::schema_diff::schema_diff_table_names::TableInfoVm;

/// `uniqueSchemasFromTables`: the distinct, trimmed, non-empty schema names
/// of a table list, sorted.
///
/// React trims each name (`t.schema?.trim()`), keeps the *first* occurrence
/// in `Set` order (a duplicate later changes nothing), and returns the
/// `[...set].sort()` array. Rust's `sort()` orders `String`s bytewise —
/// equal to JavaScript's UTF-16 code-unit order for every BMP schema name
/// (the difference exists only for astral-plane characters, which a schema
/// name is not).
pub(crate) fn unique_schemas_from_tables(tables: &[TableInfoVm]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for table in tables {
        let Some(trimmed) = table.schema.as_deref().map(str::trim) else {
            continue;
        };
        if trimmed.is_empty() {
            continue;
        }
        if !seen.iter().any(|existing| existing == trimmed) {
            seen.push(trimmed.to_string());
        }
    }
    seen.sort();
    seen
}

/// `pickDefaultSchema`: the previous schema if it still exists, else
/// `'public'` if the list has it, else the first schema, else the empty
/// string.
pub(crate) fn pick_default_schema(schemas: &[String], previous: Option<&str>) -> String {
    if let Some(previous) = previous {
        if schemas.iter().any(|schema| schema == previous) {
            return previous.to_string();
        }
    }
    if schemas.iter().any(|schema| schema == "public") {
        return "public".to_string();
    }
    schemas.first().cloned().unwrap_or_default()
}

/// `isComparedSyncState`: whether the current state carries a reviewable
/// comparison result — the four states past the comparison.
pub(crate) fn is_compared_sync_state(sync_state: SyncState) -> bool {
    matches!(
        sync_state,
        SyncState::Compared | SyncState::Executing | SyncState::Unknown | SyncState::Done
    )
}
