//! T524 independent audit (tester, not coder).
//!
//! Green probes of pure branches the shipped 10 tests do not pin: blob/bool/null
//! cell display, find no-op cases, addressed-column filtering, staged-edit
//! application and view↔original row mapping, and the sessionReadOnly verdict.
//!
//! Three divergences from React `resultFindReplace.ts` were found and are
//! reported as defects with their exact RED text (any-column `notContains`
//! using `any` instead of `every`; replace-all overwriting the whole cell
//! instead of splicing the needle; a Prefix query planning a cell that merely
//! contains the needle). Those probes are intentionally not kept as red tests.

use datazen_ui::views::result_workspace::result_table_view::{
    can_edit, effective_rows, filter_rows, grid_read_only_message_key, plan_replacements,
    read_only_reason, result_writable, view_rows, FindOperator, FindQuery, GridValue, PendingEdit,
    ResultTableVm, ServerEditability,
};

fn text(value: &str) -> GridValue {
    GridValue::Text(value.to_owned())
}

fn query(needle: &str, operator: FindOperator) -> FindQuery {
    FindQuery {
        needle: needle.to_owned(),
        column: None,
        operator,
        match_case: false,
    }
}

#[test]
fn blob_bool_and_null_cells_display_as_the_grid_shows_them() {
    let blob = GridValue::Blob(vec![0x00, 0x1f, 0xff]);
    assert_eq!(blob.display(), "00 1f ff", "blob is a lowercase hex preview");
    assert_eq!(GridValue::Bool(true).display(), "true");
    assert_eq!(GridValue::Bool(false).display(), "false");
    assert_eq!(GridValue::Null.display(), "NULL");
    let long = GridValue::Blob((0..20).collect());
    assert_eq!(long.cell_text().split_whitespace().count(), 16, "blob preview caps at 16 bytes");
}

#[test]
fn find_no_op_cases_plan_nothing_and_empty_needle_keeps_every_row() {
    let rows = vec![vec![text("acme corp")], vec![text("beta")]];
    let columns = vec!["name".to_owned()];
    assert!(plan_replacements(&rows, &columns, &query("", FindOperator::Contains), "X").is_empty());
    assert!(plan_replacements(&rows, &columns, &query("acme", FindOperator::Contains), "acme").is_empty());
    assert_eq!(filter_rows(&rows, &columns, &query("", FindOperator::Contains)), vec![0, 1]);
}

#[test]
fn addressed_column_filter_keeps_the_null_row_for_not_contains() {
    let rows = vec![vec![text("acme"), GridValue::Text("a@acme.io".to_owned())], vec![GridValue::Null, GridValue::Null]];
    let columns = vec!["name".to_owned(), "email".to_owned()];
    let mut addressed = query("acme", FindOperator::NotContains);
    addressed.column = Some("name".to_owned());
    assert_eq!(
        filter_rows(&rows, &columns, &addressed),
        vec![1],
        "the NULL name cell does not contain the needle and is kept"
    );
}

#[test]
fn staged_edits_apply_to_effective_rows_and_find_maps_back_to_originals() {
    let mut vm = ResultTableVm {
        columns: vec![datazen_ui::views::result_workspace::result_table_view::ResultColumn {
            name: "name".to_owned(),
            data_type: "text".to_owned(),
        }],
        rows: vec![vec![text("alpha")], vec![text("beta")]],
        ..Default::default()
    };
    vm.pending_edits.push(PendingEdit {
        row: 0,
        column: "name".to_owned(),
        original: text("alpha"),
        value: text("ALPHA"),
    });
    let effective = effective_rows(&vm);
    assert_eq!(effective[0][0].display(), "ALPHA");
    assert_eq!(effective[1][0].display(), "beta");

    vm.find_open = true;
    vm.find_query = query("beta", FindOperator::Contains);
    let (view, originals) = view_rows(&vm);
    assert_eq!(view.len(), 1);
    assert_eq!(view[0][0].display(), "beta");
    assert_eq!(originals, vec![1], "the filtered view maps back to row 1");
}

#[test]
fn session_read_only_verdict_blocks_writability_and_names_the_reason() {
    let mut vm = ResultTableVm::default();
    vm.source_writable_mapping_verified = true;
    vm.has_runtime_binding = true;
    vm.server_verdict = Some(ServerEditability {
        writable: false,
        reason: Some("sessionReadOnly".to_owned()),
        ..Default::default()
    });
    assert!(!result_writable(&vm), "sessionReadOnly outranks a verified mapping");
    assert_eq!(read_only_reason(&vm).as_deref(), Some("sessionReadOnly"));
    assert_eq!(grid_read_only_message_key("sessionReadOnly"), "query.gridReadOnly.sessionReadOnly");
    vm.can_commit = true;
    assert!(!can_edit(&vm));
}
