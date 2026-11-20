//! T336: `windows/data-sync/MappingPanel.tsx` ported to Rune.
//!
//! The mapping table of the Data Sync wizard's objects step: one row per
//! inspected table pair, the include checkbox, the status cell (the label
//! or, past a comparison, the row-diff counters), the per-row action
//! buttons, the incompatible reason line, and the expandable source-filter
//! / recordset editors. The React component holds one `useState` cell —
//! `filterTable`, the table whose editors are expanded — which becomes the
//! host-held [`MappingPanelVm::filter_table`] (the same trade every
//! one-open-slot component here makes).
//!
//! ## Keys
//!
//! React `data-testid` becomes the element key; per-row parts take the
//! row's composite base (`data-sync-mapping-row-{source}->{target}`) so
//! sibling rows cannot collide:
//!
//! | part | key |
//! |---|---|
//! | root (internal) | `data-sync-mapping-panel` |
//! | title span (internal) | `…-title` |
//! | summary span (React test id) | `data-sync-mapping-summary` |
//! | list container (internal) | `…-list` |
//! | header strip (internal) | `…-header` |
//! | one row | `data-sync-mapping-row-{source}->{target}` |
//! | row checkbox | `{base}-include` |
//! | row name | `{base}-name` |
//! | row status | `{base}-status` |
//! | source-filter button | `{base}-data-sync-source-filter` |
//! | recordset button | `{base}-data-sync-recordset` |
//! | schema-diff button | `{base}-schema-diff` |
//! | open-transfer button | `{base}-data-sync-open-transfer` |
//! | incompatible reason | `{base}-reason` |
//! | editor container | `{base}-editor` |
//! | incompatible hint | `data-sync-incompatible-hint` |
//!
//! ## Documented deviations from React
//!
//! * **`rows.length === 0 → null`** becomes `build` returning an empty
//!   root — the panel keeps its key but paints nothing (no child, zero
//!   height), which is the same "not visible" outcome React's `null` has.
//! * **`truncate`** becomes the text's own clipping (the name/status
//!   cells stay single-line, `min_w(0)` on the flexible cell); Rune has no
//!   text-overflow ellipsis primitive, so long names clip at the cell
//!   edge exactly as React clips (without the ellipsis glyph).
//! * **`title={...}` tooltips** (the checkbox hint, the reason's full
//!   text) ride the elements' help text channel where the kit exposes it;
//!   where it does not, the visible text carries the same string (the
//!   reason line is the full text anyway — React truncates it visually
//!   but keeps the full string in the tooltip, and Rune's clip does the
//!   visual half).
//! * **`text-amber-600 dark:text-amber-400`** is the kit warning
//!   foreground (the crate's amber mapping, T073/T148 precedent); the
//!   `bg-amber-500/5` row wash is the warning foreground at 5% alpha —
//!   no new color literal.
//! * The two "expand" buttons share the filter_table toggle: React's
//!   two buttons set the same state; each row has at most one expanded
//!   editor section, and pressing the row's own button again collapses
//!   it (`current === row.sourceTable ? null : row.sourceTable`).

// The production consumer is the Data Sync window track (T268); until it
// wires the panel in, the component tests are the only callers.
#![allow(dead_code)] // reusable component; wired by the Data Sync wizard (T268)

use std::collections::HashSet;
use std::rc::Rc;

use rune::{col, icon, row, text, El, ScrollSpec};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::icons::ICON_EXTERNAL_LINK;
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;
use crate::views::data_sync::data_sync_compare_step::DataSyncTableResult;
use crate::views::data_sync::mapping_view::{
    display_table_name, mapping_label_key, row_diff_counts, summarize_mappings, table_has_row_diffs,
};
use crate::views::data_sync::recordset_editor::{SyncRecordsetEditor, SyncRecordsetEditorVm};
use crate::views::data_transfer::source_filter_editor::{SourceFilterEditor, StructuredSourceFilter};
use crate::views::TransferRecordset;

/// The panel's data: the inspected table results, the disabled set, whether
/// a comparison has run, and the expanded editors' table key.
#[derive(Clone, Default)]
pub(crate) struct MappingPanelVm {
    pub(crate) rows: Vec<DataSyncTableResult>,
    pub(crate) disabled_tables: HashSet<String>,
    pub(crate) compared: bool,
    /// `useState<string | null>`: the source table whose filter editors
    /// are expanded, or none.
    pub(crate) filter_table: Option<String>,
}

/// The panel builder. `S` is the host state that owns the disabled set,
/// the navigation actions and the source filters.
pub(crate) struct MappingPanel<S: 'static> {
    vm: MappingPanelVm,
    on_toggle_disabled: Option<Rc<dyn Fn(&mut S, String)>>,
    on_open_schema_diff: Option<Rc<dyn Fn(&mut S)>>,
    on_open_data_transfer: Option<Rc<dyn Fn(&mut S)>>,
    on_update_source_filter:
        Option<Rc<dyn Fn(&mut S, String, Option<StructuredSourceFilter>)>>,
    on_toggle_filter_editor: Option<Rc<dyn Fn(&mut S, String)>>,
}

impl<S: 'static> MappingPanel<S> {
    pub(crate) fn new(vm: MappingPanelVm) -> Self {
        Self {
            vm,
            on_toggle_disabled: None,
            on_open_schema_diff: None,
            on_open_data_transfer: None,
            on_update_source_filter: None,
            on_toggle_filter_editor: None,
        }
    }

    /// `onToggleDisabled(sourceTable)`.
    pub(crate) fn on_toggle_disabled(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_disabled = Some(Rc::new(f));
        self
    }

    /// `onOpenSchemaDiff()`.
    pub(crate) fn on_open_schema_diff(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_schema_diff = Some(Rc::new(f));
        self
    }

    /// `onOpenDataTransfer()` — optional in React; wiring it enables the
    /// incompatible rows' transfer button.
    pub(crate) fn on_open_data_transfer(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_data_transfer = Some(Rc::new(f));
        self
    }

    /// `onUpdateSourceFilter(sourceTable, filter)` — optional in React;
    /// wiring it enables the matched rows' expand buttons and editors.
    pub(crate) fn on_update_source_filter(
        mut self,
        f: impl Fn(&mut S, String, Option<StructuredSourceFilter>) + 'static,
    ) -> Self {
        self.on_update_source_filter = Some(Rc::new(f));
        self
    }

    /// The expand toggle the two buttons share: the host owns the
    /// `filterTable` cell (the one-open-slot trade every select-like
    /// component here makes), so the panel hands the press over and the
    /// host flips its cell — `current === row.sourceTable ? null :
    /// row.sourceTable`. React's two buttons write the same state; the
    /// host's callback is that write.
    pub(crate) fn on_toggle_filter_editor(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_filter_editor = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut root = col()
            .key("data-sync-mapping-panel")
            .w_full()
            .px(16.0)
            .py(8.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base);
        if self.vm.rows.is_empty() {
            // React's `return null`: no children, zero height.
            return root;
        }
        let summary = summarize_mappings(&self.vm.rows);
        root = root.child(self.title_row(&k, &summary));
        root = root.child(self.table_list(&k, dark));
        if summary.incompatible > 0 {
            root = root.child(self.incompatible_hint(&k));
        }
        root
    }

    /// The title row: the (post-comparison) section title and the summary
    /// counters, `mb-2 flex items-center gap-2`.
    fn title_row(&self, k: &Kit, summary: &crate::views::data_sync::mapping_view::MappingSummary) -> El {
        row()
            .key("data-sync-mapping-panel-title-row")
            .w_full()
            .items_center()
            .gap(8.0)
            .pb(8.0)
            .child(
                text(if self.vm.compared {
                    t("sync.mappingAfterCompare")
                } else {
                    t("sync.mappingTitle")
                })
                .key("data-sync-mapping-panel-title")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
            )
            .child(col().key("data-sync-mapping-panel-spacer").grow(1.0))
            .child(
                text(
                    t("sync.mappingSummary")
                        .replace("{matched}", &summary.matched.to_string())
                        .replace("{incompatible}", &summary.incompatible.to_string())
                        .replace("{unmapped}", &summary.unmapped.to_string()),
                )
                .key("data-sync-mapping-summary")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
            )
    }

    /// The scrollable table: `max-h-36 overflow-auto rounded-lg border
    /// border-edge` — 144pt of scroll with a 4pt radius.
    fn table_list(&self, k: &Kit, dark: bool) -> El {
        let mut list = col()
            .key("data-sync-mapping-panel-list")
            .w_full()
            .max_h(144.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .radius(4.0)
            .border(1.0, k.c.border_base)
            .child(self.header_row(k));
        for table_row in self.vm.rows.iter() {
            list = list.child(self.table_row(k, dark, table_row));
        }
        list
    }

    /// The header strip: `bg-surface-alt px-3 py-1.5` with the checkbox
    /// gutter (w-6), the table-name column, the right-aligned status column
    /// (w-40) and the actions gutter (w-24).
    fn header_row(&self, k: &Kit) -> El {
        row()
            .key("data-sync-mapping-panel-header")
            .w_full()
            .items_center()
            .gap(12.0)
            .fill(k.c.surface_weak)
            .px(12.0)
            .py(6.0)
            .child(col().key("data-sync-mapping-panel-header-check").w(24.0))
            .child(
                text(t("sync.tableName"))
                    .key("data-sync-mapping-panel-header-name")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(col().key("data-sync-mapping-panel-header-status").w(160.0))
            .child(col().key("data-sync-mapping-panel-header-actions").w(96.0))
    }

    /// One table row: the include checkbox, the name, the status cell, the
    /// actions, the incompatible reason, and (expanded) the editors.
    fn table_row(&self, k: &Kit, dark: bool, table_row: &DataSyncTableResult) -> El {
        let base = format!(
            "data-sync-mapping-row-{}->{}",
            table_row.source_table, table_row.target_table
        );
        let is_disabled = table_row.status == crate::views::data_sync::data_sync_compare_step::DataSyncMappingStatus::Disabled
            || self.vm.disabled_tables.contains(&table_row.source_table);
        let can_toggle = table_row.status == crate::views::data_sync::data_sync_compare_step::DataSyncMappingStatus::Matched
            || table_row.status == crate::views::data_sync::data_sync_compare_step::DataSyncMappingStatus::Disabled;

        // React's row div carries `border-t border-edge px-3 py-1.5`, the
        // INCOMPATIBLE amber wash and the disabled dim — the div with the
        // test id and the source/target attributes.
        let mut line = row()
            .key(format!("{base}-cells"))
            .w_full()
            .items_center()
            .gap(12.0)
            .px(12.0)
            .py(6.0);
        line = line.child(self.include_checkbox(k, &base, table_row, is_disabled, can_toggle));
        line = line.child(
            text(display_table_name(table_row))
                .key(format!("{base}-name"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_base),
        );
        line = line.child(self.status_cell(k, &base, table_row));
        line = line.child(self.actions_cell(k, &base, table_row));

        let mut cell = col()
            .key(base.clone())
            .w_full()
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            .child(line);
        // `bg-amber-500/5` on INCOMPATIBLE rows — the outer row div's class.
        if table_row.status == crate::views::data_sync::data_sync_compare_step::DataSyncMappingStatus::Incompatible {
            cell = cell.fill(k.alpha(k.c.warning.fg, 0.05));
        }
        // `opacity-60` on disabled rows — the outer row div's class.
        if is_disabled {
            cell = cell.opacity(0.6);
        }
        if let Some(reason) = table_row
            .incompatible_reason
            .as_deref()
            .filter(|reason| !reason.is_empty())
        {
            cell = cell.child(
                text(reason.to_owned())
                    .key(format!("{base}-reason"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg),
            );
        }
        if self.editors_expanded_for(table_row) {
            cell = cell.child(self.editor_section(k, dark, &base, table_row));
        }
        cell
    }

    /// The include checkbox: `checked={!isDisabled}`, `disabled={!canToggle}`,
    /// toggling calls `onToggleDisabled` — only a MATCHED/DISABLED row with
    /// a source table can.
    fn include_checkbox(
        &self,
        k: &Kit,
        base: &str,
        table_row: &DataSyncTableResult,
        is_disabled: bool,
        can_toggle: bool,
    ) -> El {
        let mut checkbox = Checkbox::new(format!("{base}-include"), !is_disabled).disabled(!can_toggle);
        if can_toggle {
            let source = table_row.source_table.clone();
            let on_toggle = self.on_toggle_disabled.clone();
            checkbox = checkbox.on_toggle(move |s: &mut S| {
                if let Some(callback) = &on_toggle {
                    if !source.is_empty() {
                        callback(s, source.clone());
                    }
                }
            });
        }
        checkbox.build(k)
    }

    /// The status cell: past a comparison, a diff-carrying MATCHED row
    /// shows the counters; otherwise the status's own label.
    fn status_cell(&self, k: &Kit, base: &str, table_row: &DataSyncTableResult) -> El {
        let counts = row_diff_counts(table_row);
        let shows_diffs = self.vm.compared
            && table_row.status == crate::views::data_sync::data_sync_compare_step::DataSyncMappingStatus::Matched
            && table_has_row_diffs(table_row);
        let label = if shows_diffs {
            t("sync.rowDiffs")
                .replace("{inserts}", &counts.inserts.to_string())
                .replace("{updates}", &counts.updates.to_string())
                .replace("{deletes}", &counts.deletes.to_string())
        } else {
            t(mapping_label_key(table_row.status)).to_owned()
        };
        text(label)
            .key(format!("{base}-status"))
            .font(k.ty.text_xsmall)
            .color(k.c.text_base)
    }

    /// The actions cell: MATCHED rows with filter wiring get the two expand
    /// buttons; INCOMPATIBLE rows get the schema-diff button and (when
    /// wired) the open-transfer button.
    fn actions_cell(&self, k: &Kit, base: &str, table_row: &DataSyncTableResult) -> El {
        let mut actions = row()
            .key(format!("{base}-actions"))
            .w(96.0)
            .justify_end()
            .gap(4.0);
        if table_row.status == crate::views::data_sync::data_sync_compare_step::DataSyncMappingStatus::Matched
            && self.on_update_source_filter.is_some()
        {
            let source = table_row.source_table.clone();
            let on_toggle = self.on_toggle_filter_editor.clone();
            actions = actions.child(
                Button::new(t("sync.sourceFilter"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut S| {
                        if let Some(callback) = &on_toggle {
                            callback(s, source.clone());
                        }
                    })
                    .build(k)
                    .key(format!("{base}-data-sync-source-filter")),
            );
            let source = table_row.source_table.clone();
            let on_toggle = self.on_toggle_filter_editor.clone();
            actions = actions.child(
                Button::new(t("sync.recordset"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut S| {
                        if let Some(callback) = &on_toggle {
                            callback(s, source.clone());
                        }
                    })
                    .build(k)
                    .key(format!("{base}-data-sync-recordset")),
            );
        }
        if table_row.status == crate::views::data_sync::data_sync_compare_step::DataSyncMappingStatus::Incompatible {
            let on_open = self.on_open_schema_diff.clone();
            actions = actions.child(
                Button::new(t("common.schemaDiff"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut S| {
                        if let Some(callback) = &on_open {
                            callback(s);
                        }
                    })
                    .build(k)
                    .key(format!("{base}-schema-diff")),
            );
            if let Some(on_open) = self.on_open_data_transfer.clone() {
                actions = actions.child(
                    Button::new(t("common.dataTransfer"))
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::Small)
                        .on_press(move |s: &mut S| on_open(s))
                        .build(k)
                        .key(format!("{base}-data-sync-open-transfer")),
                );
            }
        }
        actions
    }

    /// The expanded editor section: the recordset editor over the row's
    /// current recordset, then the predicate editor over the whole filter.
    fn editor_section(&self, k: &Kit, dark: bool, base: &str, table_row: &DataSyncTableResult) -> El {
        let recordset = table_row
            .source_filter
            .as_ref()
            .and_then(|filter| filter.recordset.clone());
        let source = table_row.source_table.clone();

        let on_recordset_change = self.on_update_source_filter.clone();
        let current_filter = table_row.source_filter.clone();
        let mut recordset_editor = SyncRecordsetEditor::new(SyncRecordsetEditorVm {
            primary_keys: table_row.primary_keys.clone(),
            recordset: recordset.clone(),
            limit_error: false,
            order_select_open: false,
        });
        recordset_editor = recordset_editor.on_change(move |s: &mut S, next: Option<TransferRecordset>| {
            if let Some(callback) = &on_recordset_change {
                // React's `onChange` keeps the filters when a recordset
                // lands, drops the recordset when the filters remain, and
                // clears the whole filter when neither remains.
                match next {
                    Some(recordset) => {
                        let mut filter = current_filter.clone().unwrap_or_default();
                        filter.recordset = Some(recordset);
                        callback(s, source.clone(), Some(filter));
                    }
                    None => {
                        let has_filters = current_filter
                            .as_ref()
                            .is_some_and(|filter| !filter.filters.is_empty());
                        if has_filters {
                            let mut without = current_filter.clone().unwrap_or_default();
                            without.recordset = None;
                            callback(s, source.clone(), Some(without));
                        } else {
                            callback(s, source.clone(), None);
                        }
                    }
                }
            }
        });

        let on_filter_change = self.on_update_source_filter.clone();
        let source_for_predicate = table_row.source_table.clone();
        let mut predicate_editor = SourceFilterEditor::new(
            table_row.columns.clone(),
            table_row.source_filter.clone(),
        );
        predicate_editor = predicate_editor.on_change(
            move |s: &mut S, filter: Option<StructuredSourceFilter>| {
                if let Some(callback) = &on_filter_change {
                    callback(s, source_for_predicate.clone(), filter);
                }
            },
        );

        col()
            .key(format!("{base}-editor"))
            .w_full()
            .pl(36.0)
            .pt(8.0)
            .child(recordset_editor.build_inner(dark, k, &mut Vec::new()))
            .child(predicate_editor.build_inner(dark, k, &mut Vec::new()))
    }

    /// The incompatible hint line with the external-link glyph.
    fn incompatible_hint(&self, k: &Kit) -> El {
        row()
            .key("data-sync-incompatible-hint")
            .w_full()
            .items_center()
            .gap(4.0)
            .pt(8.0)
            .child(
                icon(&ICON_EXTERNAL_LINK, 12.0)
                    .key("data-sync-incompatible-hint-icon")
                    .color(k.c.text_muted),
            )
            .child(
                text(t("sync.incompatibleHint"))
                    .key("data-sync-incompatible-hint-text")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base),
            )
    }

    /// `filterTable === row.sourceTable && onUpdateSourceFilter &&
    /// row.status === 'MATCHED'`.
    fn editors_expanded_for(&self, table_row: &DataSyncTableResult) -> bool {
        self.on_update_source_filter.is_some()
            && table_row.status == crate::views::data_sync::data_sync_compare_step::DataSyncMappingStatus::Matched
            && !table_row.source_table.is_empty()
            && self.vm.filter_table.as_deref() == Some(table_row.source_table.as_str())
    }
}
