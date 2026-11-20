//! T371: `windows/data-sync/SqlPreview.tsx` ported to Rune.
//!
//! The SQL preview of the pending write: the toolbar (the title, the four
//! operation-filter chips, the loading spinner, the refresh and copy
//! buttons), the error banner with its size-limit special case, and the
//! statement text itself.
//!
//! ## The host owns the async load (React → Rune)
//!
//! React's `loadPreview` is an async command with a generation guard
//! (`generation.current`) so a stale reply never lands. Rune is
//! immediate-mode: the **host** owns the statements, the loading flag, the
//! preview error and the generation, and this component stays controlled
//! — the refresh button and the filter chips report through callbacks,
//! exactly like the attached-jobs strip (T073) routes its cancel. The
//! `copied` feedback flag is host state the same way (React's
//! `useCopyFeedback` timer is the host's deadline).
//!
//! ## Keys
//!
//! React `data-testid` becomes the element key, verbatim; the rest are
//! internal:
//!
//! | part | key |
//! |---|---|
//! | root (React test id) | `data-sync-preview` |
//! | toolbar (internal) | `…-toolbar` |
//! | title (internal) | `…-title` |
//! | the all filter chip (internal) | `…-filter-all` |
//! | one operation chip (internal) | `…-filter-<op>` (`insert`/`update`/`delete`) |
//! | loading spinner (internal) | `…-loading` |
//! | refresh button (internal) | `…-refresh` |
//! | copy button (internal) | `…-copy` |
//! | error banner (internal) | `…-error` |
//! | the text body (internal) | `…-text` |
//!
//! ## Documented deviations from React
//!
//! * **React `secondary` renders as the kit `Outline` variant** — the
//!   chip family uses Ghost for the unselected and Outline for the
//!   selected face (the crate has no `Secondary`; the toolbar module
//!   maps the same React variant onto Ghost for icon buttons, and
//!   Outline is the bordered read of it here).
//! * **`tone="2xl"` is the largest kit spinner (`Lg`, 20pt)** — the kit
//!   tops out at three sizes; the executing overlay step (T226) carries
//!   the same note.
//! * **The `<pre>` is a wrapping text column at `text_xsmall` mono** —
//!   the crate's mono token set (the React `font-mono text-[11px]
//!   leading-relaxed text-fg-secondary` at the muted step, the same
//!   nearest-token note the confirm dialog documents).
//! * **`previewText` is computed by the host** (the component receives
//!   the statements and the filter and projects them with the T390
//!   helpers), so the copy callback carries the same text the body
//!   renders.

// The production consumer is the Data Sync preview step (T226); until it
// wires this in, the component tests are the only callers.
#![allow(dead_code)] // reusable component; wired by the Data Sync preview step (T226)

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{ICON_COPY, ICON_REFRESH};
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};
use crate::views::data_sync::client_sql_preview::{
    statements_to_preview_text, DataSyncSqlStatement, OpFilter,
};
use crate::views::DataSyncOperation;

/// The React size-limit sentinel the error text is matched against.
const SIZE_LIMIT_MARKER: &str = "Data Sync SQL preview exceeds the 16 MiB IPC limit";

/// The controlled state of the preview. The host owns the async load's
/// results (statements, error, loading) and the two local flags React
/// kept in `useState` (the operation filter, the copied feedback).
#[derive(Clone, Debug, Default)]
pub(crate) struct SqlPreviewVm {
    pub op_filter: OpFilter,
    /// `None` = no statements yet (the load is in flight or failed).
    pub statements: Option<Vec<DataSyncSqlStatement>>,
    pub preview_error: String,
    pub loading: bool,
    pub copied: bool,
}

/// The builder. `S` is the host state that owns the load and the copy.
pub(crate) struct SqlPreview<S: 'static> {
    vm: SqlPreviewVm,
    on_filter: Option<Rc<dyn Fn(&mut S, OpFilter)>>,
    on_refresh: Option<Rc<dyn Fn(&mut S)>>,
    on_copy: Option<Rc<dyn Fn(&mut S, String)>>,
}

impl<S: 'static> SqlPreview<S> {
    pub(crate) fn new(vm: SqlPreviewVm) -> Self {
        Self {
            vm,
            on_filter: None,
            on_refresh: None,
            on_copy: None,
        }
    }

    /// React's `setOpFilter` (a chip press).
    pub(crate) fn on_filter(mut self, f: impl Fn(&mut S, OpFilter) + 'static) -> Self {
        self.on_filter = Some(Rc::new(f));
        self
    }

    /// React's `loadPreview` entry (the refresh button).
    pub(crate) fn on_refresh(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_refresh = Some(Rc::new(f));
        self
    }

    /// React's `handleCopy`: the host copies the preview text and runs
    /// the copied feedback deadline.
    pub(crate) fn on_copy(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_copy = Some(Rc::new(f));
        self
    }

    /// The preview on its own kit root.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut layer = Vec::new();
        let root = self.build_inner(dark, &k, &mut layer);
        rune_kit::root(root, layer)
    }

    /// The same preview without its kit root (for a parent that roots the
    /// overlay once).
    pub(crate) fn build_inner(self, dark: bool, k: &Kit, layer: &mut Vec<El>) -> El {
        let preview_text = match &self.vm.statements {
            Some(statements) => statements_to_preview_text(statements, self.vm.op_filter),
            None => String::new(),
        };
        let mut root = col()
            .key("data-sync-preview")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .child(self.toolbar(dark, k, layer));
        if !self.vm.preview_error.is_empty() {
            root = root.child(self.error_banner(k));
        }
        root.child(
            text(preview_text.clone())
                .key("data-sync-preview-text")
                .font(k.ty.mono)
                .color(k.c.text_muted)
                .wrap()
                .grow(1.0)
                .min_h(0.0)
                .padding(12.0),
        )
    }

    /// The toolbar row.
    fn toolbar(&self, dark: bool, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut bar = row()
            .key("data-sync-preview-toolbar")
            .w_full()
            .items_center()
            .gap(8.0)
            .px(12.0)
            .py(8.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .child(
                text(t("common.sqlPreviewLower"))
                    .key("data-sync-preview-title")
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_muted),
            );
        for (filter, key, label) in [
            (OpFilter::All, "data-sync-preview-filter-all", t("sync.filter.all")),
            (
                OpFilter::Op(DataSyncOperation::Insert),
                "data-sync-preview-filter-insert",
                "INSERT",
            ),
            (
                OpFilter::Op(DataSyncOperation::Update),
                "data-sync-preview-filter-update",
                "UPDATE",
            ),
            (
                OpFilter::Op(DataSyncOperation::Delete),
                "data-sync-preview-filter-delete",
                "DELETE",
            ),
        ] {
            bar = bar.child(self.filter_chip(k, filter, key, label));
        }
        if self.vm.loading {
            bar = bar.child(
                Spinner::new("data-sync-preview-loading")
                    .size(SpinnerSize::Md)
                    .tone(SpinnerTone::Muted)
                    .build(dark),
            );
        }
        let mut refresh = Button::new(t("sync.refreshPreview"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
        if let Some(on_refresh) = &self.on_refresh {
            let on_refresh = on_refresh.clone();
            refresh = refresh.on_press(move |s: &mut S| on_refresh(s));
        }
        bar = bar.child(refresh.build(k).key("data-sync-preview-refresh"));
        let mut copy = Button::new(if self.vm.copied {
            t("common.copied")
        } else {
            t("common.copy")
        })
        .variant(ButtonVariant::Outline)
        .control_size(ControlSize::Small);
        if let Some(on_copy) = &self.on_copy {
            let on_copy = on_copy.clone();
            let statements = self.vm.statements.clone();
            let filter = self.vm.op_filter;
            copy = copy.on_press(move |s: &mut S| {
                let preview_text = match &statements {
                    Some(statements) => statements_to_preview_text(statements, filter),
                    None => String::new(),
                };
                on_copy(s, preview_text);
            });
        }
        bar = bar.child(
            copy.build(k)
                .key("data-sync-preview-copy")
                .child(rune::icon(&ICON_COPY, 14.0)),
        );
        let _ = layer;
        bar
    }

    /// One filter chip: Outline while selected, Ghost otherwise.
    fn filter_chip(&self, k: &Kit, filter: OpFilter, key: &str, label: &str) -> El {
        let selected = self.vm.op_filter == filter;
        let mut button = Button::new(label)
            .variant(if selected {
                ButtonVariant::Outline
            } else {
                ButtonVariant::Ghost
            })
            .control_size(ControlSize::Small);
        if let Some(on_filter) = &self.on_filter {
            let on_filter = on_filter.clone();
            button = button.on_press(move |s: &mut S| on_filter(s, filter));
        }
        let _ = k;
        button.build(k).key(key)
    }

    /// The error banner: the size-limit special case swaps in its own
    /// message, any other error renders verbatim.
    fn error_banner(&self, k: &Kit) -> El {
        let message = if self.vm.preview_error.contains(SIZE_LIMIT_MARKER) {
            t("sync.sqlPreviewLimitReached")
        } else {
            self.vm.preview_error.as_str()
        };
        col()
            .key("data-sync-preview-error")
            .w_full()
            .padding(12.0)
            .fill(k.alpha(k.c.danger.fg, 0.08))
            .border(1.0, k.alpha(k.c.danger.fg, 0.4))
            .radius(k.radius.sm)
            .child(
                text(message)
                    .key("data-sync-preview-error-text")
                    .font(k.ty.text_small)
                    .color(k.c.danger.fg)
                    .wrap(),
            )
    }
}
