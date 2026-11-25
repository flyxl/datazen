//! The SQL preview dialog, ported 1:1 from
//! `packages/frontend/src/components/filter/FilterSqlPreviewDialog.tsx` (DB-15).
//!
//! Every statement shown here is text the backend's own read path assembled;
//! this component never builds SQL. The exact `COUNT(*)` is always shown — an
//! estimate falls back to it, so the preview presents both the plan and the
//! rescue.
//!
//! ## Immediate-mode adaptation
//!
//! React's local `copied` state lives in the host state `S`: callers pass
//! `copied(bool)` (computed as `now < copied_until`) and an `on_copy_tx` hook
//! that records `copied_until = tx.clock() + 1.5`. A press copies the composed
//! text through the host copy port (`tx.copy`), schedules a real 1500ms wake
//! (`tx.wake_after`) so the runtime produces the revert frame, then runs the
//! hook. This is the same pattern `ui::copyable::CopyButton` uses; no thread is
//! spawned and no never-expiring boolean is kept here.
//!
//! ## Keys
//!
//! | part          | key                              | React `data-testid`             |
//! |---------------|----------------------------------|---------------------------------|
//! | root          | `filter-sql-preview-dialog`      | `filter-sql-preview-dialog`     |
//! | title/close   | `<root>-title` / `-close`        | dialog header                   |
//! | body/footer   | `<root>-body` / `-footer`        | dialog slots                    |
//! | error         | `filter-sql-preview-error`       | `filter-sql-preview-error`      |
//! | data block    | `filter-sql-preview-data`        | `filter-sql-preview-data`       |
//! | count block   | `filter-sql-preview-count`       | `filter-sql-preview-count`      |
//! | exact block   | `filter-sql-preview-exact-count` | `filter-sql-preview-exact-count`|
//! | loading       | `filter-sql-preview-loading`     | `filter-sql-preview-loading`    |
//! | copy button   | `filter-sql-preview-copy`        | `filter-sql-preview-copy`       |
//!
//! ## i18n
//!
//! `filter.previewSql{Title,Description,Data,Count,ExactCount,Loading}`,
//! `filter.previewSqlKind.{exact,estimated,unsupported}`, `common.{close,copy,copied}`.
//! All exist in the locale packs; none is invented here.

use std::rc::Rc;
use std::time::Duration;

use rune::{col, text, El, ScrollSpec, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_CHECK, ICON_COPY};
use crate::ui::banner::{Banner, BannerVariant};
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

/// The "copied" feedback lifetime, matching React's `setTimeout(..., 1500)`
/// and `ui::copyable::COPIED_FEEDBACK_MS`.
const COPIED_FEEDBACK_MS: u64 = 1500;
/// Root key, fixed the way React fixes `testId="filter-sql-preview-dialog"`.
const ROOT_KEY: &str = "filter-sql-preview-dialog";
/// `<pre className="max-h-48 overflow-auto">` — 12rem at 16px.
const PRE_MAX_HEIGHT: f32 = 192.0;
/// Body cap keeps three statement blocks scrollable inside a 960×640 window.
const BODY_MAX_HEIGHT: f32 = 360.0;

/// `CountStrategy` from `packages/backend-client` (`datazen_driver_api` owns the
/// Rust kernel form, which `datazen-ui` does not depend on).
#[allow(dead_code)] // mirror; consumer: the filter panel dialog. Delete when it wires the preview and imports the DTO from datazen-ui-api.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CountStrategy {
    Exact,
    Estimated,
    Unsupported,
}

impl CountStrategy {
    /// The wire value (`#[serde(rename_all = "camelCase")]`) and the i18n suffix.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Estimated => "estimated",
            Self::Unsupported => "unsupported",
        }
    }
}

/// `TableReadPreviewSql` from `packages/backend-client`, mirrored minimally.
#[allow(dead_code)] // mirror; consumer: the filter panel dialog. Delete when it wires the preview and imports the DTO from datazen-ui-api.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableReadPreviewSql {
    pub data_sql: String,
    pub count_sql: Option<String>,
    pub exact_count_sql: String,
    pub count_kind: CountStrategy,
}

type Close<S> = Rc<dyn Fn(&mut S)>;
type Copied<S> = Rc<dyn Fn(&mut S, &mut Tx)>;

/// One labelled statement, in the order React pushes them.
struct Statement {
    key: &'static str,
    label: String,
    sql: String,
}

/// Builder for the dialog. `S` is the host state that owns the copied deadline.
#[allow(dead_code)] // consumer: the filter panel dialog (not wired). Remove when that view composes this port.
pub struct FilterSqlPreviewDialog<'a, S: 'static> {
    open: bool,
    open_epoch: u64,
    preview: Option<&'a TableReadPreviewSql>,
    error: Option<&'a str>,
    copied: bool,
    on_close: Option<Close<S>>,
    on_copy: Option<Copied<S>>,
}

#[allow(dead_code)] // consumer: the filter panel dialog (not wired). Remove with the builder.
impl<'a, S: 'static> FilterSqlPreviewDialog<'a, S> {
    pub fn new() -> Self {
        Self {
            open: false,
            open_epoch: 0,
            preview: None,
            error: None,
            copied: false,
            on_close: None,
            on_copy: None,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn open_epoch(mut self, epoch: u64) -> Self {
        self.open_epoch = epoch;
        self
    }

    /// The preview answer; `None` while the round trip is in flight.
    pub fn preview(mut self, preview: Option<&'a TableReadPreviewSql>) -> Self {
        self.preview = preview;
        self
    }

    /// Why the preview failed, when it did — the statements stay hidden.
    pub fn error(mut self, error: Option<&'a str>) -> Self {
        self.error = error;
        self
    }

    /// Caller computes `now < copied_until` from the deadline its hook records.
    pub fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    /// The dialog's Escape / header-close / footer-Close handler.
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// Called after the copy text is written and the 1500ms wake is scheduled;
    /// record `copied_until = tx.clock() + 1.5` here (re-timing on each press).
    pub fn on_copy_tx(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.on_copy = Some(Rc::new(f));
        self
    }

    /// React's `statements` array: data, count (only when the driver runs one),
    /// then the always-present exact count.
    fn statements(&self) -> Vec<Statement> {
        let Some(preview) = self.preview else {
            return Vec::new();
        };
        let mut statements = vec![Statement {
            key: "filter-sql-preview-data",
            label: t("filter.previewSqlData").to_owned(),
            sql: preview.data_sql.clone(),
        }];
        // React's `if (preview.countSql)` is falsy for `""` as well as `null`:
        // an empty string is not a statement and must not render a block.
        if let Some(sql) = preview.count_sql.as_deref().filter(|sql| !sql.is_empty()) {
            let kind_key = format!("filter.previewSqlKind.{}", preview.count_kind.as_str());
            let kind = t(&kind_key);
            statements.push(Statement {
                key: "filter-sql-preview-count",
                label: format!("{} · {}", t("filter.previewSqlCount"), kind),
                sql: sql.to_owned(),
            });
        }
        statements.push(Statement {
            key: "filter-sql-preview-exact-count",
            label: t("filter.previewSqlExactCount").to_owned(),
            sql: preview.exact_count_sql.clone(),
        });
        statements
    }

    /// React's `copyAll`: `-- <label>\n<sql>` blocks joined by a blank line.
    fn copy_all_text(&self) -> String {
        self.statements()
            .iter()
            .map(|s| format!("-- {}\n{}", s.label, s.sql))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Build into the app's layer. A closed dialog mounts nothing (the base's
    /// rule, matching `rune_kit::Dialog`).
    pub fn build(self, dark: bool, layer: &mut Vec<El>) {
        let k = kit(dark);
        let statements = self.statements();
        let has_error = self.error.is_some_and(|message| !message.is_empty());
        let has_preview = self.preview.is_some();

        let mut dialog = Dialog::new(ROOT_KEY, t("filter.previewSqlTitle"))
            .open(self.open)
            .open_epoch(self.open_epoch)
            .width(640.0)
            .fit()
            .body_max_height(BODY_MAX_HEIGHT)
            .description(t("filter.previewSqlDescription"))
            .header_pad((20.0, 14.0))
            .body_pad((20.0, 14.0))
            .footer_pad((20.0, 12.0))
            .footer_height(56.0);

        // React renders the error banner independently of the statements, so a
        // stale preview would still be shown next to a fresh failure.
        if has_error {
            let banner = Banner::new("filter-sql-preview-error", self.error.unwrap_or_default())
                .variant(BannerVariant::Boxed)
                .build(&k)
                .unwrap_or_else(|| col().key("filter-sql-preview-error"));
            dialog = dialog.child(banner);
        }

        let mut body = col().key("filter-sql-preview-statements").w_full().gap(12.0);
        for statement in &statements {
            let label = text(statement.label.clone())
                .key(format!("{}-label", statement.key))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted);
            // `<pre className="max-h-48 overflow-auto ... whitespace-pre-wrap
            // break-words">`: monospace, selectable, wrapping, capped and
            // scrollable so one long statement never pushes the footer away.
            let pre = col()
                .key(statement.key)
                .w_full()
                .min_w(0.0)
                .max_h(PRE_MAX_HEIGHT)
                .scroll(ScrollSpec { stick_bottom: false })
                .radius(k.radius.sm)
                .fill(k.c.layer_01)
                .border(1.0, k.c.border_muted)
                .px(12.0)
                .py(8.0)
                .child(
                    text(statement.sql.clone())
                        .key(format!("{}-text", statement.key))
                        .font(k.ty.mono)
                        .color(k.c.text_base)
                        .selectable(true)
                        .wrap_anywhere(),
                );
            body = body.child(
                col()
                    .key(format!("{}-block", statement.key))
                    .w_full()
                    .min_w(0.0)
                    .gap(4.0)
                    .child(label)
                    .child(pre),
            );
        }
        if !has_preview && !has_error {
            body = body.child(
                text(t("filter.previewSqlLoading"))
                    .key("filter-sql-preview-loading")
                    .font(k.ty.small)
                    .color(k.c.text_muted),
            );
        }
        dialog = dialog.child(body);

        // Footer, React order: secondary Close then primary Copy.
        let mut close = Button::new(t("common.close"))
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Regular);
        if let Some(handler) = self.on_close.clone() {
            close = close.on_press(move |s: &mut S| handler(s));
        }
        dialog = dialog.footer(close.build(&k).key("filter-sql-preview-close"));

        let label = if self.copied {
            t("common.copied")
        } else {
            t("common.copy")
        };
        let glyph = if self.copied { &ICON_CHECK } else { &ICON_COPY };
        let copy = Button::new(label)
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Regular)
            .icon(glyph)
            .disabled(!has_preview)
            .build(&k)
            .key("filter-sql-preview-copy");
        let mut copy = copy;
        if has_preview {
            let composed = self.copy_all_text();
            let handler = self.on_copy.clone();
            copy = copy.on_press_tx(move |s: &mut S, tx: &mut Tx| {
                tx.copy(composed.clone());
                tx.wake_after(Duration::from_millis(COPIED_FEEDBACK_MS));
                if let Some(handler) = &handler {
                    handler(s, tx);
                }
            });
        } else {
            copy = copy.focusable(false);
        }
        dialog = dialog.footer(copy);

        // The header X and Escape share the footer Close handler (React's one
        // `onClose`). `Dialog` mounts nothing without it.
        let handler = self.on_close.clone();
        dialog = dialog.on_close(move |s: &mut S| {
            if let Some(handler) = &handler {
                handler(s);
            }
        });
        dialog.build(&k, layer);
    }
}
