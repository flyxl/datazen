//! Catalog-bound column mapping drafts for the native insert-only transfer.
use crate::native_transfer_state::{NativeTool, NativeTransferState};
use crate::views::data_transfer::{
    column_mapping_editor::ColumnMappingEditorState, column_mapping_row::TransferColumnMapping,
    transfer_mapping_view::{ColumnMappingTablePatch, TransferTableVm},
};
use std::collections::HashSet;

pub(crate) struct NativeColumnMapping {
    pub open: bool,
    pub editor: ColumnMappingEditorState,
}

impl Default for NativeColumnMapping {
    fn default() -> Self {
        Self {
            open: false,
            editor: ColumnMappingEditorState::new(TransferTableVm::default(), false, true),
        }
    }
}

impl NativeTransferState {
    pub fn toggle_mapping_editor(&mut self) {
        if self.busy() || self.tool != NativeTool::Transfer {
            return;
        }
        if self.mapping.open {
            self.mapping = NativeColumnMapping::default();
            return;
        }
        let (Some(source), Some(target)) = (&self.source.catalog, &self.target.catalog) else {
            return;
        };
        let (Some(source_table), Some(target_table)) = (&source.choice.table, &target.choice.table)
        else {
            return;
        };
        self.mapping = NativeColumnMapping {
            open: true,
            editor: ColumnMappingEditorState::new(
                TransferTableVm {
                    source_table: source_table.clone(),
                    target_table: target_table.clone(),
                    enabled: true,
                    source_columns: Some(source.columns.clone()),
                    target_columns: Some(target.columns.clone()),
                    column_mappings: self
                        .pairs
                        .iter()
                        .map(|(left, right)| TransferColumnMapping {
                            source_column: left.clone(),
                            target_column: right.clone(),
                            ..TransferColumnMapping::default()
                        })
                        .collect(),
                    ..TransferTableVm::default()
                },
                false,
                target.read_only,
            ),
        };
    }

    pub fn change_editor_mappings(&mut self, patch: ColumnMappingTablePatch) {
        if !self.mapping.open || self.busy() || self.tool != NativeTool::Transfer {
            return;
        }
        let (Some(source), Some(target)) = (&self.source.catalog, &self.target.catalog) else {
            return;
        };
        if target.read_only
            || patch.target_table.is_some()
            || patch.create_new.is_some()
            || patch.recordset.is_some()
        {
            return;
        }
        let Some(rows) = &patch.column_mappings else {
            return;
        };
        let mut used_sources = HashSet::new();
        let mut used_targets = HashSet::new();
        let mut pairs = Vec::new();
        for row in rows {
            if !source.columns.contains(&row.source_column)
                || row.target_native_type.is_some()
                || (!row.target_column.is_empty() && !target.columns.contains(&row.target_column))
                || !used_sources.insert(&row.source_column)
            {
                self.error = Some("The mapping does not match the selected catalogs.".into());
                return;
            }
            if row.skip {
                continue;
            }
            if row.target_column.is_empty() {
                self.error = Some("Select a target column or skip that source column.".into());
                return;
            }
            if !used_targets.insert(&row.target_column) {
                self.error = Some("Each column can be mapped once.".into());
                return;
            }
            pairs.push((row.source_column.clone(), row.target_column.clone()));
        }
        self.mapping.editor.apply_table_patch(&patch);
        self.pairs = pairs;
        self.error = None;
        self.inserted = None;
    }
}
