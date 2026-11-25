//! T023-S2 independent audit: `ColumnMappingEditor` shell.
//!
//! Fresh vectors the coder tests do not cover, written against
//! `ColumnMappingEditor.tsx:35-193` and `transferMappingView.ts`
//! directly — not against the coder's test table. Two of the pure-function
//! probes assert the TypeScript `Map` semantics verbatim (a later
//! duplicate source mapping wins); the module docs claim the placeholder
//! mirrors the TS "line for line", so a first-wins result here is a
//! finding, not a test bug.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::{
    auto_match_column_mappings, clear_unmapped_column_mappings,
    normalize_column_mappings, unmapped_target_columns, ColumnMappingEditor,
    ColumnMappingEditorState, ColumnMappingTablePatch, TransferColumnMapping,
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

fn typed_mapping(
    source: &str,
    target: &str,
    skip: bool,
    native: &str,
) -> TransferColumnMapping {
    TransferColumnMapping {
        source_column: source.to_owned(),
        target_column: target.to_owned(),
        skip,
        target_native_type: Some(native.to_owned()),
    }
}

fn table() -> TransferTableVm {
    TransferTableVm {
        source_table: "users".to_owned(),
        target_table: "users".to_owned(),
        status: Default::default(),
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

/// Five source columns, for the mount/overlay probes.
fn wide_table() -> TransferTableVm {
    let mut t = table();
    t.source_columns = Some(vec![
        "id".to_owned(),
        "name".to_owned(),
        "email".to_owned(),
        "phone".to_owned(),
        "age".to_owned(),
    ]);
    t.column_mappings = vec![
        mapping("id", "id", false),
        mapping("name", "", true),
        mapping("email", "email", false),
        mapping("phone", "", true),
        mapping("age", "age", false),
    ];
    t
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

fn seeded_with(t: TransferTableVm, structure_mode: bool, disabled: bool) -> Probe {
    Probe {
        editor: ColumnMappingEditorState::new(t, structure_mode, disabled),
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

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, view, size, env(dark));
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

fn option_keys<S>(rt: &Runtime<S>) -> Vec<String> {
    // Exiting nodes stay listed so tools can watch the exit; lookups
    // treat them as gone (see ElementInfo::exiting).
    let mut keys: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with("option") && !k.contains('-'))
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

fn focused_key<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
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

// ---------------------------------------------------------------------------
// T307 placeholder vs transferMappingView.ts, line for line
// ---------------------------------------------------------------------------

#[test]
fn normalize_duplicate_sources_follow_typescript_last_wins() {
    // TS: `new Map(table.columnMappings.map((m) => [m.sourceColumn, m]))`
    // — a later entry overwrites the earlier one for the same key.
    let mut t = table();
    t.source_columns = None;
    t.column_mappings = vec![
        mapping("id", "first", false),
        mapping("id", "second", false),
    ];
    assert_eq!(
        normalize_column_mappings(&t),
        vec![mapping("id", "second", false)],
        "TS Map keeps the LAST mapping for a duplicated source"
    );
}

#[test]
fn normalize_drops_stored_rows_outside_the_source_list() {
    // A stored mapping for a source the source list does not name is
    // not rendered: TS maps over sourceColumns, never over the stored rows.
    let mut t = table();
    t.column_mappings
        .push(mapping("ghost", "ghost", false));
    assert_eq!(
        normalize_column_mappings(&t),
        vec![
            mapping("id", "id", false),
            mapping("name", "", true),
            mapping("email", "email", false),
        ]
    );
    // A synthesised row carries no native-type override.
    let mut t = table();
    t.source_columns = Some(vec!["id".to_owned(), "phone".to_owned()]);
    let rows = normalize_column_mappings(&t);
    assert_eq!(rows[1].target_native_type, None);
    assert_eq!(rows[1].target_column, "");
    assert!(rows[1].skip);
}

#[test]
fn auto_match_duplicate_existing_follow_typescript_last_wins() {
    // TS builds `existingBySource` as a Map too: last duplicate wins.
    let existing = vec![
        typed_mapping("id", "first", false, "SMALLINT"),
        typed_mapping("id", "second", false, "BIGINT"),
    ];
    let next = auto_match_column_mappings(&["id".to_owned()], &["id".to_owned()], &existing);
    assert_eq!(
        next[0].target_native_type.as_deref(),
        Some("BIGINT"),
        "TS Map keeps the LAST existing row's native type"
    );
}

#[test]
fn auto_match_unmatched_row_keeps_its_native_type_but_empties_target() {
    let next = auto_match_column_mappings(
        &["id".to_owned()],
        &[],
        &[typed_mapping("id", "renamed", false, "TEXT")],
    );
    assert_eq!(next[0].target_column, "");
    assert!(next[0].skip);
    assert_eq!(
        next[0].target_native_type.as_deref(),
        Some("TEXT"),
        "an unmatched row is emptied and skipped, its type override survives"
    );
}

#[test]
fn clear_unmapped_preserves_native_type_and_source_on_blank_rows() {
    let next = clear_unmapped_column_mappings(&[
        typed_mapping("a", "   ", false, "VARCHAR(10)"),
        mapping("b", "b", false),
    ]);
    assert_eq!(next[0].source_column, "a");
    assert_eq!(next[0].target_column, "");
    assert!(next[0].skip);
    assert_eq!(
        next[0].target_native_type.as_deref(),
        Some("VARCHAR(10)"),
        "clear only rewrites target + skip, never the type override"
    );
    assert_eq!(next[1], mapping("b", "b", false));
}

#[test]
fn unmapped_whitespace_target_counts_only_for_the_literal_column() {
    // A mapping id -> " " references the literal " " target, not "id":
    // TS `m.targetColumn` is truthy for " ", and the Rust `is_empty`
    // check agrees — so "id" is still unmapped, exactly as in TS.
    let mut t = table();
    t.column_mappings = vec![
        mapping("id", " ", false),
        mapping("name", "name", false),
        mapping("email", "email", false),
    ];
    assert_eq!(
        unmapped_target_columns(&t),
        vec!["id".to_owned(), "extra".to_owned()]
    );

    // And when " " itself is a target column, the whitespace mapping
    // does count as using it.
    let mut t = table();
    t.target_columns = Some(vec![" ".to_owned(), "extra".to_owned()]);
    t.column_mappings = vec![mapping("id", " ", false)];
    assert_eq!(unmapped_target_columns(&t), vec!["extra".to_owned()]);

    // TS filters the target list as-is, duplicates included.
    let mut t = table();
    t.target_columns = Some(vec!["a".to_owned(), "a".to_owned(), "b".to_owned()]);
    t.column_mappings = vec![];
    assert_eq!(
        unmapped_target_columns(&t),
        vec!["a".to_owned(), "a".to_owned(), "b".to_owned()]
    );
}

#[test]
fn auto_match_sources_prefers_an_explicit_empty_source_list() {
    // React `??`: an explicit `sourceColumns: []` is not nullish, so no
    // fallback to the mapping sources — the auto-match pool is empty.
    let mut t = table();
    t.source_columns = Some(Vec::new());
    assert!(t
        .auto_match_sources(&normalize_column_mappings(&t))
        .is_empty());
    assert!(normalize_column_mappings(&t).is_empty());
}

#[test]
fn table_patch_apply_only_carries_its_fields() {
    let mut t = table();
    let before = t.clone();
    ColumnMappingTablePatch {
        create_new: Some(true),
        ..ColumnMappingTablePatch::default()
    }
    .apply_to(&mut t);
    assert!(t.create_new);
    assert_eq!(t.target_table, before.target_table);
    assert_eq!(t.column_mappings, before.column_mappings);
    assert_eq!(t.source_columns, before.source_columns);
}

// ---------------------------------------------------------------------------
// Target-table input: Enter commits, Tab does not, drafts survive patches
// ---------------------------------------------------------------------------

#[test]
fn tabbing_away_from_the_target_table_commits_nothing() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-target-table-input");
    rt.paste("users_copy");
    rt.settle_all();
    rt.frame(0.0);
    let typed = rt.state.editor.target_table_edit.display().0.clone();
    assert!(typed.contains("users_copy"), "paste reached the buffer");
    press(&mut rt, Key::Tab);
    assert!(
        rt.state.patches.is_empty(),
        "Tab is not a submit: {:?}",
        rt.state.patches
    );
    assert_eq!(rt.state.blurs, 0, "Tab does not fire the blur callback");
    assert_eq!(rt.state.editor.table.target_table, "users");
    assert_ne!(
        focused_key(&rt).as_deref(),
        Some("data-transfer-target-table-input"),
        "focus left the input on Tab"
    );

    // The draft is still there: clicking back in and pressing Enter
    // commits exactly what was typed.
    click(&mut rt, "data-transfer-target-table-input");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0].target_table.as_deref(),
        Some(typed.as_str())
    );
    assert_eq!(rt.state.blurs, 1);
}

#[test]
fn an_unrelated_patch_leaves_a_target_table_draft_intact() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-target-table-input");
    rt.paste("draft_name");
    rt.settle_all();
    rt.frame(0.0);
    let typed = rt.state.editor.target_table_edit.display().0.clone();
    assert!(typed.contains("draft_name"));
    click(&mut rt, "data-transfer-auto-match");
    assert_eq!(rt.state.patches.len(), 1);
    assert!(
        rt.state.patches[0].target_table.is_none(),
        "auto-match carries no target-table value"
    );
    assert_eq!(rt.state.editor.table.target_table, "users");
    assert_eq!(
        rt.state.editor.target_table_edit.display().0,
        typed,
        "apply_table_patch only re-seeds the buffer when the patch carries a target table"
    );
}

// ---------------------------------------------------------------------------
// Buttons: create-new pool, clear no-op, warning conditions
// ---------------------------------------------------------------------------

#[test]
fn auto_match_in_create_new_mode_matches_sources_to_themselves() {
    // React 67: the target pool is the source columns while creating a
    // new table — even when the target-column list is empty.
    let mut t = table();
    t.create_new = true;
    t.target_columns = Some(Vec::new());
    let mut rt = runtime(seeded_with(t, false, false), DESIGN);
    click(&mut rt, "data-transfer-auto-match");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0]
        .column_mappings
        .clone()
        .expect("auto-match commits columnMappings");
    assert_eq!(next[0], mapping("id", "id", false));
    assert_eq!(next[1], mapping("name", "name", false));
    assert_eq!(next[2], mapping("email", "email", false));
}

#[test]
fn clear_unmapped_with_everything_mapped_commits_the_same_rows() {
    // React always calls onChange; with nothing unmapped the committed
    // rows are identical to the current ones.
    let mut rt = runtime(seeded(), DESIGN);
    // First map the one blank row, so every row is mapped.
    click(&mut rt, "data-transfer-auto-match");
    assert_eq!(rt.state.patches.len(), 1);
    click(&mut rt, "data-transfer-clear-unmapped");
    assert_eq!(rt.state.patches.len(), 2);
    let next = rt.state.patches[1]
        .column_mappings
        .clone()
        .expect("clear commits columnMappings");
    assert_eq!(
        next,
        vec![
            mapping("id", "id", false),
            mapping("name", "name", false),
            mapping("email", "email", false),
        ]
    );
}

#[test]
fn the_warning_is_absent_when_creating_a_new_table() {
    let mut t = table();
    t.create_new = true;
    let rt = runtime(seeded_with(t, false, false), DESIGN);
    assert!(
        !has_key(&rt, "data-transfer-unmapped-target-warning"),
        "create-new has no unmapped targets by definition (TS 48)"
    );
}

#[test]
fn the_warning_is_absent_when_every_target_is_used() {
    let mut t = table();
    t.target_columns = Some(vec![
        "id".to_owned(),
        "name".to_owned(),
        "email".to_owned(),
    ]);
    t.column_mappings = vec![
        mapping("id", "id", false),
        mapping("name", "name", false),
        mapping("email", "email", false),
    ];
    let rt = runtime(seeded_with(t, false, false), DESIGN);
    assert!(!has_key(&rt, "data-transfer-unmapped-target-warning"));
}

#[test]
fn an_empty_table_renders_header_and_warning_without_rows() {
    let mut tbl = table();
    tbl.source_columns = Some(Vec::new());
    tbl.column_mappings = Vec::new();
    tbl.target_columns = Some(vec!["orphan".to_owned()]);
    let rt = runtime(seeded_with(tbl, false, false), DESIGN);
    assert!(has_key(&rt, "data-transfer-column-header"));
    assert!(!has_key(&rt, "data-transfer-column-row-id"));
    assert_eq!(kit_roots(&rt), 1);
    let warning = element(&rt, "data-transfer-unmapped-target-warning");
    let expected =
        t("transfer.mapping.unmappedTargetWarning").replace("{columns}", "orphan");
    assert_eq!(warning.text.as_deref(), Some(expected.as_str()));
    assert_in_window(&rt, DESIGN.0, DESIGN.1, "empty shell");
}

// ---------------------------------------------------------------------------
// Mounting: one kit-root, merged overlay layers, header placement
// ---------------------------------------------------------------------------

#[test]
fn five_rows_mount_under_one_kit_root_with_merged_overlay_layers() {
    let mut rt = runtime(seeded_with(wide_table(), false, false), DESIGN);
    assert_eq!(kit_roots(&rt), 1, "one root for five rows");
    for source in ["id", "name", "email", "phone", "age"] {
        assert!(
            has_key(&rt, &format!("data-transfer-column-row-{source}")),
            "row {source} is laid out"
        );
    }
    assert!(option_keys(&rt).is_empty(), "no listbox while all closed");

    // Open the middle row's select: exactly one listbox's options appear.
    click(&mut rt, "data-transfer-target-select-phone-field-trigger");
    assert!(rt.state.editor.rows[3].target_select_open);
    let options = option_keys(&rt);
    assert_eq!(
        options,
        vec![
            "option0".to_owned(),
            "option1".to_owned(),
            "option2".to_owned(),
            "option3".to_owned(),
            "option4".to_owned(),
        ],
        "one listbox: pick + id/name/email/extra, no second layer's copy"
    );
    // option1 is "id": choosing it maps phone -> id and closes the listbox.
    click(&mut rt, "option1");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0]
        .column_mappings
        .clone()
        .expect("choosing commits columnMappings");
    assert_eq!(next[3], mapping("phone", "id", false));
    assert!(!rt.state.editor.rows[3].target_select_open);
    assert!(option_keys(&rt).is_empty(), "the listbox is gone");
    assert!(
        !rt.state.editor.rows[0].target_select_open,
        "choosing in one row never opens another"
    );
}

#[test]
fn choosing_a_row_target_then_escaping_another_commits_once() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    click(&mut rt, "option2");
    assert_eq!(rt.state.patches.len(), 1);
    assert!(!rt.state.editor.rows[0].target_select_open);
    assert!(option_keys(&rt).is_empty(), "choosing closed the listbox");
    click(&mut rt, "data-transfer-target-select-email-field-trigger");
    assert!(rt.state.editor.rows[2].target_select_open);
    press(&mut rt, Key::Escape);
    assert!(!rt.state.editor.rows[2].target_select_open);
    assert_eq!(
        rt.state.patches.len(),
        1,
        "Escape closes without committing"
    );
    assert!(option_keys(&rt).is_empty());
}

#[test]
fn the_header_sits_above_the_rows_scroll_container() {
    let rt = runtime(seeded(), DESIGN);
    let table_el = element(&rt, "data-transfer-column-table");
    let header = element(&rt, "data-transfer-column-header");
    let rows = element(&rt, "data-transfer-column-rows");
    let first_row = element(&rt, "data-transfer-column-row-id");
    // Sticky by construction: the header is a sibling above the scroll
    // container, not a child inside it — so it never scrolls away.
    assert!(
        (header.rect[1] - table_el.rect[1]).abs() <= 1.0,
        "header starts at the table top: header {:?} table {:?}",
        header.rect,
        table_el.rect
    );
    assert!(
        (header.rect[2] - table_el.rect[2]).abs() <= 0.5,
        "header spans the table width"
    );
    assert!(
        header.rect[1] + header.rect[3] <= rows.rect[1] + 0.5,
        "header ends where the rows container begins: header {:?} rows {:?}",
        header.rect,
        rows.rect
    );
    assert!(
        rows.rect[1] <= first_row.rect[1] + 0.5,
        "first row is inside the rows container"
    );
}

#[test]
fn every_key_is_unique_among_siblings_with_three_rows_closed() {
    // The kit reuses "value"/"input"/"chevron"/"control"/"label"/"mark"
    // keys in different subtrees (one per control), so the contract is
    // uniqueness among siblings, not global — the S1 audit pins the same
    // rule for one row; the shell must keep it with three rows mounted.
    use std::collections::HashMap;
    let rt = runtime(seeded(), DESIGN);
    let mut by_parent: HashMap<String, Vec<String>> = HashMap::new();
    for e in rt.elements() {
        if let Some(key) = e.key {
            let parent = match e.path.rfind('/') {
                Some(i) => e.path[..i].to_owned(),
                None => String::new(),
            };
            by_parent.entry(parent).or_default().push(key);
        }
    }
    for (parent, keys) in &by_parent {
        let mut sorted = keys.clone();
        sorted.sort();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            before,
            "duplicate keys under parent {parent}: {keys:?}"
        );
    }
    // And the shell's own row root keys are distinct among the rows
    // siblings: exactly one element per source-named row key.
    for source in ["id", "name", "email"] {
        let key = format!("data-transfer-column-row-{source}");
        let count = rt
            .elements()
            .iter()
            .filter(|e| e.key.as_deref() == Some(key.as_str()))
            .count();
        assert_eq!(count, 1, "exactly one element carries {key}");
    }
}

#[test]
fn structure_mode_without_create_new_shows_the_toggle_but_no_type_cells() {
    let rt = runtime(seeded_with(table(), true, false), DESIGN);
    assert!(has_key(&rt, "data-transfer-create-new-toggle"));
    assert!(!has_key(&rt, "data-transfer-column-row-id-source-type"));
    assert!(!has_key(&rt, "data-transfer-column-row-id-target-type"));
}

// ---------------------------------------------------------------------------
// Disabled + geometry
// ---------------------------------------------------------------------------

#[test]
fn a_disabled_editor_never_opens_a_row_select() {
    let mut rt = runtime(seeded_with(table(), false, true), DESIGN);
    assert!(has_key(&rt, "data-transfer-target-select-id-field-trigger"));
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    assert!(!rt.state.editor.rows[0].target_select_open);
    assert!(option_keys(&rt).is_empty(), "no listbox from a disabled row");
    assert!(rt.state.patches.is_empty());
}

#[test]
fn five_rows_stay_in_window_at_min_size_in_light_theme_structure_mode() {
    let mut probe = seeded_with(wide_table(), true, false);
    probe.dark = false;
    probe.editor.table.create_new = true;
    probe.editor.resync_rows();
    let rt = runtime(probe, MIN);
    assert_in_window(&rt, MIN.0, MIN.1, "five rows, light, min, structure create-new");
    assert_eq!(kit_roots(&rt), 1);
    assert!(has_key(&rt, "data-transfer-create-new-toggle"));
}
