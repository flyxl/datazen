//! T267: `windows/data-transfer/TransferMappingStep.tsx` port — the
//! enabled-table filter, the selected-row derivation and the effect's
//! correction query (the pure helpers), the seed (mode + disabled into
//! the editor state), the empty notice, the sidebar's per-item keys and
//! two-line text, the fallback render for a stale selection, and the
//! select / update / commit channels through real presses and the
//! editor's target-table submit.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use datazen_ui::views::{
    seed_editor_state, selected_table, selection_correction, structure_mode, enabled_tables,
    ColumnMappingTablePatch, TransferColumnMapping, TransferMappingStatus, TransferMappingStep,
    TransferMode, TransferTableVm, ColumnMappingEditorState,
};

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

fn table(source: &str, enabled: bool) -> TransferTableVm {
    TransferTableVm {
        source_table: source.to_owned(),
        target_table: format!("tgt_{source}"),
        status: TransferMappingStatus::Matched,
        create_new: false,
        enabled,
        column_mappings: vec![
            mapping("id", "id", false),
            mapping("name", "", true),
        ],
        source_columns: Some(vec!["id".to_owned(), "name".to_owned()]),
        target_columns: Some(vec!["id".to_owned(), "name".to_owned()]),
        source_column_types: vec![
            ("id".to_owned(), "INTEGER".to_owned()),
            ("name".to_owned(), "TEXT".to_owned()),
        ],
        source_primary_keys: None,
        recordset: None,
    }
}

#[derive(Clone)]
struct Probe {
    tables: Vec<TransferTableVm>,
    selected: String,
    editor: ColumnMappingEditorState,
    dark: bool,
    selects: Vec<String>,
    patches: Vec<(String, ColumnMappingTablePatch)>,
    commits: Vec<String>,
}

fn probe(tables: Vec<TransferTableVm>, selected: &str) -> Probe {
    // The editor state seeds for the selected table; an empty or
    // all-disabled list renders the notice and never reads the seed,
    // so its placeholder is inert.
    let editor = match selected_table(&tables, selected) {
        Some(selected_vm) => seed_editor_state(selected_vm, TransferMode::Data, false),
        None => seed_editor_state(&table("_unused_", true), TransferMode::Data, false),
    };
    Probe {
        editor,
        tables,
        selected: selected.to_owned(),
        dark: true,
        selects: Vec::new(),
        patches: Vec::new(),
        commits: Vec::new(),
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let el = TransferMappingStep::<Probe>::new(&s.tables, &s.selected, &s.editor)
        .editor_mut(|st: &mut Probe| &mut st.editor)
        .on_select_table(|st: &mut Probe, source| st.selects.push(source))
        .on_update_table(|st: &mut Probe, source, patch| {
            st.editor.apply_table_patch(&patch);
            st.patches.push((source, patch));
        })
        .on_target_table_commit(|st: &mut Probe, source| st.commits.push(source))
        .build(s.dark, &mut layer);
    rune_kit::root(el, layer)
}

fn view_unwired(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let el = TransferMappingStep::<Probe>::new(&s.tables, &s.selected, &s.editor)
        .build(s.dark, &mut layer);
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
    let mut rt = Runtime::new(probe, view_unwired, size, env(dark));
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

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
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

fn item_key(source: &str) -> String {
    format!("data-transfer-mapping-table-item-{source}")
}

// -- the pure helpers (React 32-43) ----------------------------------------

#[test]
fn the_enabled_filter_and_the_selected_derivation() {
    let tables = vec![
        table("users", true),
        table("ghost", false),
        TransferTableVm {
            source_table: String::new(),
            enabled: true,
            ..table("unused", true)
        },
        table("orders", true),
    ];
    let enabled = enabled_tables(&tables);
    assert_eq!(
        enabled.iter().map(|t| t.source_table.as_str()).collect::<Vec<_>>(),
        vec!["users", "orders"],
        "disabled and sourceless rows leave the universe"
    );

    // The find-else-first derivation (33-34).
    assert_eq!(selected_table(&tables, "orders").map(|t| t.source_table.clone()),
        Some("orders".to_owned()));
    assert_eq!(selected_table(&tables, "ghost").map(|t| t.source_table.clone()),
        Some("users".to_owned()), "a disabled selection falls back to the first enabled");
    assert_eq!(selected_table(&tables, "").map(|t| t.source_table.clone()),
        Some("users".to_owned()), "an empty selection falls back to the first enabled");
    assert!(selected_table(&[table("ghost", false)], "users").is_none(),
        "an all-disabled list selects nothing");
}

#[test]
fn the_correction_query_pins_the_effect_rule() {
    // React 36-41: fires only when the list is non-empty and the
    // selection is not among the enabled rows.
    let tables = vec![table("users", true), table("ghost", false), table("orders", true)];
    assert_eq!(selection_correction(&tables, "orders"), None, "a live selection stays");
    assert_eq!(selection_correction(&tables, "ghost"), Some("users".to_owned()),
        "a disabled selection corrects to the first enabled");
    assert_eq!(selection_correction(&tables, ""), Some("users".to_owned()),
        "the initial empty selection corrects");
    assert_eq!(selection_correction(&[], "users"), None, "an empty list never fires");
}

#[test]
fn structure_mode_is_the_react_disjunction() {
    assert!(structure_mode(TransferMode::Structure));
    assert!(structure_mode(TransferMode::StructureAndData));
    assert!(!structure_mode(TransferMode::Data));
}

#[test]
fn the_seed_carries_the_mode_and_disabled_into_the_editor_state() {
    let users = table("users", true);
    let state = seed_editor_state(&users, TransferMode::Structure, true);
    assert!(state.structure_mode, "structureAndData/structure set the flag");
    assert!(state.disabled, "disabled is the editor's own inertness");
    assert_eq!(state.table.source_table, "users");
    let state = seed_editor_state(&users, TransferMode::Data, false);
    assert!(!state.structure_mode);
    assert!(!state.disabled);
}

// -- the render --------------------------------------------------------------

#[test]
fn an_empty_enabled_list_renders_only_the_notice() {
    let rt = runtime(probe(Vec::new(), ""), (960.0, 640.0));
    assert!(has_key(&rt, "data-transfer-mapping-empty"));
    assert_eq!(
        text_of(&rt, "data-transfer-mapping-empty"),
        datazen_ui::i18n::t("transfer.mapping.noTables")
    );
    assert!(!has_key(&rt, "data-transfer-mapping-sidebar"));
    assert!(!has_key(&rt, "data-transfer-column-editor"));

    // An all-disabled list is the same empty state.
    let rt = runtime(probe(vec![table("ghost", false)], "ghost"), (960.0, 640.0));
    assert!(has_key(&rt, "data-transfer-mapping-empty"));
    assert!(!has_key(&rt, &item_key("ghost")));
}

#[test]
fn the_sidebar_lists_every_enabled_table_with_its_two_lines() {
    let tables = vec![table("users", true), table("orders", true)];
    let rt = runtime(probe(tables, "users"), (960.0, 640.0));

    assert!(has_key(&rt, "data-transfer-mapping-step"));
    assert!(has_key(&rt, "data-transfer-mapping-sidebar"));
    assert_eq!(
        text_of(&rt, "data-transfer-mapping-sidebar-header-label"),
        datazen_ui::i18n::t("transfer.mapping.tables").to_uppercase(),
        "the header label is the tables label, uppercased"
    );
    for (source, target) in [("users", "tgt_users"), ("orders", "tgt_orders")] {
        let base = item_key(source);
        assert!(has_key(&rt, &base), "{source} rides the per-item key");
        assert_eq!(text_of(&rt, &format!("{base}-name")), source);
        assert_eq!(text_of(&rt, &format!("{base}-target")), format!("→ {target}"));
    }
    // The disabled row never mounts.
    let tables = vec![table("users", true), table("ghost", false)];
    let rt = runtime(probe(tables, "users"), (960.0, 640.0));
    assert!(!has_key(&rt, &item_key("ghost")));
}

#[test]
fn an_empty_target_renders_the_dash() {
    let mut users = table("users", true);
    users.target_table = String::new();
    let rt = runtime(probe(vec![users], "users"), (960.0, 640.0));
    assert_eq!(
        text_of(&rt, &format!("{}-target", item_key("users"))),
        "→ —",
        "JS's `targetTable || '—'`"
    );
}

#[test]
fn the_editor_mounts_over_the_selected_table() {
    // The unwired mount shows the model value as plain text under the
    // input's key (the shell's own unwired ruling): the seeded editor
    // renders the selected table's target.
    let tables = vec![table("users", true), table("orders", true)];
    let rt = runtime_unwired(probe(tables, "orders"), (960.0, 640.0));
    assert!(has_key(&rt, "data-transfer-column-editor"),
        "the embedded shell mounts under the step");
    assert_eq!(
        text_of(&rt, "data-transfer-target-table-input"),
        "tgt_orders",
        "the seed follows the selection (React's key remount trade)"
    );
}

#[test]
fn exactly_one_kit_root_mounts_the_whole_step() {
    let tables = vec![table("users", true), table("orders", true)];
    let rt = runtime(probe(tables, "users"), (960.0, 640.0));
    assert_eq!(kit_roots(&rt), 1, "the step roots once (the mounting ruling)");
}

// -- the journeys -------------------------------------------------------------

#[test]
fn presses_fire_on_select_table_with_the_pressed_source() {
    let tables = vec![table("users", true), table("orders", true)];
    let mut rt = runtime(probe(tables, "users"), (960.0, 640.0));

    click(&mut rt, &item_key("orders"));
    assert_eq!(rt.state.selects, vec!["orders".to_owned()]);

    // The already-selected item re-fires: React's button has no guard.
    click(&mut rt, &item_key("users"));
    assert_eq!(rt.state.selects, vec!["orders".to_owned(), "users".to_owned()]);
}

#[test]
fn the_target_submit_wraps_both_channels_with_the_selected_source() {
    let tables = vec![table("users", true)];
    let mut rt = runtime(probe(tables, "users"), (1440.0, 816.0));

    // The embedded editor's target-table submit (paste + Enter), as
    // the shell's own journey drives it: the caret lands where the
    // click put it, so the patch carries the whole buffer.
    click(&mut rt, "data-transfer-target-table-input");
    rt.paste("users_copy");
    rt.settle_all();
    rt.frame(0.0);
    let typed = rt.state.editor.target_table_edit.display().0.clone();
    assert!(typed.contains("users_copy"), "the paste reached the buffer: {typed:?}");
    press(&mut rt, Key::Enter);

    assert_eq!(rt.state.patches.len(), 1, "one patch through the step");
    let (source, patch) = &rt.state.patches[0];
    assert_eq!(source, "users", "React 83: onUpdateTable(selected.sourceTable, patch)");
    assert_eq!(patch.target_table.as_deref(), Some(typed.as_str()));
    assert_eq!(rt.state.commits, vec!["users".to_owned()],
        "React 84: onTargetTableCommit(selected.sourceTable)");
}

#[test]
fn the_unwired_step_renders_without_driving_any_channel() {
    let tables = vec![table("users", true)];
    let mut rt = runtime_unwired(probe(tables, "users"), (960.0, 640.0));
    assert!(has_key(&rt, "data-transfer-mapping-sidebar"));
    assert!(has_key(&rt, &item_key("users")));
    // No callback is wired: the press lands on an inert element and
    // nothing records.
    click(&mut rt, &item_key("users"));
    assert!(rt.state.selects.is_empty());
    assert!(rt.state.patches.is_empty());
    assert!(rt.state.commits.is_empty());
}
