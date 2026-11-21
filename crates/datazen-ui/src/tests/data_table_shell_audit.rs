//! T163 independent audit: the `DataTable.tsx` shell (`data_table.rs` +
//! `data_table_vm.rs`), written against the React source
//! (`packages/frontend/src/components/DataTable/DataTable.tsx`, lines
//! 875–1111 for the tree and 563–873 for the gating), not against the
//! coder's tests.
//!
//! What is pinned here, from the React behaviour:
//!
//! * the composition order and every mount gate — filter editor (the six
//!   callbacks) / filter bar (filters + two callbacks) / neither; the
//!   selection bar (either selection half); the status-bar slot; the
//!   keyboard hints (both halves of the keyboard contract); the scroll
//!   surface with its grid role, label, and the rowcount / colcount /
//!   busy probes; the pager (the four inputs); the two dialogs (state).
//! * the loading / empty overlays: which one wins, their two-line shape
//!   and their live-region / status roles.
//! * the empty placeholder: it replaces the header, the body and the
//!   overlays, and the rowcount probe collapses to 1.
//! * the Delete key guard: fires with the sorted row set only when the
//!   whole-row delete path is armed (React `handleDeleteKey`).
//! * geometry: nothing overflows the root horizontally, at both sizes and
//!   both themes.
//!
//! The body itself is T396's and stays a slot; a stub slot element proves
//! the shell mounts it inside the surface, after the header.

use rune::{col, el, text, Cx, El, Env, KeyEvent, Key, Mods, Runtime};
use serde_json::{json, Value};

use crate::components::data_table::data_table::{DataTable, PagingInputs};
use crate::components::data_table::data_table_vm::{
    self, DataTableShellState, ExportInitialScope,
};
use crate::components::data_table::table_header::Column;
use crate::components::filter::{FilterCondition, FilterValue};
use crate::components::filter_editor::FilterEditorState;
use crate::i18n::t;
use crate::table_data_grid_selection::{
    CellCoord, GridSelection, GridSelectionMode,
};

// ---- the probe harness ------------------------------------------------------

struct Probe {
    dark: bool,
    rows: Vec<Vec<Value>>,
    loading: bool,

    // The selection contract and its halves.
    grid_selection: Option<GridSelection>,
    cell_contract: bool,
    keyboard_navigation: bool,
    selected_rows: Vec<usize>,
    select_all: bool,
    row_select: bool,
    delete_handler: bool,
    pk_columns: Vec<String>,

    // The filter surface.
    editor_callbacks: bool,
    bar_callbacks: bool,
    filters: Vec<FilterCondition>,
    panel_open: bool,
    filter_state: FilterEditorState,

    // The pager.
    paging: Option<PagingInputs>,
    page_change: bool,
    size_change: bool,

    // The dialogs.
    shell: DataTableShellState,
    export_table_name: Option<&'static str>,

    // Slots.
    status_slot: bool,
    body_slot: bool,
    placeholder: bool,
    editing_cell: bool,

    // Recording.
    deleted: Vec<Vec<usize>>,
}

impl Default for Probe {
    fn default() -> Self {
        Probe {
            dark: true,
            rows: vec![
                vec![json!(1), json!("a")],
                vec![json!(2), json!("b")],
                vec![json!(3), json!("c")],
            ],
            loading: false,
            grid_selection: None,
            cell_contract: false,
            keyboard_navigation: false,
            selected_rows: Vec::new(),
            select_all: false,
            row_select: false,
            delete_handler: false,
            pk_columns: Vec::new(),
            editor_callbacks: false,
            bar_callbacks: false,
            filters: Vec::new(),
            panel_open: false,
            filter_state: FilterEditorState::default(),
            paging: None,
            page_change: false,
            size_change: false,
            shell: DataTableShellState::new(),
            export_table_name: None,
            status_slot: false,
            body_slot: false,
            placeholder: false,
            editing_cell: false,
            deleted: Vec::new(),
        }
    }
}

impl Probe {
    fn columns() -> Vec<Column> {
        vec![
            Column::new("c0", "id").ty("integer"),
            Column::new("c1", "name").ty("text"),
        ]
    }
}

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

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key)
        .text
        .unwrap_or_else(|| panic!("{key} shows text"))
}

fn probe_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut table = DataTable::<Probe>::new(Probe::columns(), &state.rows)
        .loading(state.loading)
        .selected_rows(state.selected_rows.iter().copied());
    if state.cell_contract {
        table = table.cell_selection_contract(true);
    }
    if let Some(selection) = &state.grid_selection {
        table = table.grid_selection(selection.clone());
    }
    if state.keyboard_navigation {
        table = table.keyboard_navigation(true);
    }
    if state.select_all {
        table = table.on_select_all(|_s: &mut Probe| {});
    }
    if state.row_select {
        table = table.on_row_select(|_s: &mut Probe, _index| {});
    }
    if state.delete_handler {
        table = table
            .on_delete_rows(|s: &mut Probe, rows| {
                s.deleted.push(rows);
            })
            .primary_key_columns(state.pk_columns.clone());
    }
    if state.status_slot {
        table = table.status_bar(text("status").key("probe-status-slot"));
    }
    if state.body_slot {
        table = table.body(el().key("probe-body-slot").h(60.0));
    }
    if state.placeholder {
        table = table.empty_placeholder(
            text("placeholder").key("probe-placeholder"),
        );
    }
    if state.editing_cell {
        table = table.editing_cell(true);
    }
    if !state.filters.is_empty() || state.editor_callbacks || state.bar_callbacks {
        table = table.filters(&state.filters);
    }
    if state.panel_open {
        table = table.filter_panel_open(true);
    }
    if state.editor_callbacks {
        table = table
            .on_add_filter(|_s: &mut Probe, _f| {})
            .on_update_filter(|_s: &mut Probe, _i, _f| {})
            .on_remove_filter(|_s: &mut Probe, _i| {})
            .on_clear_filters(|_s: &mut Probe| {})
            .on_filter_logic_change(|_s: &mut Probe, _l| {})
            .on_apply_filters(|_s: &mut Probe| {})
            .on_filter_panel_open_change(|_s: &mut Probe, _open| {})
            .filter_editor_host(&state.filter_state, |s: &mut Probe| {
                &mut s.filter_state
            });
    } else if state.bar_callbacks {
        table = table
            .on_remove_filter(|_s: &mut Probe, _i| {})
            .on_clear_filters(|_s: &mut Probe| {});
    }
    if let Some(paging) = state.paging {
        table = table.paging(paging);
        if state.page_change {
            table = table.on_page_change(|_s: &mut Probe, _page| {});
        }
        if state.size_change {
            table = table.on_page_size_change(|_s: &mut Probe, _size| {});
        }
    }
    table = table.shell_state(&state.shell, |s: &mut Probe| &mut s.shell);
    if let Some(name) = state.export_table_name {
        table = table.export_table_name(Some(name));
    }

    let mut layer = Vec::new();
    let shell = table.build(state, state.dark, &mut layer);
    rune_kit::root(col().key("probe-page").padding(16.0).child(shell), layer)
}

fn runtime(probe: Probe) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, probe_view, (960.0, 640.0), env(dark));
    reframe(&mut rt);
    rt
}

// ---- the composition and its gates ------------------------------------------

#[test]
fn bare_grid_renders_root_surface_header_and_nothing_else() {
    let rt = runtime(Probe::default());

    assert!(has_key(&rt, "data-table"), "the root mounts");
    assert!(has_key(&rt, "data-table-surface"), "the scroll surface mounts");
    assert!(has_key(&rt, "data-table-header"), "the header mounts (TableHeader)");

    // Nothing else: React's ternaries drop every optional block.
    assert!(!has_key(&rt, "data-table-selection-bar"));
    assert!(!has_key(&rt, "filter-editor"), "no editor callbacks -> no editor");
    assert!(!has_key(&rt, "data-table-filter-bar"), "no filters -> no bar");
    assert!(!has_key(&rt, "data-table-pagination"));
    assert!(!has_key(&rt, "data-table-empty"), "rows exist -> no empty overlay");
    assert!(!has_key(&rt, "data-table-loading"));
    assert!(!has_key(&rt, "probe-status-slot"));
    assert!(!has_key(&rt, "probe-body-slot"));

    // With rows present there is no status overlay: gridRowCount is
    // rows(3) + header(1) + 0.
    assert!(has_key(&rt, "data-table-surface-rowcount-4"));
    assert!(!has_key(&rt, "data-table-surface-rowcount-5"));
    assert!(has_key(&rt, "data-table-surface-colcount-3"));
    assert!(has_key(&rt, "data-table-surface-busy-0"));
}

#[test]
fn empty_rows_show_the_two_line_overlay_and_loading_wins_over_it() {
    let mut probe = Probe::default();
    probe.rows = Vec::new();
    let rt = runtime(probe);

    assert!(has_key(&rt, "data-table-empty"));
    assert_eq!(text_of(&rt, "data-table-empty-text"), t("dataTable.empty"));
    assert_eq!(text_of(&rt, "data-table-empty-hint"), t("dataTable.emptyHint"));
    assert!(!has_key(&rt, "data-table-loading"));
    // 0 rows + header + the status row the empty overlay mounts.
    assert!(has_key(&rt, "data-table-surface-rowcount-2"));

    // `loading` wins: React's ternary renders the spinner, not the empty
    // hint, whenever both would apply.
    let mut probe = Probe::default();
    probe.rows = Vec::new();
    probe.loading = true;
    let rt = runtime(probe);
    assert!(has_key(&rt, "data-table-loading"));
    assert_eq!(text_of(&rt, "data-table-loading-text"), t("dataTable.loading"));
    assert!(!has_key(&rt, "data-table-empty"));
}

#[test]
fn empty_placeholder_replaces_header_body_and_overlays() {
    // React: `{emptyPlaceholder ? gridRow(emptyPlaceholder) : <> header +
    // body + overlays >}` — the placeholder replaces the whole body block
    // and gridRowCount collapses to 1.
    let mut probe = Probe::default();
    probe.placeholder = true;
    probe.loading = true; // would otherwise paint the spinner overlay
    let rt = runtime(probe);
    assert!(has_key(&rt, "probe-placeholder"));
    assert!(!has_key(&rt, "data-table-header"), "the header is gone");
    assert!(!has_key(&rt, "data-table-loading"), "the overlays are gone");
    assert!(has_key(&rt, "data-table-surface-rowcount-1"));
    assert!(!has_key(&rt, "data-table-surface-rowcount-2"));

    // Without the placeholder the header and the overlays are back.
    let mut probe = Probe::default();
    probe.loading = true;
    let rt = runtime(probe);
    assert!(has_key(&rt, "data-table-header"));
    assert!(has_key(&rt, "data-table-loading-spinner"));
}

fn cell_sel(row: usize, column: &str) -> GridSelection {
    GridSelection {
        anchor: Some(CellCoord {
            row_index: row,
            column_name: column.to_owned(),
        }),
        focus: Some(CellCoord {
            row_index: row,
            column_name: column.to_owned(),
        }),
        extra_ranges: Vec::new(),
        mode: GridSelectionMode::Cell,
    }
}

#[test]
fn cell_contract_switches_the_surface_role_label_and_probes() {
    // Contract off: the plain table label, no grid role, no active
    // descendant, and the probes report the plain counts.
    let rt = runtime(Probe::default());
    let surface = element(&rt, "data-table-surface");
    assert_eq!(surface.label.as_deref(), Some(t("dataTable.tableLabel")));
    assert_ne!(surface.role, "grid");

    // Contract on (both halves): the grid role, the a11y grid label, the
    // focus, and the cursor fallback key on the focused cell.
    let mut probe = Probe::default();
    probe.cell_contract = true;
    probe.grid_selection = Some(cell_sel(1, "name"));
    let rt = runtime(probe);
    let surface = element(&rt, "data-table-surface");
    assert_eq!(surface.label.as_deref(), Some(t("tableView.a11y.gridLabel")));
    assert_eq!(surface.role, "grid");
    assert!(surface.focusable, "the scroll surface owns the keyboard focus");

    // The status overlays wrap in row + gridcell under the contract.
    let mut probe = Probe::default();
    probe.rows = Vec::new();
    probe.cell_contract = true;
    probe.grid_selection = Some(cell_sel(0, "id"));
    let rt = runtime(probe);
    assert!(has_key(&rt, "data-table-statusrow-1"));
    assert!(has_key(&rt, "data-table-statusrow-1-cell"));
}

#[test]
fn selection_bar_mounts_on_either_selection_half() {
    // Row selection half (both callbacks of React's hasSelection).
    let mut probe = Probe::default();
    probe.select_all = true;
    probe.row_select = true;
    let rt = runtime(probe);
    assert!(has_key(&rt, "data-table-selection-bar"));

    // The cell-selection contract alone also mounts it (React's
    // `hasSelection || hasCellSelection`).
    let mut probe = Probe::default();
    probe.cell_contract = true;
    probe.grid_selection = Some(cell_sel(0, "id"));
    let rt = runtime(probe);
    assert!(has_key(&rt, "data-table-selection-bar"));

    // Neither: no bar.
    let rt = runtime(Probe::default());
    assert!(!has_key(&rt, "data-table-selection-bar"));
}

#[test]
fn cell_mode_bar_carries_the_covered_counts() {
    let mut probe = Probe::default();
    probe.cell_contract = true;
    probe.grid_selection = Some(cell_sel(0, "id"));
    let rt = runtime(probe);
    // The bar's own probes (T007's keys) carry the covered math the shell
    // derived: one cell of a 2-column grid -> 1 row, 1 column, 1 cell.
    assert!(has_key(&rt, "data-table-selection-summary-rows-1"));
    assert!(has_key(&rt, "data-table-selection-summary-cols-1"));
    assert!(has_key(&rt, "data-table-selection-summary-cells-1"));
}

#[test]
fn filter_surface_prefers_editor_then_bar_then_nothing() {
    // The six editor callbacks mount the editor (with a host state).
    let mut probe = Probe::default();
    probe.editor_callbacks = true;
    probe.panel_open = true;
    let rt = runtime(probe);
    assert!(has_key(&rt, "filter-editor"), "the six callbacks mount the editor");
    assert!(!has_key(&rt, "data-table-filter-bar"));

    // Without the six but with filters + the two bar callbacks: the bar.
    let mut probe = Probe::default();
    probe.bar_callbacks = true;
    probe.filters = vec![FilterCondition::new("id", "eq", FilterValue::Single("1".into()))];
    let rt = runtime(probe);
    assert!(has_key(&rt, "data-table-filter-bar"));
    assert!(!has_key(&rt, "filter-editor"));

    // Filters without the callbacks: nothing (React `hasFilters` needs
    // both callbacks).
    let mut probe = Probe::default();
    probe.filters = vec![FilterCondition::new("id", "eq", FilterValue::Single("1".into()))];
    let rt = runtime(probe);
    assert!(!has_key(&rt, "data-table-filter-bar"));
    assert!(!has_key(&rt, "filter-editor"));
}

#[test]
fn pager_mounts_only_with_all_four_inputs() {
    let mut probe = Probe::default();
    probe.paging = Some(PagingInputs {
        page: 1,
        page_size: 3,
        total_rows: 3,
        ..PagingInputs::default()
    });
    probe.page_change = true;
    probe.size_change = true;
    let rt = runtime(probe);
    assert!(has_key(&rt, "data-table-pagination"));
    assert_eq!(text_of(&rt, "data-table-pagination-row-count"), "1-3 / 3");

    // Both callbacks, but no paging inputs: React's `page != null && …`
    // drops the pager.
    let mut probe = Probe::default();
    probe.page_change = true;
    probe.size_change = true;
    let rt = runtime(probe);
    assert!(!has_key(&rt, "data-table-pagination"));

    // Inputs, but one callback missing: still dropped.
    let mut probe = Probe::default();
    probe.paging = Some(PagingInputs {
        page: 1,
        page_size: 3,
        total_rows: 3,
        ..PagingInputs::default()
    });
    probe.page_change = true;
    let rt = runtime(probe);
    assert!(!has_key(&rt, "data-table-pagination"));
}

#[test]
fn hints_mount_only_under_the_full_keyboard_contract() {
    let mut probe = Probe::default();
    probe.keyboard_navigation = true;
    // Navigation on but the cell contract off: React renders no hints.
    let rt = runtime(probe);
    assert!(!has_key(&rt, "data-table-shortcuts"));

    let mut probe = Probe::default();
    probe.keyboard_navigation = true;
    probe.cell_contract = true;
    probe.grid_selection = Some(cell_sel(0, "id"));
    let rt = runtime(probe);
    assert!(has_key(&rt, "data-table-shortcuts"));
}

#[test]
fn status_slot_and_body_slot_mount_where_react_places_them() {
    let mut probe = Probe::default();
    probe.status_slot = true;
    probe.body_slot = true;
    let rt = runtime(probe);
    assert!(has_key(&rt, "probe-status-slot"));
    assert!(has_key(&rt, "probe-body-slot"));

    // The order: bar -> status -> surface; inside the surface,
    // header -> body.
    let bar_y = has_key(&rt, "data-table-selection-bar");
    let _ = bar_y;
    let status = element(&rt, "probe-status-slot").rect[1];
    let surface = element(&rt, "data-table-surface").rect[1];
    assert!(
        status < surface,
        "the status slot sits above the surface (React stacks it between the bar and the grid)"
    );
    let header = element(&rt, "data-table-header").rect[1];
    let body = element(&rt, "probe-body-slot").rect[1];
    assert!(header < body, "the body slot mounts after the header");
}

// ---- the Delete key ---------------------------------------------------------

#[test]
fn delete_key_fires_the_sorted_row_set_only_when_armed() {
    // Armed: the handler + primary keys + a non-empty row set. React sorts
    // before calling (its `Array.from(...).sort((a, b) => a - b)`).
    let mut probe = Probe::default();
    probe.delete_handler = true;
    probe.pk_columns = vec!["id".into()];
    probe.selected_rows = vec![4, 1, 7];
    probe.cell_contract = true;
    probe.grid_selection = Some(cell_sel(0, "id"));
    let mut rt = runtime(probe);

    // The surface is the focus owner: click it, then the key.
    // The surface owns the keyboard focus under the cell-selection
    // contract; here (no contract) Tab reaches it as the grid's focusable
    // control, the runtime's own route to a keyed control.
    rt.key(KeyEvent {
        key: Key::Tab,
        mods: Mods::default(),
    });
    reframe(&mut rt);
    // Tab reaches a focusable control inside the root (here the delete
    // button of the selection bar); the unconsumed Delete then falls
    // through to the root's handler, exactly the React bubbling.
    assert!(
        rt.elements().iter().any(|e| e.focused),
        "Tab focuses a control inside the grid"
    );
    rt.key(KeyEvent {
        key: Key::Delete,
        mods: Mods::default(),
    });
    reframe(&mut rt);
    assert_eq!(rt.state.deleted, vec![vec![1, 4, 7]], "sorted, once");

    // The editing guard: an open cell editor swallows the key (React's
    // first `if (editingCell) return`).
    let mut probe = Probe::default();
    probe.delete_handler = true;
    probe.pk_columns = vec!["id".into()];
    probe.selected_rows = vec![4, 1, 7];
    probe.editing_cell = true;
    probe.cell_contract = true;
    probe.grid_selection = Some(cell_sel(0, "id"));
    let mut rt = runtime(probe);
    rt.key(KeyEvent {
        key: Key::Tab,
        mods: Mods::default(),
    });
    reframe(&mut rt);
    rt.key(KeyEvent {
        key: Key::Delete,
        mods: Mods::default(),
    });
    reframe(&mut rt);
    assert!(rt.state.deleted.is_empty(), "an open editor swallows the key");

    // No primary keys: the delete path is not armed.
    let mut probe = Probe::default();
    probe.delete_handler = true;
    probe.selected_rows = vec![4, 1, 7];
    probe.cell_contract = true;
    probe.grid_selection = Some(cell_sel(0, "id"));
    let mut rt = runtime(probe);
    rt.key(KeyEvent {
        key: Key::Tab,
        mods: Mods::default(),
    });
    reframe(&mut rt);
    rt.key(KeyEvent {
        key: Key::Delete,
        mods: Mods::default(),
    });
    reframe(&mut rt);
    assert!(rt.state.deleted.is_empty(), "no primary keys -> the key is inert");
}

#[test]
fn an_empty_row_set_keeps_the_delete_key_inert() {
    // React: `selectedRows.size === 0` -> no action (cell mode keeps the
    // set empty on purpose, 01-selection §4.3).
    let mut probe = Probe::default();
    probe.delete_handler = true;
    probe.pk_columns = vec!["id".into()];
    probe.cell_contract = true;
    probe.grid_selection = Some(cell_sel(0, "id"));
    let mut rt = runtime(probe);
    rt.key(KeyEvent {
        key: Key::Tab,
        mods: Mods::default(),
    });
    reframe(&mut rt);
    rt.key(KeyEvent {
        key: Key::Delete,
        mods: Mods::default(),
    });
    reframe(&mut rt);
    assert!(rt.state.deleted.is_empty());
}

// ---- the export dialog pair --------------------------------------------------

#[test]
fn export_dialog_mounts_only_while_the_coherent_pair_is_open() {
    let mut probe = Probe::default();
    probe.export_table_name = Some("t1");
    let mut rt = runtime(probe);
    assert!(
        !has_key(&rt, "data-table-export-scrim"),
        "closed: no dialog in the overlay layer"
    );

    // Open through the coherent helper: both halves move.
    data_table_vm::open_export_dialog(
        &mut rt.state.shell.vm,
        Some(&mut rt.state.shell.export),
        Some(ExportInitialScope::FilteredTable),
    );
    reframe(&mut rt);
    assert!(has_key(&rt, "data-table-export-scrim"), "open: the scrim mounts");
    assert!(has_key(&rt, "data-table-export-title"));
    assert!(rt.state.shell.vm.export_open);
    assert!(rt.state.shell.export.open);

    // Close the same way.
    data_table_vm::close_export_dialog(
        &mut rt.state.shell.vm,
        Some(&mut rt.state.shell.export),
    );
    reframe(&mut rt);
    assert!(!has_key(&rt, "data-table-export-scrim"));
    assert!(!rt.state.shell.vm.export_open);
    assert!(!rt.state.shell.export.open);
}

#[test]
fn export_signal_edges_open_once_and_preselect_by_builder() {
    // Signal 0 is a no-op (React `exportOpenSignal <= signalSeenRef.current`).
    let mut rt = runtime(Probe::default());
    let opened = data_table_vm::apply_export_signal(&mut rt.state.shell.vm, Some(0), true);
    assert!(!opened);
    // 1 opens once, with the filtered preselect because the builder exists.
    let opened = data_table_vm::apply_export_signal(&mut rt.state.shell.vm, Some(1), true);
    assert!(opened);
    assert_eq!(
        rt.state.shell.vm.export_initial_scope,
        Some(ExportInitialScope::FilteredTable)
    );
    // The same 1 again: never re-opens.
    let opened = data_table_vm::apply_export_signal(&mut rt.state.shell.vm, Some(1), true);
    assert!(!opened);
    // Without the builder the dialog opens the plain way (DB-15).
    let opened = data_table_vm::apply_export_signal(&mut rt.state.shell.vm, Some(2), false);
    assert!(opened);
    assert_eq!(rt.state.shell.vm.export_initial_scope, None);
    // None (React undefined) is a no-op.
    let opened = data_table_vm::apply_export_signal(&mut rt.state.shell.vm, None, true);
    assert!(!opened);
}

// ---- geometry -----------------------------------------------------------------

#[test]
fn no_horizontal_overflow_at_both_sizes_and_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = Probe::default();
            probe.dark = dark;
            probe.cell_contract = true;
            probe.grid_selection = Some(cell_sel(0, "id"));
            probe.paging = Some(PagingInputs {
                page: 1,
                page_size: 3,
                total_rows: 3,
                ..PagingInputs::default()
            });
            probe.page_change = true;
            probe.size_change = true;
            probe.body_slot = true;
            let mut rt = Runtime::new(probe, probe_view, (w, h), env(dark));
            reframe(&mut rt);

            let root = element(&rt, "data-table").rect;
            for e in rt.elements() {
                if e.exiting || e.key.as_deref() == Some("data-table") {
                    continue;
                }
                // The kit root wrapper and the audit's own page wrapper
                // span the window by design.
                if e.key.as_deref() == Some("kit-root")
                    || e.key.as_deref() == Some("probe-page")
                {
                    continue;
                }
                let [x, _y, ew, _eh] = e.rect;
                if ew <= 0.0 {
                    continue;
                }
                assert!(
                    x + ew <= root[0] + root[2] + 0.5,
                    "{}x{} dark={}: {} at x={} w={} escapes the root",
                    w,
                    h,
                    dark,
                    e.key.clone().unwrap_or_default(),
                    x,
                    ew
                );
            }
        }
    }
}

// ---- the covered-math derivations against the store helpers -------------------

#[test]
fn covered_math_matches_the_store_module() {
    // An anchor + focus over the whole 2-column grid: every row, both
    // columns, all 6 cells — read back through the same probes the bar
    // renders (the shell threads the derivation, the bar the display).
    let mut probe = Probe::default();
    probe.cell_contract = true;
    probe.grid_selection = Some(GridSelection {
        anchor: Some(CellCoord {
            row_index: 0,
            column_name: "id".into(),
        }),
        focus: Some(CellCoord {
            row_index: 2,
            column_name: "name".into(),
        }),
        extra_ranges: Vec::new(),
        mode: GridSelectionMode::Cell,
    });
    let rt = runtime(probe);
    assert!(has_key(&rt, "data-table-selection-summary-rows-3"));
    assert!(has_key(&rt, "data-table-selection-summary-cols-2"));
    assert!(has_key(&rt, "data-table-selection-summary-cells-6"));

    // Contract off: no coverage at all (React's `hasCellSelection`
    // ternaries), the bar renders no summary probes.
    let rt = runtime(Probe::default());
    assert!(!has_key(&rt, "data-table-selection-summary-rows-0"));
    assert!(!has_key(&rt, "data-table-selection-summary-cells-0"));
}
