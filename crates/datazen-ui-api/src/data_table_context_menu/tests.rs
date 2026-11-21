use serde_json::{json, Value};

use crate::types::SortCondition;

use super::*;

fn label(name: &str) -> String {
    format!("L:{name}")
}

fn labels() -> DataTableContextMenuLabels {
    DataTableContextMenuLabels {
        copy: label("copy"),
        copy_row: label("copy-row"),
        more_actions: label("more-actions"),
        copy_as_json: label("copy-as-json"),
        copy_as_sql_insert: label("copy-as-sql-insert"),
        copy_as_update: label("copy-as-update"),
        copy_as_csv: label("copy-as-csv"),
        copy_column_name: label("copy-column-name"),
        copy_column_data: label("copy-column-data"),
        set_null: label("set-null"),
        filter_by_value: label("filter-by-value"),
        copy_selected_rows: label("copy-selected-rows"),
        delete_row: label("delete-row"),
        export: label("export"),
        restore_row: label("restore-row"),
        referenced_by: Some(label("referenced-by")),
        set_value: Some(label("set-value")),
        set_empty: Some(label("set-empty")),
        set_default: Some(label("set-default")),
        revert_cell: Some(label("revert-cell")),
        pick_date: Some(label("pick-date")),
        load_binary_from_file: Some(label("load-binary")),
        set_current_datetime: Some(label("set-current")),
        load_candidates: Some(label("load-candidates")),
        copy_selected_cells: Some(label("copy-selected-cells")),
        copy_selected_cells_as: Some(label("copy-selected-cells-as")),
        copy_selected_cells_as_tsv: Some(label("as-tsv")),
        copy_selected_cells_as_tsv_with_header: Some(label("as-tsv-header")),
        copy_selected_cells_as_csv: Some(label("as-csv")),
        copy_selected_cells_as_json: Some(label("as-json")),
        copy_selected_cells_as_plain: Some(label("as-plain")),
        copy_selected_cells_as_html: Some(label("as-html")),
        copy_selected_cells_as_markdown: Some(label("as-markdown")),
        paste_selected_cells: Some(label("paste-selected-cells")),
        paste_rows: Some(label("paste-rows")),
        copy_rows_as_new_rows: Some(label("copy-rows-as-new-rows")),
        sort_multi: Some(label("sort-multi")),
        sort_ascending: Some(label("sort-ascending")),
        sort_descending: Some(label("sort-descending")),
        remove_sort_column: Some(label("remove-sort")),
        clear_sorts: Some(label("clear-sorts")),
        freeze_through: Some(label("freeze-through")),
        unfreeze_columns: Some(label("unfreeze-columns")),
        column_stats: Some(label("column-stats")),
        column_presentation: Some(label("column-presentation")),
    }
}

fn args() -> BuildDataTableContextMenuArgs {
    BuildDataTableContextMenuArgs {
        labels: labels(),
        ..Default::default()
    }
}

fn ids(items: &[NativeMenuItemDef]) -> Vec<String> {
    items
        .iter()
        .map(|entry| match entry {
            NativeMenuItemDef::Item { id, .. } => id.clone(),
            NativeMenuItemDef::Submenu { id, .. } => id.clone(),
            NativeMenuItemDef::Separator => "separator".to_owned(),
        })
        .collect()
}

fn find<'a>(items: &'a [NativeMenuItemDef], id: &str) -> Option<&'a NativeMenuItemDef> {
    items.iter().find(|entry| match entry {
        NativeMenuItemDef::Item { id: actual, .. }
        | NativeMenuItemDef::Submenu { id: actual, .. } => actual == id,
        NativeMenuItemDef::Separator => false,
    })
}

fn submenu_items<'a>(items: &'a [NativeMenuItemDef], id: &str) -> &'a [NativeMenuItemDef] {
    match find(items, id) {
        Some(NativeMenuItemDef::Submenu { items, .. }) => items,
        _ => panic!("missing submenu {id}"),
    }
}

fn is_enabled(items: &[NativeMenuItemDef], id: &str) -> bool {
    match find(items, id) {
        Some(NativeMenuItemDef::Item { enabled, .. }) => *enabled,
        _ => panic!("missing item {id}"),
    }
}

#[test]
fn no_context_and_no_handlers_yields_an_empty_menu() {
    assert!(build_data_table_context_menu_items(args()).is_empty());
}

#[test]
fn selection_layout_builds_whole_selection_items_in_order() {
    let mut a = args();
    a.handlers.on_copy_selected_rows = true;
    a.handlers.on_copy_rows_as_new_rows = true;
    a.handlers.on_paste_rows = true;
    a.handlers.on_copy_as_csv = true;
    a.handlers.on_copy_as_json = true;
    let items = build_data_table_context_menu_items(a);
    assert_eq!(
        ids(&items),
        [
            "copy-selected-rows",
            "copy-rows-as-new-rows",
            "paste-rows",
            "separator",
            "more-actions"
        ]
    );
    assert!(is_enabled(&items, "paste-rows"));
    assert_eq!(
        ids(submenu_items(&items, "more-actions")),
        ["copy-as-csv", "copy-as-json"]
    );
}

#[test]
fn paste_rows_defaults_enabled_but_can_be_disabled_visible() {
    let mut a = args();
    a.handlers.on_paste_rows = true;
    a.can_paste_rows = false;
    let items = build_data_table_context_menu_items(a);
    assert_eq!(ids(&items), ["paste-rows"]);
    assert!(!is_enabled(&items, "paste-rows"));
}

#[test]
fn selection_referencing_rows_has_no_leading_separator_but_danger_does() {
    let mut a = args();
    a.handlers.on_copy_selected_rows = true;
    a.handlers.on_show_referencing_rows = true;
    a.has_selected_rows = true;
    a.can_delete = true;
    a.handlers.on_delete_row = true;
    let items = build_data_table_context_menu_items(a);
    assert_eq!(
        ids(&items),
        [
            "copy-selected-rows",
            "show-referencing-rows",
            "separator",
            "delete-row"
        ]
    );
    let mut no_delete = args();
    no_delete.handlers.on_copy_selected_rows = true;
    no_delete.handlers.on_delete_row = true;
    no_delete.can_delete = false;
    assert_eq!(
        ids(&build_data_table_context_menu_items(no_delete)),
        ["copy-selected-rows"]
    );
}

#[test]
fn cell_menu_exports_with_separator_and_more_actions_last() {
    let mut a = args();
    a.has_cell_context = true;
    a.handlers.on_copy = true;
    a.export_enabled = true;
    a.handlers.on_export = true;
    a.handlers.on_copy_as_json = true;
    let items = build_data_table_context_menu_items(a);
    assert_eq!(
        ids(&items),
        ["copy", "separator", "export", "separator", "more-actions"]
    );
    assert_eq!(ids(submenu_items(&items, "more-actions")), ["copy-as-json"]);
}

#[test]
fn cell_menu_set_null_and_set_value_are_mutually_exclusive() {
    let mut a = args();
    a.has_cell_context = true;
    a.can_set_null = true;
    a.handlers.on_set_null = true;
    a.handlers.on_copy = true;
    let menu = build_data_table_context_menu_items(a);
    assert_eq!(ids(&menu), ["copy", "separator", "more-actions"]);
    assert_eq!(ids(submenu_items(&menu, "more-actions")), ["set-null"]);

    let mut a = args();
    a.has_cell_context = true;
    a.can_set_null = true;
    a.handlers.on_set_null = true;
    a.handlers.on_copy = true;
    a.set_value = Some(SetValueState {
        can_null: true,
        can_empty: false,
        row_kind: RowKind::Update,
        has_pending: false,
        can_load_candidates: false,
        can_pick_date: false,
        can_load_binary: false,
        can_set_current_datetime: false,
    });
    a.handlers.on_set_value = true;
    let menu = build_data_table_context_menu_items(a);
    assert_eq!(ids(&menu), ["copy", "set-value"]);
    assert_eq!(
        ids(submenu_items(&menu, "set-value")),
        ["set-value-setNull"]
    );
    assert!(find(&menu, "more-actions").is_none());
}

#[test]
fn cell_menu_clipboard_block_and_ignored_ranges_suffix() {
    let mut a = args();
    a.has_cell_context = true;
    a.has_cell_range = true;
    a.can_copy_selection = true;
    a.can_paste_selection = false;
    a.selection_ignored_ranges = 3;
    a.handlers.on_copy_selected_cells = true;
    a.handlers.on_paste_selected_cells = true;
    a.handlers.on_copy_selected_cells_as = true;
    let menu = build_data_table_context_menu_items(a);
    assert_eq!(
        ids(&menu),
        [
            "copy-selected-cells",
            "copy-selected-cells-as",
            "paste-selected-cells"
        ]
    );
    assert!(!is_enabled(&menu, "paste-selected-cells"));
    assert_eq!(
        ids(submenu_items(&menu, "copy-selected-cells-as")),
        [
            "copy-selected-cells-as-tsv",
            "copy-selected-cells-as-tsv-header",
            "copy-selected-cells-as-csv",
            "copy-selected-cells-as-json",
            "copy-selected-cells-as-plain",
            "copy-selected-cells-as-html",
            "copy-selected-cells-as-markdown"
        ]
    );
    match find(&menu, "copy-selected-cells") {
        Some(NativeMenuItemDef::Item {
            label: actual_label,
            ..
        }) => {
            assert_eq!(
                actual_label,
                &format!("{} (3…)", label("copy-selected-cells"))
            )
        }
        _ => panic!("missing copy-selected-cells"),
    }
}

#[test]
fn header_sort_and_freeze_disabled_states() {
    let mut a = args();
    a.has_header_context = true;
    a.header_sort = Some(HeaderSort {
        column: "b".to_owned(),
        sorts: vec![SortCondition {
            column: "a".to_owned(),
            descending: false,
        }],
    });
    a.handlers.on_sort_ascending = true;
    a.handlers.on_sort_descending = true;
    a.handlers.on_remove_sort = true;
    a.handlers.on_clear_sorts = true;
    let menu = build_data_table_context_menu_items(a);
    assert_eq!(ids(&menu), ["sort-multi", "clear-sorts", "separator"]);
    let sort = submenu_items(&menu, "sort-multi");
    assert_eq!(
        ids(sort),
        ["sort-ascending", "sort-descending", "remove-sort-column"]
    );
    assert!(!is_enabled(sort, "remove-sort-column"));
    assert!(is_enabled(&menu, "clear-sorts"));

    let mut a = args();
    a.has_header_context = true;
    a.header_freeze = Some(HeaderFreeze {
        index: 2,
        frozen: 3,
    });
    a.handlers.on_freeze_through = true;
    a.handlers.on_unfreeze_columns = true;
    let menu = build_data_table_context_menu_items(a);
    assert_eq!(
        ids(&menu),
        ["freeze-through", "unfreeze-columns", "separator"]
    );
    assert!(!is_enabled(&menu, "freeze-through"));
    assert!(is_enabled(&menu, "unfreeze-columns"));
}

#[test]
fn header_column_stats_and_presentation_need_context_and_handler() {
    let mut a = args();
    a.has_header_context = true;
    a.header_stats = Some(HeaderStats {
        column: "c".to_owned(),
    });
    a.header_presentation = Some(HeaderPresentation {
        column: "c".to_owned(),
    });
    a.handlers.on_column_stats = true;
    a.handlers.on_column_presentation = true;
    let menu = build_data_table_context_menu_items(a);
    assert_eq!(ids(&menu), ["column-stats", "column-presentation"]);
}

#[test]
fn cell_context_wins_over_header_and_multi_row_falls_back_to_selection() {
    let mut a = args();
    a.has_header_context = true;
    a.has_cell_context = true;
    a.handlers.on_copy_column_name = true;
    a.handlers.on_copy = true;
    let menu = build_data_table_context_menu_items(a);
    assert_eq!(ids(&menu), ["copy", "separator", "more-actions"]);

    let mut a = args();
    a.has_cell_context = true;
    a.multi_row_selection = true;
    a.handlers.on_copy_selected_rows = true;
    a.handlers.on_copy = true;
    let menu = build_data_table_context_menu_items(a);
    assert_eq!(ids(&menu), ["copy-selected-rows"]);
}

#[test]
fn invoke_dispatches_actions_with_arguments_and_respects_disabled_state() {
    let mut a = args();
    a.has_header_context = true;
    a.header_sort = Some(HeaderSort {
        column: "amount".to_owned(),
        sorts: vec![SortCondition {
            column: "amount".to_owned(),
            descending: true,
        }],
    });
    a.header_freeze = Some(HeaderFreeze {
        index: 1,
        frozen: 0,
    });
    a.handlers.on_sort_ascending = true;
    a.handlers.on_freeze_through = true;
    let menu = build_data_table_context_menu_items(a);
    let sort = submenu_items(&menu, "sort-multi");
    assert_eq!(
        invoke(
            &sort[0],
            &DataTableContextMenuHandlers {
                on_sort_ascending: true,
                ..Default::default()
            }
        ),
        Some(MenuAction::SortAscending("amount".to_owned()))
    );
    assert_eq!(
        invoke(
            find(&menu, "freeze-through").unwrap(),
            &DataTableContextMenuHandlers::default()
        ),
        None
    );
    assert_eq!(
        invoke(&menu[2], &DataTableContextMenuHandlers::default()),
        None
    );
    let mut disabled_args = args();
    disabled_args.handlers.on_paste_rows = true;
    disabled_args.can_paste_rows = false;
    let disabled_menu = build_data_table_context_menu_items(disabled_args);
    let disabled = find(&disabled_menu, "paste-rows").unwrap();
    assert!(!matches!(
        disabled,
        NativeMenuItemDef::Item { enabled: true, .. }
    ));
    assert_eq!(
        invoke(
            disabled,
            &DataTableContextMenuHandlers {
                on_paste_rows: true,
                ..Default::default()
            }
        ),
        None
    );
}

#[test]
fn copy_selected_cells_actions_carry_format_and_header() {
    let mut a = args();
    a.has_cell_context = true;
    a.has_cell_range = true;
    a.handlers.on_copy_selected_cells_as = true;
    let menu = build_data_table_context_menu_items(a);
    let formats = submenu_items(&menu, "copy-selected-cells-as");
    let handlers = DataTableContextMenuHandlers {
        on_copy_selected_cells_as: true,
        ..Default::default()
    };
    assert_eq!(
        invoke(&formats[1], &handlers),
        Some(MenuAction::CopySelectedCellsAs {
            format: CopyFormat::Tsv,
            include_header: true,
        })
    );
    assert_eq!(
        invoke(&formats[5], &handlers),
        Some(MenuAction::CopySelectedCellsAs {
            format: CopyFormat::Html,
            include_header: false,
        })
    );
}

#[test]
fn set_value_actions_follow_state_rules() {
    let state = SetValueState {
        can_null: true,
        can_empty: true,
        row_kind: RowKind::Insert,
        has_pending: true,
        can_load_candidates: true,
        can_pick_date: true,
        can_load_binary: true,
        can_set_current_datetime: true,
    };
    let actions = resolve_set_value_actions(&state);
    assert_eq!(
        actions
            .iter()
            .map(|action| action.id.as_str())
            .collect::<Vec<_>>(),
        [
            "setNull",
            "setEmpty",
            "setDefault",
            "pickDate",
            "loadBinaryFromFile",
            "setCurrentDatetime"
        ]
    );
    let update = SetValueState {
        row_kind: RowKind::Update,
        has_pending: true,
        ..state
    };
    let ids: Vec<&str> = resolve_set_value_actions(&update)
        .iter()
        .map(|action| action.id.as_str())
        .collect();
    assert!(ids.contains(&"revertCell"));
    assert!(!ids.contains(&"setDefault"));
    assert_eq!(LOAD_CANDIDATES_LABEL_KEY, "cellEditor.loadCandidates");
}

#[test]
fn serializers_quote_and_shape_values() {
    let rows = vec![
        vec![Value::String("a\tb".to_owned()), Value::Null],
        vec![Value::String("x\"y".to_owned()), Value::Number(2.into())],
    ];
    assert_eq!(
        serialize_data_table_rows_as_tsv(&rows),
        "\"a\tb\"\t\n\"x\"\"y\"\t2"
    );
    let names = vec!["c1".to_owned(), "c2".to_owned()];
    assert_eq!(
        serialize_data_table_rows_as_csv(&names, &rows),
        "c1,c2\na\tb,\n\"x\"\"y\",2"
    );
    assert_eq!(
        serialize_data_table_rows_as_json(&names, &rows),
        serde_json::to_string_pretty(&json!([
            {"c1": "a\tb", "c2": null},
            {"c1": "x\"y", "c2": 2}
        ]))
        .unwrap()
    );
    assert_eq!(
        serialize_data_table_column_values(0, &rows),
        "\"a\tb\"\n\"x\"\"y\""
    );
    assert_eq!(serialize_data_table_column_values(-1, &rows), "");
}

#[test]
fn explicit_formats_escape_html_markdown_and_plain() {
    let rows = vec![vec![Value::String("a&b|c\nd".to_owned())]];
    let html = serialize_cells(&rows, SerializeFormat::Html, &["h".to_owned()], false);
    assert!(html.contains("<th>h</th>"));
    assert!(html.contains("<td>a&amp;b|c\nd</td>"));
    let markdown = serialize_cells(&rows, SerializeFormat::Markdown, &["h".to_owned()], false);
    assert!(markdown.contains("a&b\\|c<br>d"));
    let plain = serialize_cells(&rows, SerializeFormat::Plain, &[], false);
    assert_eq!(plain, "a&b|c\nd");
}

#[test]
fn sql_formatters_quote_and_use_primary_keys() {
    use crate::database_types::DatabaseRegistry;

    let registry = DatabaseRegistry::default();
    let names = vec!["id".to_owned(), "name".to_owned()];
    let row = vec![
        Value::Number(1.into()),
        Value::String("O'Reilly".to_owned()),
    ];
    assert_eq!(
        format_row_as_sql_insert("t", &names, &row, None, &registry),
        "INSERT INTO \"t\" (\"id\", \"name\") VALUES (1, 'O''Reilly');"
    );
    assert_eq!(
        format_row_as_sql_update("t", &names, &row, Some(&["id".to_owned()]), None, &registry),
        "UPDATE \"t\" SET \"name\" = 'O''Reilly' WHERE \"id\" = 1;"
    );
    let null_row = vec![Value::Null, Value::String("x".to_owned())];
    assert_eq!(
        format_row_as_sql_delete("t", &names, &null_row, None, None, &registry),
        "DELETE FROM \"t\" WHERE \"id\" IS NULL;"
    );
    assert_eq!(
        format_rows_as_sql_insert("t", &names, &[row.clone(), row.clone()], None, &registry),
        "INSERT INTO \"t\" (\"id\", \"name\") VALUES (1, 'O''Reilly');\nINSERT INTO \"t\" (\"id\", \"name\") VALUES (1, 'O''Reilly');"
    );
}

#[test]
fn event_resolvers_walk_ancestors_and_validate_row_index() {
    use std::collections::HashMap;

    let cell = DomElement::leaf(vec![("data-dt-row", "2"), ("data-dt-col", "amount")]);
    assert_eq!(
        resolve_data_table_cell_from_event(Some(&cell)),
        Some(CellContext {
            row_index: 2,
            column_name: Some("amount".to_owned())
        })
    );
    let inner = DomElement::child_of(vec![("class", "text")], cell);
    assert_eq!(
        resolve_data_table_cell_from_event(Some(&inner))
            .unwrap()
            .row_index,
        2
    );
    assert_eq!(resolve_data_table_cell_from_event(None), None);
    let negative = DomElement::leaf(vec![("data-dt-row", "-1"), ("data-dt-col", "c")]);
    assert_eq!(resolve_data_table_cell_from_event(Some(&negative)), None);
    let no_col = DomElement::leaf(vec![("data-dt-row", "1")]);
    assert_eq!(resolve_data_table_cell_from_event(Some(&no_col)), None);

    let header = DomElement::leaf(vec![("data-col-header", "name")]);
    let span = DomElement::child_of(vec![], header);
    assert_eq!(
        resolve_data_table_header_col_from_event(Some(&span)).as_deref(),
        Some("name")
    );
    let empty = DomElement {
        attributes: HashMap::new(),
        parent: None,
    };
    assert_eq!(resolve_data_table_header_col_from_event(Some(&empty)), None);
}
