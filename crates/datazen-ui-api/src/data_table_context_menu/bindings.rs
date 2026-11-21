use super::resolve::MenuContext;
use super::types::*;

#[derive(Clone, Debug, PartialEq)]
pub struct MenuBindings {
    pub entries: Vec<(MenuEntryId, MenuAction)>,
    pub position: (i64, i64),
}

fn table_name(input: &MenuInput<'_>) -> String {
    input.export_table_name.unwrap_or("table").to_owned()
}

fn filter_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

pub fn build_menu_bindings(
    input: &MenuInput<'_>,
    ctx: &MenuContext,
    event: &MenuEvent,
) -> MenuBindings {
    let caps = input.capabilities;
    let mut entries: Vec<(MenuEntryId, MenuAction)> = Vec::new();
    let hit = ctx.hit.as_ref();

    if ctx.flags.has_cell_context && !ctx.flags.multi_row_selection {
        entries.push((
            MenuEntryId::Copy,
            MenuAction::Copy(CopySource::Text(ctx.cell_text_for_copy.clone())),
        ));
    }
    if let Some(row) = &ctx.action_row {
        entries.push((
            MenuEntryId::CopyRow,
            MenuAction::Copy(CopySource::RowsTsv(vec![row.clone()])),
        ));
        let columns = input.column_names.to_vec();
        entries.push((
            MenuEntryId::CopyAsJson,
            MenuAction::Copy(CopySource::RowsJson(columns, vec![row.clone()])),
        ));
        entries.push((
            MenuEntryId::CopyAsSqlInsert,
            MenuAction::Copy(CopySource::SqlInsert {
                table: table_name(input),
                columns: input.column_names.to_vec(),
                rows: vec![row.clone()],
            }),
        ));
        entries.push((
            MenuEntryId::CopyAsUpdate,
            MenuAction::Copy(CopySource::SqlUpdate {
                table: table_name(input),
                columns: input.column_names.to_vec(),
                rows: vec![row.clone()],
                primary_keys: input.primary_key_columns.to_vec(),
            }),
        ));
    } else if !ctx.selection_row_actions.is_empty() {
        let columns = input.column_names.to_vec();
        entries.push((
            MenuEntryId::CopyAsJson,
            MenuAction::Copy(CopySource::RowsJson(
                columns,
                ctx.selection_row_actions.clone(),
            )),
        ));
        entries.push((
            MenuEntryId::CopyAsSqlInsert,
            MenuAction::Copy(CopySource::SqlInsert {
                table: table_name(input),
                columns: input.column_names.to_vec(),
                rows: ctx.selection_row_actions.clone(),
            }),
        ));
        entries.push((
            MenuEntryId::CopyAsUpdate,
            MenuAction::Copy(CopySource::SqlUpdate {
                table: table_name(input),
                columns: input.column_names.to_vec(),
                rows: ctx.selection_row_actions.clone(),
                primary_keys: input.primary_key_columns.to_vec(),
            }),
        ));
    }
    if !ctx.csv_rows.is_empty() {
        entries.push((
            MenuEntryId::CopyAsCsv,
            MenuAction::Copy(CopySource::RowsCsv(
                input.column_names.to_vec(),
                ctx.csv_rows.clone(),
            )),
        ));
    }
    if let Some(column) = &ctx.effective_col_name {
        entries.push((
            MenuEntryId::CopyColumnName,
            MenuAction::Copy(CopySource::Text(column.clone())),
        ));
    }
    if ctx.effective_col_idx >= 0 {
        entries.push((
            MenuEntryId::CopyColumnData,
            MenuAction::Copy(CopySource::ColumnValues(ctx.effective_col_idx as usize)),
        ));
    }
    if ctx.flags.can_set_null && !ctx.flags.multi_row_selection {
        if let Some((row, column)) = hit {
            entries.push((
                MenuEntryId::SetNull,
                MenuAction::SetNull {
                    row: *row,
                    column: column.clone(),
                },
            ));
        }
    }
    if ctx.flags.can_filter_by_value {
        if let Some((row, column)) = hit {
            let value = ctx
                .hit_cell_value
                .as_ref()
                .map(filter_value)
                .unwrap_or_default();
            let _ = row;
            entries.push((
                MenuEntryId::FilterByValue,
                MenuAction::FilterByValue {
                    column: column.clone(),
                    value,
                },
            ));
        }
    }
    if ctx.flags.has_selected_rows {
        entries.push((
            MenuEntryId::CopySelectedRows,
            MenuAction::Copy(CopySource::RowsTsv(ctx.selected_data_rows.clone())),
        ));
    }
    if ctx.flags.can_delete {
        entries.push((
            MenuEntryId::DeleteRow,
            MenuAction::DeleteRows(ctx.delete_indices.clone()),
        ));
    }
    if ctx.flags.export_enabled {
        entries.push((MenuEntryId::Export, MenuAction::Export));
    }
    if ctx.flags.can_restore_row {
        if let Some((row, _)) = hit {
            entries.push((MenuEntryId::RestoreRow, MenuAction::RestoreRow(*row)));
        }
    }
    if caps.show_referencing_rows {
        if let Some((row, _)) = hit {
            entries.push((
                MenuEntryId::ShowReferencingRows,
                MenuAction::ShowReferencingRows(*row),
            ));
        }
    }
    if let (Some(state), Some((row, column))) = (input.set_value_state, hit) {
        let available = [
            (state.can_null, SetValueActionId::SetNull),
            (state.can_empty, SetValueActionId::SetEmpty),
            (state.row_kind_insert, SetValueActionId::SetDefault),
            (state.has_pending, SetValueActionId::RevertCell),
            (state.can_pick_date, SetValueActionId::PickDate),
            (state.can_load_binary, SetValueActionId::LoadBinaryFromFile),
            (
                state.can_set_current_datetime,
                SetValueActionId::SetCurrentDatetime,
            ),
        ];
        for (allowed, action_id) in available {
            if allowed {
                entries.push((
                    MenuEntryId::SetValue,
                    MenuAction::SetValue {
                        row: *row,
                        column: column.clone(),
                        action: action_id,
                    },
                ));
            }
        }
    }
    if caps.load_candidates {
        if let Some((row, column)) = hit {
            entries.push((
                MenuEntryId::LoadCandidates,
                MenuAction::LoadCandidates {
                    row: *row,
                    column: column.clone(),
                },
            ));
        }
    }
    if ctx.flags.can_copy_selection && caps.copy_selected_cells {
        entries.push((
            MenuEntryId::CopySelectedCells,
            MenuAction::CopySelectedCells,
        ));
    }
    if ctx.flags.can_copy_selection && caps.copy_selected_cells_as {
        entries.push((
            MenuEntryId::CopySelectedCellsAs,
            MenuAction::CopySelectedCellsAs {
                format: ClipboardFormat::Tsv,
                include_header: false,
            },
        ));
    }
    if ctx.flags.can_paste_selection && caps.paste_selected_cells {
        entries.push((
            MenuEntryId::PasteSelectedCells,
            MenuAction::PasteSelectedCells,
        ));
    }
    if caps.paste_rows {
        entries.push((MenuEntryId::PasteRows, MenuAction::PasteRows));
    }
    if caps.copy_rows_as_new_rows {
        entries.push((
            MenuEntryId::CopyRowsAsNewRows,
            MenuAction::CopyRowsAsNewRows,
        ));
    }
    if caps.add_sort {
        if let Some(header) = &ctx.header_sort {
            entries.push((
                MenuEntryId::SortAscending,
                MenuAction::SortAscending(header.column.clone()),
            ));
            entries.push((
                MenuEntryId::SortDescending,
                MenuAction::SortDescending(header.column.clone()),
            ));
        }
    }
    if caps.remove_sort {
        if let Some(header) = &ctx.header_sort {
            entries.push((
                MenuEntryId::RemoveSort,
                MenuAction::RemoveSort(header.column.clone()),
            ));
        }
    }
    if caps.clear_sorts {
        entries.push((MenuEntryId::ClearSorts, MenuAction::ClearSorts));
    }
    if caps.freeze_through {
        if let Some(freeze) = ctx.header_freeze {
            if freeze.index >= 0 {
                entries.push((
                    MenuEntryId::FreezeThrough,
                    MenuAction::FreezeThrough(freeze.index as usize),
                ));
            }
        }
    }
    if caps.unfreeze_columns {
        entries.push((MenuEntryId::UnfreezeColumns, MenuAction::UnfreezeColumns));
    }
    if caps.column_stats {
        if let Some(header) = &ctx.header_stats {
            entries.push((
                MenuEntryId::ColumnStats,
                MenuAction::ColumnStats(header.column.clone()),
            ));
        }
    }
    if caps.column_presentation {
        if let Some(header) = &ctx.header_presentation {
            entries.push((
                MenuEntryId::ColumnPresentation,
                MenuAction::ColumnPresentation(header.column.clone()),
            ));
        }
    }

    MenuBindings {
        entries,
        position: (event.client_x, event.client_y),
    }
}
