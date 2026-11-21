use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use crate::{FkCellAffordance, StickyInfo};

use super::*;

fn column(name: &str, data_type: Option<&str>) -> ColumnDefData {
    ColumnDefData {
        id: format!("id-{name}"),
        name: name.to_owned(),
        column_type: data_type.map(str::to_owned),
        nullable: None,
    }
}

fn dirty_row(state: DirtyRowState) -> DirtyRowEntryData {
    DirtyRowEntryData {
        identity_key: "key".to_owned(),
        state,
        changed_columns: vec!["name".to_owned()],
        cells: vec![DirtyCellData {
            column_name: "name".to_owned(),
            original: Some(json!("old")),
            next: Some(json!("new")),
            expression: None,
        }],
    }
}

#[test]
fn dirty_cells_and_row_state_are_per_row() {
    let cells = dirty_cells(Some(&dirty_row(DirtyRowState::Dirty)));
    assert_eq!(cells.len(), 1);
    assert_eq!(cells["name"].original, Some(json!("old")));
    assert!(dirty_cells(None).is_empty());
    assert_eq!(row_state(Some(&dirty_row(DirtyRowState::Deleted))), Some(DirtyRowState::Deleted));
    assert_eq!(row_state(None), None);
    assert!(row_deleted(Some(&dirty_row(DirtyRowState::Deleted))));
    assert!(!row_deleted(Some(&dirty_row(DirtyRowState::Dirty))));
    assert!(row_dirty(Some(&dirty_row(DirtyRowState::Dirty))));
    assert!(!row_dirty(Some(&dirty_row(DirtyRowState::Inserted))));
}

#[test]
fn row_tint_keeps_selection_highlight_and_alternating_stripe_as_separate_channels() {
    assert_eq!(
        row_tint_class(1, false, false, true),
        vec!["bg-surface-raised/50".to_owned()]
    );
    assert!(row_tint_class(1, false, false, false).is_empty());
    assert_eq!(
        row_tint_class(0, true, true, false),
        vec!["bg-accent/15".to_owned(), "dark:bg-accent/20".to_owned()],
        "selection wins over highlight"
    );
    assert_eq!(
        row_tint_class(0, false, true, false),
        vec!["bg-accent/8".to_owned(), "dark:bg-accent/12".to_owned()]
    );
}

#[test]
fn frozen_offsets_start_after_the_optional_gutter() {
    let columns = vec![column("a", None), column("b", None), column("c", None)];
    let widths = vec![100_i64, 200, 300];
    assert!(frozen_lefts(&columns, &widths, 0, true).is_empty());
    assert_eq!(frozen_lefts(&columns, &widths, 2, true), vec![40, 140, 340]);
    assert_eq!(frozen_lefts(&columns, &widths, 2, false), vec![0, 100, 300]);
    assert_eq!(frozen_lefts(&columns, &[], 2, true), vec![40, 200, 360], "160px fallback width");

    let tint = vec!["bg-surface-raised/50".to_owned()];
    assert_eq!(sticky_info(0, 2, &[40, 140, 340], &tint), Some(StickyInfo {
        left: 40,
        bg_class: "bg-surface-raised/50".to_owned(),
    }));
    assert_eq!(sticky_info(2, 2, &[40, 140, 340], &tint), None, "unpinned cells render as before");
}

#[test]
fn fk_affordances_carry_resolving_navigable_and_pickable() {
    let group = FkGroupAffordanceData {
        local_columns: vec!["a".to_owned()],
        target_columns: vec!["id".to_owned()],
        target_label: "users.id".to_owned(),
        constraint_name: "fk_a".to_owned(),
        reference_table: "users".to_owned(),
    };
    let mut composite = group.clone();
    composite.local_columns.push("b".to_owned());
    let foreign_keys = FkNavigationData {
        index: BTreeMap::from([
            ("a".to_owned(), group),
            ("c".to_owned(), composite),
        ]),
        resolving: BTreeSet::from(["3:a".to_owned()]),
        ..Default::default()
    };
    let navigable = BTreeSet::from(["a".to_owned()]);
    let affordances = fk_affordances(&foreign_keys, Some(&navigable), 3, true);
    assert_eq!(affordances["a"].target_label, "users.id");
    assert!(affordances["a"].navigable);
    assert!(affordances["a"].resolving);
    assert_eq!(affordances["a"].pickable, Some(true), "single-column and writable");
    assert_eq!(affordances["c"].pickable, Some(false), "composite key is not pickable");
    assert!(!affordances["c"].navigable);
    assert!(!affordances["c"].resolving);

    assert!(fk_affordances(&FkNavigationData::default(), None, 0, true).is_empty());
}

#[test]
fn cell_inputs_derive_editing_selection_dirty_and_fk_flags() {
    let config = VirtualBodyConfig {
        columns: vec![column("id", Some("int")), column("name", Some("varchar"))],
        editing_cell: Some((2, "name".to_owned())),
        selected_rows: BTreeSet::from([2]),
        highlighted_row: Some(2),
        column_widths: vec![80, 120],
        is_cell_selected: true,
        cell_selection_enabled: true,
        cell_id_prefix: Some("cell".to_owned()),
        cell_errors: BTreeSet::from(["2:name".to_owned()]),
        frozen_columns: 1,
        ..Default::default()
    };
    let cells = dirty_cells(Some(&dirty_row(DirtyRowState::Dirty)));
    let fks = BTreeMap::from([(
        "name".to_owned(),
        FkCellAffordance {
            target_label: "t".to_owned(),
            navigable: true,
            resolving: false,
            pickable: Some(false),
        },
    )]);
    let lefts = frozen_lefts(&config.columns, &config.column_widths, 1, true);
    let tint = row_tint_class(2, true, true, true);
    let context = CellRowContext {
        config: &config,
        row_index: 2,
        dirty_cells: &cells,
        fk_affordances: &fks,
        lefts: &lefts,
        tint_classes: &tint,
    };
    let row: Vec<Option<serde_json::Value>> = vec![Some(json!(1)), Some(json!("old"))];
    let name_input = build_cell_input(&context, &config.columns[1], 1, &row);
    assert_eq!(name_input.column_name, "name");
    assert_eq!(name_input.value, Some(json!("old")));
    assert_eq!(name_input.width, 120);
    assert!(name_input.editing, "the edit target is this cell");
    assert!(name_input.selected && name_input.grid_semantics);
    assert!(name_input.error, "the cell error key matches");
    assert!(name_input.dirty);
    assert_eq!(name_input.id.as_deref(), Some("cell-r2c1"));
    assert!(name_input.sticky.is_none(), "only the first column is frozen");
    assert_eq!(name_input.fk.as_ref().map(|fk| fk.target_label.as_str()), Some("t"));
    assert_eq!(
        name_input.dirty_tooltip.as_deref(),
        Some("old \u{2192} new")
    );
    assert!(name_input.dirty_aria_label.as_deref().is_some_and(|label| label.contains("dirtyCellAria")));

    let id_input = build_cell_input(&context, &config.columns[0], 0, &row);
    assert!(!id_input.editing);
    assert!(!id_input.dirty);
    assert!(!id_input.error);
    assert_eq!(id_input.sticky.as_ref().map(|s| s.left), Some(40));
}

#[test]
fn row_cell_views_cover_every_column() {
    let config = VirtualBodyConfig {
        columns: vec![column("id", Some("int")), column("name", Some("varchar"))],
        rows: vec![vec![Some(json!(1)), Some(json!("a"))]],
        cell_selection_enabled: true,
        ..Default::default()
    };
    let views = build_row_cell_views(&config, 0, &config.rows[0], None, None);
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].attributes.get("data-dt-col").map(String::as_str), Some("id"));
    assert_eq!(views[1].attributes.get("data-dt-col").map(String::as_str), Some("name"));
    assert_eq!(views[0].role, Some("gridcell"));
    assert!(views[0].children.iter().any(|child| matches!(child, crate::CellChild::Renderer(_))));
}

#[test]
fn cell_id_and_error_key_use_the_documented_shapes() {
    assert_eq!(cell_id(None, 2, 3), None);
    assert_eq!(cell_id(Some("cell"), 2, 3).as_deref(), Some("cell-r2c3"));
    assert_eq!(error_key(7, "name"), "7:name");
}

#[test]
fn crate_root_rows_surface_is_reexported() {
    let _ = crate::build_cell_input;
    let _ = crate::build_row_cell_views;
    let _ = crate::frozen_lefts;
    let _ = crate::sticky_info;
    let _ = crate::fk_affordances;
    let _ = crate::row_tint_class;
    let _ = crate::dirty_cells;
    let _: Option<crate::CellRowContext<'static>> = None;
}
