//! T267: `windows/data-transfer/TransferMappingStep.tsx` ported to Rune
//! (89 lines).
//!
//! The mapping step's table shell: the enabled-table sidebar (one
//! two-line button per table, the selected one washed) and the
//! column-mapping editor over the selected table. All mapping logic is
//! the T307 placeholder's (`transfer_mapping_view`) and the editor's
//! (T023-S2) — this shell only selects.
//!
//! ## Keys
//!
//! React `data-testid` becomes the element key; the per-item testid is
//! suffixed with the source table (the row rule the shell T023-S2 and
//! the Data Sync mapping panel T336 both set), which also carries
//! React's `data-source-table` (the T336 ruling for row metadata
//! attributes — the source rides the key itself):
//!
//! | part | key |
//! |---|---|
//! | root (React test id) | `data-transfer-mapping-step` |
//! | empty notice (React test id) | `data-transfer-mapping-empty` |
//! | sidebar (internal) | `data-transfer-mapping-sidebar` |
//! | sidebar header (internal) | `data-transfer-mapping-sidebar-header` |
//! | header label (internal) | `…-header-label` |
//! | one table item | `data-transfer-mapping-table-item-{source}` |
//! | item name line | `{base}-name` |
//! | item target line | `{base}-target` |
//! | the editor's flex slot (internal) | `data-transfer-mapping-editor-slot` |
//!
//! **The root key is on loan.** The window port (T147-S3) holds
//! `data-transfer-mapping-step` on its own step column while the seam
//! waits for this task; the two never mount together (the window
//! renders its seam column, not this shell). The wiring task that
//! replaces the seam hands the key back: the window's column takes an
//! internal key and this shell's root becomes the testid's only
//! carrier.
//!
//! ## The host's writes (React's props and effect)
//!
//! | React | Rune |
//! |---|---|
//! | `tables` / `selectedSourceTable` | the `tables` / `selected_source_table` build inputs, over `TransferTableVm` (the T307 stand-in) |
//! | `mode` | [`structure_mode`] — the seed input, not a render input |
//! | `disabled` | the editor state's own field (seeded through [`seed_editor_state`]); the list is never dimmed by it |
//! | `onSelectTable` | [`TransferMappingStep::on_select_table`] — one press per sidebar item, unguarded |
//! | `onUpdateTable` | [`TransferMappingStep::on_update_table`] — the editor's patches with the selected source applied (React 83) |
//! | `onTargetTableCommit` | [`TransferMappingStep::on_target_table_commit`] — the editor's target submit with the selected source (React 84) |
//! | the `useEffect` (36–41) | [`selection_correction`] — a pure query the host applies through its select channel when the tables or the selection change (the window state's `apply_inspect_rows` already clamps; the wiring task reuses the helper for the React rule exactly) |
//!
//! ## Documented deviations from React
//!
//! * **The editor state is host-held, seeded per selection.** React's
//!   `key={selected.sourceTable}` remounts the editor (resetting its
//!   row edit buffers) on every table switch; Rune cannot remount
//!   state, so the host re-seeds its one `ColumnMappingEditorState`
//!   through [`seed_editor_state`] whenever the selection changes —
//!   the same trade the shell itself documents for its host state.
//! * **The selected wash subsumes the hover.** React paints both the
//!   selected `bg-surface-alt` and `hover:bg-surface-alt`; the
//!   selected item takes the fill (`bg-surface-alt` → `layer_01`, the
//!   T141 ruling) and an unselected item takes the same colour as
//!   its hover overlay — the navigator precedent's either/or.
//! * **`truncate`** is the text's own clipping (the mapping panel's
//!   ruling): the name and target lines stay single-line and clip at
//!   the sidebar's edge, without the ellipsis glyph.
//! * **`text-left`** needs no counterpart: a `col` lays out from the
//!   leading edge.
//! * **The sidebar header's `uppercase`** is `str::to_uppercase` on
//!   the i18n text (the shell's own ruling; a no-op on the zh-CN
//!   value).
//! * **The item is a `role="button"` col, not a kit `Button`.** The
//!   React `<button>` is a two-line stack (the mono name, the muted
//!   arrow line); the kit button is a single-line fixed-height
//!   control, so the port follows the navigator's two-line list-item
//!   shape instead (`role` + `focusable` + `on_press`).
//! * **The editor mounts in a flex slot.** React gives the editor
//!   `flex-1` in the step row (basis 0, the sidebar's leftovers); the
//!   shell's own root is `w_full`, which as a plain row child would
//!   ask for the whole row and overflow past the fixed sidebar. The
//!   mount therefore wraps the shell in the editor's flex slot
//!   (basis 0, grow, shrink) and the editor fills it — the same
//!   geometry React's flex-1 produces.
//!
//! The props' own doc comment travels with the port verbatim in intent:
//! `disabled` is forwarded to the editor, not used to dim this
//! component — the table list stays selectable (a read-only act) and
//! must not be blocked while a prepare is in flight.

// The production consumer is the Data Transfer window wiring task (the
// seam in `data_transfer_window.rs`); until it wires the shell in, the
// component tests are the only callers.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec, TextStyle};
use rune_kit::Kit;

use super::column_mapping_editor::{ColumnMappingEditor, ColumnMappingEditorState};
use super::data_transfer_window_state::TransferMode;
use super::transfer_mapping_view::{ColumnMappingTablePatch, TransferTableVm};
use crate::i18n::t;
use crate::ui::kit::kit;

type EditorFn<S> = Rc<dyn Fn(&mut S) -> &mut ColumnMappingEditorState>;

/// React 32: `tables.filter((tbl) => tbl.enabled && tbl.sourceTable)` —
/// the enabled tables with a non-empty source (the sidebar's rows and
/// the effect's universe).
pub fn enabled_tables(tables: &[TransferTableVm]) -> Vec<&TransferTableVm> {
    tables
        .iter()
        .filter(|table| table.enabled && !table.source_table.is_empty())
        .collect()
}

/// React 33-34: the enabled row for `selectedSourceTable`, else the
/// first enabled row (the same cell the effect would correct to).
pub fn selected_table<'a>(
    tables: &'a [TransferTableVm],
    selected_source_table: &str,
) -> Option<&'a TransferTableVm> {
    let enabled = enabled_tables(tables);
    enabled
        .iter()
        .copied()
        .find(|table| table.source_table == selected_source_table)
        .or_else(|| enabled.first().copied())
}

/// React 36-41, the `useEffect` as a pure query: the first enabled
/// source table when the selection is stale (the list non-empty, the
/// selected table not among the enabled), else `None`. The host
/// applies the write through its select channel when it lands — a
/// render pass cannot write host state.
pub fn selection_correction(
    tables: &[TransferTableVm],
    selected_source_table: &str,
) -> Option<String> {
    let enabled = enabled_tables(tables);
    if enabled.is_empty() {
        return None;
    }
    if enabled
        .iter()
        .any(|table| table.source_table == selected_source_table)
    {
        return None;
    }
    Some(enabled[0].source_table.clone())
}

/// React 43: `mode === 'structure' || mode === 'structureAndData'`.
pub fn structure_mode(mode: TransferMode) -> bool {
    matches!(mode, TransferMode::Structure | TransferMode::StructureAndData)
}

/// React's `key={selected.sourceTable}` remount, as a seed: the host
/// builds its one editor state from the selected table — the `mode`
/// and `disabled` props are the editor's own inputs (the shell's
/// host-state trade) — whenever the selection changes.
pub fn seed_editor_state(
    selected: &TransferTableVm,
    mode: TransferMode,
    disabled: bool,
) -> ColumnMappingEditorState {
    ColumnMappingEditorState::new(selected.clone(), structure_mode(mode), disabled)
}

/// The mapping step shell (React 22-88): the sidebar and the editor
/// over the selected table, or the empty notice.
pub struct TransferMappingStep<'a, S: 'static> {
    tables: &'a [TransferTableVm],
    selected_source_table: &'a str,
    editor: &'a ColumnMappingEditorState,
    editor_mut: Option<EditorFn<S>>,
    on_select_table: Option<Rc<dyn Fn(&mut S, String)>>,
    on_update_table: Option<Rc<dyn Fn(&mut S, String, ColumnMappingTablePatch)>>,
    on_target_table_commit: Option<Rc<dyn Fn(&mut S, String)>>,
}

impl<'a, S: 'static> TransferMappingStep<'a, S> {
    /// The render inputs: the inspected tables, the selected source
    /// table, and the host-held editor state seeded for it (React's
    /// `mode` / `disabled` enter through [`seed_editor_state`]).
    pub fn new(
        tables: &'a [TransferTableVm],
        selected_source_table: &'a str,
        editor: &'a ColumnMappingEditorState,
    ) -> Self {
        Self {
            tables,
            selected_source_table,
            editor,
            editor_mut: None,
            on_select_table: None,
            on_update_table: None,
            on_target_table_commit: None,
        }
    }

    /// The editor's state accessor (the shell's `editor_mut`): one
    /// accessor wires the whole embedded editor; without it the shell
    /// renders read-only (the shell's own documented trade).
    pub fn editor_mut(
        mut self,
        f: impl Fn(&mut S) -> &mut ColumnMappingEditorState + 'static,
    ) -> Self {
        self.editor_mut = Some(Rc::new(f));
        self
    }

    /// React `onSelectTable(sourceTable)` — one press per sidebar
    /// item, unguarded (the props' doc comment: picking a different
    /// table is a read-only act and must not be blocked while a
    /// prepare is in flight).
    pub fn on_select_table(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_select_table = Some(Rc::new(f));
        self
    }

    /// React `onUpdateTable(sourceTable, patch)` (83): the editor's
    /// patch applied with the selected table's source.
    pub fn on_update_table(
        mut self,
        f: impl Fn(&mut S, String, ColumnMappingTablePatch) + 'static,
    ) -> Self {
        self.on_update_table = Some(Rc::new(f));
        self
    }

    /// React `onTargetTableCommit(sourceTable)` (84): the editor's
    /// target-table submit with the selected table's source.
    pub fn on_target_table_commit(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_target_table_commit = Some(Rc::new(f));
        self
    }

    /// The step (React 53-87): the sidebar and the editor over the
    /// selected table, in the step's `flex min-h-0 flex-1 gap-4` row.
    /// The caller roots the result once (the shell's mounting
    /// ruling); the editor's overlay layer rides `layer`.
    pub fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let enabled = enabled_tables(self.tables);

        // React 45-51: no enabled table — the muted notice, nothing
        // else (no sidebar, no editor).
        if enabled.is_empty() {
            return text(t("transfer.mapping.noTables"))
                .key("data-transfer-mapping-empty")
                .font(k.ty.text_small)
                .color(k.c.text_muted);
        }

        // React 33-34: the selected row or the first enabled row — a
        // non-empty list always selects (the empty branch returned).
        let selected = selected_table(self.tables, self.selected_source_table)
            .expect("a non-empty enabled list always selects");

        row()
            .key("data-transfer-mapping-step")
            .w_full()
            .grow(1.0)
            .min_w(0.0)
            .min_h(0.0)
            .gap(16.0)
            .child(self.sidebar(&k, &enabled, selected))
            .child(self.editor_mount(dark, layer, selected))
    }

    /// The sidebar (React 55-75): `flex w-56 shrink-0 flex-col gap-1
    /// overflow-auto rounded-lg border border-edge bg-surface` — the
    /// header strip above one two-line item per enabled table.
    fn sidebar(&self, k: &Kit, enabled: &[&TransferTableVm], selected: &TransferTableVm) -> El {
        let mut sidebar = col()
            .key("data-transfer-mapping-sidebar")
            .w(224.0)
            .min_h(0.0)
            .gap(4.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base)
            .child(self.sidebar_header(k));
        for table in enabled {
            sidebar = sidebar.child(self.sidebar_item(k, table, selected));
        }
        sidebar
    }

    /// The header strip (React 56-58): `border-b border-edge px-3 py-1.5
    /// text-[11px] font-semibold uppercase tracking-wider
    /// text-fg-muted` — the tables label (the zh-CN value is a no-op
    /// under `to_uppercase`).
    fn sidebar_header(&self, k: &Kit) -> El {
        col()
            .key("data-transfer-mapping-sidebar-header")
            .w_full()
            .px(12.0)
            .py(6.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .child(
                text(t("transfer.mapping.tables").to_uppercase())
                    .key("data-transfer-mapping-sidebar-header-label")
                    .font(TextStyle::new(11.0, 600.0).tracking(0.55))
                    .color(k.c.text_muted),
            )
    }

    /// One table item (React 59-74): a two-line button — the mono
    /// source name, the muted `→ target` line (`—` when the target is
    /// empty, JS's `||`), the selected one washed — whose press drives
    /// `onSelectTable`. `disabled` never gates it (the props' own
    /// doc comment).
    fn sidebar_item(&self, k: &Kit, table: &TransferTableVm, selected: &TransferTableVm) -> El {
        let base = format!("data-transfer-mapping-table-item-{}", table.source_table);
        let is_selected = table.source_table == selected.source_table;
        let target = if table.target_table.is_empty() {
            "—".to_owned()
        } else {
            table.target_table.clone()
        };

        // React: the name is `truncate font-mono text-xs` (medium and
        // accented when selected); the target line is `truncate
        // text-[11px] text-fg-muted`.
        let name_font = if is_selected {
            TextStyle { weight: 500.0, ..k.ty.mono }
        } else {
            k.ty.mono
        };
        let name_ink = if is_selected {
            k.c.text_accent
        } else {
            k.c.text_base
        };

        // React: `px-3 py-2 text-left text-sm hover:bg-surface-alt`,
        // the selected row adding `bg-surface-alt font-medium
        // text-accent`.
        let mut item = col()
            .key(base.clone())
            .w_full()
            .px(12.0)
            .py(8.0)
            .role("button")
            .focusable(true)
            .child(
                text(table.source_table.clone())
                    .key(format!("{base}-name"))
                    .font(name_font)
                    .color(name_ink)
                    .truncate(),
            )
            .child(
                text(format!("→ {target}"))
                    .key(format!("{base}-target"))
                    .font(k.ty.small)
                    .color(k.c.text_muted)
                    .truncate(),
            );
        if is_selected {
            // The selected wash subsumes the same-colour hover (see
            // the module docs).
            item = item.fill(k.c.layer_01);
        } else {
            item = item.hover(k.c.layer_01);
        }
        if let Some(on_select) = &self.on_select_table {
            let on_select = Rc::clone(on_select);
            let source = table.source_table.clone();
            item = item.on_press(move |s: &mut S| on_select(s, source.clone()));
        }
        item
    }

    /// The editor mount (React 77-86): the column-mapping shell over
    /// the selected table — `onChange` wrapping `onUpdateTable(
    /// selected.sourceTable, patch)` (83), the target submit wrapping
    /// `onTargetTableCommit(selected.sourceTable)` (84) — mounted in
    /// the editor's flex slot (see the module docs).
    fn editor_mount(&self, dark: bool, layer: &mut Vec<El>, selected: &TransferTableVm) -> El {
        let mut editor = ColumnMappingEditor::<S>::new(self.editor);
        if let Some(editor_mut) = &self.editor_mut {
            let editor_mut = Rc::clone(editor_mut);
            editor = editor.editor_mut(move |s: &mut S| editor_mut(s));
        }
        if let Some(on_update) = &self.on_update_table {
            let on_update = Rc::clone(on_update);
            let source = selected.source_table.clone();
            editor = editor.on_change(move |s: &mut S, patch| {
                on_update(s, source.clone(), patch);
            });
        }
        if let Some(on_commit) = &self.on_target_table_commit {
            let on_commit = Rc::clone(on_commit);
            let source = selected.source_table.clone();
            editor = editor.on_target_table_blur(move |s: &mut S| on_commit(s, source.clone()));
        }
        col()
            .key("data-transfer-mapping-editor-slot")
            .grow(1.0)
            .child(editor.build(dark, layer))
    }
}
