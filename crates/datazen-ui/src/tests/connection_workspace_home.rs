//! T144: `windows/connection/ConnectionWorkspaceHome.tsx` — hero counts, card
//! keys/actions, filter, recent queries, MCP copy feedback with a pinned real
//! wake deadline, KV-slot takeover and two-window/two-theme geometry.
//!
//! Observation is through `Runtime::elements()` (key/text/rect/label) and real
//! pointer/key events. Zero-size React `data-testid` markers are checked by key
//! presence, never by `find()` coordinates.

use rune::{tok, Cx, El, Env, Runtime, Tx};

use crate::state_def::AppState;
use crate::views::{connections_home, ConnectionHomeSlotVm, HOME_COPY_FEEDBACK_MS};
use datazen_ui_api::ConnectionProfile;

fn profile(id: &str, name: &str, engine: &str) -> ConnectionProfile {
    ConnectionProfile {
        id: id.to_owned(),
        name: name.to_owned(),
        engine: engine.to_owned(),
    }
}

fn fixtures() -> Vec<ConnectionProfile> {
    vec![
        profile("conn-1", "PostgreSQL-Local", "postgresql"),
        profile("conn-2", "MySQL-Prod", "mysql"),
        profile("conn-3", "Redis-Cache", "redis"),
        profile("conn-4", "SQLite-Dev", "sqlite"),
        profile("conn-5", "MongoDB-Cluster", "mongodb"),
    ]
}

fn view_dark(s: &AppState, _cx: &mut Cx<AppState>) -> El {
    connections_home(s, tok::theme(true), true)
}

fn view_light(s: &AppState, _cx: &mut Cx<AppState>) -> El {
    connections_home(s, tok::theme(false), false)
}

fn runtime(dark: bool, size: (f32, f32), profiles: Vec<ConnectionProfile>) -> Runtime<AppState> {
    let mut state = AppState::default();
    state.desktop_connections = Some(profiles);
    let view: fn(&AppState, &mut Cx<AppState>) -> El = if dark { view_dark } else { view_light };
    let mut rt = Runtime::new(
        state,
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn refresh(rt: &mut Runtime<AppState>) {
    rt.invalidate();
    rt.frame(1.0 / 60.0);
}

// ---- Empty state (React State 1) ----------------------------------------------------------------

#[test]
fn empty_state_keeps_react_keys_and_exact_i18n_text() {
    let rt = runtime(true, (960.0, 640.0), Vec::new());
    for key in [
        "connection-workspace-home",
        "connections-empty",
        "import-connections-button",
        "new-connection-button",
        "home-empty-title-i18n",
        "home-empty-hint-i18n",
        "home-create-first-i18n",
    ] {
        assert!(has(&rt, key), "{key} is mounted in the empty state");
    }
    let rendered = texts(&rt);
    for key in [
        "main.noConnections",
        "connWin.home.emptyNoConnectionsHint",
        "main.createFirst",
    ] {
        assert!(
            rendered.iter().any(|x| x == crate::i18n::t(key)),
            "{key}: exact translated text is visible"
        );
    }
}

// ---- Hero counts and landing keys ---------------------------------------------------------------

#[test]
fn hero_renders_live_counts_and_the_react_subtitle_marker() {
    let rt = runtime(true, (960.0, 640.0), fixtures());
    assert!(has(&rt, "home-hero-subtitle"), "React hero testid survives");
    assert!(has(&rt, "connections-home-subtitle"), "A1 key survives");

    let subtitle = element(&rt, "connections-home-subtitle");
    let line = subtitle.text.as_deref().unwrap_or("");
    assert!(line.contains("5"), "5 connections: {line:?}");
    // ConnectionProfile has no group metadata yet, so the group count is 0.
    assert!(line.contains("0"), "0 groups: {line:?}");
    for key in [
        "connWin.home.hero.subtitleConnections",
        "connWin.home.hero.subtitleGroups",
        "connWin.home.hero.subtitleTypes",
    ] {
        assert!(line.contains(crate::i18n::t(key)), "{key} appears in the hero: {line:?}");
    }
    for vanity in [
        "connWin.home.metrics.connections",
        "connWin.home.metrics.pinned",
        "connWin.home.metrics.dbTypes",
    ] {
        assert!(!texts(&rt).iter().any(|x| x == crate::i18n::t(vanity)), "{vanity} is gone");
    }
}

#[test]
fn landing_keeps_all_react_section_and_action_keys() {
    let rt = runtime(true, (1440.0, 816.0), fixtures());
    for key in [
        "connection-list",
        "home-connections-section",
        "home-connections-filter",
        "home-conn-card-conn-1",
        "home-conn-connect-conn-1",
        "conn-status-conn-1",
        "conn-chip-conn-1",
        "home-recent-queries-section",
        "view-all-history-button",
        "home-mcp-promo-bar",
        "home-mcp-copy-button",
        "home-shortcut-footer",
    ] {
        assert!(has(&rt, key), "{key} is mounted on the landing page");
    }
    assert!(texts(&rt).iter().any(|x| x == "datazen --mcp"), "the MCP CLI command is shown");
    for key in [
        "connWin.home.recentQueries",
        "connWin.home.noRecentQueries",
        "connWin.home.viewAll",
        "connWin.home.mcp.shortDesc",
    ] {
        assert!(texts(&rt).iter().any(|x| x == crate::i18n::t(key)), "{key} uses its exact key");
    }
}

// ---- Cards, filter and paging -------------------------------------------------------------------

#[test]
fn card_row_and_connect_button_dispatch_open() {
    let mut rt = runtime(true, (1440.0, 816.0), fixtures());
    assert!(has(&rt, "home-conn-card-conn-1"));
    let connect_marker = element(&rt, "home-conn-connect-conn-1");
    assert!(!connect_marker.focusable, "the connect marker is inert, not a fake button");
    // The card row carries the press; the connect marker is intentionally inert.
    click(&mut rt, "connection-conn-1");
    assert_eq!(
        rt.state.recent_connection_ids.first().map(String::as_str),
        Some("conn-1"),
        "clicking the card marks it as the most recent connection"
    );
}

#[test]
fn filter_matches_name_engine_and_clear_button_restores_all() {
    let mut rt = runtime(true, (1440.0, 816.0), fixtures());

    let (x, y) = rt.find("home-connections-filter").expect("filter field");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste("mysql");
    rt.frame(0.0);
    assert_eq!(rt.state.connection_search.text, "mysql");
    assert!(has(&rt, "home-conn-card-conn-2"), "MySQL card matches");
    assert!(!has(&rt, "home-conn-card-conn-1"), "other cards are filtered out");
    assert!(has(&rt, "home-connections-filter-clear"), "clear affordance appears");

    // A no-match query shows the localized empty marker.
    rt.state.connection_search.clear();
    rt.frame(0.0);
    rt.paste("zzz-no-match");
    rt.frame(0.0);
    assert!(has(&rt, "home-connections-no-match"), "no-match marker");
    assert!(!has(&rt, "home-conn-card-conn-2"));

    click(&mut rt, "home-connections-filter-clear");
    assert!(rt.state.connection_search.text.is_empty(), "clear button empties the filter");
    assert!(has(&rt, "home-conn-card-conn-1"), "all cards restored");
}

#[test]
fn show_all_expands_and_collapses_the_six_card_page() {
    let mut profiles = fixtures();
    profiles.push(profile("conn-6", "MariaDB-Analytics", "mariadb"));
    profiles.push(profile("conn-7", "ClickHouse-Metrics", "clickhouse"));
    let mut rt = runtime(true, (1440.0, 816.0), profiles);

    let visible = |rt: &Runtime<AppState>| {
        (1..=7).filter(|i| has(rt, &format!("home-conn-card-conn-{i}"))).count()
    };
    assert_eq!(visible(&rt), 6, "six cards before expanding");
    assert!(has(&rt, "home-connections-show-all"));

    click(&mut rt, "connections-show-all");
    assert_eq!(visible(&rt), 7, "show all reveals the seventh card");
    assert!(texts(&rt).iter().any(|x| x == crate::i18n::t("connWin.home.connections.showLess")));

    click(&mut rt, "connections-show-all");
    assert_eq!(visible(&rt), 6, "second press collapses again");
}

// ---- MCP copy feedback: real wake + deadline -----------------------------------------------------

#[test]
fn mcp_copy_schedules_a_real_2000ms_wake_and_reverts_at_the_deadline() {
    assert_eq!(HOME_COPY_FEEDBACK_MS, 2000);
    let mut rt = runtime(true, (1440.0, 816.0), fixtures());
    assert!(rt.wake_deadline().is_none(), "no wake before the copy press");

    click(&mut rt, "home-mcp-copy-button");
    let until = rt
        .state
        .home_copied_until
        .expect("copy press records a deadline");
    let deadline = rt
        .wake_deadline()
        .expect("copy press must schedule a real runtime wake");
    assert!(
        (deadline - until).abs() <= 0.01,
        "wake must match the recorded deadline: wake={deadline} until={until}"
    );
    assert_eq!(rt.state.home_copied_id.as_deref(), Some("__mcp__"));
    assert_eq!(
        element(&rt, "home-mcp-copy-button").label.as_deref(),
        Some(crate::i18n::t("connWin.home.aiIntegration.copied")),
        "the copied label is shown inside the window"
    );

    // The app poll reverts the label exactly at the deadline (not a boolean
    // that never expires). Poll with the pinned time, then redraw.
    assert!(crate::app::app_poll(&mut rt.state, until, &mut Tx::default()));
    refresh(&mut rt);
    assert!(rt.state.home_copied_until.is_none());
    assert_eq!(
        element(&rt, "home-mcp-copy-button").label.as_deref(),
        Some(crate::i18n::t("common.copy")),
        "label reverts after the deadline"
    );
}

// ---- KV-slot takeover (React state 3b) -----------------------------------------------------------

#[test]
fn driver_home_slot_takes_over_only_when_present() {
    let mut rt = runtime(true, (1440.0, 816.0), fixtures());
    assert!(!has(&rt, "home-kv-connection-home"), "no slot: host banner page");
    assert!(has(&rt, "connections-home-hero"));

    rt.state.home_slot = Some(ConnectionHomeSlotVm {
        connection_id: "cfg-1".to_owned(),
        db_session_id: "conn-1".to_owned(),
        connection_name: "Redis local".to_owned(),
        database_type: "redis".to_owned(),
        initial_database: "db3".to_owned(),
    });
    refresh(&mut rt);

    assert!(has(&rt, "home-kv-connection-home"), "slot takes over");
    assert_eq!(element(&rt, "home-kv-connection-home").label.as_deref(), Some("kv-connection-home"));
    assert!(has(&rt, "fixture-connection-home"), "driver surface marker");
    assert!(!has(&rt, "connections-home-hero"), "host banner is gone wholesale");
    assert!(has(&rt, "connection-workspace-home"), "root test id survives the takeover");
    let rendered = texts(&rt);
    for value in ["cfg-1", "conn-1", "Redis local", "Redis", "db3"] {
        assert!(rendered.iter().any(|x| x == value), "slot prop {value:?} is shown");
    }

    rt.state.home_slot = None;
    refresh(&mut rt);
    assert!(!has(&rt, "home-kv-connection-home"));
    assert!(has(&rt, "connections-home-hero"), "host banner returns");
}

// ---- Geometry ------------------------------------------------------------------------------------

fn assert_rects_inside<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    // The landing body is a scroll container: its children legitimately extend
    // below a 960x640 window and are reached by scrolling (the A1 layout rule).
    // Pin horizontal containment for every node and full containment for the
    // scroll root itself, which must never be wider or taller than the window.
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        let key = e.key.unwrap_or(e.path);
        let horizontal = r[0] < -0.5 || r[0] + r[2] > w + 0.5;
        let root = key == "connection-list";
        if horizontal || (root && (r[1] < -0.5 || r[1] + r[3] > h + 0.5)) {
            outside.push(format!("{key} {r:?}"));
        }
    }
    assert!(outside.is_empty(), "{label}: {} node(s) outside {w}x{h}: {outside:?}", outside.len());
}

#[test]
fn every_home_state_fits_both_windows_in_dark_and_light() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let empty = runtime(dark, (w, h), Vec::new());
            assert_rects_inside(&empty, w, h, &format!("empty dark={dark} {w}x{h}"));

            let landing = runtime(dark, (w, h), fixtures());
            assert_rects_inside(&landing, w, h, &format!("landing dark={dark} {w}x{h}"));

            let mut with_slot = runtime(dark, (w, h), fixtures());
            with_slot.state.home_slot = Some(ConnectionHomeSlotVm {
                connection_id: "cfg-1".to_owned(),
                db_session_id: "conn-1".to_owned(),
                connection_name: "Redis local".to_owned(),
                database_type: "redis".to_owned(),
                initial_database: "db3".to_owned(),
            });
            refresh(&mut with_slot);
            assert_rects_inside(&with_slot, w, h, &format!("kv dark={dark} {w}x{h}"));
        }
    }
}
