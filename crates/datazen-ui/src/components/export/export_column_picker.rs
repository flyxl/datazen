//! T063: `components/export/ExportColumnPicker.tsx` ported to Rune.
//!
//! The one column picker of both export dialogs (DBR-06: the grid
//! `DataExportDialog` and the table-level `ExportDialog` share one list,
//! one display order, one set of actions). The list preserves the
//! caller's display order — the export writes the columns in exactly
//! this order — and "Reset to Default" restores the default selection
//! (every column), so a cleared or half-picked list has one obvious way
//! back instead of being re-toggled by hand.
//!
//! The picker itself is stateless: the host owns the columns and the
//! selection in its `S` (React's `columns` / `selected` props and the
//! four callbacks become function pointers into `S`, the same shape as
//! `QueryArtifactExport`). The production consumers are the two export
//! dialogs (not yet wired); until then the component tests are the only
//! callers, so reusable API items carry per-item `#[allow(dead_code)]`
//! (not a module-wide blanket).
//!
//! ## Keys (prefix defaults to `export-column-picker`)
//!
//! | part | key |
//! |------|-----|
//! | root | `<prefix>` |
//! | header label | `<prefix>-label` |
//! | select-all | `<prefix>-select-all` |
//! | select-all label | `<prefix>-select-all-label` |
//! | clear-all | `<prefix>-clear-all` |
//! | clear-all label | `<prefix>-clear-all-label` |
//! | reset-to-default | `<prefix>-reset-default` |
//! | reset-to-default label | `<prefix>-reset-default-label` |
//! | list | `<prefix>-list` |
//! | one row | `<prefix>-column-<name>` |
//! | row checkbox | `<prefix>-column-<name>-checkbox` |
//! | row name | `<prefix>-column-<name>-name` |
//! | row PK chip | `<prefix>-column-<name>-pk` |
//! | row PK chip text | `<prefix>-column-<name>-pk-text` |
//! | row type label | `<prefix>-column-<name>-type` |
//!
//! ## Deliberate deviations
//!
//! * **The row text does not toggle the PK chip or the type label.**
//!   React wraps the whole row in a `<label>`, so clicking anywhere —
//!   name, PK chip, type — toggles the checkbox. Here the checkbox box
//!   and the name text toggle (both call `on_toggle`); the PK chip and
//!   the type label are plain text, because one press handler on the
//!   row would double-fire together with the checkbox's own toggle.
//! * **The actions are never disabled.** React's three buttons are
//!   always clickable; the batch-export table picker (a sibling
//!   component, not this one) disables select-all when everything is
//!   already selected, but this picker does not.
//! * **The type label colour comes from `data_type_color`.** React
//!   paints it with `dataTypeTextClass` (a `--dt-*` CSS class); the
//!   port maps the same family through the kit tokens, as
//!   `support/data_type_colors.rs` does for cells.
//! * **The PK chip is hand-built.** React is `bg-accent/15` with 10pt
//!   medium accent text; the kit `Badge` renders a larger chip, so the
//!   row builds the small chip directly (accent fill at 15%, 10pt
//!   medium, `text_accent`).

use rune::{col, row, text, El, ScrollSpec, TextStyle};
use rune_kit::Kit;

use crate::i18n::t;
use crate::support::data_type_colors::data_type_color;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

/// React `ExportColumnPickerColumn`: one row of the list. `type` is
/// spelled `col_type` (`type` is a Rust keyword); the type label is
/// greyed next to the name when known, and `is_primary_key` shows the
/// PK chip.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by the export dialogs
pub struct ExportColumnPickerColumn {
    pub name: String,
    pub col_type: Option<String>,
    pub is_primary_key: bool,
}

#[allow(dead_code)] // reusable component; wired by the export dialogs
impl ExportColumnPickerColumn {
    /// A column with no known type and no PK mark.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            col_type: None,
            is_primary_key: false,
        }
    }

    /// The column's data-type label, shown greyed next to the name.
    pub fn with_type(mut self, col_type: impl Into<String>) -> Self {
        self.col_type = Some(col_type.into());
        self
    }

    /// Marks the column as a primary key (shows the PK chip).
    pub fn primary_key(mut self) -> Self {
        self.is_primary_key = true;
        self
    }
}

/// React's `selected.has(name)` on the app's `S`.
#[allow(dead_code)] // reusable component; wired by the export dialogs
pub type IsSelectedFn<S> = fn(&S, &str) -> bool;

/// React's `onToggle(name)` on the app's `S`.
#[allow(dead_code)] // reusable component; wired by the export dialogs
pub type ToggleColumnFn<S> = fn(&mut S, String);

/// One of React's `onSelectAll` / `onClearAll` / `onResetDefault` on the
/// app's `S`.
#[allow(dead_code)] // reusable component; wired by the export dialogs
pub type PickerActionFn<S> = fn(&mut S);

/// The picker. Columns are given in display order; the export writes
/// them in exactly this order.
#[allow(dead_code)] // reusable component; wired by the export dialogs
pub struct ExportColumnPicker<S: 'static> {
    key: String,
    columns: Vec<ExportColumnPickerColumn>,
    is_selected: IsSelectedFn<S>,
    on_toggle: ToggleColumnFn<S>,
    on_select_all: PickerActionFn<S>,
    on_clear_all: PickerActionFn<S>,
    on_reset_default: PickerActionFn<S>,
}

#[allow(dead_code)] // reusable component; wired by the export dialogs
impl<S: 'static> ExportColumnPicker<S> {
    pub fn new(
        columns: Vec<ExportColumnPickerColumn>,
        is_selected: IsSelectedFn<S>,
        on_toggle: ToggleColumnFn<S>,
        on_select_all: PickerActionFn<S>,
        on_clear_all: PickerActionFn<S>,
        on_reset_default: PickerActionFn<S>,
    ) -> Self {
        Self {
            key: "export-column-picker".to_string(),
            columns,
            is_selected,
            on_toggle,
            on_select_all,
            on_clear_all,
            on_reset_default,
        }
    }

    /// Override the key prefix (two pickers in one dialog need distinct
    /// keys; the default is one fixed string).
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// Builds the picker: the header (`export.selectColumns` label plus
    /// the three action buttons) over the scrolling checkbox list
    /// (`max-h-40`, bordered, `bg-surface`).
    pub fn build(self, s: &S, dark: bool) -> El {
        let k = kit(dark);
        col()
            .key(self.key.clone())
            .w_full()
            .min_w(0.0)
            .child(self.header(&k))
            .child(self.list(s, &k, dark))
    }

    /// React's header: the label left, the three actions right
    /// (`flex items-center justify-between`, `gap-3` between actions).
    /// The actions are tiny text buttons (`text-xs text-accent`), like
    /// the select-all / clear-all rows in `batch_export_parts.rs`.
    fn header(&self, k: &Kit) -> El {
        let select_all = self.on_select_all;
        let clear_all = self.on_clear_all;
        let reset_default = self.on_reset_default;
        let action = |key: String, label: &str, run: PickerActionFn<S>| {
            row()
                .key(key.clone())
                .role("button")
                .focusable(true)
                .center_y()
                .child(
                    text(label.to_owned())
                        .key(format!("{key}-label"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_accent),
                )
                .on_press(move |state: &mut S| run(state))
        };
        row()
            .key(format!("{}-header", self.key))
            .w_full()
            .min_w(0.0)
            .items_center()
            .justify_between()
            .gap(8.0)
            .mb(4.0)
            .child(
                text(t("export.selectColumns"))
                    .key(format!("{}-label", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                row()
                    .gap(12.0)
                    .shrink()
                    .child(action(
                        format!("{}-select-all", self.key),
                        t("common.selectAll"),
                        select_all,
                    ))
                    .child(action(
                        format!("{}-clear-all", self.key),
                        t("common.deselectAll"),
                        clear_all,
                    ))
                    .child(action(
                        format!("{}-reset-default", self.key),
                        t("export.resetColumns"),
                        reset_default,
                    )),
            )
    }

    /// React's list: `max-h-40 overflow-y-auto rounded-md border
    /// border-edge bg-surface p-2`, one row per column in display order.
    /// An empty column list renders the empty box, as React does (no
    /// empty-state text).
    fn list(&self, s: &S, k: &Kit, dark: bool) -> El {
        let mut list = col()
            .key(format!("{}-list", self.key))
            .w_full()
            .min_w(0.0)
            .max_h(160.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .radius(k.radius.md)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base)
            .padding(8.0);
        for col in &self.columns {
            list = list.child(self.row(s, k, col, dark));
        }
        list
    }

    /// One row: the checkbox, the name, the PK chip when the column is
    /// a key, and the type label when known (`flex items-center gap-2
    /// rounded px-1 py-0.5 hover:bg-surface-raised`). The checkbox box
    /// and the name both toggle; the chip and the type label do not
    /// (see the module docs).
    fn row(&self, s: &S, k: &Kit, col: &ExportColumnPickerColumn, dark: bool) -> El {
        let base = format!("{}-column-{}", self.key, col.name);
        let checked = (self.is_selected)(s, &col.name);
        let toggle = self.on_toggle;

        let name = col.name.clone();
        let mut checkbox = Checkbox::new(format!("{base}-checkbox"), checked);
        checkbox = checkbox.on_toggle(move |state: &mut S| toggle(state, name.clone()));

        let toggle_name = col.name.clone();
        let mut row_el = row()
            .key(base.clone())
            .w_full()
            .items_center()
            .gap(8.0)
            .px(4.0)
            .py(2.0)
            .radius(k.radius.sm)
            .hover(k.c.hover)
            .child(checkbox.build(k))
            .child(
                text(col.name.clone())
                    .key(format!("{base}-name"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .on_press(move |state: &mut S| toggle(state, toggle_name.clone())),
            );
        if col.is_primary_key {
            row_el = row_el.child(
                row()
                    .key(format!("{base}-pk"))
                    .px(4.0)
                    .py(1.0)
                    .radius(4.0)
                    .fill(k.alpha(k.c.bg_accent, 0.15))
                    .child(
                        text("PK")
                            .key(format!("{base}-pk-text"))
                            .font(TextStyle::new(10.0, 500.0))
                            .color(k.c.text_accent),
                    ),
            );
        }
        if let Some(col_type) = &col.col_type {
            row_el = row_el.child(
                text(col_type.clone())
                    .key(format!("{base}-type"))
                    .font(TextStyle::new(10.0, 400.0))
                    .color(data_type_color(Some(col_type), None, dark)),
            );
        }
        row_el
    }
}
