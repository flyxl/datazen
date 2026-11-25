//! T177: `windows/data-sync/DataSyncObjectsStep.tsx` ported to Rune.
//!
//! Third step of the Data Sync wizard: the inspected table mapping the user
//! reviews before the comparison starts. Three branches, exactly React's
//! conditional: the inspecting spinner (`syncState === 'inspecting'`), the
//! mapping table (`rows.length > 0`, compared=false — this step always
//! shows the pre-comparison face), and the empty state. The panel's own
//! optional callbacks are required here (`onOpenDataTransfer` is not
//! optional in this step's props).
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | root (React test id) | `data-sync-objects-step` |
//! | inspecting branch | `…-inspecting` (+ `-spinner`, `-text`) |
//! | empty branch | `…-empty` |
//!
//! ## Documented deviations from React
//!
//! * **`space-y-3`** becomes the root column's `gap(12.0)` (one branch
//!   child at a time, so the spacing is latent in both).
//! * The spinner's `size="xl"`/`tone="accent"` is the crate's
//!   `SpinnerSize::Xxl` / `SpinnerTone::Accent` (`ui/spinner.rs`' registry
//!   mapping).
//! * **`text-center`** on the empty state: the branch is a full-width
//!   column with centered children.

// The production consumer is the Data Sync window track (T268); until it
// wires the step in, the component tests are the only callers.
#![allow(dead_code)] // reusable component; wired by the Data Sync wizard (T268)

use std::collections::HashSet;
use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};
use crate::views::data_sync::data_sync_compare_step::{DataSyncTableResult, SyncState};
use crate::views::data_sync::mapping_panel::{MappingPanel, MappingPanelVm};
use crate::views::data_transfer::source_filter_editor::StructuredSourceFilter;

/// The step builder. `S` is the host state that owns the disabled set, the
/// filters and the navigation actions (the panel's callbacks pass through).
pub struct DataSyncObjectsStep<S: 'static> {
    sync_state: SyncState,
    rows: Vec<DataSyncTableResult>,
    disabled_tables: HashSet<String>,
    dark: bool,
    on_toggle_disabled: Option<Rc<dyn Fn(&mut S, String)>>,
    on_open_schema_diff: Option<Rc<dyn Fn(&mut S)>>,
    on_open_data_transfer: Option<Rc<dyn Fn(&mut S)>>,
    on_update_source_filter:
        Option<Rc<dyn Fn(&mut S, String, Option<StructuredSourceFilter>)>>,
    on_toggle_filter_editor: Option<Rc<dyn Fn(&mut S, String)>>,
}

impl<S: 'static> DataSyncObjectsStep<S> {
    pub fn new(
        sync_state: SyncState,
        rows: Vec<DataSyncTableResult>,
        disabled_tables: HashSet<String>,
    ) -> Self {
        Self {
            sync_state,
            rows,
            disabled_tables,
            dark: true,
            on_toggle_disabled: None,
            on_open_schema_diff: None,
            on_open_data_transfer: None,
            on_update_source_filter: None,
            on_toggle_filter_editor: None,
        }
    }

    /// The step renders through the kit the window's theme picked.
    pub fn dark(mut self, dark: bool) -> Self {
        self.dark = dark;
        self
    }

    /// `onToggleDisabled` — passed through to the mapping panel.
    pub fn on_toggle_disabled(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_disabled = Some(Rc::new(f));
        self
    }

    /// `onOpenSchemaDiff` — passed through.
    pub fn on_open_schema_diff(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_schema_diff = Some(Rc::new(f));
        self
    }

    /// `onOpenDataTransfer` — passed through (required in this step).
    pub fn on_open_data_transfer(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_data_transfer = Some(Rc::new(f));
        self
    }

    /// `onUpdateSourceFilter` — passed through.
    pub fn on_update_source_filter(
        mut self,
        f: impl Fn(&mut S, String, Option<StructuredSourceFilter>) + 'static,
    ) -> Self {
        self.on_update_source_filter = Some(Rc::new(f));
        self
    }

    /// The mapping panel's expand toggle — passed through.
    pub fn on_toggle_filter_editor(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_filter_editor = Some(Rc::new(f));
        self
    }

    pub fn build(self, _dark: bool) -> El {
        let k = kit(self.dark);
        let root = col().key("data-sync-objects-step").w_full().gap(12.0);
        if self.sync_state == SyncState::Inspecting {
            return root.child(self.inspecting_branch(&k));
        }
        if !self.rows.is_empty() {
            return root.child(self.mapping_branch());
        }
        root.child(self.empty_branch(&k))
    }

    /// `flex justify-center py-12 text-sm text-fg-muted`: the Xxl accent
    /// spinner beside the inspecting label.
    fn inspecting_branch(&self, k: &Kit) -> El {
        row()
            .key("data-sync-objects-step-inspecting")
            .w_full()
            .justify_center()
            .py(48.0)
            .gap(8.0)
            .child(
                Spinner::new("data-sync-objects-step-spinner")
                    .size(SpinnerSize::Xxl)
                    .tone(SpinnerTone::Accent)
                    .build(self.dark),
            )
            .child(
                text(t("sync.inspecting"))
                    .key("data-sync-objects-step-text")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
    }

    /// The mapping panel, `compared=false` (the pre-comparison face), with
    /// every callback wired through.
    fn mapping_branch(&self) -> El {
        let mut panel = MappingPanel::<S>::new(MappingPanelVm {
            rows: self.rows.clone(),
            disabled_tables: self.disabled_tables.clone(),
            compared: false,
            filter_table: None,
        });
        if let Some(callback) = self.on_toggle_disabled.clone() {
            panel = panel.on_toggle_disabled(move |s, table| callback(s, table));
        }
        if let Some(callback) = self.on_open_schema_diff.clone() {
            panel = panel.on_open_schema_diff(move |s| callback(s));
        }
        if let Some(callback) = self.on_open_data_transfer.clone() {
            panel = panel.on_open_data_transfer(move |s| callback(s));
        }
        if let Some(callback) = self.on_update_source_filter.clone() {
            panel = panel.on_update_source_filter(move |s, table, filter| callback(s, table, filter));
        }
        if let Some(callback) = self.on_toggle_filter_editor.clone() {
            panel = panel.on_toggle_filter_editor(move |s, table| callback(s, table));
        }
        panel.build(self.dark)
    }

    /// `rounded-lg border border-edge bg-surface-alt px-4 py-8
    /// text-center text-sm text-fg-muted`.
    fn empty_branch(&self, k: &Kit) -> El {
        col()
            .key("data-sync-objects-step-empty")
            .w_full()
            .radius(4.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_weak)
            .px(16.0)
            .py(32.0)
            .child(
                text(t("sync.noTablesFound"))
                    .key("data-sync-objects-step-empty-text")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
    }
}
