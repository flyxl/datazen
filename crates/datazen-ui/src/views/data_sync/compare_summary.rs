//! T286: `windows/data-sync/CompareSummary.tsx` ported to Rune.
//!
//! The comparison summary strip: the title word, the five counter words
//! (inserts green, updates accent, deletes red, unchanged muted,
//! incompatible amber — the incompatible one only when non-zero), and the
//! trailing action pair (copy-report with the timed "copied" word,
//! explain-diff gated by the loading flag), each button mounted only when
//! its callback is wired.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `useState(copied)` + `setTimeout(2000)` | the host's `copied` cell (a `copied_until` deadline the host compares against the frame clock, exactly the `CopyableError` precedent) + `tx.wake_after(2000ms)` on the copy press |
//! | `handleCopyReport` (`onCopyReport(); setCopied(true)`) | the host's copy action; this component reports the press through [`CompareSummary::on_copy_report`] and the host flips its own cell |
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | root (React test id) | `data-sync-summary` |
//! | title span (internal) | `…-title` |
//! | inserts span (internal) | `…-inserts` (green) |
//! | updates span (internal) | `…-updates` (accent) |
//! | deletes span (internal) | `…-deletes` (red) |
//! | unchanged span (internal) | `…-unchanged` |
//! | incompatible span (internal) | `…-incompatible` |
//! | copy-report button (React test id) | `data-sync-copy-report` |
//! | copied text | the button label flips to `common.copied` |
//! | explain-diff button (React test id) | `data-sync-explain-diff` |
//!
//! ## Deviations
//!
//! * `text-green-600 dark:text-green-400` is the kit success/accent mapping
//!   the crate already uses for counters — inserts map to the danger
//!   complement the React classes name. No new color literals.
//! * `bg-surface-alt/30` is a wash of the kit surface at 30% alpha.
//! * `tabular-nums` has no Rune primitive; the counts are plain numerals.

// The production consumers are the Data Sync result step (T247) and the
// compare step's own summary slot; until then the component tests are the
// only callers.
#![allow(dead_code)] // reusable component; wired by T247/T268

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::data_sync::mapping_view::CompareSummaryStats;

/// The summary builder. `S` is the host state that owns the report and the
/// explain-diff action.
pub(crate) struct CompareSummary<S: 'static> {
    stats: CompareSummaryStats,
    copied: bool,
    on_copy_report: Option<Rc<dyn Fn(&mut S)>>,
    on_explain_diff: Option<Rc<dyn Fn(&mut S)>>,
    explain_loading: bool,
}

impl<S: 'static> CompareSummary<S> {
    pub(crate) fn new(stats: CompareSummaryStats) -> Self {
        Self {
            stats,
            copied: false,
            on_copy_report: None,
            on_explain_diff: None,
            explain_loading: false,
        }
    }

    /// The host's `copied` cell value for this frame.
    pub(crate) fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    /// `onCopyReport` — optional in React; wiring it mounts the button.
    pub(crate) fn on_copy_report(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy_report = Some(Rc::new(f));
        self
    }

    /// `onExplainDiff` — optional; wiring it mounts the button.
    pub(crate) fn on_explain_diff(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_explain_diff = Some(Rc::new(f));
        self
    }

    /// `explainLoading` — the explain button's disabled flag.
    pub(crate) fn explain_loading(mut self, loading: bool) -> Self {
        self.explain_loading = loading;
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let stats = &self.stats;

        let mut strip = row()
            .key("data-sync-summary")
            .w_full()
            .items_center()
            .gap(16.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .fill(k.alpha(k.c.surface_weak, 0.3))
            .px(24.0)
            .py(8.0)
            .child(
                text(t("sync.compareSummaryTitle"))
                    .key("data-sync-summary-title")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                text(count_of("sync.summaryInserts", stats.inserts))
                    .key("data-sync-summary-inserts")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent),
            )
            .child(
                text(count_of("sync.summaryUpdates", stats.updates))
                    .key("data-sync-summary-updates")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent),
            )
            .child(
                text(count_of("sync.summaryDeletes", stats.deletes))
                    .key("data-sync-summary-deletes")
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg),
            )
            .child(
                text(count_of("sync.summaryUnchanged", stats.unchanged_tables))
                    .key("data-sync-summary-unchanged")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        if stats.incompatible > 0 {
            strip = strip.child(
                text(count_of("sync.summaryIncompatible", stats.incompatible))
                    .key("data-sync-summary-incompatible")
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg),
            );
        }
        if self.on_copy_report.is_some() || self.on_explain_diff.is_some() {
            strip = strip.child(
                row()
                    .key("data-sync-summary-actions")
                    .grow(1.0)
                    .justify_end()
                    .gap(8.0),
            );
            if let Some(callback) = self.on_copy_report.clone() {
                strip = strip.child(
                    Button::new(if self.copied {
                        t("common.copied")
                    } else {
                        t("sync.copyReport")
                    })
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut S| callback(s))
                    .build(&k)
                    .key("data-sync-copy-report"),
                );
            }
            if let Some(callback) = self.on_explain_diff.clone() {
                strip = strip.child(
                    Button::new(t("sync.explainDiff"))
                        .variant(ButtonVariant::Outline)
                        .control_size(ControlSize::Small)
                        .disabled(self.explain_loading)
                        .on_press(move |s: &mut S| callback(s))
                        .build(&k)
                        .key("data-sync-explain-diff"),
                );
            }
        }
        strip
    }
}

/// `t(key, { count })`: the count word with the placeholder filled.
fn count_of(key: &str, count: usize) -> String {
    t(key).replace("{count}", &count.to_string())
}
