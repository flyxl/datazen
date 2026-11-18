//! The bar above the data grid — a Rune port of
//! `packages/frontend/src/components/DataTable/DataTableSelectionBar.tsx`.
//!
//! One horizontal strip: an optional select-all checkbox, the selected-row /
//! cell-selection summaries on the left, and a right-aligned action cluster
//! (an optional `headerActions` slot, delete-rows and export). Loading text
//! trails the actions. The order, the gating conditions and the i18n keys are
//! the React block's, verbatim.
//!
//! # Key convention
//!
//! | part                     | key                                             |
//! |--------------------------|-------------------------------------------------|
//! | root                     | `data-table-selection-bar`                      |
//! | select-all checkbox      | `data-table-select-all`                         |
//! | selected-row summary     | `data-table-selected-summary`                   |
//! | cell-selection summary   | `data-table-selection-summary`                  |
//! | summary count probes     | `data-table-selection-summary-{rows,cols,cells}-<n>` |
//! | right spacer             | `data-table-selection-bar-spacer`               |
//! | actions container        | `data-table-selection-bar-actions`              |
//! | delete-rows button       | `data-table-delete-rows`                        |
//! | export button            | `data-table-export`                             |
//! | loading text             | `data-table-loading`                            |
//!
//! # The `data-dt-selection-*` attributes
//!
//! React exposes three counts on the cell-selection span as
//! `data-dt-selection-rows` / `-cols` / `-cells`. Rune's `El` has **no**
//! arbitrary `data-*` attribute API: `ElementInfo` (`runtime/elements.rs`)
//! reports only `key`, `role`, `label`, `description`, `text`, `help`, …
//! and the accessibility dump adds `states`/`name`/`description`. So the
//! three counts are mirrored as zero-size, `a11y_hidden()` child elements
//! whose **keys encode the values** (see the table above). They take no
//! layout space, stay out of the accessibility tree, and tests read the
//! trailing integer from each key. This is a deliberate limitation of the
//! host API, not a dropped behaviour.
//!
//! # Deliberate deviations
//!
//! * The select-all control is the shared `ui::toggle::Checkbox` (kit 16pt
//!   box + label). React's `text-xs text-fg-secondary` on the `<label>` is
//!   the kit's own type scale and muted ink instead — same control, half of
//!   a select state machine, no hand-rolled box.
//! * The delete button is hand-built from kit tokens in a `role="button"`
//!   row: kit `ButtonVariant::Danger` paints a filled surface, while React
//!   wants plain `text-danger` ink and `hover:bg-surface-raised`. Building
//!   the row keeps both icon and label in the danger token while retaining
//!   focus ring, tooltip (`title`) and keyboard activation from `on_press`.
//! * `ml-auto` on the actions container is a `grow(1.0)` spacer in front of
//!   it; Rune has no auto margin.
//! * `lucide-react`'s `Trash2` maps to the registry's `ICON_TRASH` (whose
//!   body is the Trash2 path); `Download` maps to `ICON_DOWNLOAD`.

use std::rc::Rc;

use rune::{el, icon, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::icons::{ICON_DOWNLOAD, ICON_TRASH};
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

const ROOT_KEY: &str = "data-table-selection-bar";
const BAR_PAD_X: f32 = 8.0; // Tailwind px-2
const BAR_PAD_Y: f32 = 6.0; // Tailwind py-1.5
const BAR_GAP: f32 = 8.0; // Tailwind gap-2
const ACTION_GAP: f32 = 6.0; // Tailwind gap-1.5
const BUTTON_PAD_X: f32 = 6.0; // Tailwind px-1.5
const BUTTON_PAD_Y: f32 = 2.0; // Tailwind py-0.5
const BUTTON_ICON: f32 = 12.0; // Tailwind h-3 w-3

/// The minimal slice of `stores/tableData/gridSelection.ts` this bar reads.
/// The whole store layer is intentionally left in the React tree; only the
/// `mode` discriminator affects the rendered DOM.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GridSelectionMode {
    #[default]
    Row,
    Cell,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GridSelection {
    pub(crate) mode: GridSelectionMode,
}

impl GridSelection {
    pub(crate) fn new(mode: GridSelectionMode) -> GridSelection {
        GridSelection { mode }
    }
}

type SelectAllFn<S> = Rc<dyn Fn(&mut S)>;
type DeleteRowsFn<S> = Rc<dyn Fn(&mut S, Vec<usize>)>;

/// The selection bar. Build with the props the view owns; callbacks are
/// optional at the type level so a probe can render any branch, but the
/// production caller wires both `on_select_all` and `on_export`.
pub(crate) struct DataTableSelectionBar<S: 'static> {
    has_selection: bool,
    has_cell_selection: bool,
    selection: GridSelection,
    selected_rows: Vec<usize>,
    row_count: usize,
    covered_rows: Vec<usize>,
    covered_columns: usize,
    covered_cells: usize,
    on_select_all: Option<SelectAllFn<S>>,
    header_actions: Option<El>,
    on_delete_rows: Option<DeleteRowsFn<S>>,
    delete_row_indices: Vec<usize>,
    delete_row_count: usize,
    primary_key_columns: Vec<String>,
    export_enabled: bool,
    on_export: Option<SelectAllFn<S>>,
    loading: bool,
}

// The view that mounts this bar lands in a later migration task;
// `tests/data_table_selection_bar.rs` is the caller today.
#[allow(dead_code)]
impl<S: 'static> DataTableSelectionBar<S> {
    pub(crate) fn new(selection: GridSelection, row_count: usize) -> DataTableSelectionBar<S> {
        DataTableSelectionBar {
            has_selection: false,
            has_cell_selection: false,
            selection,
            selected_rows: Vec::new(),
            row_count,
            covered_rows: Vec::new(),
            covered_columns: 0,
            covered_cells: 0,
            on_select_all: None,
            header_actions: None,
            on_delete_rows: None,
            delete_row_indices: Vec::new(),
            delete_row_count: 0,
            primary_key_columns: Vec::new(),
            export_enabled: false,
            on_export: None,
            loading: false,
        }
    }

    /// React `hasSelection` — row selection is a real set for this grid.
    pub(crate) fn has_selection(mut self, has: bool) -> DataTableSelectionBar<S> {
        self.has_selection = has;
        self
    }

    /// React `hasCellSelection` — the cell-selection contract is on.
    pub(crate) fn has_cell_selection(mut self, has: bool) -> DataTableSelectionBar<S> {
        self.has_cell_selection = has;
        self
    }

    /// React `selectedRows`. Only `size` is read, so a `Vec` of row indices
    /// carries the same information as the React `ReadonlySet`.
    pub(crate) fn selected_rows(mut self, rows: Vec<usize>) -> DataTableSelectionBar<S> {
        self.selected_rows = rows;
        self
    }

    /// React `coveredRows`. Only `length` is read (and echoed as a count).
    pub(crate) fn covered_rows(mut self, rows: Vec<usize>) -> DataTableSelectionBar<S> {
        self.covered_rows = rows;
        self
    }

    pub(crate) fn covered_columns(mut self, count: usize) -> DataTableSelectionBar<S> {
        self.covered_columns = count;
        self
    }

    pub(crate) fn covered_cells(mut self, count: usize) -> DataTableSelectionBar<S> {
        self.covered_cells = count;
        self
    }

    pub(crate) fn on_select_all(mut self, f: impl Fn(&mut S) + 'static) -> DataTableSelectionBar<S> {
        self.on_select_all = Some(Rc::new(f));
        self
    }

    /// The React `headerActions` slot — an already-built element.
    pub(crate) fn header_actions(mut self, slot: El) -> DataTableSelectionBar<S> {
        self.header_actions = Some(slot);
        self
    }

    pub(crate) fn on_delete_rows(
        mut self,
        f: impl Fn(&mut S, Vec<usize>) + 'static,
    ) -> DataTableSelectionBar<S> {
        self.on_delete_rows = Some(Rc::new(f));
        self
    }

    pub(crate) fn delete_row_indices(mut self, rows: Vec<usize>) -> DataTableSelectionBar<S> {
        self.delete_row_indices = rows;
        self
    }

    pub(crate) fn delete_row_count(mut self, count: usize) -> DataTableSelectionBar<S> {
        self.delete_row_count = count;
        self
    }

    /// React `primaryKeyColumns`; only its `length` gates the delete button.
    pub(crate) fn primary_key_columns(
        mut self,
        columns: Vec<String>,
    ) -> DataTableSelectionBar<S> {
        self.primary_key_columns = columns;
        self
    }

    pub(crate) fn export_enabled(mut self, enabled: bool) -> DataTableSelectionBar<S> {
        self.export_enabled = enabled;
        self
    }

    pub(crate) fn on_export(mut self, f: impl Fn(&mut S) + 'static) -> DataTableSelectionBar<S> {
        self.on_export = Some(Rc::new(f));
        self
    }

    pub(crate) fn loading(mut self, loading: bool) -> DataTableSelectionBar<S> {
        self.loading = loading;
        self
    }

    /// Builds the strip. Colours come from `kit(dark)` only.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let c = &k.c;
        let selected = self.selected_rows.len();
        let covered = self.covered_rows.len();
        let checked = self.row_count > 0 && selected == self.row_count;
        let indeterminate = selected > 0 && selected < self.row_count;

        let mut left = row()
            .key(format!("{ROOT_KEY}-left"))
            .gap(ACTION_GAP)
            .items_center()
            .min_w(0.0);

        if self.has_selection {
            let mut checkbox = Checkbox::new("data-table-select-all", checked)
                .indeterminate(indeterminate)
                .label(t("common.selectAll"));
            if let Some(select_all) = self.on_select_all.as_ref() {
                let select_all = Rc::clone(select_all);
                checkbox = checkbox.on_toggle(move |s: &mut S| select_all(s));
            }
            left = left.child(checkbox.build(k));
        }

        if self.has_selection && selected > 0 {
            let summary = format!(
                "{} {} / {} {}",
                t("dataTable.selected"),
                selected,
                self.row_count,
                t("common.rows")
            );
            left = left.child(
                text(summary)
                    .key("data-table-selected-summary")
                    .font(k.ty.small)
                    .color(c.text_faint)
                    .truncate(),
            );
        }

        if self.has_cell_selection && self.selection.mode == GridSelectionMode::Cell {
            // `t()` returns the raw template; placeholder substitution is the
            // caller's job (same as React's `t(key, { rows, cols })`).
            let summary = t("dataTable.selectionSummary")
                .replace("{rows}", &covered.to_string())
                .replace("{cols}", &self.covered_columns.to_string());
            left = left.child(
                row()
                    .key("data-table-selection-summary")
                    .items_center()
                    .min_w(0.0)
                    .child(
                        text(summary)
                            .key("data-table-selection-summary-text")
                            .font(k.ty.small)
                            .color(c.text_faint)
                            .truncate(),
                    )
                    // React's three `data-dt-selection-*` attributes, encoded
                    // as invisible, zero-size key probes (see module docs).
                    .child(
                        el()
                            .key(format!("data-table-selection-summary-rows-{covered}"))
                            .a11y_hidden(),
                    )
                    .child(
                        el()
                            .key(format!(
                                "data-table-selection-summary-cols-{}",
                                self.covered_columns
                            ))
                            .a11y_hidden(),
                    )
                    .child(
                        el()
                            .key(format!(
                                "data-table-selection-summary-cells-{}",
                                self.covered_cells
                            ))
                            .a11y_hidden(),
                    ),
            );
        }

        let mut actions = row()
            .key(format!("{ROOT_KEY}-actions"))
            .gap(ACTION_GAP)
            .items_center();
        if let Some(slot) = self.header_actions {
            actions = actions.child(slot);
        }

        // All three React gates must hold for the delete button to exist.
        if self.delete_row_count > 0 && !self.primary_key_columns.is_empty() {
            if let Some(on_delete) = self.on_delete_rows.as_ref() {
                let on_delete = Rc::clone(on_delete);
                let indices = self.delete_row_indices.clone();
                actions = actions.child(
                    row()
                        .key("data-table-delete-rows")
                        .role("button")
                        .label(t("dataTable.deleteRow"))
                        .help(t("dataTable.deleteRow"))
                        .gap(4.0)
                        .px(BUTTON_PAD_X)
                        .py(BUTTON_PAD_Y)
                        .radius(k.radius.sm)
                        .items_center()
                        .focusable(true)
                        .focus_ring(2.0, 2.0, c.border_focus)
                        .hover(c.surface_raised)
                        .on_press(move |s: &mut S| on_delete(s, indices.clone()))
                        .child(
                            icon(&ICON_TRASH, BUTTON_ICON)
                                .key("data-table-delete-rows-icon")
                                .color(c.danger.fg),
                        )
                        .child(
                            text(t("dataTable.deleteRow"))
                                .key("label")
                                .font(k.ty.small)
                                .color(c.danger.fg),
                        ),
                );
            }
        }

        if self.export_enabled {
            let mut export = Button::new(t("export.export"))
                .variant(ButtonVariant::GhostMuted)
                .control_size(ControlSize::Small)
                .icon(&ICON_DOWNLOAD)
                .help(t("export.export"));
            if let Some(on_export) = self.on_export.as_ref() {
                let on_export = Rc::clone(on_export);
                export = export.on_press(move |s: &mut S| on_export(s));
            }
            actions = actions.child(export.build(k).key("data-table-export"));
        }

        let mut bar = row()
            .key(ROOT_KEY)
            .w_full()
            .shrink()
            .items_center()
            .gap(BAR_GAP)
            .px(BAR_PAD_X)
            .py(BAR_PAD_Y)
            .border_sides([0.0, 0.0, 1.0, 0.0], c.border_weak)
            .fill(c.layer_01)
            .child(left)
            // `ml-auto` on the actions container in React.
            .child(el().key(format!("{ROOT_KEY}-spacer")).grow(1.0).min_w(0.0))
            .child(actions);

        if self.loading {
            bar = bar.child(
                text(t("common.loading"))
                    .key("data-table-loading")
                    .font(k.ty.small)
                    .color(c.text_faint),
            );
        }

        bar
    }
}
