//! Dashboard tab: the data-driven metric-card grid and full-size chart cards.
//!
//! React derives both from the status/history snapshots and hides a card whose
//! backing field is absent. The Rune port keeps the same keys, order and exact
//! labels; `ChartCanvas` (recharts) is a separate engine, so a chart card shows
//! the legend labels and a `server-status-chart-canvas-{key}` marker carrying
//! the point count. Remove the marker once a chart port lands.

use rune::{col, icon, row, text, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::{
    ICON_ARROW_DOWN, ICON_ARROW_UP, ICON_CLOCK, ICON_CONNECTIONS, ICON_FOLDER, ICON_FOLDER_OPEN,
    ICON_LOADER2, ICON_REFRESH, ICON_SPARKLES, ICON_TABLE, ICON_X,
};
use crate::ui::kit::kit;

use super::format::{
    card_rate_from_history, format_byte_rate, format_bytes, format_rate, format_uptime, label_for_key,
    latest_series_value, number_from,
};
use super::{ServerStatusVm, StatusValue, TrendPointVm, AUTO_REFRESH_OPTIONS};

#[allow(dead_code)] // render VM; consumed by the card grid and tests.
pub struct MetricCardVm {
    pub key: &'static str,
    pub label: String,
    pub value: String,
    pub glyph: &'static rune::Icon,
}

/// React `cards`: 11 definitions, filtered to those with backing data.
#[allow(dead_code)] // pure projection; consumed by the overview and tests.
pub fn metric_cards(vm: &ServerStatusVm) -> Vec<MetricCardVm> {
    let status = match &vm.status {
        Some(status) => status,
        None => return Vec::new(),
    };
    let field = |key: &str| status.get(key).cloned().unwrap_or(StatusValue::Null);
    let history = &vm.history;
    let latest = |key: &str| {
        latest_series_value(history.get(key).map(Vec::as_slice))
    };
    let mut cards = Vec::new();

    let qps = card_rate_from_history(history, "qps", &field("qps"));
    if qps != "—" {
        cards.push(MetricCardVm { key: "qps", label: t("common.queriesPerSec").to_owned(), value: qps, glyph: &ICON_CONNECTIONS });
    }
    if let Some(connections) = number_from(&field("connections")) {
        let value = match number_from(&field("maxConnections")) {
            Some(max) => format!("{} / {}", fmt_num(connections), fmt_num(max)),
            None => fmt_num(connections),
        };
        cards.push(MetricCardVm { key: "connections", label: t("serverStatus.connections").to_owned(), value, glyph: &ICON_CONNECTIONS });
    }
    let cache = field("cacheHitRatio");
    let innodb = field("innodbHitRatio");
    if !cache.is_absent() || !innodb.is_absent() {
        let value = if !cache.is_absent() { cache.display() } else { innodb.display() };
        cards.push(MetricCardVm { key: "cacheHit", label: t("serverStatus.cacheHitRatio").to_owned(), value, glyph: &ICON_TABLE });
    }
    if let Some(active) = number_from(&field("activeQueries")) {
        cards.push(MetricCardVm { key: "activeQueries", label: t("serverStatus.activeQueries").to_owned(), value: fmt_num(active), glyph: &ICON_SPARKLES });
    }
    if let Some(net_in) = latest("netIn") {
        cards.push(MetricCardVm { key: "netIn", label: t("serverStatus.networkIn").to_owned(), value: format_byte_rate(net_in), glyph: &ICON_ARROW_DOWN });
    }
    if let Some(net_out) = latest("netOut") {
        cards.push(MetricCardVm { key: "netOut", label: t("serverStatus.networkOut").to_owned(), value: format_byte_rate(net_out), glyph: &ICON_ARROW_UP });
    }
    if let Some(deadlocks) = number_from(&field("deadlocks")) {
        cards.push(MetricCardVm { key: "deadlocks", label: t("serverStatus.deadlocks").to_owned(), value: fmt_num(deadlocks), glyph: &ICON_X });
    }
    if let Some(temp_files) = number_from(&field("tempFiles")) {
        cards.push(MetricCardVm { key: "tempFiles", label: t("serverStatus.tempFiles").to_owned(), value: fmt_num(temp_files), glyph: &ICON_FOLDER });
    }
    if let Some(slow) = number_from(&field("slowQueries")) {
        cards.push(MetricCardVm { key: "slowQueries", label: t("serverStatus.slowQueries").to_owned(), value: fmt_num(slow), glyph: &ICON_X });
    }
    if let Some(wal) = number_from(&field("walBytes")) {
        cards.push(MetricCardVm { key: "walRate", label: t("serverStatus.walRate").to_owned(), value: format_bytes(wal), glyph: &ICON_FOLDER_OPEN });
    }
    if let Some(uptime) = number_from(&field("uptimeSeconds")) {
        cards.push(MetricCardVm { key: "uptime", label: t("serverStatus.uptime").to_owned(), value: format_uptime(uptime), glyph: &ICON_CLOCK });
    }
    cards
}

/// JS `String(number)`: integers without a trailing `.0`.
fn fmt_num(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

#[allow(dead_code)] // render VM; consumed by the chart grid and tests.
pub struct ChartCardVm {
    pub key: &'static str,
    pub label: String,
    pub series_keys: Vec<String>,
    pub series_labels: Vec<String>,
    pub chart_type: &'static str,
    pub full_width: bool,
}

fn has_series(vm: &ServerStatusVm, keys: &[&str]) -> bool {
    keys.iter().any(|key| vm.history.get(*key).is_some_and(|series| !series.is_empty()))
}

/// React `chartCards`: 9 definitions gated by backing history series, in React's
/// push order. Command labels come from `serverStatus.cmd{cap(c)}`.
#[allow(dead_code)] // pure projection; consumed by the overview and tests.
pub fn chart_cards(vm: &ServerStatusVm) -> Vec<ChartCardVm> {
    let mut cards = Vec::new();
    if has_series(vm, &["qps"]) {
        cards.push(ChartCardVm {
            key: "transactions",
            label: t("common.queriesPerSec").to_owned(),
            series_keys: vec!["qps".to_owned()],
            series_labels: vec![t("serverStatus.trendTotal").to_owned()],
            chart_type: "area",
            full_width: false,
        });
    }
    if has_series(vm, &["sessionTotal", "sessionActive", "sessionIdle"]) {
        cards.push(ChartCardVm {
            key: "sessionsState",
            label: t("serverStatus.chartServerSessions").to_owned(),
            series_keys: ["sessionTotal", "sessionActive", "sessionIdle"].map(str::to_owned).to_vec(),
            series_labels: [
                t("serverStatus.trendTotal"),
                t("serverStatus.trendActive"),
                t("serverStatus.trendIdle"),
            ].map(str::to_owned).to_vec(),
            chart_type: "line",
            full_width: false,
        });
    }
    if has_series(vm, &["sessions"]) {
        cards.push(ChartCardVm {
            key: "sessions",
            label: t("serverStatus.chartSessions").to_owned(),
            series_keys: vec!["sessions".to_owned()],
            series_labels: vec![t("serverStatus.chartSessions").to_owned()],
            chart_type: "area",
            full_width: false,
        });
    }
    if has_series(vm, &["netIn", "netOut"]) {
        cards.push(ChartCardVm {
            key: "netTraffic",
            label: t("serverStatus.chartNetIn").to_owned(),
            series_keys: ["netIn", "netOut"].map(str::to_owned).to_vec(),
            series_labels: [t("serverStatus.networkIn"), t("serverStatus.networkOut")].map(str::to_owned).to_vec(),
            chart_type: "line",
            full_width: false,
        });
    }
    if has_series(vm, &["cmd_select", "cmd_insert", "cmd_update", "cmd_delete"]) {
        cards.push(ChartCardVm {
            key: "commands",
            label: t("serverStatus.chartCommands").to_owned(),
            series_keys: ["select", "insert", "update", "delete"].map(|s| format!("cmd_{s}")).to_vec(),
            series_labels: ["select", "insert", "update", "delete"]
                .map(|s| {
                    let mut capitalized = s.to_owned();
                    if let Some(first) = capitalized.get_mut(0..1) {
                        first.make_ascii_uppercase();
                    }
                    t(&format!("serverStatus.cmd{capitalized}")).to_owned()
                })
                .to_vec(),
            chart_type: "line",
            full_width: false,
        });
    }
    if has_series(vm, &["blksRead", "blksHit"]) {
        cards.push(ChartCardVm {
            key: "blockIO",
            label: t("serverStatus.chartBlockIO").to_owned(),
            series_keys: ["blksRead", "blksHit"].map(str::to_owned).to_vec(),
            series_labels: [t("serverStatus.trendBlockRead"), t("serverStatus.trendBlockHit")].map(str::to_owned).to_vec(),
            chart_type: "line",
            full_width: false,
        });
    }
    if has_series(vm, &["tupInserted", "tupUpdated", "tupDeleted"]) {
        cards.push(ChartCardVm {
            key: "tupleWrite",
            label: t("serverStatus.chartTupleWrite").to_owned(),
            series_keys: ["tupInserted", "tupUpdated", "tupDeleted"].map(str::to_owned).to_vec(),
            series_labels: [t("serverStatus.cmdInsert"), t("serverStatus.cmdUpdate"), t("serverStatus.cmdDelete")].map(str::to_owned).to_vec(),
            chart_type: "line",
            full_width: false,
        });
    }
    if has_series(vm, &["tupFetched", "tupReturned"]) {
        cards.push(ChartCardVm {
            key: "tupleRead",
            label: t("serverStatus.chartTupleRead").to_owned(),
            series_keys: ["tupFetched", "tupReturned"].map(str::to_owned).to_vec(),
            series_labels: [t("serverStatus.trendFetched"), t("serverStatus.trendReturned")].map(str::to_owned).to_vec(),
            chart_type: "line",
            full_width: false,
        });
    }
    if has_series(vm, &["commits", "rollbacks"]) {
        cards.push(ChartCardVm {
            key: "txRate",
            label: t("serverStatus.chartTransactions").to_owned(),
            series_keys: ["commits", "rollbacks"].map(str::to_owned).to_vec(),
            series_labels: [t("serverStatus.trendCommit"), t("serverStatus.trendRollback")].map(str::to_owned).to_vec(),
            chart_type: "area",
            full_width: true,
        });
    }
    cards
}

/// Point count a chart would receive from `toChartData` (the union of sample
/// timestamps across its series), shown on the stand-in canvas marker.
#[allow(dead_code)] // pure helper; consumed by the chart grid and tests.
pub fn chart_point_count(vm: &ServerStatusVm, card: &ChartCardVm) -> usize {
    let mut stamps = std::collections::BTreeSet::new();
    for key in &card.series_keys {
        if let Some(series) = vm.history.get(key) {
            for point in series {
                stamps.insert(point.t);
            }
        }
    }
    stamps.len()
}

/// Build the dashboard body: metric grid + the chart section when populated.
#[allow(dead_code)] // view part; remove when the connection host wires the panel.
pub fn build(vm: &ServerStatusVm, dark: bool) -> El {
    let k = kit(dark);
    let mut body = col().key("server-status-dashboard").w_full().min_w(0.0).gap(12.0);

    let cards = metric_cards(vm);
    if !cards.is_empty() {
        let mut grid = row().key("server-status-cards").w_full().min_w(0.0).gap(10.0).flex_wrap();
        for card in cards {
            grid = grid.child(metric_card(&card, k, dark));
        }
        body = body.child(grid);
    }

    let chart_cards = chart_cards(vm);
    if !chart_cards.is_empty() {
        body = body.child(
            row().key("server-status-chart-title").w_full().gap(6.0).center_y().child(icon(&ICON_REFRESH, 14.0).color(k.c.text_muted))
                .child(text(t("serverStatus.chartTitle")).font(k.ty.text_small).color(k.c.text_base)),
        );
        let mut grid = row().key("server-status-charts").w_full().min_w(0.0).gap(10.0).flex_wrap();
        for card in &chart_cards {
            grid = grid.child(chart_card(vm, card, k, dark));
        }
        body = body.child(grid);
    }
    body
}

fn metric_card(card: &MetricCardVm, k: &Kit, dark: bool) -> El {
    col()
        .key(format!("server-status-card-{}", card.key))
        .w(210.0)
        .gap(4.0)
        .padding(10.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .radius(8.0)
        .child(icon(card.glyph, 14.0).color(k.c.text_muted))
        .child(text(card.label.clone()).font(k.ty.text_small).color(k.c.text_muted))
        .child(text(card.value.clone()).key(format!("server-status-card-value-{}", card.key)).font(k.ty.mono).color(k.c.text_base))
        .help(if dark { card.label.clone() } else { card.label.clone() })
}

fn chart_card(vm: &ServerStatusVm, card: &ChartCardVm, k: &Kit, dark: bool) -> El {
    let _ = dark;
    let count = chart_point_count(vm, card);
    let mut panel = col()
        .key(format!("server-status-chart-{}", card.key))
        .w(340.0)
        .min_h(220.0)
        .gap(6.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .radius(8.0)
        .child(
            row().key(format!("server-status-chart-head-{}", card.key)).w_full().gap(6.0).center_y().child(icon(&ICON_LOADER2, 14.0).color(k.c.text_muted))
                .child(text(card.label.clone()).font(k.ty.text_small).color(k.c.text_base)),
        );
    let mut legend = row().key(format!("server-status-chart-legend-{}", card.key)).w_full().gap(8.0).flex_wrap();
    for label in &card.series_labels {
        legend = legend.child(text(label.clone()).font(k.ty.text_xsmall).color(k.c.text_muted));
    }
    panel = panel.child(legend).child(
        // The chart engine is a separate port; the marker pins the point count
        // and series keys the real ChartCanvas would receive.
        text(format!("{} · {}", count, card.series_keys.join(",")))
            .key(format!("server-status-chart-canvas-{}", card.key))
            .font(k.ty.mono)
            .color(k.c.text_muted),
    );
    panel
}

/// React `toChartData` union length, exposed for the pure tests.
#[allow(dead_code)] // pure helper; consumed by tests.
pub fn to_chart_point_count(keys: &[String], history: &std::collections::BTreeMap<String, Vec<TrendPointVm>>) -> usize {
    let mut stamps = std::collections::BTreeSet::new();
    for key in keys {
        if let Some(series) = history.get(key) {
            for point in series {
                stamps.insert(point.t);
            }
        }
    }
    stamps.len()
}
