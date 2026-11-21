//! T163-S2: the `DataTable.tsx` shell — the grid's orchestrator, ported.
//!
//! React's `DataTable` is the composition root of the data grid: a bordered
//! column that stacks, in order, the filter surface ([`FilterEditor`] when
//! the six editor callbacks exist, else the read-only [`FilterBar`]), the
//! selection bar, the caller's status-bar slot, the keyboard hints, the
//! scroll surface (the [`TableHeader`] plus the body slot plus the loading /
//! empty overlays), and the pagination; the export and column-presentation
//! dialogs render into the overlay `layer` while their state is on. Every
//! child of this tree is already ported except the body: `VirtualBody` is
//! T396, so this shell mounts it as a caller-supplied slot element and keeps
//! the overlays and pagination around it — when the slot is empty the grid
//! renders header + overlays only, exactly the shape React renders for a
//! body-less tree.
//!
//! The state model and the gating derivations are
//! [`super::data_table_vm`] (S1); the shell here is the JSX of
//! `DataTable.tsx` lines 875–1111 in the same order.
//!
//! ## Keys
//!
//! React's root and overlays carry no `data-testid`; the keys below are the
//! port's own handles, in the naming style of the other grid parts:
//!
//! | part                 | key                                  |
//! |----------------------|--------------------------------------|
//! | root                 | `<key>` (default `data-table`)       |
//! | scroll surface       | `<key>-surface`                      |
//! | rowcount probe       | `<key>-surface-rowcount-<n>`         |
//! | colcount probe       | `<key>-surface-colcount-<n>`         |
//! | busy probe           | `<key>-surface-busy-<0|1>`           |
//! | loading overlay      | `<key>-loading`                      |
//! | empty overlay        | `<key>-empty` / `<key>-empty-hint`   |
//! | hints mount          | `<key>-shortcuts`                    |
//! | export dialog        | `data-table-export` (T132's key)     |
//!
//! The ARIA `aria-rowcount` / `aria-colcount` / `aria-busy` the React
//! surface writes have no `El` API (see the selection bar's module docs for
//! the `data-dt-*` precedent), so they are mirrored as zero-size
//! `a11y_hidden()` key-encoded probes under the surface — the row count is
//! the formula from [`super::data_table_vm::grid_row_count`], the column
//! count is the columns plus the row-number gutter.
//!
//! ## Deliberate deviations from the React source
//!
//! * **The body is a slot.** `VirtualBody` (T396) is not ported yet; a
//!   caller passes its built element via [`DataTable::body`]. The overlays,
//!   the header and the pagination around the slot are this shell's, in the
//!   React order.
//! * **The dialog pair is host-state-coherent.** React's `exportOpen` /
//!   `exportInitialScope` local state and the Rune `DataExportVm`'s own
//!   `open` / `open_epoch` / `initial_scope` move together through
//!   [`super::data_table_vm::DataTableShellState`]; the dialog itself gates
//!   on its own `open` (T132), and the shell's `vm.export_open` mirror is
//!   what tests read.
//! * **`aria-describedby` is not reproduced** (`El` has no described-by
//!   reference — the T0-11 `aria-controls` finding again). The hints element
//!   mounts next to the surface, a sibling of it, as in React.
//! * **`aria-activedescendant`** is written from the selection's focus cell
//!   (the [`super::data_table_vm::active_cell`] fallback) with the key
//!   format `<key>-cell-<row>-<col>`; T396's body owns the real cell keys
//!   and will keep the format.
//! * **The Delete key** guards exactly as React's `handleDeleteKey`: no
//!   action while a cell editor is up, only `Delete` / `Backspace`, and only
//!   with `on_delete_rows` + primary keys + a non-empty row set; cell mode
//!   keeps the row set empty by contract, so the whole-row action stays the
//!   toolbar's (01-selection §4.3).
//! * **`selectable` / `dt-scrollbar-autohide`** are web-CSS concepts with no
//!   Rune counterpart (text selection and scrollbar auto-hide are the
//!   runtime's); the `alternatingRows` / `scrollbarAutoHide` props are
//!   threaded by the body (T396) and are not consumed here.
//! * The keyboard-navigation / clipboard / resize wiring lives in the hooks
//!   and the body (T396, T423, T431); this shell threads their static
//!   inputs and their callbacks, not their runtime.

use std::collections::HashSet;
use std::rc::Rc;

use rune::{col, el, row, text, El, Key, KeyEvent, LiveRegion, Tx};
use serde_json::Value;

use crate::components::connection::batch_export_dialog::DataExportCapability;
use crate::components::data_table::column_presentation_dialog::ValuePresentationSnapshot;
use crate::components::data_table::column_presentation_dialog_state::ColumnPresentationDialogState;
use crate::components::data_table::column_presentation_dialog_view::ColumnPresentationDialog;
use crate::components::data_table::data_export_dialog::DataExportDialog;
use crate::components::data_table::data_table_selection_bar::{
    DataTableSelectionBar, GridSelection as BarGridSelection,
    GridSelectionMode as BarGridSelectionMode,
};
use crate::components::data_table::data_table_vm::{self, DataTableShellState};
use crate::components::data_table::grid_keyboard_hints::GridKeyboardHints;
use crate::components::data_table::pagination::{PagingHint, Pagination, TotalRowsKind};
use crate::components::data_table::table_header::{Column, SortCondition, TableHeader};
use crate::components::filter::{FilterCondition, FilterConnector};
use crate::components::filter_editor::{
    FilterColumn, FilterEditor, FilterEditorState, FilterPanelMode,
};
use crate::components::filter_bar::FilterBar;
use crate::i18n::t;
use crate::table_data_grid_selection::{
    CellCoord, GridSelection, GridSelectionMode,
};
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};

const ROOT_KEY: &str = "data-table";
/// The export dialog's key (T132's own handle, kept stable so its tests and
/// the shell agree).
const EXPORT_DIALOG_KEY: &str = "data-table-export";
/// `min-h-28` on the status overlays.
const OVERLAY_MIN_H: f32 = 112.0;

/// React `PagingHint` inputs the pager needs; one optional block.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PagingInputs {
    pub(crate) page: usize,
    pub(crate) page_size: usize,
    pub(crate) total_rows: u64,
    pub(crate) kind: TotalRowsKind,
    pub(crate) counting: bool,
    pub(crate) unstable_paging: bool,
    pub(crate) estimate_unavailable: bool,
    pub(crate) hint: Option<PagingHint>,
}

impl Default for PagingInputs {
    fn default() -> Self {
        PagingInputs {
            page: 1,
            page_size: 50,
            total_rows: 0,
            kind: TotalRowsKind::Exact,
            counting: false,
            unstable_paging: false,
            estimate_unavailable: false,
            hint: None,
        }
    }
}

type HostFn<S, T> = Rc<dyn Fn(&mut S) -> &mut T>;

/// The grid shell. Build it with the props the surface owns; callbacks are
/// optional at the type level so a probe can render any gating branch, but
/// the production caller wires the contract pairs (the selection pair, the
/// filter-editor six, the pager four) or the matching block stays unmounted,
/// exactly as React's ternaries drop it.
pub(crate) struct DataTable<'a, S: 'static> {
    key: String,
    columns: Vec<Column>,
    rows: &'a [Vec<Value>],
    row_height: f32,
    loading: bool,

    // The cell-selection contract (both halves or neither, 01-selection §3.8).
    grid_selection: Option<GridSelection>,
    on_grid_selection_change: bool,
    keyboard_navigation: bool,
    selected_rows: HashSet<usize>,

    // Row selection (the toolbar's select-all half).
    on_select_all: Option<Rc<dyn Fn(&mut S)>>,
    on_row_select: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_delete_rows: Option<Rc<dyn Fn(&mut S, Vec<usize>)>>,
    primary_key_columns: Vec<String>,

    // Header pass-throughs.
    sorts: Vec<SortCondition>,
    on_sort: Option<Rc<dyn Fn(&mut S, &str, bool)>>,
    on_remove_sort: Option<Rc<dyn Fn(&mut S, &str)>>,
    column_widths: Vec<f32>,
    frozen_columns: usize,
    show_row_numbers: bool,

    // The slots React threads as nodes.
    header_actions: Option<El>,
    status_bar: Option<El>,
    empty_placeholder: Option<El>,
    body: Option<El>,

    // The filter surface.
    filters: Option<&'a [FilterCondition]>,
    filter_logic: FilterConnector,
    on_remove_filter: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_clear_filters: Option<Rc<dyn Fn(&mut S)>>,
    on_add_filter: Option<Rc<dyn Fn(&mut S, FilterCondition)>>,
    on_update_filter: Option<Rc<dyn Fn(&mut S, usize, FilterCondition)>>,
    on_filter_logic_change: Option<Rc<dyn Fn(&mut S, FilterConnector)>>,
    on_apply_filters: Option<Rc<dyn Fn(&mut S)>>,
    on_filter_panel_open_change: Option<Rc<dyn Fn(&mut S, bool)>>,
    filter_editor_state: Option<&'a FilterEditorState>,
    filter_editor_host: Option<HostFn<S, FilterEditorState>>,
    supported_operators: Option<&'a [&'static str]>,
    filter_mode: Option<FilterPanelMode>,
    filter_raw_where_supported: bool,
    any_column_searchable: usize,
    any_column_skipped: usize,
    applied_raw_where: Option<&'a str>,
    raw_where_context: Option<&'a str>,
    on_filter_mode_change: Option<Rc<dyn Fn(&mut S, FilterPanelMode)>>,
    on_any_column_value_change: Option<Rc<dyn Fn(&mut S, String)>>,
    on_apply_any_column: Option<Rc<dyn Fn(&mut S, String)>>,
    on_raw_where_value_change: Option<Rc<dyn Fn(&mut S, String)>>,
    on_apply_raw_where: Option<Rc<dyn Fn(&mut S, String)>>,
    on_preview_sql: Option<Rc<dyn Fn(&mut S)>>,
    on_export_filtered: Option<Rc<dyn Fn(&mut S)>>,

    // Export wiring (the dialog's own VM lives in the shell state).
    shell_state: Option<&'a DataTableShellState>,
    shell_host: Option<HostFn<S, DataTableShellState>>,
    export_table_name: Option<&'a str>,
    capability: DataExportCapability,
    export_signal: Option<u32>,
    has_filtered_export_sql: bool,

    // The column-presentation dialog (issue-32 T09's mount).
    presentation_state: Option<&'a ColumnPresentationDialogState>,
    presentation_snapshot: Option<&'a ValuePresentationSnapshot>,
    presentation_registry: Option<&'a crate::components::data_table::column_presentation_rules::ValuePresentationRegistry>,
    presentation_host: Option<HostFn<S, ColumnPresentationDialogState>>,

    // Paging.
    paging: Option<PagingInputs>,
    on_page_change: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_page_size_change: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_count_exact: Option<Rc<dyn Fn(&mut S)>>,

    // Editing (gates the Delete key).
    editing_cell: bool,
}

impl<'a, S: 'static> DataTable<'a, S> {
    /// The shell over the grid's columns and rows. Every other input is a
    /// builder setter, so a probe can mount any gating branch.
    pub(crate) fn new(columns: Vec<Column>, rows: &'a [Vec<Value>]) -> DataTable<'a, S> {
        DataTable {
            key: ROOT_KEY.to_owned(),
            columns,
            rows,
            row_height: data_table_vm::DATA_TABLE_ROW_HEIGHT,
            loading: false,
            grid_selection: None,
            on_grid_selection_change: false,
            keyboard_navigation: false,
            selected_rows: HashSet::new(),
            on_select_all: None,
            on_row_select: None,
            on_delete_rows: None,
            primary_key_columns: Vec::new(),
            sorts: Vec::new(),
            on_sort: None,
            on_remove_sort: None,
            column_widths: Vec::new(),
            frozen_columns: 0,
            show_row_numbers: true,
            header_actions: None,
            status_bar: None,
            empty_placeholder: None,
            body: None,
            filters: None,
            filter_logic: FilterConnector::And,
            on_remove_filter: None,
            on_clear_filters: None,
            on_add_filter: None,
            on_update_filter: None,
            on_filter_logic_change: None,
            on_apply_filters: None,
            on_filter_panel_open_change: None,
            filter_editor_state: None,
            filter_editor_host: None,
            supported_operators: None,
            filter_mode: None,
            filter_raw_where_supported: false,
            any_column_searchable: 0,
            any_column_skipped: 0,
            applied_raw_where: None,
            raw_where_context: None,
            on_filter_mode_change: None,
            on_any_column_value_change: None,
            on_apply_any_column: None,
            on_raw_where_value_change: None,
            on_apply_raw_where: None,
            on_preview_sql: None,
            on_export_filtered: None,
            shell_state: None,
            shell_host: None,
            export_table_name: None,
            capability: DataExportCapability::default(),
            export_signal: None,
            has_filtered_export_sql: false,
            presentation_state: None,
            presentation_snapshot: None,
            presentation_registry: None,
            presentation_host: None,
            paging: None,
            on_page_change: None,
            on_page_size_change: None,
            on_count_exact: None,
            editing_cell: false,
        }
    }

    pub(crate) fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// React `loading` — the spinner overlay and the panels' busy state.
    pub(crate) fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// React `rowHeight` (default `DATA_TABLE_ROW_HEIGHT`).
    pub(crate) fn row_height(mut self, height: f32) -> Self {
        self.row_height = height;
        self
    }

    /// React `gridSelection` — one half of the cell-selection contract.
    pub(crate) fn grid_selection(mut self, selection: GridSelection) -> Self {
        self.grid_selection = Some(selection);
        self
    }

    /// React `onGridSelectionChange` — the other half; the contract is on
    /// exactly when both halves are present (01-selection §3.8).
    pub(crate) fn on_grid_selection_change(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_grid_selection_change = true;
        let _ = f; // the write-back wiring lands with the body (T396)
        self
    }

    /// Mark the write-back half present without a closure (probes pin the
    /// gating without a live callback).
    pub(crate) fn cell_selection_contract(mut self, on: bool) -> Self {
        self.on_grid_selection_change = on;
        self
    }

    pub(crate) fn keyboard_navigation(mut self, on: bool) -> Self {
        self.keyboard_navigation = on;
        self
    }

    /// React `selectedRows` — the row-mode set (only its size and its
    /// indices' sort reach the DOM).
    pub(crate) fn selected_rows(mut self, rows: impl IntoIterator<Item = usize>) -> Self {
        self.selected_rows = rows.into_iter().collect();
        self
    }

    pub(crate) fn on_select_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_all = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_row_select(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_row_select = Some(Rc::new(f));
        self
    }

    /// React `onDeleteRows` — the whole-row delete, from the toolbar button
    /// or the Delete key.
    pub(crate) fn on_delete_rows(mut self, f: impl Fn(&mut S, Vec<usize>) + 'static) -> Self {
        self.on_delete_rows = Some(Rc::new(f));
        self
    }

    pub(crate) fn primary_key_columns(mut self, columns: Vec<String>) -> Self {
        self.primary_key_columns = columns;
        self
    }

    pub(crate) fn sorts(mut self, sorts: Vec<SortCondition>) -> Self {
        self.sorts = sorts;
        self
    }

    pub(crate) fn on_sort(mut self, f: impl Fn(&mut S, &str, bool) + 'static) -> Self {
        self.on_sort = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_remove_sort(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_remove_sort = Some(Rc::new(f));
        self
    }

    pub(crate) fn column_widths(mut self, widths: Vec<f32>) -> Self {
        self.column_widths = widths;
        self
    }

    pub(crate) fn frozen_columns(mut self, frozen: usize) -> Self {
        self.frozen_columns = frozen;
        self
    }

    /// React `showRowNumbers` — the gutter is the header's to paint (DB-17).
    pub(crate) fn show_row_numbers(mut self, show: bool) -> Self {
        self.show_row_numbers = show;
        self
    }

    /// React `headerActions` — the bar's right-side slot.
    pub(crate) fn header_actions(mut self, slot: El) -> Self {
        self.header_actions = Some(slot);
        self
    }

    /// React `statusBar` — the strip between the bar and the surface.
    pub(crate) fn status_bar(mut self, slot: El) -> Self {
        self.status_bar = Some(slot);
        self
    }

    /// React `emptyPlaceholder` — replaces the header, the body and the
    /// overlays wholesale.
    pub(crate) fn empty_placeholder(mut self, slot: El) -> Self {
        self.empty_placeholder = Some(slot);
        self
    }

    /// The `VirtualBody` slot (T396). Until it lands the grid renders the
    /// header, the overlays and the pagination around an empty body.
    pub(crate) fn body(mut self, slot: El) -> Self {
        self.body = Some(slot);
        self
    }

    /// React `filters` — the applied list both filter surfaces read.
    pub(crate) fn filters(mut self, filters: &'a [FilterCondition]) -> Self {
        self.filters = Some(filters);
        self
    }

    pub(crate) fn filter_logic(mut self, logic: FilterConnector) -> Self {
        self.filter_logic = logic;
        self
    }

    pub(crate) fn on_remove_filter(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_remove_filter = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_clear_filters(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear_filters = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_add_filter(mut self, f: impl Fn(&mut S, FilterCondition) + 'static) -> Self {
        self.on_add_filter = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_update_filter(
        mut self,
        f: impl Fn(&mut S, usize, FilterCondition) + 'static,
    ) -> Self {
        self.on_update_filter = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_filter_logic_change(
        mut self,
        f: impl Fn(&mut S, FilterConnector) + 'static,
    ) -> Self {
        self.on_filter_logic_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_apply_filters(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_apply_filters = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_filter_panel_open_change(
        mut self,
        f: impl Fn(&mut S, bool) + 'static,
    ) -> Self {
        self.on_filter_panel_open_change = Some(Rc::new(f));
        self
    }

    /// The editor's host state + accessor (the filter-table slot, §3.2 of
    /// the machine runbook).
    pub(crate) fn filter_editor_host(
        mut self,
        state: &'a FilterEditorState,
        host: impl Fn(&mut S) -> &mut FilterEditorState + 'static,
    ) -> Self {
        self.filter_editor_state = Some(state);
        self.filter_editor_host = Some(Rc::new(host));
        self
    }

    pub(crate) fn supported_operators(mut self, operators: &'a [&'static str]) -> Self {
        self.supported_operators = Some(operators);
        self
    }

    pub(crate) fn filter_mode(mut self, mode: FilterPanelMode) -> Self {
        self.filter_mode = Some(mode);
        self
    }

    pub(crate) fn filter_raw_where_supported(mut self, supported: bool) -> Self {
        self.filter_raw_where_supported = supported;
        self
    }

    pub(crate) fn any_column(mut self, searchable: usize, skipped: usize) -> Self {
        self.any_column_searchable = searchable;
        self.any_column_skipped = skipped;
        self
    }

    pub(crate) fn raw_where(
        mut self,
        applied: Option<&'a str>,
        context: Option<&'a str>,
    ) -> Self {
        self.applied_raw_where = applied;
        self.raw_where_context = context;
        self
    }

    pub(crate) fn on_filter_mode_change(
        mut self,
        f: impl Fn(&mut S, FilterPanelMode) + 'static,
    ) -> Self {
        self.on_filter_mode_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_any_column_value_change(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_any_column_value_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_apply_any_column(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_apply_any_column = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_raw_where_value_change(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_raw_where_value_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_apply_raw_where(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_apply_raw_where = Some(Rc::new(f));
        self
    }

    /// React `onPreviewSql` — the filter panel's SQL button.
    pub(crate) fn on_preview_sql(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_preview_sql = Some(Rc::new(f));
        self
    }

    /// React `onExportFiltered` — the panel's direct exit; the shell opens
    /// the export dialog with the filtered scope preselected (DB-15).
    pub(crate) fn on_export_filtered(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_export_filtered = Some(Rc::new(f));
        self
    }

    /// The shell state (React's `exportOpen` family + the dialog VM) and its
    /// host accessor; the export dialog mounts only when this is wired.
    pub(crate) fn shell_state(
        mut self,
        state: &'a DataTableShellState,
        host: impl Fn(&mut S) -> &mut DataTableShellState + 'static,
    ) -> Self {
        self.shell_state = Some(state);
        self.shell_host = Some(Rc::new(host));
        self
    }

    pub(crate) fn export_table_name(mut self, name: Option<&'a str>) -> Self {
        self.export_table_name = name;
        self
    }

    pub(crate) fn export_capability(mut self, capability: DataExportCapability) -> Self {
        self.capability = capability;
        self
    }

    /// React `exportOpenSignal` — replayed into the shell state by the host
    /// before the frame; the setter only records the current counter.
    pub(crate) fn export_signal(mut self, signal: u32) -> Self {
        self.export_signal = Some(signal);
        self
    }

    /// React `onBuildFilteredExportSql` presence: the filtered scope is
    /// offered exactly when the builder exists (DB-15).
    pub(crate) fn has_filtered_export_sql(mut self, has: bool) -> Self {
        self.has_filtered_export_sql = has;
        self
    }

    /// The column-presentation dialog's inputs; it mounts only when the
    /// state, the snapshot and the accessor are all wired (T09).
    pub(crate) fn presentation_dialog(
        mut self,
        state: &'a ColumnPresentationDialogState,
        snapshot: &'a ValuePresentationSnapshot,
        host: impl Fn(&mut S) -> &mut ColumnPresentationDialogState + 'static,
    ) -> Self {
        self.presentation_state = Some(state);
        self.presentation_snapshot = Some(snapshot);
        self.presentation_host = Some(Rc::new(host));
        self
    }

    /// The presentation registry is optional the way React's injectable
    /// `presentationRegistry` is (T09); the default is the global one.
    pub(crate) fn presentation_registry(
        mut self,
        registry: &'a crate::components::data_table::column_presentation_rules::ValuePresentationRegistry,
    ) -> Self {
        self.presentation_registry = Some(registry);
        self
    }

    pub(crate) fn paging(mut self, inputs: PagingInputs) -> Self {
        self.paging = Some(inputs);
        self
    }

    pub(crate) fn on_page_change(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_page_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_page_size_change(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_page_size_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_count_exact(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_count_exact = Some(Rc::new(f));
        self
    }

    /// React `editingCell != null` — the Delete key's first guard.
    pub(crate) fn editing_cell(mut self, editing: bool) -> Self {
        self.editing_cell = editing;
        self
    }

    /// Builds the shell. `layer` receives the export and presentation
    /// dialogs while their state is on; the returned element is the bordered
    /// grid column.
    pub(crate) fn build(self, _s: &S, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let rows = self.rows;
        let row_count = rows.len();
        let column_names: Vec<&str> = self.columns.iter().map(|c| c.name.as_str()).collect();

        // The gating derivations (S1), verbatim from the React component.
        let has_cell_selection = self.grid_selection.is_some() && self.on_grid_selection_change;
        let selection = self
            .grid_selection
            .clone()
            .unwrap_or_else(GridSelection::empty);
        let has_selection = self.on_select_all.is_some() && self.on_row_select.is_some();
        let covered = data_table_vm::covered_rows(has_cell_selection, &selection, &self.selected_rows);
        let covered_columns = data_table_vm::covered_columns(has_cell_selection, &selection, &column_names);
        let covered_cells = data_table_vm::covered_cells(has_cell_selection, &selection, &column_names);
        let delete_row_indices = data_table_vm::delete_row_indices(covered.as_ref(), &self.selected_rows);
        let delete_row_count = data_table_vm::delete_row_count(covered.as_ref(), self.selected_rows.len());
        let export_enabled = data_table_vm::export_enabled(
            self.export_table_name,
            row_count,
            self.capability,
        );
        let has_pagination = data_table_vm::has_pagination(
            self.paging.map(|p| p.page),
            self.paging.map(|p| p.page_size),
            self.paging.map(|p| p.total_rows),
            self.on_page_change.is_some(),
            self.on_page_size_change.is_some(),
        );
        let has_filters = data_table_vm::has_filter_bar(
            self.filters.map(|f| f.len()).unwrap_or(0),
            self.on_remove_filter.is_some(),
            self.on_clear_filters.is_some(),
        );
        let has_filter_editor = data_table_vm::has_filter_editor(
            self.on_add_filter.is_some(),
            self.on_update_filter.is_some(),
            self.on_remove_filter.is_some(),
            self.on_clear_filters.is_some(),
            self.on_filter_logic_change.is_some(),
            self.on_apply_filters.is_some(),
            self.on_filter_panel_open_change.is_some(),
        );

        let mut root = col()
            .key(self.key.clone())
            .min_h(0.0)
            .min_w(0.0)
            .grow(1.0)
            .radius(k.radius.md)
            .border_sides([1.0, 1.0, 1.0, 1.0], k.c.border_weak)
            .fill(k.c.surface_base);

        // ── The filter surface: the editor, else the bar, else nothing ──
        if has_filter_editor {
            let empty: &[FilterCondition] = &[];
            let filters = self.filters.unwrap_or(empty);
            let filter_columns: Vec<FilterColumn> = self
                .columns
                .iter()
                .map(|c| FilterColumn {
                    name: c.name.clone(),
                    column_type: c.ty.clone(),
                })
                .collect();
            let mut editor = FilterEditor::<S>::new(
                &filter_columns,
                filters,
                self.filter_logic,
                empty,
                FilterConnector::And,
                true, // the panel is open; its own collapse owns the mount
            )
            .loading(self.loading);
            if let Some(state) = self.filter_editor_state {
                editor = editor.host_state(state);
            }
            if let Some(host) = &self.filter_editor_host {
                let host = Rc::clone(host);
                editor = editor.host_mut(move |s: &mut S| host(s));
            }
            if let Some(operators) = self.supported_operators {
                editor = editor.supported_operators(operators);
            }
            if let Some(mode) = self.filter_mode {
                editor = editor.mode(mode);
            }
            editor = editor
                .raw_where_supported(self.filter_raw_where_supported)
                .any_column(self.any_column_searchable, self.any_column_skipped)
                .raw_where(self.applied_raw_where, self.raw_where_context);
            // The shell's own callbacks, threaded exactly as React's JSX.
            if let Some(f) = &self.on_filter_panel_open_change {
                let f = Rc::clone(f);
                editor = editor.on_open_change(move |s: &mut S, open: bool| f(s, open));
            }
            if let Some(f) = &self.on_filter_logic_change {
                let f = Rc::clone(f);
                editor = editor.on_logic_change(move |s: &mut S, logic| f(s, logic));
            }
            if let Some(f) = &self.on_update_filter {
                let f = Rc::clone(f);
                editor = editor.on_change(move |s: &mut S, index, condition| f(s, index, condition));
            }
            if let Some(f) = &self.on_add_filter {
                let f = Rc::clone(f);
                editor = editor.on_add(move |s: &mut S, condition| f(s, condition));
            }
            if let Some(f) = &self.on_remove_filter {
                let f = Rc::clone(f);
                editor = editor.on_remove(move |s: &mut S, index| f(s, index));
            }
            if let Some(f) = &self.on_apply_filters {
                let f = Rc::clone(f);
                editor = editor.on_apply(move |s: &mut S| f(s));
            }
            if let Some(f) = &self.on_clear_filters {
                let f = Rc::clone(f);
                editor = editor.on_clear(move |s: &mut S| f(s));
            }
            if let Some(f) = &self.on_filter_mode_change {
                let f = Rc::clone(f);
                editor = editor.on_mode_change(move |s: &mut S, mode| f(s, mode));
            }
            if let Some(f) = &self.on_any_column_value_change {
                let f = Rc::clone(f);
                editor = editor.on_any_column_value_change(move |s: &mut S, value| f(s, value));
            }
            if let Some(f) = &self.on_apply_any_column {
                let f = Rc::clone(f);
                editor = editor.on_apply_any_column(move |s: &mut S, value| f(s, value));
            }
            if let Some(f) = &self.on_raw_where_value_change {
                let f = Rc::clone(f);
                editor = editor.on_raw_where_value_change(move |s: &mut S, value| f(s, value));
            }
            if let Some(f) = &self.on_apply_raw_where {
                let f = Rc::clone(f);
                editor = editor.on_apply_raw_where(move |s: &mut S, value| f(s, value));
            }
            if let Some(f) = &self.on_preview_sql {
                let f = Rc::clone(f);
                editor = editor.on_preview_sql(move |s: &mut S| f(s));
            }
            if let Some(f) = &self.on_export_filtered {
                let f = Rc::clone(f);
                editor = editor.on_export_filtered(move |s: &mut S| f(s));
            }
            if let Some(panel) = editor.build(dark) {
                root = root.child(panel);
            }
        } else if has_filters {
            let filters = self.filters.unwrap_or(&[]);
            let mut bar = FilterBar::<S>::new("data-table-filter-bar", filters).loading(self.loading);
            if let Some(f) = &self.on_remove_filter {
                let f = Rc::clone(f);
                bar = bar.on_remove(move |s: &mut S, index| f(s, index));
            }
            if let Some(f) = &self.on_clear_filters {
                let f = Rc::clone(f);
                bar = bar.on_clear(move |s: &mut S| f(s));
            }
            if let Some(bar_el) = bar.build(dark) {
                root = root.child(bar_el);
            }
        }

        // ── The selection bar (either selection half turns it on) ──
        if has_selection || has_cell_selection {
            let bar_selection = BarGridSelection::new(match selection.mode {
                GridSelectionMode::Cell => BarGridSelectionMode::Cell,
                // `None` and `Row` both render no cell summary in the bar
                // (its mode discriminator only gates that one span).
                GridSelectionMode::None | GridSelectionMode::Row => BarGridSelectionMode::Row,
            });
            let selected: Vec<usize> = self.selected_rows.iter().copied().collect();
            let mut bar = DataTableSelectionBar::<S>::new(bar_selection, row_count)
                .has_selection(has_selection)
                .has_cell_selection(has_cell_selection)
                .selected_rows(selected)
                .covered_rows(covered.clone().unwrap_or_default())
                .covered_columns(covered_columns)
                .covered_cells(covered_cells)
                .delete_row_indices(delete_row_indices.clone())
                .delete_row_count(delete_row_count)
                .primary_key_columns(self.primary_key_columns.clone())
                .export_enabled(export_enabled)
                .loading(self.loading);
            if let Some(f) = &self.on_select_all {
                let f = Rc::clone(f);
                bar = bar.on_select_all(move |s: &mut S| f(s));
            }
            if let Some(slot) = self.header_actions.clone() {
                bar = bar.header_actions(slot);
            }
            if let Some(f) = &self.on_delete_rows {
                let f = Rc::clone(f);
                bar = bar.on_delete_rows(move |s: &mut S, rows| f(s, rows));
            }
            if export_enabled {
                if let Some(host) = &self.shell_host {
                    let host = Rc::clone(host);
                    bar = bar.on_export(move |s: &mut S| {
                        let state = host(s);
                        data_table_vm::open_export_dialog(
                            &mut state.vm,
                            Some(&mut state.export),
                            None,
                        );
                    });
                }
            }
            root = root.child(bar.build(dark));
        }

        // ── The caller's status-bar slot ──
        if let Some(status) = self.status_bar.clone() {
            root = root.child(status);
        }

        // ── The keyboard hints (only while the full keyboard contract is on) ──
        let hints_mounted = self.keyboard_navigation && has_cell_selection;
        if hints_mounted {
            root = root.child(GridKeyboardHints::new(format!("{}-shortcuts", self.key)).build(dark));
        }

        // ── The scroll surface ──
        let grid_row_count = data_table_vm::grid_row_count(
            self.empty_placeholder.is_some(),
            row_count,
            self.loading,
        );
        let has_placeholder = self.empty_placeholder.is_some();
        let mut surface = col()
            .key(format!("{}-surface", self.key))
            .min_h(0.0)
            .grow(1.0)
            .label(t(data_table_vm::aria_grid_label_key(has_cell_selection)));
        if has_cell_selection {
            surface = surface
                .role("grid")
                .focusable(true)
                .active_descendant(active_cell_key(&self.key, &selection));
        }
        // React's `aria-rowcount` / `aria-colcount` / `aria-busy`, mirrored
        // as key-encoded zero-size probes (module docs).
        surface = surface
            .child(
                el()
                    .key(format!("{}-surface-rowcount-{grid_row_count}", self.key))
                    .a11y_hidden(),
            )
            .child(
                el().key(format!(
                    "{}-surface-colcount-{}",
                    self.key,
                    column_names.len() + usize::from(self.show_row_numbers)
                ))
                .a11y_hidden(),
            )
            .child(
                el()
                    .key(format!(
                        "{}-surface-busy-{}",
                        self.key,
                        usize::from(self.loading)
                    ))
                    .a11y_hidden(),
            );
        // The Delete key (React `handleDeleteKey`): only with the delete
        // path armed and a row set to act on.
        if let Some(on_delete) = &self.on_delete_rows {
            let on_delete = Rc::clone(on_delete);
            let selected: Vec<usize> = self.selected_rows.iter().copied().collect();
            let primary_keys = !self.primary_key_columns.is_empty();
            let editing = self.editing_cell;
            surface = surface.on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                if editing {
                    return false;
                }
                if ev.key != Key::Delete && ev.key != Key::Backspace {
                    return false;
                }
                if !primary_keys || selected.is_empty() {
                    return false;
                }
                let mut rows = selected.clone();
                rows.sort_unstable();
                on_delete(s, rows);
                true
            });
        }

        if has_placeholder {
            if let Some(placeholder) = self.empty_placeholder.clone() {
                surface = surface.child(grid_row(placeholder, has_cell_selection, &self.key, 0));
            }
        } else {
            let mut header = TableHeader::<S>::new(self.columns.clone(), self.sorts.clone())
                .key(format!("{}-header", self.key))
                .sortable(self.on_sort.is_some())
                .grid_semantics(has_cell_selection)
                .frozen_columns(self.frozen_columns)
                .show_row_numbers(self.show_row_numbers);
            if !self.column_widths.is_empty() {
                header = header.column_widths(self.column_widths.clone());
            }
            if let Some(f) = &self.on_sort {
                let f = Rc::clone(f);
                header = header.on_sort(move |s: &mut S, column, descending| f(s, column, descending));
            }
            if let Some(f) = &self.on_remove_sort {
                let f = Rc::clone(f);
                header = header.on_remove_sort(move |s: &mut S, column| f(s, column));
            }
            surface = surface.child(header.build(dark, layer));

            if let Some(body) = self.body.clone() {
                surface = surface.child(body);
            }

            if self.loading {
                surface = surface.child(grid_row(
                    loading_overlay(dark),
                    has_cell_selection,
                    &self.key,
                    1,
                ));
            } else if row_count == 0 {
                surface = surface.child(grid_row(
                    empty_overlay(dark),
                    has_cell_selection,
                    &self.key,
                    1,
                ));
            }
        }
        root = root.child(surface);

        // ── The pagination ──
        if has_pagination {
            let inputs = self.paging.unwrap_or_default();
            let mut pager = Pagination::<S>::new(inputs.page, inputs.page_size, inputs.total_rows as usize)
                .total_rows_kind(inputs.kind)
                .row_count(Some(row_count))
                .has_more(Some(row_count == inputs.page_size))
                .loading(self.loading)
                .counting(inputs.counting)
                .unstable_paging(inputs.unstable_paging)
                .estimate_unavailable(inputs.estimate_unavailable)
                .paging_hint(inputs.hint);
            if let Some(f) = &self.on_page_change {
                let f = Rc::clone(f);
                pager = pager.on_page_change(move |s: &mut S, page| f(s, page));
            }
            if let Some(f) = &self.on_page_size_change {
                let f = Rc::clone(f);
                pager = pager.on_page_size_change(move |s: &mut S, size| f(s, size));
            }
            if let Some(f) = &self.on_count_exact {
                let f = Rc::clone(f);
                pager = pager.on_count_exact(move |s: &mut S| f(s));
            }
            root = root.child(pager.build(dark, layer));
        }

        // ── The dialogs (the overlay layer, mounted by state) ──
        if let (Some(shell), Some(host)) = (self.shell_state, self.shell_host.clone()) {
            // React's exportOpen: the dialog's own `open` gates the paint,
            // the shell's mirror keeps the audit channel.
            let _ = shell.vm.export_open;
            let mut dialog = DataExportDialog::<S>::new(EXPORT_DIALOG_KEY, &shell.export);
            if let Some(f) = &self.on_export_filtered {
                let f = Rc::clone(f);
                dialog = dialog.on_close(move |s: &mut S| {
                    let state = host(s);
                    data_table_vm::close_export_dialog(&mut state.vm, Some(&mut state.export));
                    f(s);
                });
            } else {
                dialog = dialog.on_close(move |s: &mut S| {
                    let state = host(s);
                    data_table_vm::close_export_dialog(&mut state.vm, Some(&mut state.export));
                });
            }
            dialog.build(_s, dark, layer);
        }
        if let (Some(state), Some(snapshot), Some(host)) = (
            self.presentation_state,
            self.presentation_snapshot,
            self.presentation_host.clone(),
        ) {
            let mut dialog =
                ColumnPresentationDialog::<S>::new(state).snapshot(snapshot);
            if let Some(registry) = self.presentation_registry {
                dialog = dialog.registry(registry);
            }
            dialog = dialog.host_mut(move |s: &mut S| host(s));
            dialog.build(dark, layer);
        }

        root
    }
}

/// The key of the cell `aria-activedescendant` points at (module docs: the
/// format T396's body keeps).
fn active_cell_key(prefix: &str, selection: &GridSelection) -> String {
    match &selection.focus {
        Some(CellCoord {
            row_index,
            column_name,
        }) => format!("{prefix}-cell-{row_index}-{column_name}"),
        None => format!("{prefix}-cell-none"),
    }
}

/// React's `gridRow`: non-row content wrapped in a `row` + `gridcell` so
/// the ARIA grid only ever contains rows (the contract is off ⇒ no roles).
fn grid_row(content: El, has_cell_selection: bool, prefix: &str, index: usize) -> El {
    if !has_cell_selection {
        return content;
    }
    let cell = el()
        .key(format!("{prefix}-statusrow-{index}-cell"))
        .role("gridcell")
        .child(content);
    el()
        .key(format!("{prefix}-statusrow-{index}"))
        .role("row")
        .child(cell)
}

/// React's loading overlay: `role="status"` + polite live region, the
/// spinner and the label, centred in a `min-h-28` strip.
fn loading_overlay(dark: bool) -> El {
    let k = kit(dark);
    row()
        .key("data-table-loading")
        .role("status")
        .live_region(LiveRegion::Polite)
        .min_h(OVERLAY_MIN_H)
        .min_w(0.0)
        .grow(1.0)
        .items_center()
        .justify_center()
        .gap(8.0)
        .child(Spinner::new("data-table-loading-spinner").size(SpinnerSize::Lg).build(dark))
        .child(
            text(t("dataTable.loading"))
                .key("data-table-loading-text")
                .font(k.ty.small)
                .color(k.c.text_muted),
        )
}

/// React's empty overlay: the two lines, centred in the same strip.
fn empty_overlay(dark: bool) -> El {
    let k = kit(dark);
    col()
        .key("data-table-empty")
        .role("status")
        .live_region(LiveRegion::Polite)
        .min_h(OVERLAY_MIN_H)
        .min_w(0.0)
        .grow(1.0)
        .items_center()
        .justify_center()
        .gap(4.0)
        .px(16.0)
        .child(
            text(t("dataTable.empty"))
                .key("data-table-empty-text")
                .font(k.ty.small)
                .color(k.c.text_muted),
        )
        .child(
            text(t("dataTable.emptyHint"))
                .key("data-table-empty-hint")
                .font(k.ty.small)
                .color(k.alpha(k.c.text_muted, 0.8)),
        )
}
