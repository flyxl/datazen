//! T628: `useTableForeignKeyNavigation` logic — the React test cases as pure
//! decision tests: FK indexing/navigability, filter/key building, FK lookup,
//! viewport anchoring, jump/refusing, breadcrumb derivation and the back/forward
//! history machine (closed-tab pruning, depth cap, viewport snapshots).

use std::collections::{BTreeMap, BTreeSet};

use datazen_ui::views::table_fk_breadcrumb::FkOrigin;
use datazen_ui::views::table_foreign_key_navigation::{
    build_fk_filters, derive_breadcrumb, find_foreign_key, fk_nav_key, index_foreign_keys,
    is_fk_cell_navigable, pick_anchor_row, resolve_jump, resolve_referencing_rows,
    resolving_add, resolving_remove, scroll_top_for_anchor_row, target_label_of, BreadcrumbVm,
    FkNavOriginVm, FkNavigationMachine, FkNavTargetIdentityVm, FkValue, ForeignKeyInfoVm,
    GridViewportVm, JumpDecision, NavigableInput, FK_NAV_MAX_DEPTH,
};

fn fk(name: &str, columns: &[&str], table: &str, target: &[&str]) -> ForeignKeyInfoVm {
    ForeignKeyInfoVm {
        name: name.to_owned(),
        columns: columns.iter().map(|s| (*s).to_owned()).collect(),
        referenced_table: table.to_owned(),
        referenced_columns: target.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn composite() -> ForeignKeyInfoVm {
    fk("orders_parent_fk", &["parent_a", "parent_b"], "sales.parents", &["x", "y"])
}

// ── fkNavigation.ts helpers ──────────────────────────────────────────────────

#[test]
fn index_maps_composite_members_to_one_group_and_first_wins() {
    let index = index_foreign_keys(&[composite()]);
    let first = index.get("parent_a").expect("parent_a indexed");
    assert_eq!(index.get("parent_b"), Some(first), "same group object per member");
    assert_eq!(first.local_columns, ["parent_a", "parent_b"]);
    assert_eq!(first.target_columns, ["x", "y"]);
    assert_eq!(first.constraint_name, "orders_parent_fk");
    assert_eq!(first.reference_table, "sales.parents");
    assert_eq!(first.target_label, "sales.parents.(x, y)");

    let composite_and_single = index_foreign_keys(&[fk("single", &["customer_id"], "customers", &["id"]), composite()]);
    assert_eq!(composite_and_single.get("customer_id").unwrap().target_label, "customers.id");

    let dupes = index_foreign_keys(&[fk("first_fk", &["c"], "t", &["id"]), fk("second_fk", &["c"], "t", &["id"])]);
    assert_eq!(dupes.get("c").unwrap().constraint_name, "first_fk");
}

#[test]
fn navigability_draws_one_arrow_on_first_visible_member() {
    let index = index_foreign_keys(&[composite()]);
    let mut row = BTreeMap::new();
    row.insert("parent_a".to_owned(), FkValue::Num(1.0));
    row.insert("parent_b".to_owned(), FkValue::Str("k".to_owned()));
    let visible = ["parent_a".to_owned(), "parent_b".to_owned()];

    assert!(is_fk_cell_navigable(&NavigableInput {
        column_name: "parent_a", row: Some(&row), index: &index, visible_columns: &visible, editing: false,
    }));
    assert!(!is_fk_cell_navigable(&NavigableInput {
        column_name: "parent_b", row: Some(&row), index: &index, visible_columns: &visible, editing: false,
    }));

    // Arrow moves to the next visible member when the first is hidden.
    let only_b = ["parent_b".to_owned()];
    assert!(is_fk_cell_navigable(&NavigableInput {
        column_name: "parent_b", row: Some(&row), index: &index, visible_columns: &only_b, editing: false,
    }));
    assert!(!is_fk_cell_navigable(&NavigableInput {
        column_name: "parent_a", row: Some(&row), index: &index, visible_columns: &only_b, editing: false,
    }));
    assert!(!is_fk_cell_navigable(&NavigableInput {
        column_name: "parent_a", row: Some(&row), index: &index, visible_columns: &[], editing: false,
    }));
}

#[test]
fn navigability_refuses_null_member_non_member_row_and_editing() {
    let index = index_foreign_keys(&[composite()]);
    let visible = ["parent_a".to_owned(), "parent_b".to_owned()];
    let mut null_b = BTreeMap::new();
    null_b.insert("parent_a".to_owned(), FkValue::Num(1.0));
    null_b.insert("parent_b".to_owned(), FkValue::Null);
    assert!(!is_fk_cell_navigable(&NavigableInput {
        column_name: "parent_a", row: Some(&null_b), index: &index, visible_columns: &visible, editing: false,
    }));
    let full = BTreeMap::from([
        ("parent_a".to_owned(), FkValue::Num(1.0)),
        ("parent_b".to_owned(), FkValue::Str("k".to_owned())),
    ]);
    assert!(!is_fk_cell_navigable(&NavigableInput {
        column_name: "parent_a", row: None, index: &index, visible_columns: &visible, editing: false,
    }));
    assert!(!is_fk_cell_navigable(&NavigableInput {
        column_name: "note", row: Some(&full), index: &index, visible_columns: &["note".to_owned()], editing: false,
    }));
    assert!(!is_fk_cell_navigable(&NavigableInput {
        column_name: "parent_a", row: Some(&full), index: &index, visible_columns: &visible, editing: true,
    }));
}

#[test]
fn filters_pair_positionally_and_refuse_null_or_length_mismatch() {
    let cols = ["x".to_owned(), "y".to_owned()];
    let filters = build_fk_filters(&cols, &[FkValue::Num(1.0), FkValue::Str("k".to_owned())]).unwrap();
    assert_eq!(filters[0].column, "x");
    assert_eq!(filters[0].operator, "eq");
    assert_eq!(filters[1].value, FkValue::Str("k".to_owned()));
    assert!(build_fk_filters(&cols, &[FkValue::Num(1.0), FkValue::Null]).is_none());
    assert!(build_fk_filters(&cols, &[FkValue::Num(1.0)]).is_none());
    assert!(build_fk_filters(&["x".to_owned()], &[FkValue::Num(1.0), FkValue::Num(2.0)]).is_none());
    assert!(build_fk_filters(&[], &[]).is_none());
}

#[test]
fn nav_key_includes_schema_and_values_and_distinguishes_null_from_empty() {
    let target = FkNavTargetIdentityVm {
        database: "app".to_owned(),
        schema: Some("public".to_owned()),
        table: "orders".to_owned(),
        columns: vec!["id".to_owned()],
    };
    let key = fk_nav_key(&target, &[FkValue::Num(1.0)]);
    let audit = FkNavTargetIdentityVm { schema: Some("audit".to_owned()), ..target.clone() };
    assert_ne!(fk_nav_key(&audit, &[FkValue::Num(1.0)]), key);
    assert_ne!(fk_nav_key(&target, &[FkValue::Num(2.0)]), key);
    assert_eq!(fk_nav_key(&target, &[FkValue::Num(1.0)]), key);

    let null_schema = FkNavTargetIdentityVm { schema: None, ..target.clone() };
    let empty_schema = FkNavTargetIdentityVm { schema: Some(String::new()), ..target };
    assert_ne!(fk_nav_key(&null_schema, &[FkValue::Num(1.0)]), fk_nav_key(&empty_schema, &[FkValue::Num(1.0)]));
}

#[test]
fn find_fk_prefers_name_then_ordered_columns() {
    let keys = [composite(), fk("orders_customer_fk", &["customer_id"], "customers", &["id"])];
    assert_eq!(find_foreign_key(&keys, &["customer_id".to_owned()], Some("orders_parent_fk")).unwrap().name, "orders_parent_fk");
    assert_eq!(find_foreign_key(&keys, &["customer_id".to_owned()], None).unwrap().name, "orders_customer_fk");
    assert_eq!(find_foreign_key(&keys, &["parent_a".to_owned(), "parent_b".to_owned()], None).unwrap().name, "orders_parent_fk");
    assert!(find_foreign_key(&keys, &["parent_b".to_owned(), "parent_a".to_owned()], None).is_none());
}

#[test]
fn anchor_picking_and_scroll_clamping() {
    assert_eq!(pick_anchor_row(280.0, 28.0, 112.0, Some(12)), (12, true), "active row on screen");
    assert_eq!(pick_anchor_row(280.0, 28.0, 112.0, Some(99)), (10, false), "off-screen active → first visible");
    assert_eq!(pick_anchor_row(-50.0, 28.0, 112.0, None), (0, false));
    assert_eq!(scroll_top_for_anchor_row(5.0, 28.0), 140.0);
    assert_eq!(scroll_top_for_anchor_row(-3.0, 28.0), 0.0);
    assert_eq!(scroll_top_for_anchor_row(f64::NAN, 28.0), 0.0);
    assert_eq!(target_label_of("t", &["id".to_owned()]), "t.id");
    assert_eq!(FK_NAV_MAX_DEPTH, 32);
}

// ── hook decisions ───────────────────────────────────────────────────────────

#[test]
fn jump_resolves_group_values_and_refuses_null_or_missing() {
    let index = index_foreign_keys(&[composite()]);
    let row = BTreeMap::from([
        ("parent_a".to_owned(), FkValue::Num(1.0)),
        ("parent_b".to_owned(), FkValue::Str("k".to_owned())),
    ]);
    let decision = resolve_jump("parent_a", 3, &[BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), row.clone()], &index);
    assert_eq!(
        decision,
        JumpDecision::Proceed {
            cell_key: "3:parent_a".to_owned(),
            foreign_key: composite(),
            values: vec![FkValue::Num(1.0), FkValue::Str("k".to_owned())],
        }
    );
    assert_eq!(resolve_jump("note", 0, &[row.clone()], &index), JumpDecision::Refuse);
    assert_eq!(resolve_jump("parent_a", 9, &[row.clone()], &index), JumpDecision::Refuse);
    let mut null_row = row;
    null_row.insert("parent_b".to_owned(), FkValue::Null);
    assert_eq!(resolve_jump("parent_a", 0, &[null_row], &index), JumpDecision::Refuse);
}

#[test]
fn resolving_set_add_and_remove() {
    let empty = BTreeSet::new();
    let added = resolving_add(&empty, "3:parent_a");
    assert!(added.contains("3:parent_a"));
    assert!(empty.is_empty(), "the original set is untouched");
    assert!(resolving_remove(&added, "3:parent_a").is_empty());
}

#[test]
fn breadcrumb_derivation_needs_an_origin() {
    let origin = FkOrigin {
        source_panel_id: "panel-a".to_owned(),
        source_table: "orders".to_owned(),
        local_columns: vec!["customer_id".to_owned()],
        target_table: "customers".to_owned(),
        target_columns: vec!["id".to_owned()],
        nav_entry_id: "fk-nav-1".to_owned(),
    };
    assert_eq!(derive_breadcrumb(None, false, 1, 1), None);
    assert_eq!(
        derive_breadcrumb(Some(&origin), true, 2, 0),
        Some(BreadcrumbVm { origin, truncated: true, can_go_back: true, can_go_forward: false })
    );
}

#[test]
fn referencing_rows_only_for_existing_rows() {
    assert_eq!(resolve_referencing_rows(1, 3), Some(1));
    assert_eq!(resolve_referencing_rows(3, 3), None);
}

// ── history machine ──────────────────────────────────────────────────────────

fn origin() -> FkNavOriginVm {
    FkNavOriginVm {
        source_panel_id: "panel-a".to_owned(),
        source_table: "orders".to_owned(),
        local_columns: vec!["customer_id".to_owned()],
        target_table: "customers".to_owned(),
        target_columns: vec!["id".to_owned()],
    }
}

fn panels(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn history_back_forward_and_forward_clearing() {
    let mut machine = FkNavigationMachine::default();
    let first = machine.push("panel-b", "key-b", origin());
    assert_eq!(first.id, "fk-nav-1");
    machine.go_back(&panels(&["panel-a", "panel-b"]));
    assert_eq!(machine.forward.len(), 1);
    machine.push("panel-c", "key-c", origin());
    assert!(machine.forward.is_empty(), "a new jump clears forward");

    let back = machine.go_back(&panels(&["panel-a", "panel-b", "panel-c"])).unwrap();
    assert_eq!(back.origin.source_panel_id, "panel-a", "back activates the source tab");
    let forward = machine.go_forward(&panels(&["panel-a", "panel-b", "panel-c"])).unwrap();
    assert_eq!(forward.panel_id, "panel-c", "forward activates the opened tab");
}

#[test]
fn history_skips_closed_tabs_and_prunes() {
    let mut machine = FkNavigationMachine::default();
    machine.push("panel-b", "key-b", origin());
    machine.push("panel-c", "key-c", origin());
    let result = machine.go_back(&panels(&["panel-a", "panel-b"])).unwrap();
    assert_eq!(result.panel_id, "panel-b", "dead panel-c was skipped");
    assert!(machine.back.is_empty(), "dead entry above the survivor dropped");

    let mut machine = FkNavigationMachine::default();
    machine.push("panel-b", "key-b", origin());
    machine.push("panel-c", "key-c", origin());
    machine.prune(&panels(&["panel-a", "panel-c"]));
    assert_eq!(machine.back.iter().map(|e| e.panel_id.as_str()).collect::<Vec<_>>(), ["panel-c"]);
}

#[test]
fn history_depth_cap_rewind_and_nav_key_lookup() {
    let mut machine = FkNavigationMachine::default();
    for index in 0..=FK_NAV_MAX_DEPTH {
        machine.push(&format!("panel-{index}"), &format!("key-{index}"), origin());
    }
    assert_eq!(machine.back.len(), FK_NAV_MAX_DEPTH);
    assert!(machine.truncated);
    assert_eq!(machine.back[0].nav_key, "key-1", "oldest entry dropped");

    let first_id = machine.find_entry_id_by_nav_key("key-1").unwrap();
    machine.rewind_to(&first_id);
    assert_eq!(machine.back.len(), 1);
    assert!(machine.forward.is_empty());
    assert!(machine.find_entry_id_by_nav_key("key-none").is_none());
}

#[test]
fn history_viewport_snapshots_are_latest_and_consumed_once() {
    let mut machine = FkNavigationMachine::default();
    machine.record_viewport("panel-a", GridViewportVm { scroll_top: 420, anchor_row: Some(15), anchor_column: Some("customer_id".to_owned()) });
    machine.record_viewport("panel-a", GridViewportVm { scroll_top: 900, anchor_row: None, anchor_column: None });
    assert_eq!(machine.peek_viewport("panel-a").unwrap().scroll_top, 900);
    assert_eq!(machine.take_viewport("panel-a").unwrap().scroll_top, 900);
    assert!(machine.take_viewport("panel-a").is_none(), "taking consumes it");
    assert!(machine.take_viewport("panel-zzz").is_none());
}

// ── T628 independent audit (appended; reuses this module's helpers) ──────

/// React `fkNavKey` is `JSON.stringify([...])`: booleans and strings with JSON
/// metacharacters must be encoded exactly (a differing key breaks the cycle /
/// dedup rule). The coder suite only pins numbers and schema presence.
#[test]
fn audit_nav_key_encodes_booleans_and_escapes_strings() {
    let target = FkNavTargetIdentityVm {
        database: "app".to_owned(),
        schema: None,
        table: "orders".to_owned(),
        columns: vec!["code".to_owned()],
    };
    let key = fk_nav_key(&target, &[FkValue::Bool(false)]);
    assert_eq!(key, r#"["app",null,"orders",["code"],[false]]"#);

    let quoted = fk_nav_key(&target, &[FkValue::Str("a\"b\\c\n".to_owned())]);
    assert_eq!(quoted, r#"["app",null,"orders",["code"],["a\"b\\c\n"]]"#);
}

/// React `findForeignKey` ignores a falsy constraint name (`if (constraintName)`),
/// so `Some("")` must fall back to the ordered-column match.
#[test]
fn audit_find_fk_empty_name_falls_back_to_columns() {
    let keys = [fk("named", &["id"], "customers", &["id"])];
    let found = find_foreign_key(&keys, &["id".to_owned()], Some(""));
    assert_eq!(found.unwrap().name, "named");
}

/// React `pickAnchorRow` keeps the active row at the exact last visible row
/// (`activeRow <= lastVisible`). The coder suite only tests inside and way out.
#[test]
fn audit_anchor_row_at_last_visible_boundary() {
    // scrollTop 280 / 28 = row 10; viewport 112 / 28 = 4 rows → last = 13.
    assert_eq!(pick_anchor_row(280.0, 28.0, 112.0, Some(13)), (13, true));
    assert_eq!(pick_anchor_row(280.0, 28.0, 112.0, Some(14)), (10, false));
}

/// React `goBack` at the bottom of an empty stack is a no-op (returns null),
/// and `reset` forgets the whole history/viewports.
#[test]
fn audit_empty_stack_back_and_reset_are_noops() {
    let mut machine = FkNavigationMachine::default();
    assert!(machine.go_back(&panels(&[])).is_none());
    assert!(machine.go_forward(&panels(&[])).is_none());

    machine.push("panel-b", "key-b", origin());
    machine.record_viewport("panel-a", GridViewportVm { scroll_top: 42, anchor_row: None, anchor_column: None });
    machine.reset();
    assert!(machine.back.is_empty());
    assert!(machine.forward.is_empty());
    assert!(machine.viewports().is_empty());
    assert!(!machine.truncated);
}

/// React `push` sets `truncated` only when the cap is exceeded; a later normal
/// push must not clear the flag.
#[test]
fn audit_truncated_flag_persists_after_normal_push() {
    let mut machine = FkNavigationMachine::default();
    for index in 0..=FK_NAV_MAX_DEPTH {
        machine.push(&format!("panel-{index}"), &format!("key-{index}"), origin());
    }
    assert!(machine.truncated);
    machine.push("panel-final", "key-final", origin());
    assert!(machine.truncated, "truncated stays true once the stack overflowed");
}
