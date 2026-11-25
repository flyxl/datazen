//! T267 independent audit: the empty notice's exact locale value, the
//! multi-byte source names riding the keys and text verbatim, the
//! seeded mode propagating into the editor's type columns, the
//! disabled-editor ruling (the list stays pressable), the sidebar's
//! fixed width and source order at both themes, and the single-line
//! target clip for a long name.

use rune::{Cx, El, Env, Runtime};

use datazen_ui::views::{
    seed_editor_state, ColumnMappingEditorState, TransferMappingStatus, TransferMappingStep,
    TransferMode, TransferTableVm,
};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn audit_table(source: &str, create_new: bool) -> TransferTableVm {
    TransferTableVm {
        source_table: source.to_owned(),
        target_table: format!("目标_{source}"),
        status: TransferMappingStatus::Matched,
        create_new,
        enabled: true,
        column_mappings: vec![datazen_ui::views::TransferColumnMapping {
            source_column: "id".to_owned(),
            target_column: "id".to_owned(),
            skip: false,
            target_native_type: None,
        }],
        source_columns: Some(vec!["id".to_owned()]),
        target_columns: Some(vec!["id".to_owned()]),
        source_column_types: vec![("id".to_owned(), "INTEGER".to_owned())],
        source_primary_keys: None,
        recordset: None,
    }
}

#[derive(Clone)]
struct Audit {
    tables: Vec<TransferTableVm>,
    selected: String,
    editor: ColumnMappingEditorState,
    dark: bool,
    selects: Vec<String>,
}

fn audit(tables: Vec<TransferTableVm>, selected: &str, mode: TransferMode, disabled: bool, dark: bool) -> Audit {
    // The editor state seeds for the selected table — or the first
    // table, mirroring the render's find-else-first derivation; an
    // empty list renders the notice and never reads the seed.
    let fallback = tables
        .iter()
        .find(|t| t.source_table == selected)
        .or_else(|| tables.first())
        .cloned();
    let editor = match fallback {
        Some(table) => seed_editor_state(&table, mode, disabled),
        None => seed_editor_state(&audit_table("_unused", false), mode, disabled),
    };
    Audit {
        editor,
        tables,
        selected: selected.to_owned(),
        dark,
        selects: Vec::new(),
    }
}

fn audit_view(s: &Audit, _cx: &mut Cx<Audit>) -> El {
    let mut layer = Vec::new();
    let el = TransferMappingStep::<Audit>::new(&s.tables, &s.selected, &s.editor)
        .editor_mut(|st: &mut Audit| &mut st.editor)
        .on_select_table(|st: &mut Audit, source| st.selects.push(source))
        .build(s.dark, &mut layer);
    rune_kit::root(el, layer)
}

fn mount(state: Audit, size: (f32, f32)) -> Runtime<Audit> {
    let dark = state.dark;
    let mut rt = Runtime::new(state, audit_view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn keyed<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_by_content<S>(rt: &Runtime<S>, content: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(content))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = keyed(rt, key)
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

/// The empty notice is the pack's exact string, not the key lookup's
/// paraphrase.
#[test]
fn the_empty_notice_is_the_exact_locale_value() {
    let rt = mount(
        audit(Vec::new(), "", TransferMode::Data, false, true),
        (960.0, 640.0),
    );
    let notice = keyed(&rt, "data-transfer-mapping-empty").expect("the notice mounts");
    assert_eq!(notice.text.as_deref(), Some("没有可映射的已启用表。"));
}

/// Multi-byte source names ride the per-item key and the name line
/// verbatim; the target line keeps its arrow prefix byte for byte.
#[test]
fn multi_byte_names_ride_keys_text_and_arrow_verbatim() {
    let tables = vec![audit_table("订单_2026", false)];
    let rt = mount(audit(tables, "订单_2026", TransferMode::Data, false, true), (960.0, 640.0));
    let base = "data-transfer-mapping-table-item-订单_2026";
    let name = keyed(&rt, &format!("{base}-name")).expect("the name line mounts");
    assert_eq!(name.text.as_deref(), Some("订单_2026"));
    let target = keyed(&rt, &format!("{base}-target")).expect("the target line mounts");
    assert_eq!(target.text.as_deref(), Some("→ 目标_订单_2026"));
}

/// The seeded mode propagates into the embedded editor: a create-new
/// table seeded under structure mode shows the type columns; the same
/// table under data mode shows none.
#[test]
fn the_seeded_mode_decides_the_editors_type_columns() {
    let create_new = vec![audit_table("users", true)];

    let structure = mount(
        audit(create_new.clone(), "users", TransferMode::Structure, false, true),
        (960.0, 640.0),
    );
    assert!(
        text_by_content(&structure, "源类型") && text_by_content(&structure, "目标类型"),
        "structure mode seeds the type columns"
    );

    let data = mount(
        audit(create_new, "users", TransferMode::Data, false, true),
        (960.0, 640.0),
    );
    assert!(
        !text_by_content(&data, "源类型") && !text_by_content(&data, "目标类型"),
        "data mode seeds no type columns"
    );
}

/// The props' own doc comment: `disabled` is forwarded to the editor,
/// never used to dim this component — the list stays pressable while
/// the editor is inert.
#[test]
fn the_list_stays_pressable_while_the_editor_is_disabled() {
    let tables = vec![audit_table("users", false), audit_table("orders", false)];
    let mut rt = mount(
        audit(tables, "users", TransferMode::Data, true, true),
        (960.0, 640.0),
    );
    assert!(keyed(&rt, "data-transfer-column-editor").is_some());
    click(&mut rt, "data-transfer-mapping-table-item-orders");
    assert_eq!(rt.state.selects, vec!["orders".to_owned()],
        "a read-only act must not be blocked while a prepare is in flight");
}

/// The sidebar keeps React's w-56 (224pt) and lists the tables in the
/// tables' order, at both themes, all inside the minimum window.
#[test]
fn the_sidebar_keeps_its_width_order_and_window_at_both_themes() {
    for dark in [true, false] {
        let tables = vec![audit_table("users", false), audit_table("orders", false)];
        let rt = mount(audit(tables, "users", TransferMode::Data, false, dark), (960.0, 640.0));

        let sidebar = keyed(&rt, "data-transfer-mapping-sidebar")
            .unwrap_or_else(|| panic!("the sidebar mounts (dark={dark})"));
        assert!(
            (sidebar.rect[2] - 224.0).abs() < 1.0,
            "w-56 is 224pt, not {:?} (dark={dark})",
            sidebar.rect
        );

        let first = keyed(&rt, "data-transfer-mapping-table-item-users-name")
            .expect("the first item mounts");
        let second = keyed(&rt, "data-transfer-mapping-table-item-orders-name")
            .expect("the second item mounts");
        assert!(
            first.rect[1] < second.rect[1],
            "the tables order is the list order (dark={dark})"
        );

        for e in rt.elements() {
            let r = e.rect;
            assert!(
                r[0] >= -0.5 && r[1] >= -0.5 && r[0] + r[2] <= 960.5 && r[1] + r[3] <= 640.5,
                "{:?} out of the minimum window (dark={dark})",
                e.key
            );
        }
    }
}

/// A long target stays one line: the item's target line clips instead
/// of wrapping (React's `truncate`).
#[test]
fn a_long_target_stays_one_line() {
    let mut orders = audit_table("orders", false);
    orders.target_table = "一个很长很长的目标表名字，长到必须被裁剪".repeat(2);
    let rt = mount(
        audit(vec![orders], "orders", TransferMode::Data, false, true),
        (960.0, 640.0),
    );
    let target = keyed(&rt, "data-transfer-mapping-table-item-orders-target")
        .expect("the target line mounts");
    assert!(
        target.rect[3] <= 18.0,
        "the truncated line is one text_small line, not {:?}",
        target.rect
    );
}
