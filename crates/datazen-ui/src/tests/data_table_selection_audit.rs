//! Independent tester audit for T007 (`DataTableSelectionBar.tsx` → Rune).
//!
//! Coverage-driven: every assertion here reads the rendered element tree or
//! the accessibility dump, never the component's own state (*the* failure mode
//! this migration track has been bitten by before: "tests green ≠ drawn right").
//! Nothing in the coder's `data_table_selection_bar.rs` test file is weakened,
//! deleted or skipped — these are additional probes over untested branches.

use std::collections::HashSet;

use rune::{col, text, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::data_table::data_table_selection_bar::{
    DataTableSelectionBar, GridSelection, GridSelectionMode,
};
use crate::i18n::{has, t};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn reframe<S>(rt: &mut Runtime<S>) {
    rt.invalidate();
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn focused_key<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

#[derive(Clone)]
struct AuditProbe {
    dark: bool,
    has_selection: bool,
    has_cell_selection: bool,
    mode: GridSelectionMode,
    selected_rows: Vec<usize>,
    row_count: usize,
    covered_rows: Vec<usize>,
    covered_cols: usize,
    covered_cells: usize,
    with_delete: bool,
    delete_rows: Vec<usize>,
    delete_count: usize,
    pk_columns: Vec<String>,
    export_enabled: bool,
    loading: bool,
    with_header_action: bool,
    select_all_calls: usize,
    delete_calls: usize,
    export_calls: usize,
    deleted: Vec<usize>,
}

impl Default for AuditProbe {
    fn default() -> Self {
        AuditProbe {
            dark: true,
            has_selection: true,
            has_cell_selection: false,
            mode: GridSelectionMode::Row,
            selected_rows: Vec::new(),
            row_count: 10,
            covered_rows: Vec::new(),
            covered_cols: 0,
            covered_cells: 0,
            with_delete: true,
            delete_rows: vec![0, 1],
            delete_count: 2,
            pk_columns: vec!["id".to_owned()],
            export_enabled: false,
            loading: false,
            with_header_action: false,
            select_all_calls: 0,
            delete_calls: 0,
            export_calls: 0,
            deleted: Vec::new(),
        }
    }
}

fn build_bar(state: &AuditProbe) -> DataTableSelectionBar<AuditProbe> {
    let mut bar =
        DataTableSelectionBar::<AuditProbe>::new(GridSelection::new(state.mode), state.row_count)
            .has_selection(state.has_selection)
            .has_cell_selection(state.has_cell_selection)
            .selected_rows(state.selected_rows.clone())
            .covered_rows(state.covered_rows.clone())
            .covered_columns(state.covered_cols)
            .covered_cells(state.covered_cells)
            .delete_row_indices(state.delete_rows.clone())
            .delete_row_count(state.delete_count)
            .primary_key_columns(state.pk_columns.clone())
            .export_enabled(state.export_enabled)
            .loading(state.loading)
            .on_select_all(|s: &mut AuditProbe| {
                s.select_all_calls += 1;
                s.selected_rows = if s.selected_rows.len() == s.row_count {
                    Vec::new()
                } else {
                    (0..s.row_count).collect()
                };
            })
            .on_export(|s: &mut AuditProbe| s.export_calls += 1);
    if state.with_delete {
        bar = bar.on_delete_rows(|s: &mut AuditProbe, rows| {
            s.delete_calls += 1;
            s.deleted = rows;
        });
    }
    if state.with_header_action {
        bar = bar.header_actions(text(t("common.save")).key("audit-header-action"));
    }
    bar
}

fn audit_view(state: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    rune_kit::root(
        col().w_full().padding(16.0).child(build_bar(state).build(state.dark)),
        Vec::new(),
    )
}

/// Same bar, but with the app-wide `web_focus()` input semantics turned on so we
/// can prove Enter is wired to the control (DataZen itself currently leaves it
/// off — a documented, app-wide keyboard-navigation gap, not a T007 regression).
fn audit_view_web_focus(state: &AuditProbe, cx: &mut Cx<AuditProbe>) -> El {
    cx.web_focus();
    audit_view(state, cx)
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

// ---- 1. select-all: Tab reaches it, Space toggles, Enter is wired ---------------------------

#[test]
fn select_all_is_reachable_by_tab_and_space_toggles_it_exactly_once() {
    let mut rt = Runtime::new(AuditProbe::default(), audit_view, (960.0, 640.0), env(true));
    reframe(&mut rt);

    // The control is an explicit focus stop — the regression this guards against
    // is a bare, non-focusable row silently taking the checkbox's place.
    assert!(
        element(&rt, "data-table-select-all").focusable,
        "select-all must be focusable"
    );
    press(&mut rt, Key::Tab);
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("data-table-select-all"),
        "Tab reaches the select-all checkbox"
    );

    press(&mut rt, Key::Text(" ".into()));
    assert_eq!(rt.state.select_all_calls, 1, "Space fires the toggle once");
    assert_eq!(rt.state.selected_rows.len(), 10, "none -> all");
    reframe(&mut rt);

    press(&mut rt, Key::Text(" ".into()));
    assert_eq!(rt.state.select_all_calls, 2, "a second Space fires exactly once more");
    assert!(rt.state.selected_rows.is_empty(), "all -> none");
}

#[test]
fn select_all_also_takes_enter_under_web_focus_semantics() {
    // Proves the kit checkbox's activation is not Space-only at the control
    // level; Enter reaches it once the app enables the (global) web focus mode.
    let mut rt = Runtime::new(
        AuditProbe::default(),
        audit_view_web_focus,
        (960.0, 640.0),
        env(true),
    );
    reframe(&mut rt);
    press(&mut rt, Key::Tab);
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("data-table-select-all"),
        "Tab reaches the checkbox under web_focus too"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.select_all_calls, 1,
        "Enter presses the focused checkbox when web_focus is enabled"
    );
}

// ---- 2. cell summary: placeholders fully substituted, no raw braces ------------------------

fn count_probe<S>(rt: &Runtime<S>, prefix: &str) -> Option<usize> {
    rt.elements()
        .iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key.as_deref())
        .find_map(|key| key.strip_prefix(prefix)?.parse().ok())
}

#[test]
fn cell_summary_substitutes_every_placeholder_and_shows_no_raw_braces() {
    let mut rt = Runtime::new(
        AuditProbe {
            has_cell_selection: true,
            mode: GridSelectionMode::Cell,
            covered_rows: vec![0, 3, 7],
            covered_cols: 5,
            covered_cells: 9,
            ..AuditProbe::default()
        },
        audit_view,
        (960.0, 640.0),
        env(true),
    );
    reframe(&mut rt);

    let rendered = element(&rt, "data-table-selection-summary-text")
        .text
        .unwrap_or_default();
    let template = t("dataTable.selectionSummary");
    assert_ne!(rendered, template, "the raw template must not reach the screen");
    assert!(!rendered.contains('{') && !rendered.contains('}'), "no raw placeholder braces: {rendered:?}");
    assert!(rendered.contains('3') && rendered.contains('5'), "both counts substituted: {rendered:?}");
    assert_eq!(count_probe(&rt, "data-table-selection-summary-rows-"), Some(3));
    assert_eq!(count_probe(&rt, "data-table-selection-summary-cols-"), Some(5));
    assert_eq!(count_probe(&rt, "data-table-selection-summary-cells-"), Some(9));
}

// ---- 3. i18n: keys resolve to localized text, never echo back as the key --------------------

#[test]
fn all_seven_keys_resolve_to_localized_text_not_key_echoes() {
    for key in [
        "common.selectAll",
        "dataTable.selected",
        "common.rows",
        "dataTable.selectionSummary",
        "dataTable.deleteRow",
        "export.export",
        "common.loading",
    ] {
        assert!(has(key), "{key} is absent from the locale pack");
        let value = t(key);
        assert_ne!(value, key, "{key} echoed back instead of being translated");
        assert!(has_cjk(value), "{key} resolved to non-localized text: {value:?}");
    }
}

// ---- 4. palette discipline: no raw oklch literals in the component --------------------------

#[test]
fn the_component_uses_palette_tokens_not_raw_oklch_colors() {
    let source = include_str!("../components/data_table/data_table_selection_bar.rs");
    for literal in ["oklch(", "oklcha("] {
        assert!(
            !source.contains(literal),
            "selection bar must not hard-code {literal} colors"
        );
    }
    // The counted colours all come from the kit palette / type tokens.
    for token in ["c.text_faint", "c.danger.fg", "c.border_focus", "c.surface_raised", "c.border_weak", "c.layer_01", "k.ty.small"] {
        assert!(source.contains(token), "expected palette token {token} to be used");
    }
}

// ---- 5. stable / unique element keys -------------------------------------------------------
// `ElementInfo::key` is a *local* (parent-scoped) key in Rune; the full identity
// is `ElementInfo::path`. Two children under different parents may both be
// keyed "label" (kit Checkbox content, hand-built delete row, kit export
// Button). The real invariant is: keys are unique among siblings and paths are
// globally unique / stable across identical rebuilds.

#[test]
fn element_keys_are_unique_among_siblings_and_paths_are_stable() {
    let mut rt = Runtime::new(
        AuditProbe {
            has_cell_selection: true,
            mode: GridSelectionMode::Cell,
            selected_rows: (0..10).collect(),
            covered_rows: (0..12).collect(),
            covered_cols: 24,
            covered_cells: 288,
            export_enabled: true,
            loading: true,
            with_header_action: true,
            ..AuditProbe::default()
        },
        audit_view,
        (1440.0, 816.0),
        env(true),
    );
    reframe(&mut rt);

    let live: Vec<rune::ElementInfo> = rt.elements().into_iter().filter(|e| !e.exiting).collect();

    // Full paths are the runtime's identity: they must be globally unique.
    let mut paths = HashSet::new();
    let mut dup_paths = Vec::new();
    for e in &live {
        if !paths.insert(e.path.clone()) {
            dup_paths.push(e.path.clone());
        }
    }
    assert!(dup_paths.is_empty(), "duplicate element paths: {dup_paths:?}");
    assert!(paths.len() > 10, "expected a well-populated bar, got {} paths", paths.len());

    // Keys are scoped to their parent: (parent path, key) must be unique.
    let mut siblings = HashSet::new();
    let mut dup_siblings = Vec::new();
    let mut keyed = 0usize;
    for e in &live {
        if let Some(key) = e.key.as_deref() {
            keyed += 1;
            let parent = e.path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
            if !siblings.insert((parent.to_owned(), key.to_owned())) {
                dup_siblings.push(format!("{parent}/{key}"));
            }
        }
    }
    assert!(keyed > 10, "expected a well-keyed bar, found only {keyed} keys");
    assert!(dup_siblings.is_empty(), "duplicate sibling keys: {dup_siblings:?}");

    // Rebuild the identical bar and require the same keyed paths: keys are
    // stable, not regenerated per frame.
    let mut rt2 = Runtime::new(
        AuditProbe {
            has_cell_selection: true,
            mode: GridSelectionMode::Cell,
            selected_rows: (0..10).collect(),
            covered_rows: (0..12).collect(),
            covered_cols: 24,
            covered_cells: 288,
            export_enabled: true,
            loading: true,
            with_header_action: true,
            ..AuditProbe::default()
        },
        audit_view,
        (1440.0, 816.0),
        env(true),
    );
    reframe(&mut rt2);
    let paths2: HashSet<String> = rt2.elements().into_iter().filter(|e| !e.exiting).map(|e| e.path).collect();
    assert_eq!(paths, paths2, "identical rebuilds must produce identical element paths");
}

// ---- 6. geometry: the actions cluster is actually right-aligned -----------------------------

#[test]
fn the_grow_spacer_right_aligns_the_actions_cluster() {
    let mut rt = Runtime::new(
        AuditProbe {
            selected_rows: (0..10).collect(),
            has_cell_selection: true,
            mode: GridSelectionMode::Cell,
            covered_rows: (0..12).collect(),
            covered_cols: 24,
            covered_cells: 288,
            export_enabled: true,
            loading: true,
            with_header_action: true,
            ..AuditProbe::default()
        },
        audit_view,
        (960.0, 640.0),
        env(true),
    );
    reframe(&mut rt);

    let spacer = element(&rt, "data-table-selection-bar-spacer");
    assert!(spacer.rect[2] > 0.0, "the ml-auto grow spacer must consume width: {:?}", spacer.rect);

    let export = element(&rt, "data-table-export");
    let w = 960.0;
    assert!(
        export.rect[0] > w * 0.5,
        "export must sit in the right half (actions right-aligned): {:?}",
        export.rect
    );
    let delete = element(&rt, "data-table-delete-rows");
    assert!(
        delete.rect[0] < export.rect[0],
        "delete precedes export inside the right-aligned cluster"
    );
}

// ---- 7. absence branches -------------------------------------------------------------------

#[test]
fn the_header_slot_is_absent_when_not_provided_and_every_gate_stays_independent() {
    let mut rt = Runtime::new(AuditProbe::default(), audit_view, (960.0, 640.0), env(true));
    reframe(&mut rt);
    assert!(!has_key(&rt, "audit-header-action"), "no slot unless supplied");
    assert!(!has_key(&rt, "data-table-export"), "export hidden by default");
    assert!(!has_key(&rt, "data-table-loading"), "no loading span by default");

    // Flip each delete gate on its own from a fully-on baseline.
    rt.state.with_delete = false;
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-delete-rows"), "no handler -> hidden");

    rt.state.with_delete = true;
    rt.state.delete_count = 0;
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-delete-rows"), "count 0 -> hidden");

    rt.state.delete_count = 2;
    rt.state.pk_columns.clear();
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-delete-rows"), "no PK -> hidden");

    rt.state.pk_columns = vec!["id".to_owned()];
    reframe(&mut rt);
    assert!(has_key(&rt, "data-table-delete-rows"), "all three restored -> shown");
}
