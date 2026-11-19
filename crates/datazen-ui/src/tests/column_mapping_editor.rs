//! T023-S2: `ColumnMappingEditor` shell — the target-table input +
//! create-new toggle, auto-match / clear-unmapped, unmapped-target
//! warning, table header, and row-list assembly over the S1 row
//! (`ColumnMappingEditor.tsx:35-193`).
//!
//! Probes the rendered element tree (keys, single `kit-root` mount,
//! header geometry, row order), drives the real kit controls through
//! pointer/keyboard journeys (toggle, buttons, row skip/select, target
//! input via paste + Enter), and checks the T307-placeholder pure
//! helpers against `transferMappingView.ts`. Every assertion observes
//! the runtime tree or the probe's recorded patches, not the
//! component's own report.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::i18n::{has, t};
use crate::views::{
    auto_match_column_mappings, clear_unmapped_column_mappings,
    normalize_column_mappings, show_create_new_toggle, show_target_type,
    unmapped_target_columns, ColumnMappingEditor, ColumnMappingEditorState,
    ColumnMappingTablePatch, TransferColumnMapping, TransferMappingStatus,
    TransferTableVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn mapping(source: &str, target: &str, skip: bool) -> TransferColumnMapping {
    TransferColumnMapping {
        source_column: source.to_owned(),
        target_column: target.to_owned(),
        skip,
        target_native_type: None,
    }
}

fn table() -> TransferTableVm {
    TransferTableVm {
        source_table: "users".to_owned(),
        target_table: "users".to_owned(),
        status: TransferMappingStatus::Matched,
        create_new: false,
        enabled: true,
        column_mappings: vec![
            mapping("id", "id", false),
            mapping("name", "", true),
            mapping("email", "email", false),
        ],
        source_columns: Some(vec![
            "id".to_owned(),
            "name".to_owned(),
            "email".to_owned(),
        ]),
        target_columns: Some(vec![
            "id".to_owned(),
            "name".to_owned(),
            "email".to_owned(),
            "extra".to_owned(),
        ]),
        source_column_types: vec![
            ("id".to_owned(), "INTEGER".to_owned()),
            ("name".to_owned(), "TEXT".to_owned()),
            ("email".to_owned(), "TEXT".to_owned()),
        ],
        source_primary_keys: None,
        recordset: None,
    }
}

#[derive(Clone)]
struct Probe {
    editor: ColumnMappingEditorState,
    dark: bool,
    patches: Vec<ColumnMappingTablePatch>,
    blurs: usize,
}

fn seeded() -> Probe {
    Probe {
        editor: ColumnMappingEditorState::new(table(), false, false),
        dark: true,
        patches: Vec::new(),
        blurs: 0,
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let el = ColumnMappingEditor::<Probe>::new(&s.editor)
        .editor_mut(|st: &mut Probe| &mut st.editor)
        .on_change(|st: &mut Probe, patch| {
            st.editor.apply_table_patch(&patch);
            st.patches.push(patch);
        })
        .on_target_table_blur(|st: &mut Probe| {
            st.blurs += 1;
        })
        .build(s.dark, &mut layer);
    rune_kit::root(el, layer)
}

/// The shell without wiring: the read-only shape, for the unwired test.
fn unwired_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let el = ColumnMappingEditor::<Probe>::new(&s.editor).build(s.dark, &mut layer);
    rune_kit::root(el, layer)
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn runtime_unwired(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, unwired_view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn kit_roots<S>(rt: &Runtime<S>) -> usize {
    rt.elements()
        .iter()
        .filter(|e| e.key.as_deref() == Some("kit-root"))
        .count()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[test]
fn the_i18n_keys_the_shell_asks_for_resolve() {
    for key in [
        "transfer.mapping.targetTable",
        "transfer.mapping.createNew",
        "transfer.mapping.autoMatch",
        "transfer.mapping.clearUnmapped",
        "transfer.mapping.unmappedTargetWarning",
        "transfer.mapping.sourceColumn",
        "transfer.mapping.targetColumn",
        "transfer.mapping.sourceType",
        "transfer.mapping.targetType",
        "transfer.mapping.skip",
        "transfer.mapping.pickTargetColumn",
    ] {
        assert!(has(key), "{key} must be in the zh-CN locale table");
        assert_ne!(t(key), key, "{key} must not echo the key");
    }
}

#[test]
fn normalize_follows_the_react_rules() {
    let t = table();
    // Stored rows pass through; the synthesized row is empty + skipped.
    assert_eq!(
        normalize_column_mappings(&t),
        vec![
            mapping("id", "id", false),
            mapping("name", "", true),
            mapping("email", "email", false),
        ]
    );

    // A source column with no stored mapping is synthesised.
    let mut t = table();
    t.source_columns = Some(vec!["id".to_owned(), "phone".to_owned()]);
    assert_eq!(
        normalize_column_mappings(&t),
        vec![mapping("id", "id", false), mapping("phone", "", true)]
    );

    // Absent sourceColumns falls back to the distinct stored sources,
    // in first-seen order (duplicates collapse).
    let mut t = table();
    t.source_columns = None;
    t.column_mappings = vec![
        mapping("b", "b", false),
        mapping("a", "", true),
        mapping("b", "b", false),
    ];
    assert_eq!(
        normalize_column_mappings(&t),
        vec![mapping("b", "b", false), mapping("a", "", true)]
    );
}

#[test]
fn auto_match_matches_by_name_and_keeps_native_types() {
    let mut existing = mapping("id", "renamed", false);
    existing.target_native_type = Some("BIGINT".to_owned());
    let next = auto_match_column_mappings(
        &["id".to_owned(), "name".to_owned()],
        &["id".to_owned()],
        &[existing],
    );
    assert_eq!(next[0].target_column, "id");
    assert!(!next[0].skip);
    assert_eq!(next[0].target_native_type.as_deref(), Some("BIGINT"));
    assert_eq!(next[1].target_column, "");
    assert!(next[1].skip);
    assert_eq!(next[1].target_native_type, None);

    // The pool decides, not the stored value: a stored target outside
    // the pool does not rescue the row.
    let next = auto_match_column_mappings(
        &["id".to_owned()],
        &[],
        &[mapping("id", "id", false)],
    );
    assert_eq!(next[0].target_column, "");
    assert!(next[0].skip);
}

#[test]
fn clear_unmapped_only_touches_rows_without_a_target() {
    let mut mapped = mapping("a", "a", false);
    mapped.target_native_type = Some("TEXT".to_owned());
    let next = clear_unmapped_column_mappings(&[
        mapped,
        mapping("b", "   ", false),
        mapping("c", "", true),
    ]);
    assert_eq!(next[0], {
        let mut m = mapping("a", "a", false);
        m.target_native_type = Some("TEXT".to_owned());
        m
    });
    assert_eq!(next[1].target_column, "");
    assert!(next[1].skip, "blank target is cleared and skipped");
    assert_eq!(next[2].target_column, "");
    assert!(next[2].skip);
}

#[test]
fn unmapped_target_columns_follows_the_react_rules() {
    // Seeded: "name" (its mapping is skipped, so it counts as unused)
    // and "extra" are the targets no live mapping references.
    assert_eq!(
        unmapped_target_columns(&table()),
        vec!["name".to_owned(), "extra".to_owned()]
    );

    // Creating a new table has no unmapped targets.
    let mut t = table();
    t.create_new = true;
    assert!(unmapped_target_columns(&t).is_empty());

    // No target columns at all: none unmapped.
    let mut t = table();
    t.target_columns = None;
    assert!(unmapped_target_columns(&t).is_empty());
    let mut t = table();
    t.target_columns = Some(Vec::new());
    assert!(unmapped_target_columns(&t).is_empty());

    // A skipped mapping does not count as using its target.
    let mut t = table();
    t.column_mappings = vec![mapping("id", "extra", true)];
    assert_eq!(
        unmapped_target_columns(&t),
        vec!["id".to_owned(), "name".to_owned(), "email".to_owned(), "extra".to_owned()]
    );
}

#[test]
fn the_toggle_and_type_visibility_rules() {
    let t = table();
    assert!(!show_create_new_toggle(false, &t));
    assert!(!show_target_type(false, &t));

    // Structure mode alone shows the toggle, but not the type columns.
    assert!(show_create_new_toggle(true, &t));
    assert!(!show_target_type(true, &t));

    // Status CREATE_NEW shows the toggle even outside structure mode.
    let mut t = table();
    t.status = TransferMappingStatus::CreateNew;
    assert!(show_create_new_toggle(false, &t));

    // The type columns need structure mode AND create-new together.
    let mut t = table();
    t.create_new = true;
    assert!(show_create_new_toggle(false, &t));
    assert!(!show_target_type(false, &t));
    assert!(show_target_type(true, &t));
}

#[test]
fn the_shell_renders_the_react_keys_and_mounts_once() {
    let rt = runtime(seeded(), DESIGN);
    for key in [
        "data-transfer-column-editor",
        "data-transfer-target-table-input",
        "data-transfer-auto-match",
        "data-transfer-clear-unmapped",
        "data-transfer-column-header",
        "data-transfer-column-row-id",
        "data-transfer-column-row-name",
        "data-transfer-column-row-email",
        "data-transfer-recordset",
        "data-transfer-source-filter",
    ] {
        assert!(has_key(&rt, key), "{key} is laid out");
    }
    // No create-new toggle: not structure mode, status is MATCHED,
    // createNew is false.
    assert!(!has_key(&rt, "data-transfer-create-new-toggle"));
    // The unmapped warning names the one unreferenced target.
    assert!(has_key(&rt, "data-transfer-unmapped-target-warning"));
    let warning = element(&rt, "data-transfer-unmapped-target-warning");
    let expected =
        t("transfer.mapping.unmappedTargetWarning").replace("{columns}", "name, extra");
    assert_eq!(
        warning.text.as_deref(),
        Some(expected.as_str()),
        "warning interpolates the unmapped columns"
    );
    // The mounting decision, observed: three rows, exactly one kit-root.
    assert_eq!(kit_roots(&rt), 1, "one root for the whole editor");
    assert!(
        !has_key(&rt, "data-transfer-column-row"),
        "React's repeated row testid cannot be a Rune sibling key"
    );
}

#[test]
fn the_rows_stack_in_order_under_the_header() {
    for dark in [true, false] {
        for size in [MIN, DESIGN] {
            let mut probe = seeded();
            probe.dark = dark;
            let rt = runtime(probe, size);
            let header = element(&rt, "data-transfer-column-header");
            let rows = ["id", "name", "email"]
                .map(|s| element(&rt, &format!("data-transfer-column-row-{s}")));
            // Header above the first row, rows in source order, no overlap.
            assert!(header.rect[1] + header.rect[3] <= rows[0].rect[1] + 0.5);
            for pair in rows.windows(2) {
                assert!(
                    pair[0].rect[1] + pair[0].rect[3] <= pair[1].rect[1] + 0.5,
                    "rows stack without overlap: {:?} vs {:?}",
                    pair[0].rect,
                    pair[1].rect
                );
            }
            // Every row fills the table width (the build_inner w_full).
            let table_el = element(&rt, "data-transfer-column-table");
            for row in &rows {
                assert!(
                    (row.rect[2] - table_el.rect[2]).abs() <= 0.5,
                    "row fills the table width: row {:?} table {:?}",
                    row.rect,
                    table_el.rect
                );
            }
            assert_in_window(&rt, size.0, size.1, "shell geometry");
        }
    }
}

#[test]
fn the_header_shows_the_type_columns_only_in_structure_create_new() {
    let mut probe = seeded();
    probe.editor.structure_mode = true;
    probe.editor.table.create_new = true;
    probe.editor.resync_rows();
    let rt = runtime(probe, DESIGN);
    // The rows carry the type cells — assert through them, where the
    // widths are pinned by the S1 geometry tests — and the toggle shows.
    assert!(has_key(&rt, "data-transfer-column-row-id-source-type"));
    assert!(has_key(&rt, "data-transfer-column-row-id-target-type"));
    assert!(has_key(&rt, "data-transfer-create-new-toggle"));
}

#[test]
fn enabling_create_new_clears_the_target_name() {
    let mut probe = seeded();
    probe.editor.structure_mode = true;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-create-new-toggle");
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0].create_new,
        Some(true),
        "the toggle commits createNew"
    );
    assert_eq!(
        rt.state.patches[0].target_table.as_deref(),
        Some(""),
        "enabling create-new clears the name (React 118-119)"
    );
    assert_eq!(rt.state.editor.table.target_table, "");
    assert_eq!(
        rt.state.editor.target_table_edit.display().0,
        "",
        "the cleared model re-seeds the buffer"
    );

    // Toggling back off keeps the (now empty) name — React only clears
    // on the way in.
    click(&mut rt, "data-transfer-create-new-toggle");
    assert_eq!(rt.state.patches.len(), 2);
    assert_eq!(rt.state.patches[1].create_new, Some(false));
    assert_eq!(rt.state.patches[1].target_table.as_deref(), Some(""));
}

#[test]
fn typing_a_target_table_and_pressing_enter_commits_and_blurs() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-target-table-input");
    rt.paste("users_copy");
    rt.settle_all();
    rt.frame(0.0);
    // The caret lands where the click put it, so read the buffer rather
    // than assuming the paste appended.
    let typed = rt.state.editor.target_table_edit.display().0.clone();
    assert!(
        typed.contains("users_copy"),
        "the paste reached the buffer: {typed:?}"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0].target_table.as_deref(),
        Some(typed.as_str())
    );
    assert_eq!(rt.state.editor.table.target_table, typed);
    assert_eq!(
        rt.state.blurs, 1,
        "the blur callback fires on submit (no Rune on_blur hook)"
    );
}

#[test]
fn auto_match_fills_names_and_clear_skips_the_blank_row() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-auto-match");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0]
        .column_mappings
        .clone()
        .expect("auto-match commits columnMappings");
    assert_eq!(next[0], mapping("id", "id", false));
    assert_eq!(next[1], mapping("name", "name", false));
    assert_eq!(next[2], mapping("email", "email", false));
    // "extra" still has no source column to match, so the warning stays.
    assert!(
        has_key(&rt, "data-transfer-unmapped-target-warning"),
        "auto-match cannot map a target with no source column"
    );

    // Clear-unmapped: seed a live row with a blank target first.
    let mut probe = seeded();
    probe.editor.table.column_mappings[1] = mapping("name", "", false);
    probe.editor.resync_rows();
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-clear-unmapped");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0]
        .column_mappings
        .clone()
        .expect("clear commits columnMappings");
    assert_eq!(next[1], mapping("name", "", true));
    assert_eq!(next[0], mapping("id", "id", false), "mapped rows untouched");
}

#[test]
fn auto_match_sources_falls_back_to_the_mapping_sources() {
    let mut t = table();
    t.source_columns = None;
    assert_eq!(
        t.auto_match_sources(&normalize_column_mappings(&t)),
        vec!["id".to_owned(), "name".to_owned(), "email".to_owned()]
    );
    let t = table();
    assert_eq!(
        t.auto_match_sources(&normalize_column_mappings(&t)),
        vec!["id".to_owned(), "name".to_owned(), "email".to_owned()]
    );
}

#[test]
fn opening_a_row_select_through_the_shell_wires_open_and_close() {
    let mut rt = runtime(seeded(), DESIGN);
    assert!(!rt.state.editor.rows[0].target_select_open);
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    assert!(
        rt.state.editor.rows[0].target_select_open,
        "the trigger press flips the shell-owned open flag"
    );
    assert!(has_key(&rt, "option2"), "the listbox opened");
    click(&mut rt, "option2");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0]
        .column_mappings
        .clone()
        .expect("choosing commits columnMappings");
    assert_eq!(next[0], mapping("id", "name", false));
    assert!(
        !rt.state.editor.rows[0].target_select_open,
        "choosing closes the listbox"
    );
    assert!(!has_key(&rt, "option2"), "the listbox is gone");
}

#[test]
fn pressing_escape_closes_an_open_row_select() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    assert!(rt.state.editor.rows[0].target_select_open);
    press(&mut rt, Key::Escape);
    assert!(
        !rt.state.editor.rows[0].target_select_open,
        "Escape closes the listbox via on_close"
    );
    assert!(
        rt.state.patches.is_empty(),
        "closing commits nothing: {:?}",
        rt.state.patches
    );
}

#[test]
fn a_row_skip_click_flows_up_as_a_table_patch() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-skip-email");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0]
        .column_mappings
        .clone()
        .expect("row change commits columnMappings");
    assert_eq!(next[2], mapping("email", "email", true));
    assert_eq!(next[0], mapping("id", "id", false), "other rows untouched");
    assert_eq!(next[1], mapping("name", "", true));
    assert!(
        rt.state.editor.table.column_mappings[2].skip,
        "the patch landed in the table"
    );
}

#[test]
fn a_disabled_editor_commits_nothing() {
    let mut probe = seeded();
    probe.editor.disabled = true;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-auto-match");
    click(&mut rt, "data-transfer-clear-unmapped");
    click(&mut rt, "data-transfer-skip-id");
    click(&mut rt, "data-transfer-target-table-input");
    rt.paste("should-not-commit");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert!(
        rt.state.patches.is_empty(),
        "disabled commits nothing: {:?}",
        rt.state.patches
    );
    assert_eq!(rt.state.blurs, 0);
    assert_eq!(rt.state.editor.table.target_table, "users");
    assert!(!rt.state.editor.table.column_mappings[0].skip);
}

#[test]
fn the_unwired_shell_renders_read_only() {
    let rt = runtime_unwired(seeded(), DESIGN);
    // The model value is there as plain text under the input's key.
    assert_eq!(
        element(&rt, "data-transfer-target-table-input").text.as_deref(),
        Some("users")
    );
    assert!(has_key(&rt, "data-transfer-column-row-id"));
    assert_eq!(kit_roots(&rt), 1);
}

#[test]
fn the_shell_stays_inside_both_window_sizes_in_both_themes() {
    for dark in [true, false] {
        for size in [MIN, DESIGN] {
            let mut probe = seeded();
            probe.dark = dark;
            let rt = runtime(probe, size);
            assert_in_window(&rt, size.0, size.1, "shell");
            let mut probe = seeded();
            probe.dark = dark;
            probe.editor.structure_mode = true;
            probe.editor.table.create_new = true;
            probe.editor.resync_rows();
            let rt = runtime(probe, size);
            assert_in_window(&rt, size.0, size.1, "shell, structure create-new");
        }
    }
}
