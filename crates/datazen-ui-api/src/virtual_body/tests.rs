use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::*;

fn column(name: &str, data_type: Option<&str>) -> ColumnDefData {
    ColumnDefData {
        id: format!("id-{name}"),
        name: name.to_owned(),
        column_type: data_type.map(str::to_owned),
        nullable: None,
    }
}

#[test]
fn constants_and_dom_gap_are_pinned() {
    assert_eq!(OVERSCAN, 12);
    assert_eq!(DEFAULT_COLUMN_WIDTH, 160);
    assert_eq!(GUTTER_WIDTH, 40);
    assert_eq!(VIRTUAL_BODY_GAP.len(), 9);
    assert!(VIRTUAL_BODY_GAP.contains(&"useVirtualTable getVirtualItems / getTotalSize"));
}

#[test]
fn row_click_routes_inside_cell_to_detail_only() {
    let modifiers = modifiers_from_flags(true, true);
    assert_eq!(
        row_click_action(3, true, modifiers),
        RowClickAction::CellSelection { open_detail: 3 }
    );
    assert_eq!(
        row_click_action(3, false, modifiers),
        RowClickAction::SelectRow { index: 3, modifiers }
    );
    assert!(modifiers.multi && modifiers.range);
    assert_eq!(modifiers_from_flags(false, true), RowModifiers { multi: false, range: true });
}

#[test]
fn row_keyboard_activation_is_disabled_by_cell_selection() {
    assert_eq!(row_key_down_action(false, "Enter", 2), RowKeyAction::SelectRow { index: 2 });
    assert_eq!(row_key_down_action(false, " ", 2), RowKeyAction::SelectRow { index: 2 });
    assert_eq!(row_key_down_action(false, "a", 2), RowKeyAction::None);
    assert_eq!(row_key_down_action(true, "Enter", 2), RowKeyAction::None);
}

#[test]
fn row_double_click_targets_the_first_column_and_gutter_prefers_its_handler() {
    let names = vec!["id".to_owned(), "name".to_owned()];
    assert_eq!(row_double_click_action(&names), Some("id"));
    assert_eq!(row_double_click_action(&[]), None);

    let modifiers = RowModifiers { multi: true, range: false };
    assert_eq!(
        gutter_click_action(4, modifiers, true),
        GutterClickAction::Gutter { index: 4, modifiers }
    );
    assert_eq!(
        gutter_click_action(4, modifiers, false),
        GutterClickAction::Row { index: 4, modifiers }
    );
}

#[test]
fn display_rows_include_one_placeholder_per_draft() {
    assert_eq!(display_rows(2, 0), vec![DisplayRow::Real(0), DisplayRow::Real(1)]);
    assert_eq!(
        display_rows(2, 2),
        vec![
            DisplayRow::Real(0),
            DisplayRow::Real(1),
            DisplayRow::Draft(0),
            DisplayRow::Draft(1),
        ]
    );
    assert_eq!(row_kind(1, 2), DisplayRow::Real(1));
    assert_eq!(row_kind(2, 2), DisplayRow::Draft(0));
}

#[test]
fn virtual_table_windows_with_overscan_and_reports_total_height() {
    let result = virtual_table(100, 20, 200, 60);
    assert_eq!(result.total_height, 2000);
    assert_eq!(result.virtual_rows.first().map(|row| row.index), Some(0), "10 - 12 clamps to 0");
    assert_eq!(result.virtual_rows.last().map(|row| row.index), Some(24), "13 + 12 = 25 (exclusive)");
    assert_eq!(result.virtual_rows.first().map(|row| row.start), Some(0));
    assert!(result.virtual_rows.iter().any(|row| row.index == 10));
    assert_eq!(result.virtual_rows.first().map(|row| row.key.as_str()), Some("virtual-row-0"));
}

#[test]
fn virtual_table_handles_the_ends_and_the_empty_case() {
    let top = virtual_table(50, 10, 0, 30);
    assert_eq!(top.virtual_rows.first().map(|row| row.index), Some(0));
    assert_eq!(top.virtual_rows.last().map(|row| row.index), Some(14));
    let bottom = virtual_table(50, 10, 1000, 10);
    assert_eq!(bottom.virtual_rows.last().map(|row| row.index), Some(49));
    let empty = virtual_table(0, 10, 0, 100);
    assert!(empty.virtual_rows.is_empty());
    assert_eq!(empty.total_height, 0);
    assert_eq!(container_height(500, 20, true), 520);
    assert_eq!(container_height(500, 20, false), 500);
}

#[test]
fn editor_info_prefers_a_nonempty_provided_map() {
    let columns = vec![column("id", Some("int")), column("name", Some("varchar"))];
    let fallback = build_column_editor_info_from_defs(&columns, None);
    assert_eq!(fallback["id"].cell_type, "number");
    assert!(fallback["id"].integer_only);
    assert!(fallback["name"].nullable, "a missing nullable defaults to true");

    let provided = BTreeMap::from([(
        "id".to_owned(),
        ColumnEditorInfoData {
            cell_type: "custom".to_owned(),
            nullable: false,
            integer_only: false,
            read_only_reason: None,
        },
    )]);
    let chosen = editor_info(Some(&provided), &columns, None);
    assert_eq!(chosen, provided);
    let empty = BTreeMap::new();
    assert_eq!(editor_info(Some(&empty), &columns, None), fallback, "an empty map falls back");
    assert_eq!(editor_info(None, &columns, None), fallback);
}

#[test]
fn cell_type_resolution_honors_hints_and_integer_detection() {
    assert_eq!(resolve_cell_type(None, None), "unknown");
    assert_eq!(resolve_cell_type(Some(" BIGINT "), None), "number");
    assert_eq!(resolve_cell_type(Some("tinyint(1)"), None), "number");
    assert_eq!(resolve_cell_type(Some("jsonb"), None), "json");
    assert_eq!(resolve_cell_type(Some("timestamp"), None), "datetime");
    assert!(is_integer_cell_type(Some("bigint"), None));
    assert!(!is_integer_cell_type(Some("numeric"), None), "numeric is not an integer type");
    assert!(!is_integer_cell_type(Some("text"), None));

    let hints = CellTypeHintsData {
        type_overrides: BTreeMap::from([("mytype".to_owned(), "number".to_owned())]),
        enum_types: vec!["status".to_owned()],
    };
    assert_eq!(resolve_cell_type(Some("MyType"), Some(&hints)), "number");
    assert_eq!(resolve_cell_type(Some("status"), Some(&hints)), "enum");
    assert!(!is_integer_cell_type(Some("mytype"), Some(&hints)), "override is number, not integer");
}

fn group(local: &[&str]) -> FkGroupAffordanceData {
    FkGroupAffordanceData {
        local_columns: local.iter().map(|name| (*name).to_owned()).collect(),
        target_columns: local.iter().map(|name| (*name).to_owned()).collect(),
        target_label: "target".to_owned(),
        constraint_name: "fk".to_owned(),
        reference_table: "users".to_owned(),
    }
}

#[test]
fn foreign_key_navigation_requires_every_member_and_the_first_visible_column() {
    let index = BTreeMap::from([("customer_id".to_owned(), group(&["customer_id"]))]);
    let visible = vec!["name".to_owned(), "customer_id".to_owned()];
    let mut row = BTreeMap::new();
    row.insert("customer_id".to_owned(), Some(json!(5)));
    assert!(is_fk_cell_navigable("customer_id", &row, &index, &visible, false));
    assert!(!is_fk_cell_navigable("customer_id", &row, &index, &visible, true), "editing disables the arrow");
    assert!(!is_fk_cell_navigable("missing", &row, &index, &visible, false));

    row.insert("customer_id".to_owned(), Some(Value::Null));
    assert!(!is_fk_cell_navigable("customer_id", &row, &index, &visible, false));
    row.insert("customer_id".to_owned(), None);
    assert!(!is_fk_cell_navigable("customer_id", &row, &index, &visible, false));

    row.insert("customer_id".to_owned(), Some(json!(5)));
    let hidden_member = vec!["name".to_owned()];
    assert!(!is_fk_cell_navigable("customer_id", &row, &index, &hidden_member, false), "no visible member means no arrow");
}

#[test]
fn foreign_key_navigable_by_row_uses_full_records_and_hides_missing_members() {
    let index = BTreeMap::from([
        ("a".to_owned(), group(&["a", "b"])),
        ("b".to_owned(), group(&["a", "b"])),
    ]);
    let config = VirtualBodyConfig {
        columns: vec![column("a", None), column("b", None)],
        rows: vec![vec![Some(json!(1)), Some(json!(2))]],
        foreign_keys: Some(FkNavigationData {
            index,
            rows: vec![BTreeMap::from([
                ("a".to_owned(), json!(1)),
                ("b".to_owned(), json!(2)),
            ])],
            visible_columns: vec!["a".to_owned(), "b".to_owned()],
            resolving: BTreeSet::new(),
        }),
        ..Default::default()
    };
    let navigable = fk_navigable_by_row(&config);
    let row0 = navigable.get(&0).expect("one row");
    assert!(row0.contains("a"));
    assert!(!row0.contains("b"), "the arrow lands on the first visible member");

    let mut without_full = config.clone();
    if let Some(fk) = without_full.foreign_keys.as_mut() {
        fk.rows.clear();
    }
    let navigable = fk_navigable_by_row(&without_full);
    assert!(navigable.get(&0).is_some_and(|set| set.contains("a")), "positional rows are a fallback");
}

#[test]
fn error_columns_group_by_row_and_skip_malformed_keys() {
    let errors = BTreeSet::from([
        "0:name".to_owned(),
        "0:id".to_owned(),
        "2:email".to_owned(),
        "bad:name".to_owned(),
        "1:".to_owned(),
        ":name".to_owned(),
        "nocolon".to_owned(),
    ]);
    let grouped = error_columns_by_row(&errors);
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[&0], BTreeSet::from(["id".to_owned(), "name".to_owned()]));
    assert_eq!(grouped[&2], BTreeSet::from(["email".to_owned()]));
}

#[test]
fn dirty_text_helpers_cover_null_empty_json_and_expressions() {
    assert_eq!(format_dirty_value(None), "t:tableData.dirtyValueNull");
    assert_eq!(format_dirty_value(Some(&Value::Null)), "t:tableData.dirtyValueNull");
    assert_eq!(format_dirty_value(Some(&json!(""))), "t:tableData.dirtyValueEmpty");
    assert_eq!(format_dirty_value(Some(&json!({"a": 1}))), "{\"a\":1}");
    assert_eq!(format_dirty_value(Some(&json!(7))), "7");
    assert_eq!(format_dirty_expression(), "t:tableData.dirtyValueExpression");

    assert_eq!(
        dirty_cell_tooltip(Some(&json!("a")), Some(&json!("b")), None),
        "a \u{2192} b"
    );
    assert_eq!(
        dirty_cell_tooltip(Some(&json!("a")), Some(&json!("placeholder")), Some("currentTimestamp")),
        format!("a \u{2192} {}", format_dirty_expression())
    );
    let label = dirty_cell_aria_label(Some(&json!("a")), Some(&json!("b")), None);
    assert!(label.contains("t:tableData.dirtyCellAria"));
    assert!(label.contains("original=a"));
    assert!(label.contains("next=b"));
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::display_rows;
    let _ = crate::row_kind;
    let _ = crate::virtual_table;
    let _ = crate::container_height;
    let _ = crate::row_click_action;
    let _ = crate::row_key_down_action;
    let _ = crate::row_double_click_action;
    let _ = crate::gutter_click_action;
    let _ = crate::editor_info;
    let _ = crate::fk_navigable_by_row;
    let _ = crate::error_columns_by_row;
    let _ = crate::dirty_cell_tooltip;
    assert_eq!(crate::OVERSCAN, 12);
    let _: Option<crate::VirtualBodyConfig> = None;
    let _: Option<crate::DraftRowViewModel> = None;
    let _: Option<crate::VirtualRowData> = None;
    let _: Option<crate::CellEditingData> = None;
}
