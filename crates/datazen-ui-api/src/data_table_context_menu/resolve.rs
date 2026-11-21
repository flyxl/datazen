use super::types::*;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuFlags {
    pub has_cell_context: bool,
    pub has_header_context: bool,
    pub header_sort: bool,
    pub header_freeze: bool,
    pub header_stats: bool,
    pub header_presentation: bool,
    pub has_selected_rows: bool,
    pub multi_row_selection: bool,
    pub export_enabled: bool,
    pub can_filter_by_value: bool,
    pub can_set_null: bool,
    pub can_delete: bool,
    pub can_restore_row: bool,
    pub set_value_available: bool,
    pub has_cell_range: bool,
    pub can_copy_selection: bool,
    pub can_paste_selection: bool,
    pub selection_ignored_ranges: usize,
    pub can_paste_rows: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuContext {
    pub hit: Option<(usize, String)>,
    pub hit_header_col: Option<String>,
    pub header_sort: Option<HeaderSort>,
    pub header_freeze: Option<HeaderFreeze>,
    pub header_stats: Option<HeaderColumn>,
    pub header_presentation: Option<HeaderColumn>,
    pub effective_col_name: Option<String>,
    pub effective_col_idx: isize,
    pub hit_col_idx: isize,
    pub hit_cell_value: Option<Value>,
    pub cell_text_for_copy: String,
    pub selected_indices: Vec<usize>,
    pub selected_data_rows: Vec<Vec<Value>>,
    pub action_row: Option<Vec<Value>>,
    pub selection_row_actions: Vec<Vec<Value>>,
    pub delete_indices: Vec<usize>,
    pub csv_rows: Vec<Vec<Value>>,
    pub flags: MenuFlags,
}

fn column_index(columns: &[String], name: &str) -> isize {
    columns
        .iter()
        .position(|column| column == name)
        .map(|index| index as isize)
        .unwrap_or(-1)
}

fn js_scalar_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => String::new(),
        Value::Number(number) => number.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

pub fn resolve_menu_context(input: &MenuInput<'_>, event: &MenuEvent) -> MenuContext {
    let hit = match &event.target {
        HitTarget::Cell {
            row_index,
            column_name,
        } => Some((*row_index, column_name.clone())),
        _ => None,
    };
    let hit_header_col = if hit.is_none() {
        match &event.target {
            HitTarget::Header { column_name } => Some(column_name.clone()),
            _ => None,
        }
    } else {
        None
    };
    let caps = input.capabilities;
    let header_sort = hit_header_col
        .as_ref()
        .filter(|_| input.sorts.is_some())
        .map(|column| HeaderSort {
            column: column.clone(),
        });
    let header_freeze = hit_header_col.as_ref().filter(|_| {
        input.frozen_columns.is_some() || caps.freeze_through || caps.unfreeze_columns
    }).map(|column| HeaderFreeze {
        index: column_index(input.column_names, column),
        frozen: input.frozen_columns.unwrap_or(0),
    });
    let header_stats = hit_header_col
        .as_ref()
        .filter(|_| caps.column_stats)
        .map(|column| HeaderColumn {
            column: column.clone(),
        });
    let header_presentation = hit_header_col
        .as_ref()
        .filter(|_| caps.column_presentation)
        .map(|column| HeaderColumn {
            column: column.clone(),
        });
    let effective_col_name = hit
        .as_ref()
        .map(|(_, name)| name.clone())
        .or_else(|| hit_header_col.clone());
    let effective_col_idx = effective_col_name
        .as_deref()
        .map(|name| column_index(input.column_names, name))
        .unwrap_or(-1);
    let hit_row = hit
        .as_ref()
        .and_then(|(row, _)| input.rows.get(*row).cloned());
    let hit_col_idx = hit
        .as_ref()
        .map(|(_, column)| column_index(input.column_names, column))
        .unwrap_or(-1);
    let hit_cell_value = hit_row.as_ref().map(|row| {
        if hit_col_idx >= 0 {
            row.get(hit_col_idx as usize).cloned().unwrap_or(Value::Null)
        } else {
            Value::Null
        }
    });
    let cell_text_for_copy = match &hit_cell_value {
        Some(value) if value.is_null() => String::new(),
        Some(value @ (Value::Object(_) | Value::Array(_))) => {
            serde_json::to_string(value).unwrap_or_default()
        }
        Some(value) => js_scalar_string(value),
        None => input.context_cell_text.unwrap_or("").to_owned(),
    };
    let selected_indices: Vec<usize> = input.selected_rows.iter().copied().collect();
    let selected_data_rows: Vec<Vec<Value>> = selected_indices
        .iter()
        .filter_map(|index| input.rows.get(*index).cloned())
        .collect();
    let has_selected_rows = !selected_data_rows.is_empty();
    let has_cell_context = hit.is_some() && hit_row.is_some();
    let multi_row_selection = has_cell_context
        && input.selected_rows.len() > 1
        && hit
            .as_ref()
            .map(|(row, _)| input.selected_rows.contains(row))
            .unwrap_or(false);
    let action_row = if has_cell_context && hit_row.is_some() && !multi_row_selection {
        hit_row.clone()
    } else {
        None
    };
    let selection_row_actions = if multi_row_selection || !has_cell_context {
        selected_data_rows.clone()
    } else {
        Vec::new()
    };
    let can_filter_by_value = !input.loading && has_cell_context && caps.add_filter && hit.is_some();
    let set_null_allowed = input.enable_set_null.unwrap_or(caps.cell_edit);
    let can_set_null = has_cell_context && set_null_allowed && caps.cell_edit && hit.is_some();
    let delete_indices = if has_selected_rows {
        selected_indices.clone()
    } else if let Some((row, _)) = &hit {
        vec![*row]
    } else {
        Vec::new()
    };
    let can_delete = caps.delete_rows && !input.primary_key_columns.is_empty() && !delete_indices.is_empty();
    let can_restore_row = hit
        .as_ref()
        .filter(|(row, _)| {
            input
                .dirty_by_row
                .and_then(|map| map.get(row))
                .map(|state| *state == RowState::Deleted)
                .unwrap_or(false)
        })
        .map(|_| caps.restore_row)
        .unwrap_or(false);
    let csv_rows = if action_row.is_some() {
        vec![action_row.clone().unwrap_or_default()]
    } else {
        selected_data_rows.clone()
    };
    let flags = MenuFlags {
        has_cell_context,
        has_header_context: hit_header_col.is_some(),
        header_sort: header_sort.is_some(),
        header_freeze: header_freeze.is_some(),
        header_stats: header_stats.is_some(),
        header_presentation: header_presentation.is_some(),
        has_selected_rows,
        multi_row_selection,
        export_enabled: input.export_enabled,
        can_filter_by_value,
        can_set_null,
        can_delete,
        can_restore_row,
        set_value_available: input.set_value_state.is_some(),
        has_cell_range: input.has_cell_range,
        can_copy_selection: input.can_copy_selection,
        can_paste_selection: input.can_paste_selection,
        selection_ignored_ranges: input.selection_ignored_ranges,
        can_paste_rows: input.can_paste_rows,
    };
    MenuContext {
        hit,
        hit_header_col,
        header_sort,
        header_freeze,
        header_stats,
        header_presentation,
        effective_col_name,
        effective_col_idx,
        hit_col_idx,
        hit_cell_value,
        cell_text_for_copy,
        selected_indices,
        selected_data_rows,
        action_row,
        selection_row_actions,
        delete_indices,
        csv_rows,
        flags,
    }
}

pub fn build_menu_labels(input: &MenuInput<'_>, ctx: &MenuContext, t: &dyn Fn(&str) -> String) -> MenuLabels {
    let copy = t("common.copy");
    let selected_rows = t("export.selectedRows");
    let copy_selected_rows = if input.selected_rows.len() > 1 {
        format!("{copy} {selected_rows} ({})", input.selected_rows.len())
    } else {
        format!("{copy} {selected_rows}")
    };
    let delete_row = if ctx.delete_indices.len() > 1 {
        format!("{} ({})", t("dataTable.deleteRow"), ctx.delete_indices.len())
    } else {
        t("dataTable.deleteRow")
    };
    let export = if !input.selected_rows.is_empty() {
        format!("{} ({} {})", t("export.export"), selected_rows, input.selected_rows.len())
    } else {
        t("export.export")
    };
    MenuLabels {
        copy,
        copy_row: t("dataTable.copyRow"),
        more_actions: t("dataTable.moreActions"),
        copy_as_json: t("dataTable.copyAsJson"),
        copy_as_sql_insert: t("dataTable.copyAsSqlInsert"),
        copy_as_update: t("dataTable.copyAsUpdate"),
        copy_as_csv: t("dataTable.copyAsCsv"),
        copy_column_name: t("dataTable.copyColumnName"),
        copy_column_data: t("dataTable.copyColumnData"),
        set_null: t("dataTable.setNull"),
        filter_by_value: t("dataTable.filterByValue"),
        copy_selected_rows,
        delete_row,
        export,
        restore_row: t("tableData.restoreRow"),
        referenced_by: t("dataTable.referencedBy"),
        set_value: t("cellEditor.setValue"),
        set_empty: t("cellEditor.setEmpty"),
        set_default: t("cellEditor.setDefault"),
        revert_cell: t("cellEditor.revertCell"),
        pick_date: t("dataTable.pickDate"),
        load_binary_from_file: t("dataTable.loadBinaryFromFile"),
        set_current_datetime: t("dataTable.setCurrentDatetime"),
        load_candidates: t("cellEditor.loadCandidates"),
        copy_selected_cells: t("dataTable.copySelectedCells"),
        copy_selected_cells_as: t("dataTable.copySelectedCellsAs"),
        copy_selected_cells_as_tsv: t("dataTable.copySelectedCellsAsTsv"),
        copy_selected_cells_as_tsv_with_header: t("dataTable.copySelectedCellsAsTsvWithHeader"),
        copy_selected_cells_as_csv: t("dataTable.copySelectedCellsAsCsv"),
        copy_selected_cells_as_json: t("dataTable.copySelectedCellsAsJson"),
        copy_selected_cells_as_plain: t("dataTable.copySelectedCellsAsPlain"),
        copy_selected_cells_as_html: t("dataTable.copySelectedCellsAsHtml"),
        copy_selected_cells_as_markdown: t("dataTable.copySelectedCellsAsMarkdown"),
        paste_selected_cells: t("dataTable.pasteSelectedCells"),
        paste_rows: t("dataTable.pasteRows"),
        copy_rows_as_new_rows: t("dataTable.copyRowsAsNewRows"),
        sort_multi: t("dataTable.sortMulti"),
        sort_ascending: t("dataTable.sortAscending"),
        sort_descending: t("dataTable.sortDescending"),
        remove_sort_column: t("dataTable.removeSortColumn"),
        clear_sorts: t("dataTable.clearSorts"),
        freeze_through: t("dataTable.freezeThrough"),
        unfreeze_columns: t("dataTable.unfreezeColumns"),
        column_stats: t("dataTable.columnStats"),
        column_presentation: t("dataTable.columnPresentation"),
    }
}
