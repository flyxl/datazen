//! T007: real probes for the `DataTableSelectionBar.tsx` port.
//!
//! Each probe mounts the bar on its own in a padded page, the way the view
//! will. Assertions read the element tree / accessibility dump (never the
//! component's own state) for the checkbox's toggled state, the two summary
//! texts, the three count probes, every button gate, the header slot and the
//! loading text; the last test walks every laid-out element at both window
//! sizes to prove there is no horizontal overflow.

use rune::{col, text, Cx, El, Env, Runtime};

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

/// A direct `rt.state` edit happens outside a handler, so it needs `invalidate()`
/// for the next frame to re-run the view (`Runtime::invalidate` docs).
fn reframe<S>(rt: &mut Runtime<S>) {
    rt.invalidate();
    rt.frame(1.0 / 60.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

// Rune keeps removed nodes listed for their exit transition (`ElementInfo::exiting`),
// so a presence assertion must look at the live tree only — the same pattern the
// other component probes use.
fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn find_json<'a>(node: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    if node.get("key").and_then(|v| v.as_str()) == Some(key) {
        return Some(node);
    }
    node.get("children")?
        .as_array()?
        .iter()
        .find_map(|child| find_json(child, key))
}

/// The kit Checkbox writes `aria-checked` (`checked`/`unchecked`/`mixed`)
/// through `El::checked` / `checked_mixed`; the a11y dump reports it as a
/// `states` array. Reading it here keeps the assertion on the rendered tree.
fn a11y_states<S>(rt: &Runtime<S>, key: &str) -> Vec<String> {
    let json = rt.accessibility_json();
    let node = find_json(&json, key)
        .unwrap_or_else(|| panic!("{key} is in the accessibility tree: {json:#}"));
    node.get("states")
        .and_then(|v| v.as_array())
        .map(|array| {
            array
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn is_state<S>(rt: &Runtime<S>, key: &str, state: &str) -> bool {
    a11y_states(rt, key).iter().any(|s| s == state)
}

/// Reads the trailing integer off a key-encoded count probe. React exposes
/// these as `data-dt-selection-*`; Rune encodes them in the key (module docs).
fn count_probe<S>(rt: &Runtime<S>, prefix: &str) -> Option<usize> {
    rt.elements()
        .iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key.as_deref())
        .find_map(|key| key.strip_prefix(prefix)?.parse().ok())
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, context: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let [x, y, width, height] = e.rect;
        if x < -0.5 || y < -0.5 || x + width > w + 0.5 || y + height > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), e.rect));
        }
    }
    assert!(
        outside.is_empty(),
        "{context}: {} node(s) cross the {w}x{h} window: {outside:?}",
        outside.len()
    );
}

#[derive(Clone)]
struct Probe {
    dark: bool,
    has_selection: bool,
    has_cell_selection: bool,
    mode: GridSelectionMode,
    selected_rows: Vec<usize>,
    row_count: usize,
    covered_rows: Vec<usize>,
    covered_cols: usize,
    covered_cells: usize,
    with_delete_handler: bool,
    delete_rows: Vec<usize>,
    delete_count: usize,
    pk_columns: Vec<String>,
    export_enabled: bool,
    loading: bool,
    with_header_action: bool,
    select_all_calls: usize,
    deleted: Vec<usize>,
    delete_calls: usize,
    export_calls: usize,
}

impl Default for Probe {
    fn default() -> Self {
        Probe {
            dark: true,
            has_selection: true,
            has_cell_selection: false,
            mode: GridSelectionMode::Row,
            selected_rows: Vec::new(),
            row_count: 10,
            covered_rows: Vec::new(),
            covered_cols: 0,
            covered_cells: 0,
            with_delete_handler: true,
            delete_rows: vec![0, 1],
            delete_count: 2,
            pk_columns: vec!["id".to_owned()],
            export_enabled: false,
            loading: false,
            with_header_action: false,
            select_all_calls: 0,
            deleted: Vec::new(),
            delete_calls: 0,
            export_calls: 0,
        }
    }
}

fn probe_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut bar =
        DataTableSelectionBar::<Probe>::new(GridSelection::new(state.mode), state.row_count)
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
            .on_select_all(|s: &mut Probe| {
                s.select_all_calls += 1;
                s.selected_rows = if s.selected_rows.len() == s.row_count {
                    Vec::new()
                } else {
                    (0..s.row_count).collect()
                };
            })
            .on_export(|s: &mut Probe| s.export_calls += 1);
    if state.with_delete_handler {
        bar = bar.on_delete_rows(|s: &mut Probe, rows| {
            s.delete_calls += 1;
            s.deleted = rows;
        });
    }
    if state.with_header_action {
        bar = bar.header_actions(text(t("common.save")).key("probe-header-action"));
    }
    rune_kit::root(
        col().w_full().padding(16.0).child(bar.build(state.dark)),
        Vec::new(),
    )
}

// ---- select-all ----------------------------------------------------------------------------

#[test]
fn select_all_checkbox_tracks_count_and_fires_the_toggle_callback() {
    let mut rt = Runtime::new(Probe::default(), probe_view, (960.0, 640.0), env(true));
    reframe(&mut rt);

    assert!(has_key(&rt, "data-table-select-all"));
    assert_eq!(
        element(&rt, "data-table-select-all").label.as_deref(),
        Some(t("common.selectAll")),
        "the React label is the common.selectAll key"
    );
    assert!(
        is_state(&rt, "data-table-select-all", "unchecked"),
        "no rows selected -> unchecked"
    );

    rt.state.selected_rows = vec![0, 1, 2];
    reframe(&mut rt);
    assert!(
        is_state(&rt, "data-table-select-all", "mixed"),
        "0 < 3 < 10 -> indeterminate"
    );
    assert!(!is_state(&rt, "data-table-select-all", "checked"));

    rt.state.selected_rows = (0..10).collect();
    reframe(&mut rt);
    assert!(
        is_state(&rt, "data-table-select-all", "checked"),
        "3 == 10 -> checked"
    );
    assert!(!is_state(&rt, "data-table-select-all", "mixed"));

    // Pressing the checkbox label runs the callback, which flips the set.
    click(&mut rt, "data-table-select-all/content/label");
    assert_eq!(rt.state.select_all_calls, 1, "onSelectAll reached the state");
    assert!(rt.state.selected_rows.is_empty(), "all -> none");
    reframe(&mut rt);
    assert!(is_state(&rt, "data-table-select-all", "unchecked"));

    click(&mut rt, "data-table-select-all/content/label");
    assert_eq!(rt.state.select_all_calls, 2);
    assert_eq!(rt.state.selected_rows.len(), 10, "none -> all");
    reframe(&mut rt);
    assert!(is_state(&rt, "data-table-select-all", "checked"));

    // A zero-row grid is never "checked", even if a stale index is around.
    rt.state.selected_rows.clear();
    rt.state.row_count = 0;
    reframe(&mut rt);
    assert!(is_state(&rt, "data-table-select-all", "unchecked"));
}

// ---- selected summary -----------------------------------------------------------------------

#[test]
fn selected_summary_prints_counts_only_when_selection_exists() {
    let mut rt = Runtime::new(
        Probe {
            selected_rows: vec![2, 5, 7],
            ..Probe::default()
        },
        probe_view,
        (960.0, 640.0),
        env(true),
    );
    reframe(&mut rt);
    let expected = format!("{} 3 / 10 {}", t("dataTable.selected"), t("common.rows"));
    assert_eq!(
        element(&rt, "data-table-selected-summary").text.as_deref(),
        Some(expected.as_str()),
        "React prints the selected count and the row count"
    );

    // hasSelection false hides both the checkbox and the summary.
    rt.state.has_selection = false;
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-selected-summary"));
    assert!(!has_key(&rt, "data-table-select-all"));

    // Contract on but an empty set hides only the summary.
    rt.state.has_selection = true;
    rt.state.selected_rows.clear();
    reframe(&mut rt);
    assert!(has_key(&rt, "data-table-select-all"));
    assert!(!has_key(&rt, "data-table-selected-summary"));
}

// ---- cell-selection summary -----------------------------------------------------------------

#[test]
fn cell_summary_is_gated_by_mode_and_reports_all_three_counts() {
    let mut rt = Runtime::new(
        Probe {
            has_cell_selection: true,
            mode: GridSelectionMode::Cell,
            covered_rows: vec![0, 3],
            covered_cols: 4,
            covered_cells: 8,
            ..Probe::default()
        },
        probe_view,
        (960.0, 640.0),
        env(true),
    );
    reframe(&mut rt);

    let expected = t("dataTable.selectionSummary")
        .replace("{rows}", "2")
        .replace("{cols}", "4");
    assert_eq!(
        element(&rt, "data-table-selection-summary-text")
            .text
            .as_deref(),
        Some(expected.as_str()),
        "`t()` leaves placeholders for the caller to format"
    );
    assert_eq!(count_probe(&rt, "data-table-selection-summary-rows-"), Some(2));
    assert_eq!(count_probe(&rt, "data-table-selection-summary-cols-"), Some(4));
    assert_eq!(count_probe(&rt, "data-table-selection-summary-cells-"), Some(8));

    for key in [
        "data-table-selection-summary-rows-2",
        "data-table-selection-summary-cols-4",
        "data-table-selection-summary-cells-8",
    ] {
        let probe = element(&rt, key);
        assert!(
            probe.rect[2] <= 0.5 && probe.rect[3] <= 0.5,
            "{key} is a zero-size count probe: {:?}",
            probe.rect
        );
    }

    // Row mode hides the span even with the contract on.
    rt.state.mode = GridSelectionMode::Row;
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-selection-summary"));
    assert_eq!(count_probe(&rt, "data-table-selection-summary-rows-"), None);

    // Cell mode but contract off hides it too.
    rt.state.mode = GridSelectionMode::Cell;
    rt.state.has_cell_selection = false;
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-selection-summary"));

    // An empty cover reports zero rows, not a missing probe.
    rt.state.has_cell_selection = true;
    rt.state.covered_rows.clear();
    reframe(&mut rt);
    assert_eq!(count_probe(&rt, "data-table-selection-summary-rows-"), Some(0));
}

// ---- delete / export -----------------------------------------------------------------------

#[test]
fn delete_button_requires_the_handler_the_count_and_a_primary_key() {
    let mut rt = Runtime::new(Probe::default(), probe_view, (960.0, 640.0), env(true));
    reframe(&mut rt);

    let button = element(&rt, "data-table-delete-rows");
    assert_eq!(button.role, "button");
    assert_eq!(
        button.label.as_deref(),
        Some(t("dataTable.deleteRow")),
        "the accessible label is the React title/label key"
    );
    assert_eq!(
        button.help.as_deref(),
        Some(t("dataTable.deleteRow")),
        "React's `title` maps to the tooltip/help"
    );
    assert!(button.focusable, "the delete button is a real button");
    assert!(has_key(&rt, "data-table-delete-rows-icon"));
    assert_eq!(element(&rt, "data-table-delete-rows-icon").role, "icon");

    click(&mut rt, "data-table-delete-rows");
    assert_eq!(rt.state.delete_calls, 1, "onClick reached the handler");
    assert_eq!(rt.state.deleted, vec![0, 1], "deleteRowIndices are passed through");

    // Gate 1: no onDeleteRows handler.
    rt.state.with_delete_handler = false;
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-delete-rows"));

    // Gate 2: deleteRowCount == 0.
    rt.state.with_delete_handler = true;
    rt.state.delete_count = 0;
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-delete-rows"));

    // Gate 3: no primary-key columns.
    rt.state.delete_count = 2;
    rt.state.pk_columns.clear();
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-delete-rows"));

    // All three restored: back.
    rt.state.pk_columns = vec!["id".to_owned()];
    reframe(&mut rt);
    assert!(has_key(&rt, "data-table-delete-rows"));
}

#[test]
fn export_button_is_gated_by_export_enabled_and_fires_on_export() {
    let mut rt = Runtime::new(
        Probe {
            export_enabled: true,
            ..Probe::default()
        },
        probe_view,
        (960.0, 640.0),
        env(true),
    );
    reframe(&mut rt);

    let button = element(&rt, "data-table-export");
    assert_eq!(button.role, "button");
    assert_eq!(button.label.as_deref(), Some(t("export.export")));
    assert_eq!(button.help.as_deref(), Some(t("export.export")));
    assert!(button.focusable);

    click(&mut rt, "data-table-export");
    assert_eq!(rt.state.export_calls, 1, "onExport reached the state");

    rt.state.export_enabled = false;
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-export"));
}

#[test]
fn icons_come_from_the_shared_registry() {
    let source = include_str!("../components/data_table/data_table_selection_bar.rs");
    let icons = include_str!("../icons.rs");
    assert!(source.contains("ICON_TRASH"), "delete reuses the registry trash glyph");
    assert!(
        source.contains("ICON_DOWNLOAD"),
        "export reuses the registry download glyph"
    );
    assert!(
        icons.contains("pub(crate) static ICON_TRASH: rune::Icon"),
        "ICON_TRASH still lives in icons.rs"
    );
    assert!(
        icons.contains("pub(crate) static ICON_DOWNLOAD: rune::Icon"),
        "ICON_DOWNLOAD still lives in icons.rs"
    );
}

// ---- header slot and loading ---------------------------------------------------------------

#[test]
fn header_actions_slot_sits_with_the_actions_and_loading_trails_them() {
    let mut rt = Runtime::new(
        Probe {
            with_header_action: true,
            export_enabled: true,
            loading: true,
            ..Probe::default()
        },
        probe_view,
        (960.0, 640.0),
        env(true),
    );
    reframe(&mut rt);

    assert!(has_key(&rt, "probe-header-action"), "headerActions slot is mounted");
    assert_eq!(
        element(&rt, "data-table-loading").text.as_deref(),
        Some(t("common.loading"))
    );

    let elements = rt.elements();
    let keys: Vec<&str> = elements
        .iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key.as_deref())
        .collect();
    let at = |key: &str| {
        keys.iter()
            .position(|k| *k == key)
            .unwrap_or_else(|| panic!("{key} is laid out"))
    };
    assert!(
        at("data-table-selection-bar-actions") < at("probe-header-action"),
        "the slot is inside the actions container"
    );
    assert!(
        at("probe-header-action") < at("data-table-delete-rows"),
        "React renders headerActions before delete/export"
    );
    assert!(
        at("data-table-export") < at("data-table-loading"),
        "loading trails the actions container"
    );

    rt.state.loading = false;
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-loading"), "no loading span when idle");
}

// ---- i18n and geometry ---------------------------------------------------------------------

#[test]
fn every_i18n_key_the_bar_uses_exists_in_the_locale_pack() {
    for key in [
        "common.selectAll",
        "dataTable.selected",
        "common.rows",
        "dataTable.selectionSummary",
        "dataTable.deleteRow",
        "export.export",
        "common.loading",
    ] {
        assert!(has(key), "{key} is missing from the zh-CN locale");
    }
}

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[test]
fn the_bar_does_not_overflow_either_window_in_either_theme() {
    let full = Probe {
        selected_rows: (0..10).collect(),
        has_cell_selection: true,
        mode: GridSelectionMode::Cell,
        covered_rows: (0..12).collect(),
        covered_cols: 24,
        covered_cells: 288,
        export_enabled: true,
        loading: true,
        with_header_action: true,
        ..Probe::default()
    };
    for dark in [true, false] {
        for (w, h) in [MIN, DESIGN] {
            let context = format!("dark={dark} {w}x{h}");
            let mut rt = Runtime::new(
                Probe { dark, ..full.clone() },
                probe_view,
                (w, h),
                env(dark),
            );
            reframe(&mut rt);
            for key in [
                "data-table-selection-bar",
                "data-table-select-all",
                "data-table-selected-summary",
                "data-table-selection-summary",
                "data-table-delete-rows",
                "data-table-export",
                "data-table-loading",
            ] {
                assert!(has_key(&rt, key), "{context}: expected {key}");
            }
            assert_in_window(&rt, w, h, &context);
        }
    }
}
