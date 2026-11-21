use std::collections::BTreeMap;

use crate::{
    build_grid_cell_view, ColumnCellFacts, EditorCellFacts, FkCellAffordance, FkTextKind,
    GridCellInput, GridCellView, StickyInfo,
};

use super::indexes::{dirty_cell_aria_label, dirty_cell_tooltip};
use super::types::{
    ColumnDefData, DirtyCellData, DirtyRowEntryData, DirtyRowState, FkNavigationData,
    VirtualBodyConfig, DEFAULT_COLUMN_WIDTH, GUTTER_WIDTH,
};

pub fn dirty_cells(row: Option<&DirtyRowEntryData>) -> BTreeMap<String, DirtyCellData> {
    let mut cells = BTreeMap::new();
    if let Some(row) = row {
        for cell in &row.cells {
            cells.insert(cell.column_name.clone(), cell.clone());
        }
    }
    cells
}

pub fn row_state(row: Option<&DirtyRowEntryData>) -> Option<DirtyRowState> {
    row.map(|entry| entry.state)
}

pub fn row_deleted(row: Option<&DirtyRowEntryData>) -> bool {
    row_state(row) == Some(DirtyRowState::Deleted)
}

pub fn row_dirty(row: Option<&DirtyRowEntryData>) -> bool {
    row_state(row) == Some(DirtyRowState::Dirty)
}

pub fn row_tint_class(
    index: usize,
    selected: bool,
    highlighted: bool,
    alternating_rows: bool,
) -> Vec<String> {
    let mut classes = Vec::new();
    if alternating_rows && index % 2 == 1 {
        classes.push("bg-surface-raised/50".to_owned());
    }
    if selected {
        classes.push("bg-accent/15".to_owned());
        classes.push("dark:bg-accent/20".to_owned());
    } else if highlighted {
        classes.push("bg-accent/8".to_owned());
        classes.push("dark:bg-accent/12".to_owned());
    }
    classes
}

pub fn frozen_lefts(
    columns: &[ColumnDefData],
    column_widths: &[i64],
    frozen_columns: usize,
    show_row_numbers: bool,
) -> Vec<i64> {
    let mut lefts = Vec::new();
    if frozen_columns == 0 {
        return lefts;
    }
    let mut left = if show_row_numbers { GUTTER_WIDTH } else { 0 };
    for index in 0..columns.len() {
        lefts.push(left);
        if index < frozen_columns {
            left += column_widths.get(index).copied().unwrap_or(DEFAULT_COLUMN_WIDTH);
        }
    }
    lefts
}

pub fn sticky_info(
    column_index: usize,
    frozen_columns: usize,
    lefts: &[i64],
    tint_classes: &[String],
) -> Option<StickyInfo> {
    if frozen_columns == 0 || frozen_columns <= column_index {
        return None;
    }
    Some(StickyInfo {
        left: lefts.get(column_index).copied().unwrap_or(0),
        bg_class: tint_classes.join(" "),
    })
}

pub fn fk_affordances(
    foreign_keys: &FkNavigationData,
    navigable: Option<&std::collections::BTreeSet<String>>,
    row_index: usize,
    can_write: bool,
) -> BTreeMap<String, FkCellAffordance> {
    let mut affordances = BTreeMap::new();
    if foreign_keys.index.is_empty() {
        return affordances;
    }
    for (column_name, group) in &foreign_keys.index {
        let key = format!("{row_index}:{column_name}");
        affordances.insert(
            column_name.clone(),
            FkCellAffordance {
                target_label: group.target_label.clone(),
                navigable: navigable.is_some_and(|set| set.contains(column_name)),
                resolving: foreign_keys.resolving.contains(&key),
                pickable: Some(group.local_columns.len() == 1 && can_write),
            },
        );
    }
    affordances
}

pub fn column_names(columns: &[ColumnDefData]) -> Vec<String> {
    columns.iter().map(|column| column.name.clone()).collect()
}

pub fn cell_id(prefix: Option<&str>, row_index: usize, column_index: usize) -> Option<String> {
    prefix.map(|prefix| format!("{prefix}-r{row_index}c{column_index}"))
}

pub fn error_key(row_index: usize, column_name: &str) -> String {
    format!("{row_index}:{column_name}")
}

pub struct CellRowContext<'a> {
    pub config: &'a VirtualBodyConfig,
    pub row_index: usize,
    pub dirty_cells: &'a BTreeMap<String, DirtyCellData>,
    pub fk_affordances: &'a BTreeMap<String, FkCellAffordance>,
    pub lefts: &'a [i64],
    pub tint_classes: &'a [String],
}

pub fn build_cell_input(
    context: &CellRowContext<'_>,
    column: &ColumnDefData,
    column_index: usize,
    row: &[Option<serde_json::Value>],
) -> GridCellInput {
    let config = context.config;
    let row_index = context.row_index;
    let info = config.cell_editing.as_ref().and_then(|editing| editing.column_editor_info.get(&column.name));
    let editor = config.cell_editing.as_ref().map(|editing| EditorCellFacts {
        panel_id: Some(editing.panel_id.clone()),
        has_value_handler: editing.has_cell_value_handler,
        has_load_candidates: editing.has_load_candidates,
        read_only_reason: editing.read_only_reason.clone(),
    });
    let dirty = context.dirty_cells.get(&column.name);
    let column_info = info.map(|info| ColumnCellFacts {
        nullable: info.nullable,
        integer_only: info.integer_only,
        read_only_reason: info.read_only_reason.clone(),
    });
    let active_cell = config.active_cell.as_ref();
    let active = config.cell_selection_enabled
        && active_cell.is_some_and(|coord| {
            coord.row_index == row_index && coord.column_name == column.name
        });
    let selected = config.cell_selection_enabled && config.is_cell_selected;

    GridCellInput {
        row_index,
        column_name: column.name.clone(),
        column_type: column.column_type.clone().unwrap_or_default(),
        value: row.get(column_index).cloned().unwrap_or(None),
        width: column_width(config, column_index) as u32,
        sticky: sticky_info(column_index, config.frozen_columns, context.lefts, context.tint_classes),
        selected,
        error: config.cell_errors.contains(&error_key(row_index, &column.name)),
        active,
        editing: config.editing_cell.as_ref().is_some_and(|cell| {
            cell.0 == row_index && cell.1 == column.name
        }),
        cell_type: info.map(|info| info.cell_type.clone()),
        column_info,
        editor,
        id: cell_id(config.cell_id_prefix.as_deref(), row_index, column_index),
        grid_semantics: config.cell_selection_enabled,
        dirty: dirty.is_some(),
        dirty_tooltip: dirty.map(|cell| {
            dirty_cell_tooltip(cell.original.as_ref(), cell.next.as_ref(), cell.expression.as_deref())
        }),
        dirty_aria_label: dirty.map(|cell| {
            dirty_cell_aria_label(cell.original.as_ref(), cell.next.as_ref(), cell.expression.as_deref())
        }),
        fk: context.fk_affordances.get(&column.name).cloned(),
        has_fk_activate: config.has_fk_activate,
        has_quick_look: config.on_cell_quick_look,
        presentation_plan: config.presentation_plan_by_column.get(&column.name).cloned(),
    }
}

fn column_width(config: &VirtualBodyConfig, index: usize) -> i64 {
    config.column_widths.get(index).copied().unwrap_or(DEFAULT_COLUMN_WIDTH)
}

pub fn build_row_cell_views(
    config: &VirtualBodyConfig,
    row_index: usize,
    row: &[Option<serde_json::Value>],
    dirty_row: Option<&DirtyRowEntryData>,
    navigable: Option<&std::collections::BTreeSet<String>>,
) -> Vec<GridCellView> {
    let cells = dirty_cells(dirty_row);
    let lefts = frozen_lefts(
        &config.columns,
        &config.column_widths,
        config.frozen_columns,
        config.show_row_numbers,
    );
    let tint = row_tint_class(
        row_index,
        config.selected_rows.contains(&row_index),
        config.highlighted_row == Some(row_index),
        config.alternating_rows,
    );
    let can_write = config.cell_editing.as_ref().is_some_and(|editing| editing.has_cell_value_handler)
        || config.on_cell_edit;
    let fks = match config.foreign_keys.as_ref() {
        Some(foreign_keys) => fk_affordances(foreign_keys, navigable, row_index, can_write),
        None => BTreeMap::new(),
    };
    let context = CellRowContext {
        config,
        row_index,
        dirty_cells: &cells,
        fk_affordances: &fks,
        lefts: &lefts,
        tint_classes: &tint,
    };
    let text = |kind: FkTextKind, label: &str| match kind {
        FkTextKind::Jump => format!("t:fk.jump:{label}"),
        FkTextKind::Resolving => "t:fk.resolving".to_owned(),
        FkTextKind::Selector => format!("t:fk.select:{label}"),
    };
    config
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            build_grid_cell_view(&build_cell_input(&context, column, index, row), &text)
        })
        .collect()
}
