use super::types::{
    CopyFormat, DataTableContextMenuHandlers, MenuAction, NativeMenuItemDef, SetValueActionId,
};

pub(crate) fn item(
    id: &str,
    label: String,
    present: bool,
    action: MenuAction,
) -> Option<NativeMenuItemDef> {
    if !present {
        return None;
    }
    Some(NativeMenuItemDef::Item {
        id: id.to_owned(),
        label,
        action,
        enabled: true,
    })
}

pub(crate) fn enabled_item(
    id: &str,
    label: String,
    present: bool,
    enabled: bool,
    action: MenuAction,
) -> Option<NativeMenuItemDef> {
    if !present {
        return None;
    }
    Some(NativeMenuItemDef::Item {
        id: id.to_owned(),
        label,
        action,
        enabled,
    })
}

pub(crate) fn push(defs: Vec<Option<NativeMenuItemDef>>) -> Vec<NativeMenuItemDef> {
    defs.into_iter().flatten().collect()
}

pub(crate) fn submenu(
    id: &str,
    label: String,
    items: Vec<NativeMenuItemDef>,
) -> Option<NativeMenuItemDef> {
    if items.is_empty() {
        None
    } else {
        Some(NativeMenuItemDef::Submenu {
            id: id.to_owned(),
            label,
            items,
        })
    }
}

fn handler_present(action: &MenuAction, handlers: &DataTableContextMenuHandlers) -> bool {
    match action {
        MenuAction::Copy => handlers.on_copy,
        MenuAction::CopyRow => handlers.on_copy_row,
        MenuAction::CopyAsJson => handlers.on_copy_as_json,
        MenuAction::CopyAsSqlInsert => handlers.on_copy_as_sql_insert,
        MenuAction::CopyAsUpdate => handlers.on_copy_as_update,
        MenuAction::CopyAsCsv => handlers.on_copy_as_csv,
        MenuAction::CopyColumnName => handlers.on_copy_column_name,
        MenuAction::CopyColumnData => handlers.on_copy_column_data,
        MenuAction::SetNull => handlers.on_set_null,
        MenuAction::FilterByValue => handlers.on_filter_by_value,
        MenuAction::CopySelectedRows => handlers.on_copy_selected_rows,
        MenuAction::DeleteRow => handlers.on_delete_row,
        MenuAction::Export => handlers.on_export,
        MenuAction::RestoreRow => handlers.on_restore_row,
        MenuAction::ShowReferencingRows => handlers.on_show_referencing_rows,
        MenuAction::SetValue(_) => handlers.on_set_value,
        MenuAction::LoadCandidates => handlers.on_load_candidates,
        MenuAction::CopySelectedCells => handlers.on_copy_selected_cells,
        MenuAction::CopySelectedCellsAs { .. } => handlers.on_copy_selected_cells_as,
        MenuAction::PasteSelectedCells => handlers.on_paste_selected_cells,
        MenuAction::PasteRows => handlers.on_paste_rows,
        MenuAction::CopyRowsAsNewRows => handlers.on_copy_rows_as_new_rows,
        MenuAction::SortAscending(_) => handlers.on_sort_ascending,
        MenuAction::SortDescending(_) => handlers.on_sort_descending,
        MenuAction::RemoveSort(_) => handlers.on_remove_sort,
        MenuAction::ClearSorts => handlers.on_clear_sorts,
        MenuAction::FreezeThrough(_) => handlers.on_freeze_through,
        MenuAction::UnfreezeColumns => handlers.on_unfreeze_columns,
        MenuAction::ColumnStats(_) => handlers.on_column_stats,
        MenuAction::ColumnPresentation(_) => handlers.on_column_presentation,
    }
}

pub fn invoke(
    item: &NativeMenuItemDef,
    handlers: &DataTableContextMenuHandlers,
) -> Option<MenuAction> {
    match item {
        NativeMenuItemDef::Item {
            action, enabled, ..
        } => {
            if *enabled && handler_present(action, handlers) {
                Some(action.clone())
            } else {
                None
            }
        }
        NativeMenuItemDef::Separator | NativeMenuItemDef::Submenu { .. } => None,
    }
}

pub fn is_set_value_action(action: &MenuAction, id: SetValueActionId) -> bool {
    matches!(action, MenuAction::SetValue(value) if *value == id)
}

pub fn is_copy_selected_cells_as(
    action: &MenuAction,
    format: CopyFormat,
    include_header: bool,
) -> bool {
    matches!(
        action,
        MenuAction::CopySelectedCellsAs {
            format: actual_format,
            include_header: actual_header,
        } if *actual_format == format && *actual_header == include_header
    )
}
