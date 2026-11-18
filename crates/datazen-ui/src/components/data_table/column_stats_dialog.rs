//! Column statistics dialog (DB-12): what the header menu's "Column Stats"
//! opens, migrated from `packages/frontend/src/components/DataTable/ColumnStatsDialog.tsx`.
//!
//! The widget renders a [`ColumnStatsState`] and owns no fetching: the numbers
//! are the server's aggregates, never the loaded page (the PRD's 100M-row
//! acceptance case). A value the server did not report renders as an em dash —
//! an honest "no value" beats a zero that was never meant.
//!
//! | part           | key                              |
//! |----------------|----------------------------------|
//! | panel          | `<key>`                          |
//! | title          | `<key>-title`                    |
//! | close          | `<key>-close`                    |
//! | body           | `<key>-body`                     |
//! | loading row    | `<key>-status`                   |
//! | spinner        | `<key>-spinner`                  |
//! | error banner   | `<key>-error`                    |
//! | empty line     | `<key>-empty`                    |
//! | stats block    | `<key>-stats`                    |
//! | stat row       | `<key>-row-<i>`                  |
//! | stat label     | `<key>-cell-<i>-label`           |
//! | stat value     | `<key>-cell-<i>-value`           |
//! | avg note       | `<key>-avg-note`                 |
//!
//! The panel, title, close and body keys come from [`Dialog`]; the component
//! only adds body content. As in React, `idle` mounts nothing (the dialog is
//! closed), `loading` shows a spinner with the loading label, `error` shows a
//! danger banner (the caller's message, else the generic one) and `ready` shows
//! the statistics rows: rows/non-null/nulls/distinct as grouped counts, min/max
//! always, avg only when the column type supports it, plus the note explaining
//! why avg is absent.
//!
//! ## Deviations from the React source (deliberate)
//!
//! - `toLocaleString()` becomes [`format_count`], a fixed comma grouping: the
//!   native UI has one number format, not a locale-sensitive one.
//! - The value column has no `max-w-96` cap; it takes the remaining row width
//!   and still right-aligns and truncates with a tooltip.
//! - The empty state (`ready` with no aggregate object) has no i18n key in the
//!   zh-CN packs yet, so it keeps the English literal "No statistics reported."
//!   (`columnStats.empty` is listed as a missing key in the track report).
//! - The header close button's accessible label comes from [`Dialog`], which
//!   still hardcodes "Close" instead of `common.close`; that is a base gap,
//!   tracked outside this widget.

use rune::{col, row, text, Align, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::banner::Banner;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};

/// Which part of the lifecycle the dialog renders.
#[allow(dead_code)] // component lands before its query-results caller
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ColumnStatsStatus {
    /// No request is in flight and the dialog is closed.
    #[default]
    Idle,
    /// The aggregate statement is running on the connection.
    Loading,
    /// The aggregates arrived.
    Ready,
    /// The aggregate statement failed.
    Error,
}

/// One server aggregate value, mirroring the frontend's `Value | null`.
///
/// `Null` means the server did not report a value (SQL NULL / not requested),
/// which is not the same thing as an empty string: the dialog paints `—` for
/// the former and `''` for the latter.
#[allow(dead_code)] // component lands before its query-results caller
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum StatsValue {
    /// No value was reported.
    #[default]
    Null,
    /// A text value; the empty string is a value, not a missing one.
    Text(String),
    /// A numeric value, formatted like JavaScript's `String(value)`.
    Number(f64),
    /// A boolean value.
    Bool(bool),
}

impl StatsValue {
    /// React's `formatValue`: `null`/`undefined` → `—`, empty string → `''`,
    /// anything else as the string form.
    #[allow(dead_code)] // component lands before its query-results caller
    pub(crate) fn format(&self) -> String {
        match self {
            Self::Null => "—".to_owned(),
            Self::Text(value) if value.is_empty() => "''".to_owned(),
            Self::Text(value) => value.clone(),
            Self::Number(value) => format!("{value}"),
            Self::Bool(value) => value.to_string(),
        }
    }
}

/// The aggregates of one column. `None` counts are "the server had no value".
#[allow(dead_code)] // component lands before its query-results caller
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ColumnStats {
    /// `COUNT(*)` — the rows of the table, independent of the column.
    pub(crate) row_count: Option<i64>,
    /// `COUNT(col)` — the non-null values of the column.
    pub(crate) non_null_count: Option<i64>,
    /// `rowCount - nonNullCount` — the NULL cells of the column.
    pub(crate) null_count: Option<i64>,
    /// `COUNT(DISTINCT col)`.
    pub(crate) distinct_count: Option<i64>,
    /// `MIN(col)` as the server reported it.
    pub(crate) min: StatsValue,
    /// `MAX(col)` as the server reported it.
    pub(crate) max: StatsValue,
    /// `AVG(col)`; ignored when `supports_avg` is false.
    pub(crate) avg: StatsValue,
    /// Whether `avg` was asked of the server at all.
    pub(crate) supports_avg: bool,
}

/// The hook's state, as far as the dialog reads it.
#[allow(dead_code)] // component lands before its query-results caller
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ColumnStatsState {
    /// Which branch of the body renders.
    pub(crate) status: ColumnStatsStatus,
    /// The column the numbers belong to; `None` while idle.
    pub(crate) column: Option<String>,
    /// The aggregates; `None` until `ready`.
    pub(crate) stats: Option<ColumnStats>,
    /// The failure text; `None` falls back to the generic message.
    pub(crate) error: Option<String>,
}

/// A close handler boxed so the component itself stays non-generic, the same
/// shape `ui/table.rs` uses for its row wrappers.
type CloseHook = Box<dyn FnOnce(Dialog) -> Dialog>;

/// The column-statistics dialog panel.
#[allow(dead_code)] // component lands before its query-results caller
pub(crate) struct ColumnStatsDialog<'a> {
    key: &'a str,
    state: &'a ColumnStatsState,
    close: Option<CloseHook>,
}

impl<'a> ColumnStatsDialog<'a> {
    /// A dialog for `state`, mounted under `key` (for example
    /// `"column-stats-dialog"`).
    #[allow(dead_code)] // component lands before its query-results caller
    pub(crate) fn new(key: &'a str, state: &'a ColumnStatsState) -> Self {
        Self {
            key,
            state,
            close: None,
        }
    }

    /// The dismissal policy: the header close control, Escape, and (when the
    /// caller enables it) a press outside the panel. Without it nothing mounts,
    /// exactly as [`Dialog`] behaves.
    #[allow(dead_code)] // component lands before its query-results caller
    pub(crate) fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.close = Some(Box::new(move |dialog| dialog.on_close(f.clone())));
        self
    }

    /// The dialog title: `Column stats — <column>` while a column is known,
    /// the bare title otherwise.
    fn title(&self) -> String {
        match self.state.column.as_deref() {
            Some(column) if !column.is_empty() => {
                format!("{} — {column}", t("columnStats.title"))
            }
            _ => t("columnStats.title").to_owned(),
        }
    }

    /// Paints the panel into the modal layer.
    #[allow(dead_code)] // component lands before its query-results caller
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        let k = kit(dark);
        // React's dialog is as tall as its content: the loading block, one
        // error line, or the statistics list. `.fit()` sized the panel before
        // this port, so even a two-row loading state reserved a blank 368pt
        // panel. The body cap keeps a tall list scrolling inside the window.
        let mut dialog = Dialog::new(self.key, self.title())
            .open(self.state.status != ColumnStatsStatus::Idle)
            .fit()
            .body_max_height(448.0);
        let children = self.body(&k, dark);
        if let Some(close) = self.close {
            dialog = close(dialog);
        }
        for child in children {
            dialog = dialog.child(child);
        }
        dialog.build(k, layer);
    }

    /// The body children for the current status: one branch, never merged.
    fn body(&self, k: &Kit, dark: bool) -> Vec<El> {
        match self.state.status {
            ColumnStatsStatus::Idle => Vec::new(),
            ColumnStatsStatus::Loading => vec![self.loading_row(dark)],
            ColumnStatsStatus::Error => self.error_banner(k).into_iter().collect(),
            ColumnStatsStatus::Ready => vec![self.stats_block(k)],
        }
    }

    /// The spinner and its label, on one muted row (React's `py-6` block).
    fn loading_row(&self, dark: bool) -> El {
        let key = self.key;
        row()
            .key(format!("{key}-status"))
            .w_full()
            .py(24.0)
            .items_center()
            .child(
                Spinner::new(format!("{key}-spinner"))
                    .size(SpinnerSize::Sm)
                    .tone(SpinnerTone::Muted)
                    .label(t("columnStats.loading"))
                    .build(dark),
            )
    }

    /// The failure line: the caller's message, else the generic one. `Banner`
    /// paints the danger token and gives the row an `alert` role.
    fn error_banner(&self, k: &Kit) -> Option<El> {
        let message = self
            .state
            .error
            .clone()
            .unwrap_or_else(|| t("columnStats.error").to_owned());
        Banner::new(format!("{}-error", self.key), message).build(k)
    }

    /// The `ready` branch: labels and values in a two-column list, or the
    /// empty line when the hook reported no aggregate object at all.
    fn stats_block(&self, k: &Kit) -> El {
        let key = self.key;
        let Some(stats) = self.state.stats.as_ref() else {
            // React renders no row at all for a missing aggregate object, and
            // zh-CN has no `columnStats.empty` string: the empty container is
            // the whole state, with no invented English line inside it.
            return col().key(format!("{key}-empty")).w_full().min_w(288.0);
        };

        let rows = stats_rows(stats);
        let mut block = col()
            .key(format!("{key}-stats"))
            .w_full()
            .min_w(288.0)
            .children(rows.iter().enumerate().map(|(index, (label, value))| {
                self.stat_row(k, index, label, value, index + 1 == rows.len())
            }));
        if !stats.supports_avg {
            block = block.child(
                text(t("columnStats.avgUnsupported"))
                    .key(format!("{key}-avg-note"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .pt(8.0),
            );
        }
        block
    }

    /// One `label … value` row with React's bottom rule and truncating value.
    fn stat_row(&self, k: &Kit, index: usize, label: &str, value: &str, last: bool) -> El {
        let key = self.key;
        let mut line = row()
            .key(format!("{key}-row-{index}"))
            .role("row")
            .w_full()
            .gap(24.0)
            // React uses `items-baseline`; rune's row alignment only offers
            // start/end/center/stretch, and the label and mono value share a
            // line height, so centering keeps the two columns level.
            .items_center()
            .py(6.0)
            .child(
                text(label.to_owned())
                    .key(format!("{key}-cell-{index}-label"))
                    .role("term")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted)
                    .shrink(),
            )
            .child(
                text(value.to_owned())
                    .key(format!("{key}-cell-{index}-value"))
                    .role("definition")
                    .font(k.ty.mono)
                    .color(k.c.text_base)
                    .align(Align::End)
                    .grow(1.0)
                    .min_w(0.0)
                    .max_w(384.0)
                    .truncate()
                    .help_when_truncated(value.to_owned()),
            );
        if !last {
            // React's `border-b border-edge/60` on every row but the last.
            line = line
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_muted.alpha(0.6))
                .border_layout();
        }
        line
    }
}

/// The label/value pairs in the React order: counts, min, max, then avg when
/// the column type supports it.
fn stats_rows(stats: &ColumnStats) -> Vec<(String, String)> {
    let mut rows = vec![
        (
            t("columnStats.rows").to_owned(),
            count_text(stats.row_count),
        ),
        (
            t("columnStats.nonNull").to_owned(),
            count_text(stats.non_null_count),
        ),
        (
            t("columnStats.nulls").to_owned(),
            count_text(stats.null_count),
        ),
        (
            t("columnStats.distinct").to_owned(),
            count_text(stats.distinct_count),
        ),
        (t("columnStats.min").to_owned(), stats.min.format()),
        (t("columnStats.max").to_owned(), stats.max.format()),
    ];
    if stats.supports_avg {
        rows.push((t("columnStats.avg").to_owned(), stats.avg.format()));
    }
    rows
}

/// A count as the dialog paints it: grouped digits, or `—` when the server
/// reported none.
fn count_text(count: Option<i64>) -> String {
    count.map(format_count).unwrap_or_else(|| "—".to_owned())
}

/// `toLocaleString()`'s visible effect with one fixed format: thousands
/// separated by commas.
fn format_count(count: i64) -> String {
    let digits = count.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    if count < 0 {
        grouped.insert(0, '-');
    }
    grouped
}
