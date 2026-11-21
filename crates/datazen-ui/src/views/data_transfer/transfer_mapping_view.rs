//! The mapping view's pure helpers, ported from React
//! `packages/frontend/src/windows/data-transfer/transferMappingView.ts`
//! (T305 — the ledger line this module was seeded under as T307, the
//! old-generation number for the same file).
//!
//! Seven pure functions plus one private rule, each mirroring its
//! TypeScript source line for line (see the doc on each function for
//! the TS reference), over the table model ([`TransferTableVm`],
//! [`ColumnMappingTablePatch`], [`TransferMappingStatus`],
//! [`show_create_new_toggle`], [`show_target_type`]) this module owns.
//! The shell (T023-S2) and the Data Transfer window (T147) route every
//! mapping-logic decision through this module and no other mapping
//! logic exists in them — the one-definition rule the gate pair's own
//! doc comment states.

// The transfer view track is the production consumer; the status enum
// carries the full React union although this slice only matches on
// `CREATE_NEW`, so the dead-code lint is silenced here rather than in a
// crate-wide allow.
#![allow(dead_code)]

use super::column_mapping_row::TransferColumnMapping;
use super::recordset_editor::TransferRecordset;

/// React `TransferMappingStatus` (`commands/transfer.ts:8-14`).
///
/// Only the `CREATE_NEW` arm matters in this slice (it shows the
/// create-new toggle); the rest are carried so the model stays the
/// React union and T307 / the transfer view can match on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TransferMappingStatus {
    #[default]
    Matched,
    CreateNew,
    UnmappedSource,
    UnmappedTarget,
    Disabled,
    Incompatible,
}

impl TransferMappingStatus {
    /// The TS union's own spelling (`'MATCHED' | 'CREATE_NEW' | …`,
    /// `commands/transfer.ts:7-13`) — what React's objects row renders.
    pub(crate) fn spelling(self) -> &'static str {
        match self {
            TransferMappingStatus::Matched => "MATCHED",
            TransferMappingStatus::CreateNew => "CREATE_NEW",
            TransferMappingStatus::UnmappedSource => "UNMAPPED_SOURCE",
            TransferMappingStatus::UnmappedTarget => "UNMAPPED_TARGET",
            TransferMappingStatus::Disabled => "DISABLED",
            TransferMappingStatus::Incompatible => "INCOMPATIBLE",
        }
    }
}

/// React `TransferTableResult` (`commands/transfer.ts:132-150`), cut down
/// to what the shell (React 35–193) reads.
///
/// `source_columns` / `target_columns` are `Option` because React
/// distinguishes "absent" from "empty": absent source columns fall back
/// to the mapping rows' sources (`normalizeColumnMappings`), and absent
/// target columns behave as `[]` in the auto-match / warning paths.
/// `source_column_types` is the `Record<string, string>` as an ordered
/// association list (a handful of columns; linear lookup is enough).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TransferTableVm {
    pub source_table: String,
    pub target_table: String,
    pub status: TransferMappingStatus,
    pub create_new: bool,
    pub enabled: bool,
    pub column_mappings: Vec<TransferColumnMapping>,
    pub source_columns: Option<Vec<String>>,
    pub target_columns: Option<Vec<String>>,
    pub source_column_types: Vec<(String, String)>,
    /// React `table.sourcePrimaryKeys ?? []` — the recordset editor's
    /// key list (T023-S3 reads it; the shell itself never does).
    pub source_primary_keys: Option<Vec<String>>,
    /// React `table.recordset` — the recordset editor's model (T023-S3).
    pub recordset: Option<TransferRecordset>,
}

impl TransferTableVm {
    /// `table.sourceColumnTypes?.[column]`.
    pub(crate) fn source_type(&self, column: &str) -> Option<&str> {
        self.source_column_types
            .iter()
            .find(|(name, _)| name == column)
            .map(|(_, ty)| ty.as_str())
    }

    /// `table.sourceColumns ?? mappings.map((m) => m.sourceColumn)` — the
    /// source list `handleAutoMatch` works from (React 66).
    pub(crate) fn auto_match_sources(&self, mappings: &[TransferColumnMapping]) -> Vec<String> {
        match &self.source_columns {
            Some(columns) => columns.clone(),
            None => mappings
                .iter()
                .map(|m| m.source_column.clone())
                .collect(),
        }
    }
}

/// React `Partial<TransferTableResult>` as the shell reports it: the
/// three fields the shell (React 35–193) ever commits, plus the
/// recordset the recordset editor (React 195–440, T023-S3) commits.
///
/// `recordset` is `Option<Option<TransferRecordset>>` because React
/// distinguishes "leave it" (`None`) from "clear it" (`Some(None)`,
/// the enable toggle switching the recordset off).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ColumnMappingTablePatch {
    pub target_table: Option<String>,
    pub create_new: Option<bool>,
    pub column_mappings: Option<Vec<TransferColumnMapping>>,
    pub recordset: Option<Option<TransferRecordset>>,
}

impl ColumnMappingTablePatch {
    /// Apply the patch to a table, exactly as the parent's `onChange`
    /// spreads it over the stored `TransferTableResult`.
    pub(crate) fn apply_to(&self, table: &mut TransferTableVm) {
        if let Some(target_table) = &self.target_table {
            table.target_table = target_table.clone();
        }
        if let Some(create_new) = self.create_new {
            table.create_new = create_new;
        }
        if let Some(column_mappings) = &self.column_mappings {
            table.column_mappings = column_mappings.clone();
        }
        if let Some(recordset) = &self.recordset {
            table.recordset = recordset.clone();
        }
    }
}

/// React `showCreateNewToggle` (lines 39):
/// `structureMode || table.status === 'CREATE_NEW' || table.createNew`.
pub(crate) fn show_create_new_toggle(structure_mode: bool, table: &TransferTableVm) -> bool {
    structure_mode || table.status == TransferMappingStatus::CreateNew || table.create_new
}

/// React `showTargetType` (line 40): `structureMode && table.createNew`.
pub(crate) fn show_target_type(structure_mode: bool, table: &TransferTableVm) -> bool {
    structure_mode && table.create_new
}

// ---------------------------------------------------------------------------
// The helper functions (`transferMappingView.ts:4-56`): the signatures
// are the TS ones, transliterated.
// ---------------------------------------------------------------------------

/// `normalizeColumnMappings` (ported)
/// (`transferMappingView.ts:4-17`): every source column gets a mapping row.
/// Absent `sourceColumns` falls back to the distinct sources of the
/// stored mappings, in first-seen order; a source with no stored mapping
/// is synthesised as `{ targetColumn: '', skip: true }`. When the stored
/// mappings repeat a source, the last one wins — TS builds the lookup as
/// `new Map(...)`, so a later entry overwrites the earlier one.
pub(crate) fn normalize_column_mappings(table: &TransferTableVm) -> Vec<TransferColumnMapping> {
    let source_columns = match &table.source_columns {
        Some(columns) => columns.clone(),
        None => {
            let mut seen: Vec<String> = Vec::new();
            for mapping in &table.column_mappings {
                if !seen.contains(&mapping.source_column) {
                    seen.push(mapping.source_column.clone());
                }
            }
            seen
        }
    };
    source_columns
        .iter()
        .map(|source_column| {
            // Last wins, as in the TS `new Map(...)`: search from the back.
            table
                .column_mappings
                .iter()
                .rev()
                .find(|m| &m.source_column == source_column)
                .cloned()
                .unwrap_or(TransferColumnMapping {
                    source_column: source_column.clone(),
                    target_column: String::new(),
                    skip: true,
                    target_native_type: None,
                })
        })
        .collect()
}

/// `autoMatchColumnMappings` (ported)
/// (`transferMappingView.ts:20-37`): match source columns to target
/// columns by identical name. A matched row takes the source name as its
/// target and `skip: false`; an unmatched row is emptied and skipped.
/// A row's `targetNativeType` override survives the match. When the
/// existing rows repeat a source, the last one wins — TS builds
/// `existingBySource` as a `Map`, so a later entry overwrites the
/// earlier one.
pub(crate) fn auto_match_column_mappings(
    source_columns: &[String],
    target_columns: &[String],
    existing: &[TransferColumnMapping],
) -> Vec<TransferColumnMapping> {
    source_columns
        .iter()
        .map(|source_column| {
            let matched = target_columns.iter().any(|c| c == source_column);
            // Last wins, as in the TS `new Map(...)`: search from the back.
            let prev = existing
                .iter()
                .rev()
                .find(|m| &m.source_column == source_column);
            TransferColumnMapping {
                source_column: source_column.clone(),
                target_column: if matched {
                    source_column.clone()
                } else {
                    String::new()
                },
                skip: !matched,
                target_native_type: prev.and_then(|m| m.target_native_type.clone()),
            }
        })
        .collect()
}

/// `clearUnmappedColumnMappings` (ported)
/// (`transferMappingView.ts:40-45`): rows without a target column are
/// emptied and marked skipped; mapped rows pass through unchanged.
pub(crate) fn clear_unmapped_column_mappings(
    mappings: &[TransferColumnMapping],
) -> Vec<TransferColumnMapping> {
    mappings
        .iter()
        .map(|m| {
            if m.target_column.trim().is_empty() {
                TransferColumnMapping {
                    target_column: String::new(),
                    skip: true,
                    ..m.clone()
                }
            } else {
                m.clone()
            }
        })
        .collect()
}

/// `unmappedTargetColumns` (ported)
/// (`transferMappingView.ts:48-56`): target columns not referenced by any
/// active (non-skipped, non-empty) mapping. A table being created, or one
/// with no target columns, has no unmapped targets.
pub(crate) fn unmapped_target_columns(table: &TransferTableVm) -> Vec<String> {
    if table.create_new {
        return Vec::new();
    }
    let Some(target_columns) = &table.target_columns else {
        return Vec::new();
    };
    if target_columns.is_empty() {
        return Vec::new();
    }
    let used: Vec<&str> = table
        .column_mappings
        .iter()
        .filter(|m| !m.skip && !m.target_column.is_empty())
        .map(|m| m.target_column.as_str())
        .collect();
    target_columns
        .iter()
        .filter(|col| !used.contains(&col.as_str()))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// The gate pair (`transferMappingView.ts:74-100`): the Data Transfer
// window (T147) routes every gate decision through this module.
// ---------------------------------------------------------------------------

/// `tableHasActiveMappings` (`transferMappingView.ts:58-60`): the table
/// has at least one non-skipped mapping with a non-empty target column.
fn table_has_active_mappings(table: &TransferTableVm) -> bool {
    normalize_column_mappings(table)
        .iter()
        .any(|m| !m.skip && !m.target_column.trim().is_empty())
}

/// `rowClearsGate` (`transferMappingView.ts:68-73`), line for line:
/// whether one row, on its own, is complete enough to carry the gate. A
/// table that does not exist yet has nothing the backend can look the name
/// up for, so the name has to be one somebody typed. An enabled
/// create-new row behind nothing but whitespace is the one shape the
/// backend will happily echo back and then write as a nameless table, so
/// it does not clear the gate.
fn row_clears_gate(row: &TransferTableVm) -> bool {
    if !row.enabled {
        return false;
    }
    if row.create_new && row.target_table.trim().is_empty() {
        return false;
    }
    table_has_active_mappings(row)
}

/// `mappingGateAllowsAdvance` (ported)
/// (`transferMappingView.ts:82-84`): the one definition of "the mapping
/// step may be left". The Next gate and the post-prepare re-check both
/// call this on the *rows they hold at the moment they run*, so a rule
/// written twice cannot drift into two components disagreeing about the
/// same table.
pub(crate) fn mapping_gate_allows_advance(rows: &[TransferTableVm]) -> bool {
    rows.iter().any(row_clears_gate)
}

/// `mappingGateBlockReason` (ported)
/// (`transferMappingView.ts:92-100`): why the gate is shut, as an i18n
/// key. Only the unnamed-create-new case is reported. "Nothing is mapped
/// yet" is the state the user is still working in, not a dead end that
/// deserves a banner.
pub(crate) fn mapping_gate_block_reason(rows: &[TransferTableVm]) -> Option<&'static str> {
    if rows.iter().any(row_clears_gate) {
        return None;
    }
    let blocked = rows
        .iter()
        .any(|row| row.enabled && row.create_new && table_has_active_mappings(row));
    if blocked {
        Some("transfer.mapping.targetNameRequired")
    } else {
        None
    }
}
