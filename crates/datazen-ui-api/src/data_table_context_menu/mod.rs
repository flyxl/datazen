mod events;
mod items;
mod serialize;
mod sql;
mod tests;
mod types;
mod values;

pub(crate) use serialize::{
    cell_to_text, escape_delimited, escape_html_cell, escape_markdown_cell_text, table_width,
};

pub use events::{
    resolve_data_table_cell_from_event, resolve_data_table_header_col_from_event, DomElement,
};
pub use items::invoke;
pub use serialize::{
    row_to_named_record, serialize_cells, serialize_data_table_column_values,
    serialize_data_table_rows_as_csv, serialize_data_table_rows_as_json,
    serialize_data_table_rows_as_tsv, SerializeFormat,
};
pub use sql::{
    format_row_as_sql_delete, format_row_as_sql_insert, format_row_as_sql_update,
    format_rows_as_sql_insert, format_rows_as_sql_update,
};
pub use types::{
    BuildDataTableContextMenuArgs, CellContext, CopyFormat, DataTableContextMenuHandlers,
    DataTableContextMenuLabels, HeaderFreeze, HeaderPresentation, HeaderSort, HeaderStats,
    MenuAction, NativeMenuItemDef, RowKind, SetValueAction, SetValueActionId, SetValueState,
};
pub use values::{resolve_set_value_actions, set_value_action_label, LOAD_CANDIDATES_LABEL_KEY};

use items::{enabled_item, item, push, submenu};

fn set_value_submenu(args: &BuildDataTableContextMenuArgs) -> Option<NativeMenuItemDef> {
    let Some(state) = &args.set_value else {
        return None;
    };
    if !args.handlers.on_set_value {
        return None;
    }
    let mut entries: Vec<NativeMenuItemDef> = resolve_set_value_actions(state)
        .into_iter()
        .map(|action| {
            let id = format!("set-value-{}", action.id.as_str());
            item(
                &id,
                set_value_action_label(action.id, &args.labels),
                true,
                MenuAction::SetValue(action.id),
            )
        })
        .flatten()
        .collect();
    if state.can_load_candidates && args.handlers.on_load_candidates {
        if let Some(entry) = item(
            "set-value-load-candidates",
            args.labels.load_candidates.clone().unwrap_or_default(),
            true,
            MenuAction::LoadCandidates,
        ) {
            entries.push(entry);
        }
    }
    submenu(
        "set-value",
        args.labels.set_value.clone().unwrap_or_default(),
        entries,
    )
}

fn copy_as_item(
    id: &str,
    label: Option<&String>,
    present: bool,
    format: CopyFormat,
    include_header: bool,
) -> Option<NativeMenuItemDef> {
    item(
        id,
        label.cloned().unwrap_or_default(),
        present,
        MenuAction::CopySelectedCellsAs {
            format,
            include_header,
        },
    )
}

fn clipboard_block(args: &BuildDataTableContextMenuArgs) -> Vec<NativeMenuItemDef> {
    if !args.has_cell_range {
        return Vec::new();
    }
    let present = args.handlers.on_copy_selected_cells_as;
    let copy_as = submenu(
        "copy-selected-cells-as",
        args.labels
            .copy_selected_cells_as
            .clone()
            .unwrap_or_default(),
        push(vec![
            copy_as_item(
                "copy-selected-cells-as-tsv",
                args.labels.copy_selected_cells_as_tsv.as_ref(),
                present,
                CopyFormat::Tsv,
                false,
            ),
            copy_as_item(
                "copy-selected-cells-as-tsv-header",
                args.labels.copy_selected_cells_as_tsv_with_header.as_ref(),
                present,
                CopyFormat::Tsv,
                true,
            ),
            copy_as_item(
                "copy-selected-cells-as-csv",
                args.labels.copy_selected_cells_as_csv.as_ref(),
                present,
                CopyFormat::Csv,
                false,
            ),
            copy_as_item(
                "copy-selected-cells-as-json",
                args.labels.copy_selected_cells_as_json.as_ref(),
                present,
                CopyFormat::Json,
                false,
            ),
            copy_as_item(
                "copy-selected-cells-as-plain",
                args.labels.copy_selected_cells_as_plain.as_ref(),
                present,
                CopyFormat::Plain,
                false,
            ),
            copy_as_item(
                "copy-selected-cells-as-html",
                args.labels.copy_selected_cells_as_html.as_ref(),
                present,
                CopyFormat::Html,
                false,
            ),
            copy_as_item(
                "copy-selected-cells-as-markdown",
                args.labels.copy_selected_cells_as_markdown.as_ref(),
                present,
                CopyFormat::Markdown,
                false,
            ),
        ]),
    );
    let copy_label =
        if args.selection_ignored_ranges > 0 && args.labels.copy_selected_cells.is_some() {
            format!(
                "{} ({}…)",
                args.labels.copy_selected_cells.clone().unwrap_or_default(),
                args.selection_ignored_ranges
            )
        } else {
            args.labels.copy_selected_cells.clone().unwrap_or_default()
        };
    push(vec![
        enabled_item(
            "copy-selected-cells",
            copy_label,
            args.handlers.on_copy_selected_cells,
            args.can_copy_selection,
            MenuAction::CopySelectedCells,
        ),
        copy_as,
        enabled_item(
            "paste-selected-cells",
            args.labels.paste_selected_cells.clone().unwrap_or_default(),
            args.handlers.on_paste_selected_cells,
            args.can_paste_selection,
            MenuAction::PasteSelectedCells,
        ),
    ])
}

fn sort_block(args: &BuildDataTableContextMenuArgs) -> Vec<NativeMenuItemDef> {
    let Some(header_sort) = &args.header_sort else {
        return Vec::new();
    };
    let column_in_sorts = header_sort
        .sorts
        .iter()
        .any(|sort| sort.column == header_sort.column);
    let menu = submenu(
        "sort-multi",
        args.labels.sort_multi.clone().unwrap_or_default(),
        push(vec![
            item(
                "sort-ascending",
                args.labels.sort_ascending.clone().unwrap_or_default(),
                args.handlers.on_sort_ascending,
                MenuAction::SortAscending(header_sort.column.clone()),
            ),
            item(
                "sort-descending",
                args.labels.sort_descending.clone().unwrap_or_default(),
                args.handlers.on_sort_descending,
                MenuAction::SortDescending(header_sort.column.clone()),
            ),
            enabled_item(
                "remove-sort-column",
                args.labels.remove_sort_column.clone().unwrap_or_default(),
                args.handlers.on_remove_sort,
                column_in_sorts,
                MenuAction::RemoveSort(header_sort.column.clone()),
            ),
        ]),
    );
    let clear = enabled_item(
        "clear-sorts",
        args.labels.clear_sorts.clone().unwrap_or_default(),
        args.handlers.on_clear_sorts,
        !header_sort.sorts.is_empty(),
        MenuAction::ClearSorts,
    );
    push(vec![menu, clear])
}

fn freeze_block(args: &BuildDataTableContextMenuArgs) -> Vec<NativeMenuItemDef> {
    let Some(header_freeze) = &args.header_freeze else {
        return Vec::new();
    };
    push(vec![
        enabled_item(
            "freeze-through",
            args.labels.freeze_through.clone().unwrap_or_default(),
            args.handlers.on_freeze_through,
            header_freeze.index + 1 != header_freeze.frozen,
            MenuAction::FreezeThrough(header_freeze.index),
        ),
        enabled_item(
            "unfreeze-columns",
            args.labels.unfreeze_columns.clone().unwrap_or_default(),
            args.handlers.on_unfreeze_columns,
            header_freeze.frozen > 0,
            MenuAction::UnfreezeColumns,
        ),
    ])
}

fn append_export(
    mut out: Vec<NativeMenuItemDef>,
    args: &BuildDataTableContextMenuArgs,
) -> Vec<NativeMenuItemDef> {
    if args.export_enabled {
        if let Some(export) = item(
            "export",
            args.labels.export.clone(),
            args.handlers.on_export,
            MenuAction::Export,
        ) {
            if out.is_empty() {
                return vec![export];
            }
            out.push(NativeMenuItemDef::Separator);
            out.push(export);
        }
    }
    out
}

fn append_more(
    mut out: Vec<NativeMenuItemDef>,
    label: String,
    more_actions: Vec<NativeMenuItemDef>,
) -> Vec<NativeMenuItemDef> {
    if let Some(more) = submenu("more-actions", label, more_actions) {
        if out.is_empty() {
            return vec![more];
        }
        out.push(NativeMenuItemDef::Separator);
        out.push(more);
    }
    out
}

fn header_menu(args: &BuildDataTableContextMenuArgs) -> Vec<NativeMenuItemDef> {
    let sorts = [sort_block(args), freeze_block(args)].concat();
    let has_sorts = !sorts.is_empty();
    let mut defs: Vec<Option<NativeMenuItemDef>> = sorts.into_iter().map(Some).collect();
    defs.push(if has_sorts {
        Some(NativeMenuItemDef::Separator)
    } else {
        None
    });
    defs.push(item(
        "column-stats",
        args.labels.column_stats.clone().unwrap_or_default(),
        args.header_stats.is_some() && args.handlers.on_column_stats,
        MenuAction::ColumnStats(
            args.header_stats
                .as_ref()
                .map(|stats| stats.column.clone())
                .unwrap_or_default(),
        ),
    ));
    defs.push(item(
        "column-presentation",
        args.labels.column_presentation.clone().unwrap_or_default(),
        args.header_presentation.is_some() && args.handlers.on_column_presentation,
        MenuAction::ColumnPresentation(
            args.header_presentation
                .as_ref()
                .map(|header| header.column.clone())
                .unwrap_or_default(),
        ),
    ));
    defs.push(item(
        "copy-column-name",
        args.labels.copy_column_name.clone(),
        args.handlers.on_copy_column_name,
        MenuAction::CopyColumnName,
    ));
    defs.push(item(
        "copy-column-data",
        args.labels.copy_column_data.clone(),
        args.handlers.on_copy_column_data,
        MenuAction::CopyColumnData,
    ));
    append_export(push(defs), args)
}

fn cell_menu(args: &BuildDataTableContextMenuArgs) -> Vec<NativeMenuItemDef> {
    let set_value_menu = set_value_submenu(args);
    let uses_set_value_menu = set_value_menu.is_some();

    let frequent = push(vec![
        item(
            "copy",
            args.labels.copy.clone(),
            args.handlers.on_copy,
            MenuAction::Copy,
        ),
        item(
            "copy-row",
            args.labels.copy_row.clone(),
            args.handlers.on_copy_row,
            MenuAction::CopyRow,
        ),
    ])
    .into_iter()
    .chain(clipboard_block(args))
    .chain(push(vec![if args.can_filter_by_value {
        item(
            "filter-by-value",
            args.labels.filter_by_value.clone(),
            args.handlers.on_filter_by_value,
            MenuAction::FilterByValue,
        )
    } else {
        None
    }]))
    .collect::<Vec<_>>();

    let danger = if args.can_delete {
        push(vec![item(
            "delete-row",
            args.labels.delete_row.clone(),
            args.handlers.on_delete_row,
            MenuAction::DeleteRow,
        )])
    } else {
        Vec::new()
    };
    let restore = if args.can_restore_row {
        push(vec![item(
            "restore-row",
            args.labels.restore_row.clone(),
            args.handlers.on_restore_row,
            MenuAction::RestoreRow,
        )])
    } else {
        Vec::new()
    };
    let referenced_by = if args.handlers.on_show_referencing_rows {
        push(vec![item(
            "show-referencing-rows",
            args.labels.referenced_by.clone().unwrap_or_default(),
            true,
            MenuAction::ShowReferencingRows,
        )])
    } else {
        Vec::new()
    };

    let more_actions = push(vec![
        item(
            "copy-as-json",
            args.labels.copy_as_json.clone(),
            args.handlers.on_copy_as_json,
            MenuAction::CopyAsJson,
        ),
        item(
            "copy-as-sql-insert",
            args.labels.copy_as_sql_insert.clone(),
            args.handlers.on_copy_as_sql_insert,
            MenuAction::CopyAsSqlInsert,
        ),
        item(
            "copy-as-update",
            args.labels.copy_as_update.clone(),
            args.handlers.on_copy_as_update,
            MenuAction::CopyAsUpdate,
        ),
        item(
            "copy-as-csv",
            args.labels.copy_as_csv.clone(),
            args.handlers.on_copy_as_csv,
            MenuAction::CopyAsCsv,
        ),
        item(
            "copy-column-name",
            args.labels.copy_column_name.clone(),
            args.handlers.on_copy_column_name,
            MenuAction::CopyColumnName,
        ),
        item(
            "copy-column-data",
            args.labels.copy_column_data.clone(),
            args.handlers.on_copy_column_data,
            MenuAction::CopyColumnData,
        ),
        if args.can_set_null && !uses_set_value_menu {
            item(
                "set-null",
                args.labels.set_null.clone(),
                args.handlers.on_set_null,
                MenuAction::SetNull,
            )
        } else {
            None
        },
        if args.has_selected_rows {
            item(
                "copy-selected-rows",
                args.labels.copy_selected_rows.clone(),
                args.handlers.on_copy_selected_rows,
                MenuAction::CopySelectedRows,
            )
        } else {
            None
        },
    ]);

    let mut out = frequent;
    if let Some(menu) = set_value_menu {
        out.push(menu);
    }
    let danger_zone = [referenced_by, danger, restore].concat();
    if !danger_zone.is_empty() {
        if !out.is_empty() {
            out.push(NativeMenuItemDef::Separator);
        }
        out.extend(danger_zone);
    }
    let out = append_export(out, args);
    append_more(out, args.labels.more_actions.clone(), more_actions)
}

fn selection_menu(args: &BuildDataTableContextMenuArgs) -> Vec<NativeMenuItemDef> {
    let frequent = push(vec![
        item(
            "copy-selected-rows",
            args.labels.copy_selected_rows.clone(),
            args.handlers.on_copy_selected_rows,
            MenuAction::CopySelectedRows,
        ),
        item(
            "copy-rows-as-new-rows",
            args.labels
                .copy_rows_as_new_rows
                .clone()
                .unwrap_or_default(),
            args.handlers.on_copy_rows_as_new_rows,
            MenuAction::CopyRowsAsNewRows,
        ),
        enabled_item(
            "paste-rows",
            args.labels.paste_rows.clone().unwrap_or_default(),
            args.handlers.on_paste_rows,
            args.can_paste_rows,
            MenuAction::PasteRows,
        ),
        if args.can_filter_by_value {
            item(
                "filter-by-value",
                args.labels.filter_by_value.clone(),
                args.handlers.on_filter_by_value,
                MenuAction::FilterByValue,
            )
        } else {
            None
        },
    ]);
    let referenced_by = if args.handlers.on_show_referencing_rows {
        push(vec![item(
            "show-referencing-rows",
            args.labels.referenced_by.clone().unwrap_or_default(),
            true,
            MenuAction::ShowReferencingRows,
        )])
    } else {
        Vec::new()
    };
    let danger = if args.can_delete && args.has_selected_rows {
        push(vec![item(
            "delete-row",
            args.labels.delete_row.clone(),
            args.handlers.on_delete_row,
            MenuAction::DeleteRow,
        )])
    } else {
        Vec::new()
    };
    let more_actions = push(vec![
        item(
            "copy-as-csv",
            args.labels.copy_as_csv.clone(),
            args.handlers.on_copy_as_csv,
            MenuAction::CopyAsCsv,
        ),
        item(
            "copy-as-json",
            args.labels.copy_as_json.clone(),
            args.handlers.on_copy_as_json,
            MenuAction::CopyAsJson,
        ),
        item(
            "copy-as-sql-insert",
            args.labels.copy_as_sql_insert.clone(),
            args.handlers.on_copy_as_sql_insert,
            MenuAction::CopyAsSqlInsert,
        ),
        item(
            "copy-as-update",
            args.labels.copy_as_update.clone(),
            args.handlers.on_copy_as_update,
            MenuAction::CopyAsUpdate,
        ),
        item(
            "copy-column-name",
            args.labels.copy_column_name.clone(),
            args.handlers.on_copy_column_name,
            MenuAction::CopyColumnName,
        ),
        item(
            "copy-column-data",
            args.labels.copy_column_data.clone(),
            args.handlers.on_copy_column_data,
            MenuAction::CopyColumnData,
        ),
    ]);

    let mut out = frequent;
    if !referenced_by.is_empty() {
        if out.is_empty() {
            out = referenced_by;
        } else {
            out.extend(referenced_by);
        }
    }
    if !danger.is_empty() {
        if !out.is_empty() {
            out.push(NativeMenuItemDef::Separator);
        }
        out.extend(danger);
    }
    let out = append_export(out, args);
    append_more(out, args.labels.more_actions.clone(), more_actions)
}

pub fn build_data_table_context_menu_items(
    args: BuildDataTableContextMenuArgs,
) -> Vec<NativeMenuItemDef> {
    if args.has_header_context && !args.has_cell_context {
        header_menu(&args)
    } else if args.has_cell_context && !args.multi_row_selection {
        cell_menu(&args)
    } else {
        selection_menu(&args)
    }
}
