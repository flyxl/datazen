use super::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn columns() -> Vec<String> {
    vec!["id".to_owned(), "name".to_owned(), "payload".to_owned()]
}

fn input<'a>(
    column_names: &'a [String],
    rows: &'a [Vec<Value>],
    selected: &'a BTreeSet<usize>,
    capabilities: MenuCapabilities,
) -> MenuInput<'a> {
    MenuInput {
        column_names,
        rows,
        selected_rows: selected,
        loading: false,
        export_table_name: None,
        export_enabled: true,
        database_type: None,
        primary_key_columns: &[],
        enable_set_null: None,
        context_cell_text: None,
        dirty_by_row: None,
        has_cell_range: false,
        can_copy_selection: false,
        can_paste_selection: false,
        selection_ignored_ranges: 0,
        can_paste_rows: true,
        sorts: None,
        frozen_columns: None,
        set_value_state: None,
        capabilities,
    }
}

fn cell_event(row: usize, column: &str) -> MenuEvent {
    MenuEvent {
        target: HitTarget::Cell {
            row_index: row,
            column_name: column.to_owned(),
        },
        client_x: 11,
        client_y: 22,
    }
}

fn header_event(column: &str) -> MenuEvent {
    MenuEvent {
        target: HitTarget::Header {
            column_name: column.to_owned(),
        },
        client_x: 11,
        client_y: 22,
    }
}

fn t(key: &str) -> String {
    format!("T:{key}")
}

#[test]
fn literals_and_gap_are_pinned() {
    assert_eq!(CONTEXT_MENU_DOM_GAP.len(), 4);
    assert_eq!(ClipboardFormat::Tsv.as_str(), "tsv");
    assert_eq!(ClipboardFormat::Html.as_str(), "html");
    for (id, literal) in [
        (SetValueActionId::SetNull, "setNull"),
        (SetValueActionId::RevertCell, "revertCell"),
        (SetValueActionId::LoadBinaryFromFile, "loadBinaryFromFile"),
    ] {
        assert_eq!(id.as_str(), literal);
    }
}

#[test]
fn cell_text_comes_from_row_data_not_the_dom() {
    let cols = columns();
    let rows = vec![
        vec![json!(1), json!("alpha"), json!({"a": 1})],
        vec![json!(2), Value::Null, json!([1, 2])],
    ];
    let selected = BTreeSet::new();
    let mut opts = input(&cols, &rows, &selected, MenuCapabilities::default());
    let ctx = resolve_menu_context(&opts, &cell_event(0, "name"));
    assert_eq!(ctx.cell_text_for_copy, "alpha");
    assert_eq!(ctx.hit_cell_value, Some(json!("alpha")));

    let ctx = resolve_menu_context(&opts, &cell_event(1, "name"));
    assert_eq!(ctx.cell_text_for_copy, "");

    let ctx = resolve_menu_context(&opts, &cell_event(0, "payload"));
    assert_eq!(ctx.cell_text_for_copy, "{\"a\":1}");

    let ctx = resolve_menu_context(&opts, &cell_event(1, "payload"));
    assert_eq!(ctx.cell_text_for_copy, "[1,2]");

    opts.context_cell_text = Some("dom-text");
    let ctx = resolve_menu_context(&opts, &MenuEvent {
        target: HitTarget::Plain,
        client_x: 0,
        client_y: 0,
    });
    assert_eq!(ctx.cell_text_for_copy, "dom-text");
    let ctx = resolve_menu_context(&opts, &cell_event(0, "id"));
    assert_eq!(ctx.cell_text_for_copy, "1");
}

#[test]
fn header_entries_are_bound_only_on_a_header_hit() {
    let cols = columns();
    let rows: Vec<Vec<Value>> = Vec::new();
    let selected = BTreeSet::new();
    let sorts = vec![SortCondition {
        column: "id".to_owned(),
        descending: true,
    }];
    let caps = MenuCapabilities {
        add_sort: true,
        remove_sort: true,
        clear_sorts: true,
        freeze_through: true,
        unfreeze_columns: true,
        column_stats: true,
        column_presentation: true,
        ..Default::default()
    };
    let mut opts = input(&cols, &rows, &selected, caps);
    opts.sorts = Some(&sorts);
    opts.frozen_columns = Some(1);

    let header = resolve_menu_context(&opts, &header_event("name"));
    assert_eq!(header.hit_header_col.as_deref(), Some("name"));
    assert_eq!(header.header_sort.as_ref().map(|h| h.column.as_str()), Some("name"));
    assert_eq!(
        header.header_freeze,
        Some(HeaderFreeze {
            index: 1,
            frozen: 1
        })
    );
    assert!(header.flags.header_stats && header.flags.header_presentation);

    let cell = resolve_menu_context(&opts, &cell_event(0, "name"));
    assert_eq!(cell.hit_header_col, None);
    assert_eq!(cell.header_sort, None);
    assert_eq!(cell.header_freeze, None);
    assert!(!cell.flags.header_stats && !cell.flags.header_presentation);

    let bindings = build_menu_bindings(&opts, &header, &header_event("name"));
    let ids: Vec<MenuEntryId> = bindings.entries.iter().map(|(id, _)| *id).collect();
    assert!(ids.contains(&MenuEntryId::SortAscending));
    assert!(ids.contains(&MenuEntryId::SortDescending));
    assert!(ids.contains(&MenuEntryId::RemoveSort));
    assert!(ids.contains(&MenuEntryId::FreezeThrough));
    assert!(ids.contains(&MenuEntryId::ColumnPresentation));
    assert_eq!(bindings.position, (11, 22));
}

#[test]
fn a_multi_row_selection_shifts_actions_to_the_selection() {
    let cols = columns();
    let rows = vec![
        vec![json!(1), json!("a"), json!(null)],
        vec![json!(2), json!("b"), json!(null)],
        vec![json!(3), json!("c"), json!(null)],
    ];
    let selected: BTreeSet<usize> = [2, 0].into_iter().collect();
    let caps = MenuCapabilities {
        cell_edit: true,
        delete_rows: true,
        ..Default::default()
    };
    let mut opts = input(&cols, &rows, &selected, caps);
    let primary_keys = ["id".to_owned()];
    opts.primary_key_columns = &primary_keys;
    let ctx = resolve_menu_context(&opts, &cell_event(0, "name"));
    assert_eq!(ctx.selected_indices, vec![0, 2]);
    assert!(ctx.flags.multi_row_selection);
    assert_eq!(ctx.action_row, None);
    assert_eq!(ctx.selection_row_actions.len(), 2);
    assert_eq!(ctx.delete_indices, vec![0, 2]);
    assert!(ctx.flags.can_set_null);
    assert!(ctx.flags.can_delete);

    let bindings = build_menu_bindings(&opts, &ctx, &cell_event(0, "name"));
    assert!(!bindings.entries.iter().any(|(id, _)| *id == MenuEntryId::Copy));
    assert!(!bindings.entries.iter().any(|(id, _)| *id == MenuEntryId::SetNull));
    assert!(bindings.entries.iter().any(|(id, _)| *id == MenuEntryId::CopySelectedRows));
    assert!(bindings.entries.iter().any(|(id, action)| matches!(action, MenuAction::DeleteRows(v) if *v == vec![0, 2])));
}

#[test]
fn can_filter_set_null_delete_and_restore_follow_their_guards() {
    let cols = columns();
    let rows = vec![vec![json!(1), json!("a"), json!(null)]];
    let selected = BTreeSet::new();
    let caps = MenuCapabilities {
        add_filter: true,
        cell_edit: true,
        delete_rows: true,
        restore_row: true,
        ..Default::default()
    };
    let mut opts = input(&cols, &rows, &selected, caps);
    let primary_keys = ["id".to_owned()];
    opts.primary_key_columns = &primary_keys;
    let mut dirty = BTreeMap::new();
    dirty.insert(0, RowState::Deleted);
    opts.dirty_by_row = Some(&dirty);

    let ctx = resolve_menu_context(&opts, &cell_event(0, "name"));
    assert!(ctx.flags.can_filter_by_value);
    assert!(ctx.flags.can_set_null);
    assert!(ctx.flags.can_delete);
    assert!(ctx.flags.can_restore_row);
    let filter = build_menu_bindings(&opts, &ctx, &cell_event(0, "name"))
        .entries
        .into_iter()
        .find(|(id, _)| *id == MenuEntryId::FilterByValue)
        .map(|(_, action)| action);
    assert_eq!(
        filter,
        Some(MenuAction::FilterByValue {
            column: "name".to_owned(),
            value: "a".to_owned()
        })
    );

    opts.loading = true;
    opts.enable_set_null = Some(false);
    opts.primary_key_columns = &[];
    opts.dirty_by_row = None;
    let ctx = resolve_menu_context(&opts, &cell_event(0, "name"));
    assert!(!ctx.flags.can_filter_by_value);
    assert!(!ctx.flags.can_set_null);
    assert!(!ctx.flags.can_delete);
    assert!(!ctx.flags.can_restore_row);
}

#[test]
fn labels_pin_i18n_keys_and_count_variants() {
    let cols = columns();
    let rows: Vec<Vec<Value>> = vec![Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    let selected: BTreeSet<usize> = [1, 2, 3].into_iter().collect();
    let mut opts = input(&cols, &rows, &selected, MenuCapabilities::default());
    let ctx = resolve_menu_context(&opts, &header_event("id"));
    let labels = build_menu_labels(&opts, &ctx, &t);
    assert_eq!(labels.copy, "T:common.copy");
    assert_eq!(labels.copy_as_json, "T:dataTable.copyAsJson");
    assert_eq!(labels.copy_selected_rows, "T:common.copy T:export.selectedRows (3)");
    assert_eq!(labels.delete_row, "T:dataTable.deleteRow (3)");
    assert_eq!(labels.export, "T:export.export (T:export.selectedRows 3)");

    let empty = BTreeSet::new();
    opts.selected_rows = &empty;
    let ctx = resolve_menu_context(&opts, &header_event("id"));
    let labels = build_menu_labels(&opts, &ctx, &t);
    assert_eq!(labels.copy_selected_rows, "T:common.copy T:export.selectedRows");
    assert_eq!(labels.export, "T:export.export");
}

#[test]
fn copy_sources_carry_the_right_rows_and_arguments() {
    let cols = columns();
    let rows = vec![vec![json!(1), json!("a"), json!(null)]];
    let selected: BTreeSet<usize> = [0].into_iter().collect();
    let mut opts = input(&cols, &rows, &selected, MenuCapabilities::default());
    opts.export_table_name = Some("orders");
    let primary_keys = ["id".to_owned()];
    opts.primary_key_columns = &primary_keys;
    let ctx = resolve_menu_context(&opts, &cell_event(0, "name"));
    let bindings = build_menu_bindings(&opts, &ctx, &cell_event(0, "name"));
    let action = |id| bindings.entries.iter().find(|(entry, _)| *entry == id).map(|(_, a)| a.clone());
    assert_eq!(
        action(MenuEntryId::CopyRow),
        Some(MenuAction::Copy(CopySource::RowsTsv(vec![rows[0].clone()])))
    );
    assert_eq!(
        action(MenuEntryId::CopyAsJson),
        Some(MenuAction::Copy(CopySource::RowsJson(cols.clone(), vec![rows[0].clone()])))
    );
    assert_eq!(
        action(MenuEntryId::CopyAsSqlInsert),
        Some(MenuAction::Copy(CopySource::SqlInsert {
            table: "orders".to_owned(),
            columns: cols.clone(),
            rows: vec![rows[0].clone()],
        }))
    );
    assert!(matches!(action(MenuEntryId::CopyAsUpdate), Some(MenuAction::Copy(CopySource::SqlUpdate { table, primary_keys, .. })) if table == "orders" && primary_keys == vec!["id".to_owned()]));
    assert_eq!(
        action(MenuEntryId::CopyAsCsv),
        Some(MenuAction::Copy(CopySource::RowsCsv(cols.clone(), vec![rows[0].clone()])))
    );
    assert_eq!(
        action(MenuEntryId::CopyColumnName),
        Some(MenuAction::Copy(CopySource::Text("name".to_owned())))
    );
    assert_eq!(
        action(MenuEntryId::CopyColumnData),
        Some(MenuAction::Copy(CopySource::ColumnValues(1)))
    );
}

#[test]
fn optional_entries_obey_their_capability_and_flag_guards() {
    let cols = columns();
    let rows = vec![vec![json!(1), json!("a"), json!(null)]];
    let selected = BTreeSet::new();
    let caps = MenuCapabilities {
        copy_selected_cells: true,
        copy_selected_cells_as: true,
        paste_selected_cells: true,
        paste_rows: true,
        copy_rows_as_new_rows: true,
        show_referencing_rows: true,
        load_candidates: true,
        ..Default::default()
    };
    let mut opts = input(&cols, &rows, &selected, caps);
    let ctx = resolve_menu_context(&opts, &cell_event(0, "name"));
    let ids: Vec<MenuEntryId> = build_menu_bindings(&opts, &ctx, &cell_event(0, "name"))
        .entries
        .iter()
        .map(|(id, _)| *id)
        .collect();
    assert!(!ids.contains(&MenuEntryId::CopySelectedCells));
    assert!(!ids.contains(&MenuEntryId::PasteSelectedCells));
    assert!(ids.contains(&MenuEntryId::PasteRows));
    assert!(ids.contains(&MenuEntryId::ShowReferencingRows));
    assert!(ids.contains(&MenuEntryId::LoadCandidates));

    opts.can_copy_selection = true;
    opts.can_paste_selection = true;
    let ctx = resolve_menu_context(&opts, &cell_event(0, "name"));
    let ids: Vec<MenuEntryId> = build_menu_bindings(&opts, &ctx, &cell_event(0, "name"))
        .entries
        .iter()
        .map(|(id, _)| *id)
        .collect();
    assert!(ids.contains(&MenuEntryId::CopySelectedCells));
    assert!(ids.contains(&MenuEntryId::CopySelectedCellsAs));
    assert!(ids.contains(&MenuEntryId::PasteSelectedCells));
}

#[test]
fn set_value_entries_follow_the_state() {
    let cols = columns();
    let rows = vec![vec![json!(1), json!("a"), json!(null)]];
    let selected = BTreeSet::new();
    let mut opts = input(&cols, &rows, &selected, MenuCapabilities::default());
    opts.set_value_state = Some(SetValueState {
        can_null: true,
        can_empty: true,
        has_pending: true,
        can_pick_date: true,
        ..Default::default()
    });
    let ctx = resolve_menu_context(&opts, &cell_event(0, "name"));
    assert!(ctx.flags.set_value_available);
    let actions: Vec<MenuAction> = build_menu_bindings(&opts, &ctx, &cell_event(0, "name"))
        .entries
        .into_iter()
        .filter(|(id, _)| *id == MenuEntryId::SetValue)
        .map(|(_, action)| action)
        .collect();
    assert_eq!(actions.len(), 4);
    assert!(actions.iter().any(|action| matches!(action, MenuAction::SetValue { action: SetValueActionId::SetNull, .. })));
    assert!(actions.iter().any(|action| matches!(action, MenuAction::SetValue { action: SetValueActionId::PickDate, .. })));
}

#[test]
fn crate_root_reexports_the_context_menu_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::MenuInput>();
    resolves::<crate::MenuContext>();
    resolves::<crate::MenuBindings>();
    resolves::<crate::MenuAction>();
    resolves::<crate::SetValueState>();
    assert_eq!(crate::CONTEXT_MENU_DOM_GAP.len(), 4);
    assert!(crate::ClipboardFormat::Json.as_str() == "json");
}
