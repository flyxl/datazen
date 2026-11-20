//! The 40pt content status bar — 1:1 port of React `ContentStatusBar.tsx`.
//!
//! [`StatusBarVm`] carries the React props (including the three-way row-count
//! strategy) so the text matrix can be driven without a live session. The
//! existing `views::chrome::content_status_bar` function builds the VM from
//! `AppState` and keeps every `content-status-bar` / `status-*` key.
//!
//! Driver-contributed KV slots are not available in this crate; when a slot is
//! present the React container key `conn-status-kv-bar` is mounted (empty),
//! otherwise the centre context cluster renders, byte-identical to the pre-slot
//! footer. No inline colour or glyph is added.

use rune::tok::{self, type_};
use rune::{row, text, El};

use crate::i18n::t;

/// React `CountStrategy` — the footer never turns "unknown" into a zero.
#[allow(dead_code)] // VM surface; built by chrome::content_status_bar and the tests
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CountStrategy {
    #[default]
    Exact,
    Estimated,
    Unsupported,
}

/// React `formatRowCount`: grouping below a million, then one decimal with an
/// `M`/`B` suffix. The tilde belongs to the i18n text, never to this number.
#[allow(dead_code)] // pure helper; consumed by row_count_text and the tests
pub(crate) fn format_row_count(value: u64) -> String {
    if value < 1_000_000 {
        let digits = value.to_string();
        let mut out = String::with_capacity(digits.len() + digits.len() / 3);
        for (index, byte) in digits.bytes().enumerate() {
            if index > 0 && (digits.len() - index) % 3 == 0 {
                out.push(',');
            }
            out.push(byte as char);
        }
        out
    } else if value < 1_000_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else {
        format!("{:.1}B", value as f64 / 1_000_000_000.0)
    }
}

/// React `ContentStatusBarProps` as a cloneable value.
#[allow(dead_code)] // VM surface; built by chrome::content_status_bar and the tests
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StatusBarVm {
    pub(crate) connected: bool,
    /// `getDbLabel(databaseType)`, when a driver type is known.
    pub(crate) database_label: Option<String>,
    pub(crate) connection_name: Option<String>,
    pub(crate) current_database: Option<String>,
    pub(crate) table_name: Option<String>,
    pub(crate) column_count: u64,
    pub(crate) total_rows: u64,
    pub(crate) total_rows_kind: CountStrategy,
    /// A driver contributed a KV status-bar slot.
    pub(crate) has_kv_slot: bool,
    /// `formatModKey(platform)` — `⌘` on macOS, `Ctrl` elsewhere.
    pub(crate) mod_key: String,
    /// `keySep` in React: `''` on macOS, `'+'` elsewhere.
    pub(crate) key_sep: String,
    /// Resolved connected text. The T0-03 sweep pins `t("conn.connected")` in
    /// `views/chrome.rs`, so the host supplies it (React uses `connWin.connected`;
    /// both resolve to the same zh-CN text).
    pub(crate) connected_label: String,
    /// Legacy English shortcut copy pinned by `tests/chrome_metrics.rs`. The
    /// controlled builder uses localized React text when this is `None`.
    pub(crate) shortcuts_override: Option<String>,
}

impl StatusBarVm {
    /// React's footer row-count sentence, kept in its own words.
    #[allow(dead_code)] // pure helper; consumed by build and the tests
    pub(crate) fn row_count_text(&self) -> Option<String> {
        match self.total_rows_kind {
            CountStrategy::Unsupported => Some(t("pagination.rowsUnknown").to_owned()),
            CountStrategy::Estimated => Some(format!(
                "{} {}",
                t("pagination.rowCountEstimated")
                    .replace("{count}", &format_row_count(self.total_rows)),
                t("connWin.rowCount")
            )),
            CountStrategy::Exact if self.total_rows > 0 => Some(format!(
                "{} {}",
                format_row_count(self.total_rows),
                t("connWin.rowCount")
            )),
            CountStrategy::Exact => None,
        }
    }

    /// React's `.filter(Boolean).join(' · ')` centre cluster.
    #[allow(dead_code)] // pure helper; consumed by build and the tests
    pub(crate) fn context_text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for part in [
            self.database_label.clone(),
            self.connection_name.clone(),
            self.current_database.clone(),
            self.table_name.clone(),
        ] {
            if let Some(part) = part {
                if !part.is_empty() {
                    parts.push(part);
                }
            }
        }
        if self.column_count > 0 {
            parts.push(format!("{} {}", self.column_count, t("connWin.fields")));
        }
        if let Some(rows) = self.row_count_text() {
            parts.push(rows);
        }
        parts.join(" · ")
    }

    /// React's shortcut cluster: `mod{sep}N … · mod{sep}W … · Space …`.
    #[allow(dead_code)] // pure helper; consumed by build and the tests
    pub(crate) fn shortcuts_text(&self) -> String {
        format!(
            "{m}{s}N {nq} · {m}{s}W {close} · Space {detail}",
            m = self.mod_key,
            s = self.key_sep,
            nq = t("common.newQuery"),
            close = t("common.close"),
            detail = t("detail.title"),
        )
    }
}

/// Build the footer as a `role="status"` 40pt row.
#[allow(dead_code)] // reusable component; mounted by chrome::content_status_bar
pub(crate) fn build(vm: &StatusBarVm, theme: tok::Theme) -> El {
    let dot = if vm.connected {
        tok::accent(2, true)
    } else {
        theme.text_secondary
    };
    let mut bar = row()
        .key("content-status-bar")
        .role("status")
        .w_full()
        .h(40.0)
        .px(16.0)
        .center_y()
        .fill(theme.control_off)
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("status-connection-dot")
                        .w(8.0)
                        .h(8.0)
                        .radius(4.0)
                        .fill(dot),
                )
                .child(
                    text(if vm.connected {
                        vm.connected_label.as_str()
                    } else {
                        // No locale key exists for the disconnected state; keep
                        // the neutral Chinese word the pre-port footer used.
                        "未连接"
                    })
                    .key("status-connection-label")
                    .font(type_::LABEL)
                    .color(theme.text),
                ),
        );

    if vm.has_kv_slot {
        bar = bar.child(
            row()
                .key("conn-status-kv-bar")
                .label("kv-status-bar")
                .grow(1.0)
                .min_w(0.0)
                .px(16.0),
        );
    } else {
        bar = bar.child(
            row().grow(1.0).min_w(0.0).justify_center().child(
                text(vm.context_text())
                    .key("status-context")
                    .font(type_::LABEL)
                    .color(theme.text_secondary)
                    .truncate(),
            ),
        );
    }

    bar.child(
        row().gap(8.0).center_y().child(
            text(vm
                .shortcuts_override
                .clone()
                .unwrap_or_else(|| vm.shortcuts_text()))
                .key("status-shortcuts")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        ),
    )
}
