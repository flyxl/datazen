use std::collections::BTreeSet;

use super::types::{
    reconcile_grid_selection, selection_from_anchor_focus, CellCoord, GridSelection,
    GridSelectionMode, SelectionPatch, TableStateForSelection,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionOpts {
    pub multi: bool,
    pub range: bool,
}

pub fn displayed_column_names(
    columns: &[String],
    visible_columns: Option<&[String]>,
    column_order: Option<&[String]>,
) -> Vec<String> {
    if visible_columns.is_none() && column_order.is_none() {
        return columns.to_vec();
    }
    let visible: BTreeSet<String> = visible_columns
        .map(|names| names.iter().cloned().collect())
        .unwrap_or_else(|| columns.iter().cloned().collect());
    let schema_order: Vec<String> = columns
        .iter()
        .filter(|name| visible.contains(*name))
        .cloned()
        .collect();
    let explicit_order = match column_order {
        None => return schema_order,
        Some(order) => order,
    };
    let mut explicit = BTreeSet::new();
    let mut ordered = Vec::new();
    for name in explicit_order {
        if visible.contains(name) && explicit.insert(name.clone()) {
            ordered.push(name.clone());
        }
    }
    for name in schema_order {
        if !explicit.contains(&name) {
            ordered.push(name);
        }
    }
    ordered
}

pub fn panel_column_order(state: &TableStateForSelection) -> Vec<String> {
    displayed_column_names(
        &state.columns,
        state.visible_columns.as_deref(),
        state.column_order.as_deref(),
    )
}

pub fn apply_row_select(
    state: &TableStateForSelection,
    index: usize,
    opts: Option<SelectionOpts>,
) -> SelectionPatch {
    let opts = opts.unwrap_or_default();
    if opts.range {
        if let Some(last) = state.last_selected_index {
            let lo = last.min(index);
            let hi = last.max(index);
            let mut selected = state.selected_rows.clone();
            for value in lo..=hi {
                selected.insert(value);
            }
            return SelectionPatch {
                selected_rows: Some(selected),
                last_selected_index: Some(Some(index)),
                ..Default::default()
            };
        }
    }
    if opts.multi {
        let mut selected = state.selected_rows.clone();
        if !selected.remove(&index) {
            selected.insert(index);
        }
        return SelectionPatch {
            selected_rows: Some(selected),
            last_selected_index: Some(Some(index)),
            ..Default::default()
        };
    }
    if !opts.range && state.selected_rows.len() == 1 && state.selected_rows.contains(&index) {
        return SelectionPatch {
            selected_rows: Some(BTreeSet::new()),
            last_selected_index: Some(None),
            ..Default::default()
        };
    }
    let mut selected = BTreeSet::new();
    selected.insert(index);
    SelectionPatch {
        selected_rows: Some(selected),
        last_selected_index: Some(Some(index)),
        ..Default::default()
    }
}

pub fn apply_toggle_select_all(state: &TableStateForSelection) -> SelectionPatch {
    let all_selected = state.selected_rows.len() == state.rows_count && state.rows_count > 0;
    let selected = if all_selected {
        BTreeSet::new()
    } else {
        (0..state.rows_count).collect()
    };
    SelectionPatch {
        selected_rows: Some(selected),
        last_selected_index: Some(None),
        ..Default::default()
    }
}

pub fn apply_begin_cell_selection(coord: &CellCoord) -> SelectionPatch {
    SelectionPatch {
        grid_selection: Some(selection_from_anchor_focus(
            coord,
            coord,
            GridSelectionMode::Cell,
        )),
        selected_rows: Some(BTreeSet::new()),
        last_selected_index: Some(None),
    }
}

pub fn apply_row_mode_transition(
    state: &TableStateForSelection,
    index: usize,
    opts: Option<SelectionOpts>,
) -> SelectionPatch {
    let mut patch = apply_row_select(state, index, opts);
    patch.grid_selection = Some(GridSelection::row_mode());
    patch
}

pub fn apply_grid_selection(
    state: &TableStateForSelection,
    selection: &GridSelection,
) -> SelectionPatch {
    match selection.mode {
        GridSelectionMode::None => apply_clear_grid_selection(),
        GridSelectionMode::Row => SelectionPatch {
            grid_selection: Some(GridSelection::row_mode()),
            selected_rows: Some(BTreeSet::new()),
            last_selected_index: Some(None),
        },
        GridSelectionMode::Cell => SelectionPatch {
            grid_selection: Some(selection.clone()),
            selected_rows: Some(BTreeSet::new()),
            last_selected_index: Some(None),
        },
    }
}

pub fn apply_clear_grid_selection() -> SelectionPatch {
    SelectionPatch {
        grid_selection: Some(GridSelection::empty()),
        selected_rows: Some(BTreeSet::new()),
        last_selected_index: Some(None),
    }
}

pub fn apply_reconcile_selection(
    state: &TableStateForSelection,
    column_order: &[String],
) -> SelectionPatch {
    SelectionPatch {
        grid_selection: Some(reconcile_grid_selection(
            &state.grid_selection,
            state.rows_count,
            column_order,
        )),
        ..Default::default()
    }
}

pub fn row_index_selection_reset() -> SelectionPatch {
    apply_clear_grid_selection()
}

#[derive(Clone, Debug, PartialEq)]
pub enum SelectionAction {
    SelectRow {
        panel_id: String,
        index: usize,
        opts: Option<SelectionOpts>,
    },
    ToggleSelectAll {
        panel_id: String,
    },
    SetRowModeSelection {
        panel_id: String,
        row_index: Option<usize>,
        opts: Option<SelectionOpts>,
    },
    SetGridSelection {
        panel_id: String,
        selection: GridSelection,
    },
    ClearGridSelection {
        panel_id: String,
    },
    BeginCellSelection {
        panel_id: String,
        coord: CellCoord,
    },
    ReconcileGridSelectionForColumns {
        panel_id: String,
        column_order: Vec<String>,
    },
}

pub fn resolve_selection_patch(
    state: &TableStateForSelection,
    action: &SelectionAction,
) -> (String, SelectionPatch) {
    match action {
        SelectionAction::SelectRow {
            panel_id,
            index,
            opts,
        } => (
            panel_id.clone(),
            apply_row_mode_transition(state, *index, *opts),
        ),
        SelectionAction::ToggleSelectAll { panel_id } => {
            let mut patch = apply_toggle_select_all(state);
            patch.grid_selection = Some(GridSelection::row_mode());
            (panel_id.clone(), patch)
        }
        SelectionAction::SetRowModeSelection {
            panel_id,
            row_index,
            opts,
        } => {
            let patch = match row_index {
                None => apply_clear_grid_selection(),
                Some(index) => apply_row_mode_transition(state, *index, *opts),
            };
            (panel_id.clone(), patch)
        }
        SelectionAction::SetGridSelection {
            panel_id,
            selection,
        } => (panel_id.clone(), apply_grid_selection(state, selection)),
        SelectionAction::ClearGridSelection { panel_id } => {
            (panel_id.clone(), apply_clear_grid_selection())
        }
        SelectionAction::BeginCellSelection { panel_id, coord } => {
            (panel_id.clone(), apply_begin_cell_selection(coord))
        }
        SelectionAction::ReconcileGridSelectionForColumns {
            panel_id,
            column_order,
        } => (
            panel_id.clone(),
            apply_reconcile_selection(state, column_order),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> TableStateForSelection {
        TableStateForSelection {
            columns: ["a", "b", "c"].iter().map(|v| (*v).to_owned()).collect(),
            rows_count: 5,
            ..Default::default()
        }
    }

    #[test]
    fn displayed_columns_honor_visibility_and_explicit_order() {
        let columns = ["a", "b", "c"]
            .iter()
            .map(|v| (*v).to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            displayed_column_names(&columns, None, None),
            ["a", "b", "c"]
        );
        let visible = ["a", "c"]
            .iter()
            .map(|v| (*v).to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            displayed_column_names(&columns, Some(&visible), None),
            ["a", "c"]
        );
        let order = ["c", "a", "?"]
            .iter()
            .map(|v| (*v).to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            displayed_column_names(&columns, None, Some(&order)),
            ["c", "a", "b"]
        );
        let duplicate = ["a", "a"]
            .iter()
            .map(|v| (*v).to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            displayed_column_names(&columns, None, Some(&duplicate)),
            ["a", "b", "c"]
        );
        assert_eq!(panel_column_order(&state()), ["a", "b", "c"]);
    }

    #[test]
    fn row_select_covers_range_multi_toggle_and_single_clear() {
        let mut item = state();
        item.last_selected_index = Some(1);
        let patch = apply_row_select(
            &item,
            3,
            Some(SelectionOpts {
                range: true,
                multi: false,
            }),
        );
        let rows = patch.selected_rows.unwrap();
        assert_eq!(rows.iter().copied().collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(patch.last_selected_index, Some(Some(3)));

        let mut item = state();
        item.selected_rows.insert(2);
        let patch = apply_row_select(
            &item,
            2,
            Some(SelectionOpts {
                multi: true,
                range: false,
            }),
        );
        assert!(patch.selected_rows.unwrap().is_empty());

        let mut item = state();
        item.selected_rows.insert(4);
        let patch = apply_row_select(&item, 4, None);
        assert!(patch.selected_rows.unwrap().is_empty());
        assert_eq!(patch.last_selected_index, Some(None));

        let patch = apply_row_select(&state(), 0, None);
        assert_eq!(
            patch
                .selected_rows
                .unwrap()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [0]
        );
    }

    #[test]
    fn toggle_all_selects_every_row_or_clears() {
        let patch = apply_toggle_select_all(&state());
        assert_eq!(patch.selected_rows.unwrap().len(), 5);
        let mut item = state();
        item.selected_rows = (0..5).collect();
        let patch = apply_toggle_select_all(&item);
        assert!(patch.selected_rows.unwrap().is_empty());
        let patch = apply_toggle_select_all(&TableStateForSelection::default());
        assert!(patch.selected_rows.unwrap().is_empty());
    }

    #[test]
    fn row_mode_transition_and_grid_setter_normalize_geometry() {
        let patch = apply_row_mode_transition(&state(), 2, None);
        assert_eq!(patch.grid_selection.unwrap().mode, GridSelectionMode::Row);
        assert_eq!(
            patch
                .selected_rows
                .unwrap()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [2]
        );

        let illegal = GridSelection {
            anchor: Some(CellCoord::new(0, "a")),
            focus: Some(CellCoord::new(1, "b")),
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Row,
        };
        let patch = apply_grid_selection(&state(), &illegal);
        assert_eq!(patch.grid_selection.unwrap(), GridSelection::row_mode());

        let cell = GridSelection {
            anchor: Some(CellCoord::new(0, "a")),
            focus: Some(CellCoord::new(1, "b")),
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Cell,
        };
        let patch = apply_grid_selection(&state(), &cell);
        assert_eq!(patch.grid_selection.unwrap(), cell);
        assert!(patch.selected_rows.unwrap().is_empty());
    }

    #[test]
    fn begin_cell_and_clear_converge_all_three_fields() {
        let coord = CellCoord::new(1, "b");
        let patch = apply_begin_cell_selection(&coord);
        assert_eq!(patch.grid_selection.unwrap().anchor, Some(coord.clone()));
        assert!(patch.selected_rows.unwrap().is_empty());
        assert_eq!(patch.last_selected_index, Some(None));
        assert_eq!(apply_clear_grid_selection(), row_index_selection_reset());
    }

    #[test]
    fn reconcile_only_patches_geometry() {
        let mut item = state();
        item.selected_rows.insert(3);
        item.grid_selection = GridSelection {
            anchor: Some(CellCoord::new(9, "a")),
            focus: Some(CellCoord::new(0, "a")),
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Cell,
        };
        let patch = apply_reconcile_selection(&item, &["a".to_owned()]);
        assert_eq!(patch.grid_selection.unwrap(), GridSelection::empty());
        assert_eq!(patch.selected_rows, None);
        assert_eq!(patch.last_selected_index, None);
    }

    #[test]
    fn every_store_action_routes_to_one_panel_and_one_patch() {
        let item = state();
        let actions = [
            SelectionAction::SelectRow {
                panel_id: "p1".to_owned(),
                index: 1,
                opts: None,
            },
            SelectionAction::ToggleSelectAll {
                panel_id: "p2".to_owned(),
            },
            SelectionAction::SetRowModeSelection {
                panel_id: "p3".to_owned(),
                row_index: None,
                opts: None,
            },
            SelectionAction::SetGridSelection {
                panel_id: "p4".to_owned(),
                selection: GridSelection::row_mode(),
            },
            SelectionAction::ClearGridSelection {
                panel_id: "p5".to_owned(),
            },
            SelectionAction::BeginCellSelection {
                panel_id: "p6".to_owned(),
                coord: CellCoord::new(0, "a"),
            },
            SelectionAction::ReconcileGridSelectionForColumns {
                panel_id: "p7".to_owned(),
                column_order: vec!["a".to_owned()],
            },
        ];
        assert_eq!(actions.len(), 7);
        for (index, action) in actions.iter().enumerate() {
            let (panel, _) = resolve_selection_patch(&item, action);
            assert_eq!(panel, format!("p{}", index + 1));
        }
        let (_, patch) = resolve_selection_patch(
            &item,
            &SelectionAction::SetRowModeSelection {
                panel_id: "p".to_owned(),
                row_index: Some(2),
                opts: None,
            },
        );
        assert_eq!(patch.grid_selection.unwrap().mode, GridSelectionMode::Row);
    }
}
