//! The connections home (A1), aligned with the React ConnectionWorkspaceHome:
//! a centered content column, a hero with live counts and engine badges, a
//! header row with filter / import / new-connection actions, the bordered
//! card list with status pills and connect buttons, show-all paging, and the
//! recent-queries card. Zero connections renders the centered empty state.
//! Import and the connection form ride the root's modal layer
//! (`views/modals.rs`); the interaction keys (`connection-{id}`,
//! `connection-new`, `import-*`, the recent keys) are the ones the existing
//! tests and flows press.

use super::*;

use crate::icons::{ICON_CLOCK, ICON_CONNECTIONS, ICON_DOWNLOAD, ICON_PLUS, ICON_SEARCH};
use crate::state_def::AppState;
use datazen_ui_api::ConnectionProfile;
use rune::tok::{self, type_};
use rune::{
    col, icon, oklch, oklcha, rich, row, text, text_input, Align, Color, El, ScrollSpec, Span,
    TextCase, TextStyle,
};

/// How many cards render before the show-all row appears (React `COLLAPSED_COUNT`).
const COLLAPSED_COUNT: usize = 6;
/// The centered content column's width (React `max-w-5xl`).
const CONTENT_WIDTH: f32 = 1008.0;

/// The connections page body: banners, and the centered empty state or the
/// landing. The connection form and import dialogs ride the root modal layer
/// (`views/modals.rs`).
pub(crate) fn connections_home(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let connections = state.visible_connections();
    if connections.is_empty() {
        return empty_page(state, theme, dark);
    }
    landing(state, theme, dark, connections)
}

/// The honest banners the connections page shows over either body: the
/// desktop adapter's failure sentence and an open job in flight.
fn banners(state: &AppState, theme: tok::Theme) -> Vec<El> {
    let mut found = Vec::new();
    if let Some(message) = &state.desktop_error {
        found.push(
            row()
                .key("desktop-adapter-banner")
                .w_full()
                .min_h(30.0)
                .px(12.0)
                .py(6.0)
                .gap(8.0)
                .center_y()
                .radius(8.0)
                .fill(oklcha(0.66, 0.15, 58.0, 0.14))
                .child(
                    text(message.clone())
                        .key("desktop-adapter-message")
                        .font(type_::LABEL)
                        .color(theme.text)
                        .w_full(),
                ),
        );
    }
    if state.connect_loading.is_some() {
        found.push(
            row()
                .key("connect-loading-row")
                .w_full()
                .h(30.0)
                .px(12.0)
                .center_y()
                .radius(8.0)
                .fill(theme.control_track)
                .child(
                    text("Connecting…")
                        .key("connect-loading")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    found
}

/// React State 1: no connections at all — a centered card with the actions.
fn empty_page(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let accent = tok::accent(0, dark);
    let mut page = col()
        .key("connection-list")
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .gap(12.0)
        .center();
    for banner in banners(state, theme) {
        page = page.child(banner);
    }
    page.child(
        col()
            .key("connections-empty")
            .w(380.0)
            .gap(16.0)
            .center()
            .child(
                row()
                    .key("connections-empty-icon")
                    .size(48.0, 48.0)
                    .radius(14.0)
                    .center()
                    .fill(oklcha(if dark { 0.62 } else { 0.6 }, 0.17, 256.0, 0.14))
                    .child(icon(&ICON_CONNECTIONS, 22.0).color(accent)),
            )
            .child(
                text("No connections yet")
                    .key("connections-empty-title")
                    .font(TextStyle::new(14.0, 600.0))
                    .color(theme.text),
            )
            .child(
                text("Import connections from another tool, or create your first one.")
                    .key("connections-empty-hint")
                    .font(type_::LABEL)
                    .color(theme.text_secondary)
                    .align(Align::Center),
            )
            .child(
                row()
                    .key("connections-empty-actions")
                    .h(32.0)
                    .gap(8.0)
                    .center_y()
                    .child(
                        row()
                            .key("import-connections")
                            .h(30.0)
                            .px(12.0)
                            .gap(6.0)
                            .center_y()
                            .radius(8.0)
                            .fill(theme.card)
                            .border(1.0, theme.card_border)
                            .child(icon(&ICON_DOWNLOAD, 14.0).color(theme.text_secondary))
                            .child(
                                text("Import connections…")
                                    .font(type_::LABEL)
                                    .color(theme.text),
                            )
                            .on_press(|state: &mut AppState| state.open_import_dialog()),
                    )
                    .child(
                        row()
                            .key("connections-create-first")
                            .h(30.0)
                            .px(12.0)
                            .gap(6.0)
                            .center_y()
                            .radius(8.0)
                            .fill(accent)
                            .child(icon(&ICON_PLUS, 14.0).color(on_accent()))
                            .child(
                                text("Create first connection")
                                    .font(type_::LABEL)
                                    .color(on_accent()),
                            )
                            .on_press(|state: &mut AppState| state.open_connection_dialog()),
                    ),
            ),
    )
}

/// React State 3: the landing — hero, header row, card list, recent queries
/// and the shortcut footer, inside a centered scroll column. Import runs in
/// the modal dialog (the React share dialog), not an inline bar.
fn landing(
    state: &AppState,
    theme: tok::Theme,
    dark: bool,
    connections: Vec<ConnectionProfile>,
) -> El {
    let mut page = col()
        .key("connection-list")
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .max_w(CONTENT_WIDTH)
        .self_center()
        .gap(20.0)
        .pb(4.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        });
    for banner in banners(state, theme) {
        page = page.child(banner);
    }
    page.child(home_hero(state, theme, dark, &connections))
        .child(home_header(state, theme, dark))
        .child(connection_cards(state, theme, dark, &connections))
        .child(recent_queries_card(theme))
        .child(shortcut_footer(theme))
}

/// A1 hero: title, live counts and the engine badge cluster (React HomeHero).
fn home_hero(
    state: &AppState,
    theme: tok::Theme,
    dark: bool,
    connections: &[ConnectionProfile],
) -> El {
    let mut engines: Vec<String> = connections.iter().map(|c| c.engine.clone()).collect();
    engines.sort();
    engines.dedup();
    let engine_count = engines.len();
    let connection_count = connections.len();
    let (count_text, count_spans) = counts_line(connection_count, engine_count, theme.text);

    let mut hero = row()
        .key("connections-home-hero")
        .w_full()
        .items_end()
        .gap(16.0)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(4.0)
                .child(
                    text("Select a connection")
                        .key("connections-home-title")
                        .font(TextStyle::new(17.0, 700.0))
                        .color(theme.text),
                )
                .child(
                    rich(count_text, count_spans)
                        .key("connections-home-subtitle")
                        .font(TextStyle::new(12.5, 400.0))
                        .color(theme.text_secondary),
                ),
        );
    if !engines.is_empty() {
        hero = hero.child(badge_cluster(theme, dark, &engines));
    }
    hero
}

/// "N connections · M engine types" with the numbers bold (React's `<b>` runs).
fn counts_line(connection_count: usize, engine_count: usize, strong: Color) -> (String, Vec<Span>) {
    let connection_word = if connection_count == 1 {
        "connection"
    } else {
        "connections"
    };
    let engine_word = if engine_count == 1 {
        "engine type"
    } else {
        "engine types"
    };
    let line = format!("{connection_count} {connection_word} · {engine_count} {engine_word}");
    let mut spans = Vec::new();
    if let Some(after) = line.find(' ') {
        spans.push(Span::new(0, after).bold().color(strong));
    }
    if let Some(at) = line.find(" · ") {
        let rest = at + 3;
        if let Some(tail) = line.get(rest..) {
            if let Some(len) = tail.find(' ') {
                spans.push(Span::new(rest, rest + len).bold().color(strong));
            }
        }
    }
    (line, spans)
}

/// Up to four overlapping engine badges (React's `-space-x-1.5` cluster).
fn badge_cluster(theme: tok::Theme, dark: bool, engines: &[String]) -> El {
    let mut cluster = row().key("connections-badge-cluster").items_center();
    for (index, engine) in engines.iter().take(4).enumerate() {
        let mut badge = engine_badge(engine, engine, theme, dark, 26.0);
        if index > 0 {
            badge = badge.ml(-7.0);
        }
        badge = badge.border(2.0, theme.bg);
        cluster = cluster.child(badge);
    }
    cluster
}

/// The header row: section label left, filter / import / new connection
/// right (React's `justify-between` header).
fn home_header(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let accent = tok::accent(0, dark);
    row()
        .key("connections-header")
        .w_full()
        .gap(8.0)
        .center_y()
        .child(
            text("Your connections")
                .key("connections-section-title")
                .font(TextStyle::new(11.5, 600.0).tracking(0.6))
                .color(theme.text_secondary)
                .text_case(TextCase::Uppercase)
                .grow(1.0),
        )
        .child(
            text_input(&state.connection_search, type_::LABEL)
                .key("home-connections-filter")
                .w(220.0)
                .h(28.0)
                .px(9.0)
                .radius(7.0)
                .fill(theme.card)
                .border(1.0, theme.card_border)
                .placeholder("Filter connections…", theme.text_secondary),
        )
        .child(
            row()
                .key("import-connections")
                .h(28.0)
                .px(10.0)
                .gap(6.0)
                .center_y()
                .radius(7.0)
                .fill(theme.card)
                .border(1.0, theme.card_border)
                .label("Import connections")
                .child(icon(&ICON_DOWNLOAD, 13.0).color(theme.text_secondary))
                .child(
                    text("Import connections…")
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .on_press(|state: &mut AppState| state.open_import_dialog()),
        )
        .child(
            row()
                .key("connection-new")
                .h(28.0)
                .px(10.0)
                .gap(6.0)
                .center_y()
                .radius(7.0)
                .fill(accent)
                .label("New connection")
                .child(icon(&ICON_PLUS, 13.0).color(on_accent()))
                .child(text("New connection").font(type_::LABEL).color(on_accent()))
                .on_press(|state: &mut AppState| state.open_connection_dialog()),
        )
}

/// The bordered card list: one row per connection, paged at six (React
/// ConnectionCardList). Rows keep the `connection-{id}` press key.
fn connection_cards(
    state: &AppState,
    theme: tok::Theme,
    dark: bool,
    connections: &[ConnectionProfile],
) -> El {
    let needle = state.connection_search.display().0.trim().to_lowercase();
    let matched: Vec<&ConnectionProfile> = connections
        .iter()
        .filter(|profile| {
            needle.is_empty()
                || profile.name.to_lowercase().contains(&needle)
                || profile.engine.to_lowercase().contains(&needle)
                || profile.id.to_lowercase().contains(&needle)
        })
        .collect();
    let total = matched.len();
    let visible_count = if state.connections_show_all || total <= COLLAPSED_COUNT {
        total
    } else {
        COLLAPSED_COUNT
    };

    let mut section = col().key("connections-cards-section").w_full().gap(6.0);
    let mut cards = col()
        .key("connections-card-list")
        .w_full()
        .radius(12.0)
        .fill(theme.card)
        .border(1.0, theme.card_border)
        .clip_rounded(12.0);
    if total == 0 {
        cards = cards.child(no_match_state(theme, &needle));
        return section.child(cards);
    }
    for (index, profile) in matched.iter().take(visible_count).enumerate() {
        let id = profile.id.clone();
        let mut card = row()
            .key(format!("connection-{id}"))
            .w_full()
            .min_h(56.0)
            .px(14.0)
            .py(10.0)
            .gap(12.0)
            .center_y()
            .hover(theme.hover)
            .label(format!("Connect {}", profile.name))
            .child(engine_badge(&id, &profile.engine, theme, dark, 34.0))
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(3.0)
                    .child(
                        row()
                            .key(format!("conn-name-row-{id}"))
                            .gap(6.0)
                            .center_y()
                            .child(
                                text(profile.name.clone())
                                    .font(TextStyle::new(13.0, 600.0))
                                    .color(theme.text),
                            )
                            .child(engine_chip(&id, &profile.engine, theme)),
                    )
                    .child(
                        text(engine_label(&profile.engine))
                            .font(type_::MONO_CAPTION)
                            .color(theme.text_secondary),
                    ),
            )
            .child(status_pill(state, theme, dark, &id))
            .child(
                row()
                    .key(format!("conn-open-{id}"))
                    .h(26.0)
                    .px(10.0)
                    .center_y()
                    .radius(7.0)
                    .fill(if state.connection_is_open(&id) {
                        theme.control_off
                    } else {
                        tok::accent(0, dark)
                    })
                    .child(
                        text(if state.connection_is_open(&id) {
                            "Open"
                        } else {
                            "Connect"
                        })
                        .font(type_::LABEL)
                        .color(if state.connection_is_open(&id) {
                            theme.text
                        } else {
                            on_accent()
                        }),
                    ),
            )
            .on_press(move |state: &mut AppState| state.open_connection(&id));
        if index > 0 {
            card = card.border_sides([1.0, 0.0, 0.0, 0.0], theme.separator);
        }
        cards = cards.child(card);
    }
    section = section.child(cards);
    if total > COLLAPSED_COUNT {
        section = section.child(paging_row(state, theme, visible_count, total));
    }
    section
}

/// "Showing X of Y · Show all" (React's paging row).
fn paging_row(state: &AppState, theme: tok::Theme, visible_count: usize, total: usize) -> El {
    let show_all_label = if state.connections_show_all {
        "Show less".to_owned()
    } else {
        format!("Show all {total}")
    };
    row()
        .key("connections-paging")
        .w_full()
        .px(2.0)
        .h(24.0)
        .gap(8.0)
        .center_y()
        .child(
            text(format!("Showing {visible_count} of {total}"))
                .key("connections-showing")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(col().grow(1.0))
        .child(
            row()
                .key("connections-show-all")
                .h(22.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .label(if state.connections_show_all {
                    "Show less"
                } else {
                    "Show all"
                })
                .child(
                    text(show_all_label)
                        .font(type_::LABEL)
                        .color(tok::accent(0, true)),
                )
                .on_press(|state: &mut AppState| {
                    state.connections_show_all = !state.connections_show_all;
                }),
        )
}

/// The filter matched nothing: the card list shows the search's empty state.
fn no_match_state(theme: tok::Theme, needle: &str) -> El {
    col()
        .key("connections-no-match")
        .w_full()
        .py(24.0)
        .gap(6.0)
        .center()
        .child(icon(&ICON_SEARCH, 18.0).color(theme.text_secondary))
        .child(
            text(format!("No matches for “{needle}”"))
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(
            text("Try a different name, engine or id.")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
}

/// React RecentQueriesList's honest empty card: the host does not expose
/// query history to the Rune UI yet (the query workspace says the same).
fn recent_queries_card(theme: tok::Theme) -> El {
    col()
        .key("connections-recent-queries")
        .w_full()
        .gap(8.0)
        .child(
            text("Recent queries")
                .key("connections-recent-title")
                .font(TextStyle::new(11.5, 600.0).tracking(0.6))
                .color(theme.text_secondary)
                .text_case(TextCase::Uppercase),
        )
        .child(
            col()
                .key("connections-recent-card")
                .w_full()
                .radius(12.0)
                .fill(theme.card)
                .border(1.0, theme.card_border)
                .px(16.0)
                .py(14.0)
                .child(
                    row()
                        .key("connections-recent-empty")
                        .w_full()
                        .h(36.0)
                        .gap(8.0)
                        .center()
                        .child(icon(&ICON_CLOCK, 16.0).color(theme.text_secondary))
                        .child(
                            text("No recent queries")
                                .font(type_::LABEL)
                                .color(theme.text_secondary),
                        ),
                ),
        )
}

/// The wired shortcut the landing can honestly advertise (React's footer).
fn shortcut_footer(theme: tok::Theme) -> El {
    row()
        .key("connections-shortcut-footer")
        .w_full()
        .py(2.0)
        .gap(6.0)
        .justify_center()
        .center_y()
        .child(
            row()
                .key("connections-shortcut-key")
                .h(20.0)
                .px(6.0)
                .center_y()
                .radius(5.0)
                .fill(theme.bg)
                .border(1.0, theme.card_border)
                .child(
                    text("⌘↩")
                        .font(type_::MONO_CAPTION)
                        .color(theme.text_secondary),
                ),
        )
        .child(
            text("Run query")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
}

/// The per-connection status pill: connected (green), connecting (accent) or
/// offline (muted) — React's connected/connecting/offline pills. A connect
/// in flight names the row the last press opened (the most recent one).
fn status_pill(state: &AppState, theme: tok::Theme, dark: bool, id: &str) -> El {
    let open = state.connection_is_open(id);
    let connecting = state.connect_loading.is_some()
        && state
            .recent_connection_ids
            .first()
            .is_some_and(|first| first == id);
    let (bg, fg, dot, label) = if open {
        (
            oklcha(if dark { 0.7 } else { 0.55 }, 0.16, 152.0, 0.16),
            tok::accent(4, dark),
            tok::accent(4, dark),
            "Connected",
        )
    } else if connecting {
        (
            oklcha(if dark { 0.66 } else { 0.6 }, 0.17, 256.0, 0.16),
            tok::accent(0, dark),
            tok::accent(0, dark),
            "Connecting…",
        )
    } else {
        (
            theme.control_track,
            theme.text_secondary,
            theme.text_secondary,
            "Offline",
        )
    };
    row()
        .key(format!("conn-status-{id}"))
        .h(20.0)
        .px(8.0)
        .gap(5.0)
        .center_y()
        .radius(10.0)
        .fill(bg)
        .child(
            col()
                .key(format!("conn-status-dot-{id}"))
                .size(6.0, 6.0)
                .radius(3.0)
                .fill(dot),
        )
        .child(text(label).font(TextStyle::new(10.5, 500.0)).color(fg))
}

/// A rounded engine monogram: stable hue per engine string, white initial.
fn engine_badge(id: &str, engine: &str, theme: tok::Theme, dark: bool, size: f32) -> El {
    let hue = engine_hue(engine);
    let fill = oklch(if dark { 0.5 } else { 0.58 }, 0.12, hue);
    let initial = engine
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    row()
        .key(format!("conn-badge-{id}"))
        .size(size, size)
        .radius(size * 0.28)
        .center()
        .fill(fill)
        .child(
            text(initial)
                .font(TextStyle::new(size * 0.42, 700.0))
                .color(oklch(0.99, 0.005, 256.0)),
        )
}

/// The small bordered chip naming the engine (React's type chip).
fn engine_chip(id: &str, engine: &str, theme: tok::Theme) -> El {
    row()
        .key(format!("conn-chip-{id}"))
        .h(16.0)
        .px(6.0)
        .center_y()
        .radius(5.0)
        .fill(theme.bg)
        .border(1.0, theme.card_border)
        .child(
            text(engine.to_lowercase())
                .font(TextStyle::new(9.5, 500.0))
                .color(theme.text_secondary),
        )
}

/// Display label for an engine id ("postgres" → "PostgreSQL").
pub(crate) fn engine_label(engine: &str) -> String {
    match engine.to_lowercase().as_str() {
        "postgres" | "postgresql" | "pg" => "PostgreSQL".into(),
        "sqlite" | "sqlite3" => "SQLite".into(),
        "mysql" => "MySQL".into(),
        "mariadb" => "MariaDB".into(),
        "mongodb" | "mongo" => "MongoDB".into(),
        "redis" => "Redis".into(),
        "sqlserver" | "mssql" => "SQL Server".into(),
        "clickhouse" => "ClickHouse".into(),
        "duckdb" => "DuckDB".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// Stable accent hue per engine: a few known hues, otherwise a string hash.
fn engine_hue(engine: &str) -> f32 {
    match engine.to_lowercase().as_str() {
        "postgres" | "postgresql" | "pg" => 256.0,
        "sqlite" | "sqlite3" => 152.0,
        "mysql" | "mariadb" => 58.0,
        "mongodb" | "mongo" => 152.0,
        "redis" => 25.0,
        "sqlserver" | "mssql" => 220.0,
        "clickhouse" => 300.0,
        "duckdb" => 80.0,
        _ => {
            let mut hash: u64 = 0xcbf29ce484222325;
            for byte in engine.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
            (hash % 360) as f32
        }
    }
}

/// Text that stays readable on the accent fill in both appearances.
fn on_accent() -> Color {
    oklch(0.985, 0.005, 256.0)
}
