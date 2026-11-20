//! T226: `windows/data-sync/DataSyncPreviewStep.tsx` ported to Rune.
//!
//! Fifth step of the Data Sync wizard: the SQL preview of the pending
//! write, with a progress overlay while the execution runs. Three faces,
//! exactly React's conditionals: the executing overlay (mounted while
//! `syncState === 'executing'`, over whichever face is below), the SQL
//! preview (both db session ids present), and the session-required word.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | the overlay's `absolute inset-0 z-10 … bg-surface/90` | a fill of the kit background at 90% alpha painted over the content (the overlay mounts after it, so it draws above) |
//! | `Spinner size="2xl" tone="accent"` | the kit's largest spinner (`Lg`, 20pt — the kit tops out at three sizes; the T371 toolbar documents the same note) |
//! | `executeProgress \|\| t('sync.executing')` | the empty progress string falls back to the pack word |
//! | `<SqlPreview …13 props>` | [`SqlPreview`] with the host-built VM (T371's controlled contract: the host owns the statements, the filter, the loading flag and the copy feedback; this step forwards the three callbacks) |
//! | `sourceDbSessionId && targetDbSessionId` | the `Option` pair — both present renders the preview, either absent renders the required word |
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | root (internal) | `data-sync-preview-step` |
//! | executing overlay (React test id) | `data-sync-executing-overlay` (+ `-spinner`, `-text`) |
//! | session-required word (React test id) | `data-sync-preview-session-required` |
//! | the preview body | `SqlPreview`'s own keys |

// The production consumer is the Data Sync window track (T268); until it
// wires the step in, the component tests are the only callers.
#![allow(dead_code)] // reusable component; wired by the Data Sync wizard (T268)

use std::rc::Rc;

use rune::{col, text, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};
use crate::views::data_sync::client_sql_preview::OpFilter;
use crate::views::data_sync::data_sync_compare_step::{DataSyncTableSelection, SyncState};
use crate::views::data_sync::data_sync_compare_step::DataSyncTableResult;
use crate::views::data_sync::sql_preview::{SqlPreview, SqlPreviewVm};

/// React `DataSyncSelectedRow` (`backend-client/types/dataSync.ts:271`):
/// one reviewed row of the execution ticket. The T024 compare step never
/// read it; this step's props are the first consumer, so it lands here.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DataSyncSelectedRow {
    pub(crate) source_table: String,
    pub(crate) target_table: String,
    /// The reviewed operation of this row.
    pub(crate) operation: crate::views::DataSyncOperation,
    /// The composite primary key (`Value[]` → `serde_json::Value`).
    pub(crate) key: Vec<serde_json::Value>,
}

/// The step's data.
pub(crate) struct DataSyncPreviewStepVm {
    pub(crate) sync_state: SyncState,
    /// The execution progress text (the feedback store's
    /// `executeProgress`).
    pub(crate) execute_progress: String,
    /// The source db session id — absent until the comparison binds one.
    pub(crate) source_db_session_id: Option<String>,
    /// The target db session id, same rule.
    pub(crate) target_db_session_id: Option<String>,
    /// The tables of the pending write (the preview's input).
    pub(crate) tables: Vec<DataSyncTableResult>,
    /// The sync options of the pending write.
    pub(crate) options: crate::views::data_sync::data_sync_compare_step::SyncOptions,
    /// The reviewed rows.
    pub(crate) selected_rows: Vec<DataSyncSelectedRow>,
    /// The server-owned selections.
    pub(crate) table_selections: Vec<DataSyncTableSelection>,
    /// The preview's controlled state (the host-built statements, filter,
    /// loading and error — T371's contract).
    pub(crate) preview: SqlPreviewVm,
}

/// The step builder. `S` is the host state that owns the preview load and
/// the copy feedback.
pub(crate) struct DataSyncPreviewStep<S: 'static> {
    vm: DataSyncPreviewStepVm,
    dark: bool,
    on_filter: Option<Rc<dyn Fn(&mut S, OpFilter)>>,
    on_refresh: Option<Rc<dyn Fn(&mut S)>>,
    on_copy: Option<Rc<dyn Fn(&mut S, String)>>,
}

impl<S: 'static> DataSyncPreviewStep<S> {
    pub(crate) fn new(vm: DataSyncPreviewStepVm) -> Self {
        Self {
            vm,
            dark: true,
            on_filter: None,
            on_refresh: None,
            on_copy: None,
        }
    }

    /// The step renders through the kit the window's theme picked.
    pub(crate) fn dark(mut self, dark: bool) -> Self {
        self.dark = dark;
        self
    }

    /// `SqlPreview`'s filter-chip press.
    pub(crate) fn on_filter(mut self, f: impl Fn(&mut S, OpFilter) + 'static) -> Self {
        self.on_filter = Some(Rc::new(f));
        self
    }

    /// `SqlPreview`'s refresh press.
    pub(crate) fn on_refresh(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_refresh = Some(Rc::new(f));
        self
    }

    /// `SqlPreview`'s copy press (the text it copied).
    pub(crate) fn on_copy(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_copy = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(self.dark);
        let _ = dark;
        let mut root = col()
            .key("data-sync-preview-step")
            .w_full()
            .min_h(0.0)
            .grow(1.0)
            .clip_rounded(4.0)
            .border(1.0, k.c.border_base);

        // The content face: the preview when both sessions exist, the
        // required word otherwise. React's `sourceDbSessionId &&
        // targetDbSessionId` is truthiness — an EMPTY session id string is
        // falsy, so the gate reads present-and-non-empty on both sides.
        let sessions_ready = self
            .vm
            .source_db_session_id
            .as_deref()
            .is_some_and(|id| !id.is_empty())
            && self
                .vm
                .target_db_session_id
                .as_deref()
                .is_some_and(|id| !id.is_empty());
        if sessions_ready {
            let mut preview = SqlPreview::<S>::new(self.vm.preview.clone());
            if let Some(callback) = self.on_filter.clone() {
                preview = preview.on_filter(move |s, filter| callback(s, filter));
            }
            if let Some(callback) = self.on_refresh.clone() {
                preview = preview.on_refresh(move |s| callback(s));
            }
            if let Some(callback) = self.on_copy.clone() {
                preview = preview.on_copy(move |s, text| callback(s, text));
            }
            root = root.child(preview.build(self.dark));
        } else {
            root = root.child(self.session_required(&k));
        }

        // The executing overlay rides above the content face.
        if self.vm.sync_state == SyncState::Executing {
            root = root.child(self.executing_overlay(&k));
        }
        root
    }

    /// `absolute inset-0 z-10 … bg-surface/90`: the centered spinner over
    /// the 90% background wash.
    fn executing_overlay(&self, k: &Kit) -> El {
        col()
            .key("data-sync-executing-overlay")
            .w_full()
            .grow(1.0)
            .items_center()
            .justify_center()
            .gap(8.0)
            .fill(k.alpha(k.c.bg_base, 0.9))
            .child(
                Spinner::new("data-sync-executing-overlay-spinner")
                    .size(SpinnerSize::Lg)
                    .tone(SpinnerTone::Accent)
                    .build(self.dark),
            )
            .child(
                text(if self.vm.execute_progress.is_empty() {
                    t("sync.executing").to_owned()
                } else {
                    self.vm.execute_progress.clone()
                })
                .key("data-sync-executing-overlay-text")
                .font(k.ty.text_small)
                .color(k.c.text_muted),
            )
    }

    /// The session-required word: `flex flex-1 items-center justify-center`.
    fn session_required(&self, k: &Kit) -> El {
        col()
            .key("data-sync-preview-session-required")
            .w_full()
            .grow(1.0)
            .items_center()
            .justify_center()
            .child(
                text(t("sync.sessionRequired"))
                    .key("data-sync-preview-session-required-text")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
    }
}
