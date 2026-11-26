//! Column-mapping editor shell — T023-S2.
//!
//! Ported from `ColumnMappingEditor` in
//! `packages/frontend/src/windows/data-transfer/ColumnMappingEditor.tsx`
//! (React lines 35–193): the target-table input + create-new toggle,
//! the auto-match / clear-unmapped buttons, the unmapped-target warning,
//! the table header, and the row-list assembly over the S1
//! [`ColumnMappingRow`]. The recordset editor (React 195–440, T023-S3)
//! is mounted by the shell in the `data-transfer-recordset` slot (see
//! [`ColumnMappingEditor::recordset_section`]); the source-filter editor
//! (T204) is not in this slice — the shell renders its mount point (see
//! below) and nothing else for it.
//!
//! ## Key convention
//!
//! React `data-testid` becomes the element key, verbatim where the id is
//! already unique:
//!
//! | part | key |
//! |---|---|
//! | editor root | `data-transfer-column-editor` |
//! | top row (target table + toggle) | `data-transfer-column-top` (internal) |
//! | target-table label column | `data-transfer-target-table-label` (internal) |
//! | target-table input | `data-transfer-target-table-input` |
//! | create-new toggle | `data-transfer-create-new-toggle` |
//! | actions row | `data-transfer-column-actions` (internal) |
//! | auto-match button | `data-transfer-auto-match` |
//! | clear-unmapped button | `data-transfer-clear-unmapped` |
//! | unmapped-target warning | `data-transfer-unmapped-target-warning` |
//! | table container | `data-transfer-column-table` (internal) |
//! | table header | `data-transfer-column-header` (internal) |
//! | rows scroll container | `data-transfer-column-rows` (internal) |
//! | one row | `data-transfer-column-row-{source}` |
//! | recordset editor (T023-S3) | `data-transfer-recordset` |
//! | source-filter mount (T204 fills it) | `data-transfer-source-filter` |
//!
//! React repeats the row testid `data-transfer-column-row` on every row;
//! Rune keys must be unique among siblings, so the shell suffixes the
//! source column name (the same rule S1 documents for the probe key).
//!
//! ## Mounting decision (T023-S1 D3, resolved here)
//!
//! Every S1 row's `build` ends in `rune_kit::root`, which forces the row
//! to full-window size and adds its own overlay layer — stacking rows as
//! built would nest N full-window `kit-root`s, each stretching its row to
//! the window. S2 therefore mounts rows through
//! [`ColumnMappingRow::build_inner`]: the row without its kit root,
//! returning the row element plus the overlay layer its target select's
//! listbox needs. The shell calls `build_inner` once per row, collects
//! the layers into the one `layer` the caller passes in, and returns the
//! inner editor element — the caller (`Section` wiring, or the test
//! probe) calls `rune_kit::root` exactly once over the whole editor.
//! The tests pin this: exactly one `kit-root` node exists for any number
//! of rows.
//!
//! ## T307 placeholder: the mapping helpers live in `transfer_mapping_view`
//!
//! The shell depends on `transferMappingView.ts` (T307, not ported):
//! `normalizeColumnMappings`, `autoMatchColumnMappings`,
//! `clearUnmappedColumnMappings`, `unmappedTargetColumns`. Until T307
//! lands, [`super::transfer_mapping_view`] carries them as pure
//! functions over [`TransferTableVm`] — each mirroring its TypeScript
//! source line for line — plus the table model they operate on. They
//! are the VM stand-in the task prescribes: local pure logic, labelled
//! as such, for T307 to replace with the real port. The shell never
//! pretends to be wired to T307: every mapping-logic call site goes
//! through that module and no other mapping logic exists in the shell.
//!
//! ## T204 mount point
//!
//! `SourceFilterEditor` (T204, not ported) is rendered by React after
//! the recordset editor. S2 renders an empty keyed
//! `data-transfer-source-filter` shell — the mount point T204 fills —
//! exactly as `AiChatPanel` leaves empty keyed shells for the slices it
//! does not own. The filter model (`TransferSourceFilter`) is not
//! modelled here; T204 owns it.
//!
//! ## Documented deviations from React
//!
//! * **Text commits on submit, not per keystroke** (inherited from S1):
//!   the target-table input commits its `TextEdit` buffer on submit
//!   (Enter), like the row inputs. React commits
//!   `{ targetTable }` on every keystroke.
//! * **`onTargetTableBlur` fires on submit.** Rune has no focus-loss
//!   hook on a text field (`El` exposes no `on_blur`; the parent's
//!   `onTargetTableCommit` in `TransferMappingStep.tsx:84` is what the
//!   blur is for), so the builder's `on_target_table_blur` callback
//!   runs from the target-table submit, right after the
//!   `{ targetTable }` patch — the moment the user has finished editing
//!   the name. Tabbing away without pressing Enter does not fire it.
//! * **The header is sticky by construction, not by `sticky`.** React
//!   puts the header inside the scroll container with `sticky top-0`;
//!   Rune has no sticky positioning, so the header sits above the rows'
//!   scroll container as a sibling. The observable layout is the same:
//!   the header stays put while the rows scroll beneath it.
//! * **No `flex-wrap`.** React's top row and actions row wrap; Rune
//!   rows do not wrap, so on a narrow window the top row lays its two
//!   children out in one line instead of wrapping. The gate's 960pt
//!   minimum still fits both.
//! * **Uppercase header labels** are produced with
//!   `str::to_uppercase` on the i18n text (a no-op for the zh-CN values,
//!   the same rendering React's `uppercase` class gives the en values).
//! * **Invalid target-table name** (create-new with a blank name) gets
//!   the `danger.fg` border only. React also sets `aria-invalid`; Rune
//!   has no equivalent attribute, and `ElementInfo` does not expose
//!   border colour, so the invalid state is not tree-asserted —
//!   `show_target_type` / `show_create_new_toggle` and the blank-name
//!   guard are asserted through the toggle behaviour instead.
//! * **Unwired renders read-only.** Like `AiChatPanel` with no
//!   accessor, a shell built without `editor_mut` shows the model
//!   values as plain text (the row's own no-edit fallback) instead of
//!   focusable fields; no callback exists to drive, so nothing can
//!   commit. The S1 `TextField` debug assertion (a field without
//!   `on_edit` cannot take focus) is what forces this shape.
//! * **Disabled is inert in the handler, not only in the DOM.**
//!   Every commit closure returns early when `disabled`, mirroring
//!   React's `commit()` — the kit controls are additionally built
//!   disabled, so the guard is the second gate, not the only one.

// The production consumer is the transfer `Section` wiring (not yet
// ported); until then the component tests are the only callers, so the
// dead-code lint is silenced here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec, TextEdit, TextStyle, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use super::column_mapping_row::{
    target_options, ColumnMappingPatch, ColumnMappingRow, ColumnMappingRowVm,
    TransferColumnMapping,
};
use super::recordset_editor::{
    RecordsetEditor, RecordsetEditorVm, RecordsetHost, TransferRecordset,
};
use super::transfer_mapping_view::{
    auto_match_column_mappings, clear_unmapped_column_mappings,
    normalize_column_mappings, show_create_new_toggle, show_target_type,
    unmapped_target_columns, ColumnMappingTablePatch, TransferTableVm,
};
use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

// ---------------------------------------------------------------------------
// Host-owned editor state
// ---------------------------------------------------------------------------

/// The per-row host state: the two `TextEdit` buffers the row edits into
/// and the target select's open flag. The shell owns one per normalized
/// mapping row, keyed by source column, and re-seeds the buffers from the
/// model whenever a patch lands (see [`ColumnMappingEditorState::resync_rows`]).
#[derive(Clone, Debug)]
pub struct ColumnMappingRowHost {
    pub source_column: String,
    pub target_column_edit: TextEdit,
    pub target_native_type_edit: TextEdit,
    pub target_select_open: bool,
}

impl ColumnMappingRowHost {
    fn new(mapping: &TransferColumnMapping) -> Self {
        Self {
            source_column: mapping.source_column.clone(),
            target_column_edit: TextEdit::new(mapping.target_column.clone()),
            target_native_type_edit: TextEdit::new(
                mapping.target_native_type.clone().unwrap_or_default(),
            ),
            target_select_open: false,
        }
    }
}

/// The host-owned editor state: the table snapshot, the two shell props,
/// and the edit buffers. The shell takes `&state` each frame and drives
/// every callback through one `editor_mut` accessor, the same shape as
/// `AiChatPanel`'s `panel_mut`.
#[derive(Clone, Debug)]
pub struct ColumnMappingEditorState {
    pub table: TransferTableVm,
    pub structure_mode: bool,
    pub disabled: bool,
    pub target_table_edit: TextEdit,
    pub rows: Vec<ColumnMappingRowHost>,
    /// The recordset editor's host state (T023-S3): its edit buffers
    /// and the order select's open flag, re-seeded from the model
    /// whenever a recordset patch lands (see [`Self::resync_recordset`]).
    pub recordset_host: RecordsetHost,
}

impl ColumnMappingEditorState {
    pub fn new(table: TransferTableVm, structure_mode: bool, disabled: bool) -> Self {
        let target_table_edit = TextEdit::new(table.target_table.clone());
        let mut state = Self {
            table,
            structure_mode,
            disabled,
            target_table_edit,
            rows: Vec::new(),
            recordset_host: RecordsetHost::default(),
        };
        state.resync_rows();
        state.resync_recordset();
        state
    }

    /// The normalized mapping rows the shell renders (the same list React
    /// maps over: `normalizeColumnMappings(table)`).
    pub fn mappings(&self) -> Vec<TransferColumnMapping> {
        normalize_column_mappings(&self.table)
    }

    /// Apply a shell patch to the table and re-seed every edit buffer from
    /// the new model, mirroring React's controlled inputs (which re-render
    /// from `table` after every commit). The target-table buffer is only
    /// re-seeded when the patch carries a new target-table value, so an
    /// unrelated patch cannot wipe a draft the user is still typing.
    pub fn apply_table_patch(&mut self, patch: &ColumnMappingTablePatch) {
        patch.apply_to(&mut self.table);
        if patch.target_table.is_some() {
            self.target_table_edit
                .replace_text(self.table.target_table.clone());
        }
        if patch.recordset.is_some() {
            self.resync_recordset();
        }
        self.resync_rows();
    }

    /// Re-seed the recordset editor's buffers from the model, keyed by
    /// the table's primary keys (`table.sourcePrimaryKeys ?? []`).
    pub fn resync_recordset(&mut self) {
        let primary_keys = self
            .table
            .source_primary_keys
            .clone()
            .unwrap_or_default();
        self.recordset_host
            .resync(&self.table.recordset, &primary_keys);
    }

    /// Rebuild the per-row hosts from the current normalized mappings,
    /// reusing a host (with its select-open flag) when the same source
    /// column is still present, and re-seeding both edit buffers from the
    /// mapping — so an auto-match or clear that rewrites the model cannot
    /// leave a stale buffer behind.
    pub fn resync_rows(&mut self) {
        let mappings = normalize_column_mappings(&self.table);
        let mut old: HashMap<String, ColumnMappingRowHost> = std::mem::take(&mut self.rows)
            .into_iter()
            .map(|host| (host.source_column.clone(), host))
            .collect();
        let mut next = Vec::with_capacity(mappings.len());
        for mapping in &mappings {
            let mut host = match old.remove(&mapping.source_column) {
                Some(host) => host,
                None => ColumnMappingRowHost::new(mapping),
            };
            host.target_column_edit
                .replace_text(mapping.target_column.clone());
            host.target_native_type_edit.replace_text(
                mapping.target_native_type.clone().unwrap_or_default(),
            );
            next.push(host);
        }
        self.rows = next;
    }

    /// The S1 view model for row `index`, or `None` when the index is
    /// outside the normalized rows (defensive: the shell skips it).
    pub fn row_vm(&self, index: usize) -> Option<ColumnMappingRowVm> {
        let mappings = self.mappings();
        let mapping = mappings.get(index)?.clone();
        let table = &self.table;
        let target_columns = table.target_columns.clone().unwrap_or_default();
        Some(ColumnMappingRowVm {
            row: mapping.clone(),
            create_new: table.create_new,
            show_target_type: show_target_type(self.structure_mode, table),
            source_type: table
                .source_type(&mapping.source_column)
                .map(str::to_owned),
            target_options: target_options(
                &target_columns,
                &mapping.target_column,
                t("transfer.mapping.pickTargetColumn"),
            ),
            target_select_open: self
                .rows
                .get(index)
                .map(|host| host.target_select_open)
                .unwrap_or(false),
            disabled: self.disabled,
        })
    }
}

type EditorFn<S> = Rc<dyn Fn(&mut S) -> &mut ColumnMappingEditorState>;
type TableChange<S> = Rc<dyn Fn(&mut S, ColumnMappingTablePatch)>;
type EditFn<S> = Rc<dyn for<'a> Fn(&'a mut S) -> &'a mut TextEdit>;
type Action<S> = Rc<dyn Fn(&mut S)>;

/// `ColumnMappingEditor` (React 35–193): the target-table input +
/// create-new toggle, the auto-match / clear-unmapped buttons, the
/// unmapped-target warning, the table header, and the row list.
///
/// `build` returns the inner editor element, **not** a kit root: the
/// caller roots the editor once (see the module docs on the mounting
/// decision). The rows' select listboxes are pushed into `layer`, which
/// the caller mounts alongside the root.
pub struct ColumnMappingEditor<'a, S: 'static> {
    state: &'a ColumnMappingEditorState,
    editor: Option<EditorFn<S>>,
    on_change: Option<TableChange<S>>,
    on_target_table_blur: Option<Action<S>>,
    existing_table_only: bool,
}

impl<'a, S: 'static> ColumnMappingEditor<'a, S> {
    pub fn new(state: &'a ColumnMappingEditorState) -> Self {
        Self {
            state,
            editor: None,
            on_change: None,
            on_target_table_blur: None,
            existing_table_only: false,
        }
    }

    /// The state accessor every callback drives, e.g.
    /// `|state| &mut state.editor`. One accessor wires the whole editor;
    /// without it the shell renders read-only (see the module docs).
    pub fn editor_mut(
        mut self,
        f: impl Fn(&mut S) -> &mut ColumnMappingEditorState + 'static,
    ) -> Self {
        self.editor = Some(Rc::new(f));
        self
    }

    /// React `onChange`: receives the table patch for every commit the
    /// shell makes (target-table submit, toggle, auto-match, clear,
    /// and each row's patch, re-expressed as a full `columnMappings`).
    pub fn on_change(
        mut self,
        f: impl Fn(&mut S, ColumnMappingTablePatch) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// React `onTargetTableBlur`. Rune has no focus-loss hook, so this
    /// fires from the target-table submit instead (see the module docs).
    pub fn on_target_table_blur(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_target_table_blur = Some(Rc::new(f));
        self
    }

    /// The native insert-only worker takes neither a draft table name nor
    /// recordset/filter parameters. Exclude controls that it cannot honor.
    pub fn existing_table_only(mut self) -> Self {
        self.existing_table_only = true;
        self
    }

    pub fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let state = self.state;
        let table = &state.table;
        let mappings = state.mappings();
        let unmapped_targets = unmapped_target_columns(table);
        let show_types = show_target_type(state.structure_mode, table);
        let target_table_invalid =
            table.create_new && table.target_table.trim().is_empty();

        let change: TableChange<S> = self
            .on_change
            .clone()
            .unwrap_or_else(|| Rc::new(|_, _| {}));
        let editor = self.editor.clone();

        // React root: `flex min-h-0 flex-1 flex-col gap-3`.
        let mut root = col()
            .key("data-transfer-column-editor")
            .w_full()
            .h_full()
            .min_w(0.0)
            .min_h(0.0)
            .gap(12.0);

        if !self.existing_table_only {
            root = root.child(self.top_row(k, dark, &change, &editor, target_table_invalid));
        }
        root = root.child(self.actions_row(k, &change, &mappings));

        if !unmapped_targets.is_empty() {
            let warning = t("transfer.mapping.unmappedTargetWarning")
                .replace("{columns}", &unmapped_targets.join(", "));
            root = root.child(
                text(warning)
                    .key("data-transfer-unmapped-target-warning")
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg)
                    .wrap(),
            );
        }

        // React table: `min-h-0 flex-1 overflow-auto rounded-lg border
        // border-edge bg-surface`. The header lives above the rows'
        // scroll container (sticky by construction, see the module docs).
        let mut table_el = col()
            .key("data-transfer-column-table")
            .w_full()
            .min_w(0.0)
            .grow(1.0)
            .min_h(0.0)
            .fill(k.c.surface_base)
            .border(1.0, k.c.border_base);
        table_el = table_el.child(self.header_row(k, show_types));
        table_el = table_el.child(self.rows_list(dark, layer, &change, &editor));
        root = root.child(table_el);

        // Mount points the slices below own: T023-S3 fills
        // `data-transfer-recordset` (see `recordset_section`), T204
        // fills `data-transfer-source-filter`. The filter shell is an
        // empty keyed shell, as `AiChatPanel` does for T195/T334.
        if self.existing_table_only {
            return root;
        }
        let (recordset_el, recordset_layer) = self.recordset_section(dark, &change, &editor);
        layer.extend(recordset_layer);
        root = root.child(recordset_el);
        root = root.child(
            col()
                .key("data-transfer-source-filter")
                .w_full()
                .min_w(0.0),
        );

        root
    }

    /// React 96–128: the target-table label + input and, when shown, the
    /// create-new toggle. `flex flex-wrap items-end gap-3` (no wrap in
    /// Rune — see the module docs).
    fn top_row(
        &self,
        k: &'static rune_kit::Kit,
        dark: bool,
        change: &TableChange<S>,
        editor: &Option<EditorFn<S>>,
        target_table_invalid: bool,
    ) -> El {
        let state = self.state;
        let table = &state.table;
        let disabled = state.disabled;

        let mut top = row()
            .key("data-transfer-column-top")
            .items_end()
            .gap(12.0)
            .w_full()
            .min_w(0.0);

        // `flex min-w-[12rem] flex-1 flex-col gap-1 text-sm`. The
        // `min-w-[12rem]` (192pt) is deliberately not set: `El::min_w`
        // is set semantics, so a 192 here would be dead — the
        // `.min_w(0.0)` below overwrites it.
        let mut label_col = col()
            .key("data-transfer-target-table-label")
            .grow(1.0)
            .min_w(0.0)
            .gap(4.0);
        label_col = label_col.child(
            text(t("transfer.mapping.targetTable"))
                .font(k.ty.ui)
                .color(k.c.text_muted),
        );
        label_col = label_col.child(self.target_table_field(k, dark, change, editor, target_table_invalid));
        top = top.child(label_col);

        if show_create_new_toggle(state.structure_mode, table) {
            // `flex items-center gap-2 pb-1 text-sm`: the kit checkbox
            // carries its label, so the row is the checkbox itself.
            let create_new = table.create_new;
            let target_table = table.target_table.clone();
            let change = Rc::clone(change);
            let mut checkbox = Checkbox::new("data-transfer-create-new-toggle", create_new)
                .label(t("transfer.mapping.createNew"))
                .disabled(disabled);
            checkbox = checkbox.on_toggle(move |s: &mut S| {
                if disabled {
                    return;
                }
                let next_create = !create_new;
                // React: enabling create-new clears the name — the name
                // of a table that does not exist yet has to be typed, not
                // prefilled with the source name.
                let next_target = if next_create {
                    String::new()
                } else {
                    target_table.clone()
                };
                change(
                    s,
                    ColumnMappingTablePatch {
                        create_new: Some(next_create),
                        target_table: Some(next_target),
                        ..ColumnMappingTablePatch::default()
                    },
                );
            });
            top = top.child(checkbox.build(k));
        }

        top
    }

    /// The target-table input. React's `<Input value={table.targetTable}
    /// onChange={commit({ targetTable })} onBlur={onTargetTableBlur}
    /// aria-invalid={createNew && !targetTable.trim()} />` — a default
    /// `h-10` (40pt) input; the commit happens on submit and the blur
    /// callback fires with it (see the module docs).
    fn target_table_field(
        &self,
        k: &'static rune_kit::Kit,
        dark: bool,
        change: &TableChange<S>,
        editor: &Option<EditorFn<S>>,
        invalid: bool,
    ) -> El {
        let state = self.state;
        let disabled = state.disabled;
        let Some(edit_acc) = editor.clone() else {
            // Unwired: the model value as plain text, the row's no-edit
            // fallback shape (a field without `on_edit` cannot take focus,
            // so there is no read-only field to build). This is a designed
            // path, not a programmer error, so there is no debug assertion
            // here — unlike the row's missing-edit fallback.
            return text(state.table.target_table.clone())
                .key("data-transfer-target-table-input")
                .font(k.ty.ui)
                .color(k.c.text_base)
                .truncate();
        };
        let mut field = TextField::new(
            "data-transfer-target-table-input",
            &state.target_table_edit,
        )
        .height(40.0)
        .disabled(disabled);
        if invalid {
            field = field.border(1.0, k.c.danger.fg);
        }
        let edit_fn: EditFn<S> = Rc::new(move |s: &mut S| &mut edit_acc(s).target_table_edit);
        field = field.on_edit({
            let edit_fn = Rc::clone(&edit_fn);
            move |s: &mut S| edit_fn(s)
        });
        let change = Rc::clone(change);
        let blur = self.on_target_table_blur.clone();
        field = field.on_submit(move |s: &mut S, _tx: &mut Tx| {
            if disabled {
                return;
            }
            let value = edit_fn(s).display().0.to_string();
            change(
                s,
                ColumnMappingTablePatch {
                    target_table: Some(value),
                    ..ColumnMappingTablePatch::default()
                },
            );
            if let Some(blur) = &blur {
                blur(s);
            }
        });
        field.build(dark)
    }

    /// React 131–149: the auto-match / clear-unmapped ghost small buttons.
    fn actions_row(
        &self,
        k: &'static rune_kit::Kit,
        change: &TableChange<S>,
        mappings: &[TransferColumnMapping],
    ) -> El {
        let state = self.state;
        let table = &state.table;
        let disabled = state.disabled;
        let mut actions = row()
            .key("data-transfer-column-actions")
            .gap(8.0)
            .w_full()
            .min_w(0.0);

        // `handleAutoMatch`: the target pool is the source columns while
        // creating a new table, the target columns otherwise.
        let source_columns = table.auto_match_sources(mappings);
        let target_columns = if table.create_new {
            source_columns.clone()
        } else {
            table.target_columns.clone().unwrap_or_default()
        };
        let existing = mappings.to_vec();
        let auto_change = Rc::clone(change);
        let mut auto_match = Button::new(t("transfer.mapping.autoMatch"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(disabled);
        auto_match = auto_match.on_press(move |s: &mut S| {
            if disabled {
                return;
            }
            auto_change(
                s,
                ColumnMappingTablePatch {
                    column_mappings: Some(auto_match_column_mappings(
                        &source_columns,
                        &target_columns,
                        &existing,
                    )),
                    ..ColumnMappingTablePatch::default()
                },
            );
        });
        actions = actions.child(auto_match.build(k).key("data-transfer-auto-match"));

        let mappings = mappings.to_vec();
        let clear_change = Rc::clone(change);
        let mut clear = Button::new(t("transfer.mapping.clearUnmapped"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(disabled);
        clear = clear.on_press(move |s: &mut S| {
            if disabled {
                return;
            }
            clear_change(
                s,
                ColumnMappingTablePatch {
                    column_mappings: Some(clear_unmapped_column_mappings(&mappings)),
                    ..ColumnMappingTablePatch::default()
                },
            );
        });
        actions = actions.child(clear.build(k).key("data-transfer-clear-unmapped"));

        actions
    }

    /// React 152–166: the sticky header — source / [source type] /
    /// target / [target type] / skip, `text-[11px] font-semibold
    /// uppercase tracking-wider text-fg-muted`, `px-3 py-1.5`, on the
    /// `bg-surface-alt` strip with a bottom border.
    fn header_row(&self, k: &'static rune_kit::Kit, show_types: bool) -> El {
        let mut header = row()
            .key("data-transfer-column-header")
            .items_center()
            .gap(12.0)
            .px(12.0)
            .py(6.0)
            .w_full()
            .min_w(0.0)
            .fill(k.c.surface_weak)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base);
        header = header.child(header_cell(
            t("transfer.mapping.sourceColumn").to_uppercase(),
            None,
            k,
        ));
        if show_types {
            header = header.child(header_cell(
                t("transfer.mapping.sourceType").to_uppercase(),
                Some(112.0),
                k,
            ));
        }
        header = header.child(header_cell(
            t("transfer.mapping.targetColumn").to_uppercase(),
            None,
            k,
        ));
        if show_types {
            header = header.child(header_cell(
                t("transfer.mapping.targetType").to_uppercase(),
                Some(144.0),
                k,
            ));
        }
        // `w-16 shrink-0 text-center`.
        header = header.child(
            row().w(64.0).center().child(header_cell(
                t("transfer.mapping.skip").to_uppercase(),
                None,
                k,
            )),
        );
        header
    }

    /// React 167–181: the mapping rows, one S1 row per normalized mapping,
    /// mounted through `build_inner` (see the module docs). Rows without a
    /// host entry — impossible after `resync_rows`, guarded anyway — are
    /// skipped rather than panicking on an index.
    fn rows_list(
        &self,
        dark: bool,
        layer: &mut Vec<El>,
        change: &TableChange<S>,
        editor: &Option<EditorFn<S>>,
    ) -> El {
        let state = self.state;
        let mappings = state.mappings();
        let mut list = col()
            .key("data-transfer-column-rows")
            .w_full()
            .min_w(0.0)
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            });
        for (index, mapping) in mappings.iter().enumerate() {
            let Some(vm) = state.row_vm(index) else {
                continue;
            };
            let Some(host) = state.rows.get(index) else {
                continue;
            };
            let row_key = format!("data-transfer-column-row-{}", mapping.source_column);
            let row_source = mapping.source_column.clone();
            let mut row_builder = ColumnMappingRow::<S>::new(row_key, vm);
            if let Some(acc) = editor.clone() {
                let change = Rc::clone(change);
                let disabled = state.disabled;
                // Each closure below captures its own clone of the
                // accessor; the row builder takes them one at a time.
                let change_acc = Rc::clone(&acc);
                let close_source = row_source.clone();
                row_builder = row_builder
                    .target_column_edit(&host.target_column_edit)
                    .target_native_type_edit(&host.target_native_type_edit)
                    .on_change(move |s: &mut S, patch: ColumnMappingPatch| {
                        if disabled {
                            return;
                        }
                        let next = {
                            let ed = change_acc(s);
                            let mut current = ed.mappings();
                            if index >= current.len() {
                                return;
                            }
                            patch.apply_to(&mut current[index]);
                            current
                        };
                        // Choosing closes the listbox: the kit's
                        // `on_choose` documents that closing is the app's
                        // call, so the shell closes the row it chose from
                        // (matched by source column, not by index — the
                        // index can shift across a resync).
                        {
                            let ed = change_acc(s);
                            if let Some(host) = ed
                                .rows
                                .iter_mut()
                                .find(|host| host.source_column == close_source)
                            {
                                host.target_select_open = false;
                            }
                        }
                        change(
                            s,
                            ColumnMappingTablePatch {
                                column_mappings: Some(next),
                                ..ColumnMappingTablePatch::default()
                            },
                        );
                    });
                let edit_acc = Rc::clone(&acc);
                row_builder = row_builder.on_target_column_edit(move |s: &mut S| {
                    &mut edit_acc(s).rows[index].target_column_edit
                });
                let edit_acc = Rc::clone(&acc);
                row_builder = row_builder.on_target_native_type_edit(move |s: &mut S| {
                    &mut edit_acc(s).rows[index].target_native_type_edit
                });
                let toggle_acc = Rc::clone(&acc);
                row_builder = row_builder.on_toggle_select(move |s: &mut S| {
                    let ed = toggle_acc(s);
                    ed.rows[index].target_select_open = !ed.rows[index].target_select_open;
                });
                let close_acc = Rc::clone(&acc);
                row_builder = row_builder.on_close_select(move |s: &mut S| {
                    close_acc(s).rows[index].target_select_open = false;
                });
            }
            let (row_el, row_layer) = row_builder.build_inner(dark);
            layer.extend(row_layer);
            list = list.child(row_el);
        }
        list
    }

    /// React 195–440: the recordset editor, mounted through
    /// `build_inner` like the rows (see the module docs). The view
    /// model reads React's own fallbacks: `table.sourceColumns ?? []`
    /// for the order options, `table.sourcePrimaryKeys ?? []` for the
    /// keys — not the mapping sources the shell falls back to
    /// elsewhere. The recordset commits arrive as a table patch
    /// carrying `{ recordset }`, `None` when the enable toggle
    /// switches the recordset off.
    fn recordset_section(
        &self,
        dark: bool,
        change: &TableChange<S>,
        editor: &Option<EditorFn<S>>,
    ) -> (El, Vec<El>) {
        let state = self.state;
        let table = &state.table;
        let vm = RecordsetEditorVm {
            columns: table.source_columns.clone().unwrap_or_default(),
            source_column_types: table.source_column_types.clone(),
            primary_keys: table.source_primary_keys.clone().unwrap_or_default(),
            recordset: table.recordset.clone(),
            order_select_open: state.recordset_host.order_open,
            disabled: state.disabled,
        };
        let mut builder = RecordsetEditor::<S>::new(vm)
            .start_edit(&state.recordset_host.start_edit)
            .end_edit(&state.recordset_host.end_edit)
            .tuple_start_edits(&state.recordset_host.tuple_start_edits)
            .tuple_end_edits(&state.recordset_host.tuple_end_edits)
            .limit_edit(&state.recordset_host.limit_edit);
        if let Some(acc) = editor.clone() {
            let disabled = state.disabled;
            let change = Rc::clone(change);
            builder = builder.on_change(move |s: &mut S, next: Option<TransferRecordset>| {
                if disabled {
                    return;
                }
                change(
                    s,
                    ColumnMappingTablePatch {
                        recordset: Some(next),
                        ..ColumnMappingTablePatch::default()
                    },
                );
            });
            let edit_acc = Rc::clone(&acc);
            builder = builder
                .on_start_edit(move |s: &mut S| &mut edit_acc(s).recordset_host.start_edit);
            let edit_acc = Rc::clone(&acc);
            builder =
                builder.on_end_edit(move |s: &mut S| &mut edit_acc(s).recordset_host.end_edit);
            let edit_acc = Rc::clone(&acc);
            builder = builder.on_limit_edit(move |s: &mut S| {
                &mut edit_acc(s).recordset_host.limit_edit
            });
            let tuple_acc = Rc::clone(&acc);
            builder = builder.on_tuple_start_edit(move |s: &mut S, index: usize| {
                &mut tuple_acc(s).recordset_host.tuple_start_edits[index]
            });
            let tuple_acc = Rc::clone(&acc);
            builder = builder.on_tuple_end_edit(move |s: &mut S, index: usize| {
                &mut tuple_acc(s).recordset_host.tuple_end_edits[index]
            });
            let toggle_acc = Rc::clone(&acc);
            builder = builder.on_toggle_order(move |s: &mut S| {
                let host = &mut toggle_acc(s).recordset_host;
                host.order_open = !host.order_open;
            });
            let close_acc = Rc::clone(&acc);
            builder = builder.on_close_order(move |s: &mut S| {
                close_acc(s).recordset_host.order_open = false;
            });
        }
        builder.build_inner(dark)
    }
}

/// One header cell: 11pt semibold uppercase muted text. `width` is
/// `Some(w)` for the fixed `w-28` / `w-36` cells, `None` for the `flex-1`
/// source / target cells.
fn header_cell(label: String, width: Option<f32>, k: &rune_kit::Kit) -> El {
    let mut cell = text(label)
        .font(TextStyle {
            size: 11.0,
            weight: 600.0,
            tracking: 0.55,
            ..k.ty.small
        })
        .color(k.c.text_muted)
        .truncate();
    match width {
        Some(w) => cell = cell.w(w),
        None => cell = cell.grow(1.0).min_w(0.0),
    }
    cell
}
