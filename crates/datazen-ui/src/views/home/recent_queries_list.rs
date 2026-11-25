//! Port of `packages/frontend/src/windows/connection/home/RecentQueriesList.tsx`.
//!
//! The "Recent queries" section of the landing page: five rows with SQL,
//! source name, relative time, duration and hover actions (Re-run / Copy), plus
//! an empty state and a "View all" action. React owns the copy-feedback flag;
//! this port keeps it in the VM and routes every action through an injected host
//! port — no fake IPC, clipboard write, timer or thread.
//!
//! ## Keys
//!
//! The existing home tests pin `connections-recent-queries`,
//! `connections-recent-title`, `connections-recent-card`,
//! `connections-recent-empty` and `connections-recent-view-all`; all are kept.
//! React's `data-testid`s (`home-recent-queries-section`,
//! `view-all-history-button`, `home-query-source-{id}`, `home-query-rerun-{id}`,
//! `home-query-copy-{id}`) are kept as sibling observation handles, as the
//! other home ports do.
//!
//! # Local stand-ins (removal stage)
//!
//! `QueryHistoryVm` / `SavedConnectionVm` mirror React props until the host
//! store owns them. `relative_label` reproduces `lib/relativeTime.ts` and its
//! i18n composition; the >7-day locale date is a plain `YYYY-MM-DD` because
//! Rune has no `Intl` — replace with the locale formatter when one lands.
//! No new colours: kit tokens only; icons come from the registry.
//!
//! Not composed into `connections_home.rs` yet: every item carries a per-item
//! `#[allow(dead_code)]` with the removal stage "drop when the landing host
//! mounts this section".

use std::rc::Rc;

use rune::{col, icon as rune_icon, row, text, El, Key, KeyEvent, Tx};
use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::{
    ICON_CHECK, ICON_CHEVRON_RIGHT, ICON_CLOCK, ICON_COPY, ICON_LOADER2, ICON_PLAY,
};
use crate::ui::kit::kit;

pub const MAX_VISIBLE: usize = 5;
pub const COPIED_FEEDBACK_MS: u64 = 2000;

pub const ROOT_KEY: &str = "connections-recent-queries";
pub const TITLE_KEY: &str = "connections-recent-title";
pub const VIEW_ALL_KEY: &str = "connections-recent-view-all";
pub const CARD_KEY: &str = "connections-recent-card";
pub const EMPTY_KEY: &str = "connections-recent-empty";

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;
const RELATIVE_WINDOW_MS: i64 = 7 * DAY_MS;

/// React `QueryHistoryEntry` fields this section reads.
#[allow(dead_code)] // view VM; consumer: the landing host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryHistoryVm {
    pub id: String,
    pub connection_id: String,
    pub database: String,
    pub sql: String,
    /// Epoch milliseconds, as `new Date(executedAt).getTime()` would yield.
    pub executed_at_ms: i64,
    pub execution_time_ms: i64,
    pub success: bool,
}

/// React `ConnectionConfig` fields this section reads.
#[allow(dead_code)] // view VM; consumer: the landing host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedConnectionVm {
    pub id: String,
    pub name: String,
}

/// React's `copiedSqlId` + `useCopyFeedback` flag and the loading state.
#[allow(dead_code)] // VM; consumer: the landing host (not wired).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentQueriesVm {
    pub entries: Vec<QueryHistoryVm>,
    pub saved_connections: Vec<SavedConnectionVm>,
    pub loading: bool,
    pub copied_sql_id: Option<String>,
    pub copied: bool,
    /// Host clock in milliseconds; used by the relative-time helper only.
    pub now_ms: i64,
}

#[allow(dead_code)] // state helper; remove with the host.
impl RecentQueriesVm {
    /// React gates the per-row marker on both flags. A rolled-back copy must
    /// not leave a stale "copied" marker on the last attempted row.
    pub fn copied_row_id(&self) -> Option<&str> {
        if self.copied {
            self.copied_sql_id.as_deref()
        } else {
            None
        }
    }

    /// `connectionNameById.get(id) || database || 'default'`.
    pub fn source_name(&self, entry: &QueryHistoryVm) -> String {
        self.saved_connections
            .iter()
            .find(|connection| connection.id == entry.connection_id)
            .map(|connection| connection.name.clone())
            .or_else(|| {
                if entry.database.is_empty() {
                    None
                } else {
                    Some(entry.database.clone())
                }
            })
            .unwrap_or_else(|| "default".to_owned())
    }

    /// React `entries.slice(0, MAX_VISIBLE)`.
    pub fn visible_entries(&self) -> &[QueryHistoryVm] {
        let end = MAX_VISIBLE.min(self.entries.len());
        &self.entries[..end]
    }
}

/// React `getRelativeTimeParts` + the hook's locale composition. `None` means
/// the segment is omitted entirely (invalid timestamp or failed parts).
#[allow(dead_code)] // pure helper; remove when the lib port lands.
pub fn relative_label(timestamp_ms: i64, now_ms: i64) -> Option<String> {
    if timestamp_ms <= 0 {
        return None;
    }
    let delta = now_ms - timestamp_ms;
    if delta < MINUTE_MS {
        return Some(t("connWin.home.queries.justNow").to_owned());
    }
    if delta > RELATIVE_WINDOW_MS {
        // Beyond the relative window React formats a short locale date.
        return Some(chrono_date(timestamp_ms));
    }
    if delta < HOUR_MS {
        return Some(
            t("connWin.home.queries.minutesAgo")
                .replace("{count}", &(delta / MINUTE_MS).to_string()),
        );
    }
    if delta < DAY_MS {
        return Some(
            t("connWin.home.queries.hoursAgo")
                .replace("{count}", &(delta / HOUR_MS).to_string()),
        );
    }
    Some(
        t("connWin.home.queries.daysAgo")
            .replace("{count}", &(delta / DAY_MS).to_string()),
    )
}

fn chrono_date(timestamp_ms: i64) -> String {
    match chrono::DateTime::from_timestamp_millis(timestamp_ms) {
        Some(instant) => instant.format("%Y-%m-%d").to_string(),
        None => String::new(),
    }
}

/// Host requests behind the three actions.
#[allow(dead_code)] // host port; consumer: the landing host (not wired).
pub struct RecentQueriesHostPort<S> {
    pub on_select: Option<Rc<dyn Fn(&mut S, &str) + 'static>>,
    pub on_open_history: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_copy: Option<Rc<dyn Fn(&mut S, &str, &str) + 'static>>,
    pub on_copied_change: Option<Rc<dyn Fn(&mut S, Option<String>, bool) + 'static>>,
}

impl<S> Default for RecentQueriesHostPort<S> {
    fn default() -> Self {
        Self {
            on_select: None,
            on_open_history: None,
            on_copy: None,
            on_copied_change: None,
        }
    }
}

/// Build the section into `layer`.
#[allow(dead_code)] // view entry; remove when the landing host mounts it.
pub fn build<S: 'static>(
    vm: &RecentQueriesVm,
    dark: bool,
    host: RecentQueriesHostPort<S>,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let mut section = col()
        .key(ROOT_KEY)
        .w_full()
        .gap(10.0)
        .child(
            row()
                .key("home-recent-queries-section")
                .w_full()
                .h(20.0)
                .center_y()
                .child(
                    text(t("connWin.home.recentQueries"))
                        .key(TITLE_KEY)
                        .font(k.ty.small_medium)
                        .color(k.c.text_muted)
                        .grow(1.0),
                )
                .child(view_all(dark, &host)),
        )
        .child(
            col()
                .key(CARD_KEY)
                .w_full()
                .radius(12.0)
                .border(1.0, k.c.border_base)
                .fill(k.c.layer_02)
                .padding(16.0)
                .child(card_body(vm, dark, &host)),
        );
    if vm.loading {
        section = section.child(
            row()
                .key("connections-recent-loading")
                .w_full()
                .gap(8.0)
                .center_y()
                .child(
                    rune_icon(&ICON_LOADER2, 14.0)
                        .key("connections-recent-loading-spinner")
                        .color(k.c.text_muted),
                )
                .child(
                    text(t("common.loading"))
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
    }
    layer.push(section);
}

fn view_all<S: 'static>(dark: bool, host: &RecentQueriesHostPort<S>) -> El {
    let k = kit(dark);
    let mut button = row()
        .key(VIEW_ALL_KEY)
        .h(20.0)
        .px(4.0)
        .gap(4.0)
        .center_y()
        .child(text(t("connWin.home.viewAll")).font(k.ty.text_small).color(k.c.text_accent))
        .child(rune_icon(&ICON_CHEVRON_RIGHT, 12.0).color(k.c.text_accent))
        .child(
            row()
                .key("view-all-history-button")
                .size(0.0, 0.0)
                .a11y_hidden(),
        );
    if let Some(on_open) = &host.on_open_history {
        let on_open = on_open.clone();
        button = button.on_press(move |state: &mut S| on_open(state));
    }
    button
}

fn card_body<S: 'static>(
    vm: &RecentQueriesVm,
    dark: bool,
    host: &RecentQueriesHostPort<S>,
) -> El {
    let k = kit(dark);
    if vm.entries.is_empty() {
        return col()
            .key(EMPTY_KEY)
            .w_full()
            .gap(8.0)
            .center()
            .child(rune_icon(&ICON_CLOCK, 24.0).color(k.c.text_muted).opacity(0.4))
            .child(
                text(t("connWin.home.noRecentQueries"))
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            );
    }
    let mut body = col().key("connections-recent-rows").w_full().gap(8.0);
    for entry in vm.visible_entries() {
        body = body.child(recent_row(vm, entry, dark, host));
    }
    body
}

fn recent_row<S: 'static>(
    vm: &RecentQueriesVm,
    entry: &QueryHistoryVm,
    dark: bool,
    host: &RecentQueriesHostPort<S>,
) -> El {
    let k = kit(dark);
    let source = vm.source_name(entry);
    let relative = relative_label(entry.executed_at_ms, vm.now_ms);
    let copied = vm.copied_row_id() == Some(entry.id.as_str());

    let mut meta = row().grow(1.0).min_w(0.0).gap(6.0).center_y().clip_soft(0.0);
    meta = meta.child(
        row()
            .key(format!("home-query-status-{}", entry.id))
            .size(6.0, 6.0)
            .radius(3.0)
            .fill(if entry.success { k.c.success.fg } else { k.c.danger.fg }),
    );
    meta = meta.child(
        text(source.clone())
            .key(format!("home-query-source-{}", entry.id))
            .font(k.ty.small)
            .color(k.c.text_muted)
            .grow(1.0)
            .min_w(0.0)
            .truncate(),
    );
    if let Some(relative) = relative {
        meta = meta.child(text("·").font(k.ty.text_small).color(k.c.text_muted));
        meta = meta.child(text(relative).font(k.ty.text_small).color(k.c.text_muted));
    }
    meta = meta
        .child(text("·").font(k.ty.text_small).color(k.c.text_muted))
        .child(
            text(format!("{}ms", entry.execution_time_ms))
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        );

    let mut actions = row().gap(6.0).center_y().shrink();
    if host.on_select.is_some() {
        let mut rerun = row()
            .key(format!("home-query-rerun-{}", entry.id))
            .gap(4.0)
            .center_y()
            .child(rune_icon(&ICON_PLAY, 12.0).color(k.c.text_muted))
            .child(text(t("connWin.home.queries.rerun")).font(k.ty.text_small).color(k.c.text_muted));
        if let Some(on_select) = &host.on_select {
            let id = entry.id.clone();
            let on_select = on_select.clone();
            rerun = rerun.on_press(move |state: &mut S| on_select(state, &id));
        }
        actions = actions.child(rerun);
    }
    let mut copy = row()
        .key(format!("home-query-copy-{}", entry.id))
        .gap(4.0)
        .center_y()
        .child(
            rune_icon(if copied { &ICON_CHECK } else { &ICON_COPY }, 12.0)
                .color(if copied { k.c.success.fg } else { k.c.text_muted }),
        )
        .child(
            text(if copied {
                t("connWin.home.aiIntegration.copied")
            } else {
                t("connWin.home.aiIntegration.copy")
            })
            .font(k.ty.text_small)
            .color(if copied { k.c.success.fg } else { k.c.text_muted }),
        );
    if let (Some(on_copy), Some(on_copied_change)) = (&host.on_copy, &host.on_copied_change) {
        let id = entry.id.clone();
        let sql = entry.sql.clone();
        let on_copy = on_copy.clone();
        let on_copied_change = on_copied_change.clone();
        copy = copy.on_press(move |state: &mut S| {
            on_copy(state, &id, &sql);
            on_copied_change(state, Some(id.clone()), true);
        });
    }
    actions = actions.child(copy);

    let mut row_el = col()
        .key(format!("connections-recent-row-{}", entry.id))
        .w_full()
        .gap(4.0)
        .padding(10.0)
        .radius(8.0)
        .border(1.0, k.c.border_base)
        .fill(k.c.surface_base)
        .child(row().w_full().min_w(0.0).gap(8.0).center_y().child(meta).child(actions))
        .child(
            text(entry.sql.clone())
                .key(format!("home-query-sql-{}", entry.id))
                .font(k.ty.mono)
                .color(k.c.text_muted)
                .truncate(),
        );
    if let Some(on_select) = &host.on_select {
        let press_id = entry.id.clone();
        let key_id = entry.id.clone();
        let on_press = on_select.clone();
        let on_key = on_select.clone();
        // Rune routes Space to the native `on_press` of a focused button but
        // passes Return to `on_key`, so Enter needs this explicit binding.
        row_el = row_el
            .on_press(move |state: &mut S| on_press(state, &press_id))
            .on_key(move |state: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                if row_activates(ev) {
                    on_key(state, &key_id);
                    true
                } else {
                    false
                }
            });
    }
    row_el.role("button").focusable(host.on_select.is_some())
}

/// React's keyboard activation for a history row (Enter or Space only).
pub fn row_activates(key: &KeyEvent) -> bool {
    // Rune delivers Space as `Text(" ")`, matching the runtime's key mapping.
    matches!(key.key, Key::Enter) || matches!(&key.key, Key::Text(text) if text == " ")
}

/// Host-side copy feedback deadline helper (`tx.wake_after`).
#[allow(dead_code)] // helper; remove when the host wires copy.
pub fn schedule_copied_reset(tx: &mut Tx) {
    tx.wake_after(std::time::Duration::from_millis(COPIED_FEEDBACK_MS));
}
