//! T144 independent audit: `ConnectionWorkspaceHome` port.
//!
//! Observes the rendered tree through `Runtime` (keys, text, rects) and real
//! pointer events. Covers branches the coder's suite does not: React testid
//! markers as click targets, engine/id filtering, pinned status-pill and chip
//! text, the copy marker before/at its deadline, and second-copy retargeting.

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

fn runtime(profiles: Vec<ConnectionProfile>, size: (f32, f32)) -> Runtime<AppState> {
    let mut state = AppState::default();
    state.desktop_connections = Some(profiles);
    let mut rt = Runtime::new(
        state,
        view_dark,
        size,
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
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

/// React's card testid and its Connect testid both fire `onSelectConnection`.
/// The port keeps those keys on zero-size markers inside the pressable row; a
/// pointer press at either marker must still reach the row and open the card.
#[test]
fn react_card_and_connect_markers_are_clickable() {
    let mut rt = runtime(fixtures(), (1440.0, 816.0));

    click(&mut rt, "home-conn-card-conn-1");
    assert_eq!(
        rt.state.recent_connection_ids.first().map(String::as_str),
        Some("conn-1"),
        "clicking the React card testid opens that connection"
    );

    click(&mut rt, "home-conn-connect-conn-2");
    assert_eq!(
        rt.state.recent_connection_ids.first().map(String::as_str),
        Some("conn-2"),
        "clicking the React Connect testid opens that connection"
    );
}

/// Filtering by engine id and by connection id (not just name) must match, as
/// the A1 card list filter does; a miss still shows the localized no-match key.
#[test]
fn filter_matches_engine_and_id_not_only_name() {
    let mut rt = runtime(fixtures(), (1440.0, 816.0));
    let (x, y) = rt.find("home-connections-filter").expect("filter field");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);

    rt.paste("postgresql");
    rt.frame(0.0);
    assert!(has(&rt, "home-conn-card-conn-1"), "engine id matches");
    assert!(!has(&rt, "home-conn-card-conn-2"));

    rt.state.connection_search.clear();
    rt.invalidate();
    rt.frame(0.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste("conn-3");
    rt.frame(0.0);
    assert!(has(&rt, "home-conn-card-conn-3"), "connection id matches");
    assert!(!has(&rt, "home-conn-card-conn-1"));
}

/// Status pill and engine chip carry the exact pinned translations per state.
#[test]
fn status_pill_and_engine_chip_pin_exact_text() {
    let mut rt = runtime(fixtures(), (1440.0, 816.0));
    let rendered = texts(&rt);
    assert!(
        rendered.iter().any(|x| x == crate::i18n::t("connWin.home.status.offline")),
        "offline pill label is exact"
    );
    assert!(
        rendered.iter().any(|x| x == "PostgreSQL"),
        "engine chip shows the engine label"
    );

    // An open connection switches the pill text to `conn.connected`.
    rt.state.desktop_connection = Some("conn-1".to_owned());
    rt.invalidate();
    rt.frame(1.0 / 60.0);
    assert!(has(&rt, "conn-status-conn-1"), "pill stays keyed when open");
    assert!(
        texts(&rt).iter().any(|x| x == crate::i18n::t("conn.connected")),
        "open pill uses the connected translation"
    );
}

/// The copy marker is request-bound: it holds before the deadline, a second
/// copy retargets it, and `app_poll` only clears at the recorded deadline.
#[test]
fn copy_marker_holds_before_deadline_and_retargets() {
    assert_eq!(HOME_COPY_FEEDBACK_MS, 2000);
    let mut rt = runtime(fixtures(), (1440.0, 816.0));

    click(&mut rt, "home-mcp-copy-button");
    let until = rt.state.home_copied_until.expect("deadline recorded");
    let wake = rt.wake_deadline().expect("real wake armed");
    assert!((wake - until).abs() <= 0.01, "wake pinned to the deadline");

    // A poll before the deadline leaves the marker and its label intact.
    assert!(!crate::app_poll(&mut rt.state, until - 0.5, &mut Tx::default()));
    refresh(&mut rt);
    assert_eq!(rt.state.home_copied_id.as_deref(), Some("__mcp__"));
    assert_eq!(
        element(&rt, "home-mcp-copy-button").label.as_deref(),
        Some(crate::i18n::t("connWin.home.aiIntegration.copied"))
    );

    // A second copy 1s later moves the recorded deadline and the wake by 1s.
    rt.frame(1.0);
    click(&mut rt, "home-mcp-copy-button");
    let until2 = rt.state.home_copied_until.expect("second deadline recorded");
    // click() steps a 1/60s frame after the press, so allow that slack.
    assert!((until2 - until - 1.0).abs() <= 0.05, "deadline advances with the clock");
    let wake2 = rt.wake_deadline().expect("second wake armed");
    assert!(wake2 <= until2 + 0.01, "a wake is pending no later than the latest deadline");

    // Polling at the earlier (superseded) deadline must NOT clear the marker.
    assert!(!crate::app_poll(&mut rt.state, until, &mut Tx::default()));
    assert_eq!(rt.state.home_copied_id.as_deref(), Some("__mcp__"));

    assert!(crate::app_poll(&mut rt.state, until2, &mut Tx::default()));
    refresh(&mut rt);
    assert!(rt.state.home_copied_until.is_none());
    assert!(rt.state.home_copied_id.is_none());
}

/// React state 3b's takeover carries the slot props; the root test id survives
/// while the host banner is removed wholesale, then restored on removal.
#[test]
fn kv_slot_props_and_root_testid_survive_takeover() {
    let mut rt = runtime(fixtures(), (960.0, 640.0));
    assert!(!has(&rt, "home-kv-connection-home"));
    rt.state.home_slot = Some(ConnectionHomeSlotVm {
        connection_id: "cfg-1".to_owned(),
        db_session_id: "conn-1".to_owned(),
        connection_name: "Redis local".to_owned(),
        database_type: "redis".to_owned(),
        initial_database: "db3".to_owned(),
    });
    refresh(&mut rt);
    assert!(has(&rt, "connection-workspace-home"), "root test id survives");
    assert_eq!(element(&rt, "home-kv-connection-home").label.as_deref(), Some("kv-connection-home"));
    let rendered = texts(&rt);
    for value in ["cfg-1", "conn-1", "Redis local", "Redis", "db3"] {
        assert!(rendered.iter().any(|x| x == value), "slot prop {value:?} is shown");
    }
    assert!(!has(&rt, "connections-home-hero"));
}

/// Rect-based containment, independent of the view's own reported values,
/// for all three home states at both required windows in dark mode.
#[test]
fn audit_rects_stay_inside_both_windows() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let empty = runtime(Vec::new(), (w, h));
        check_rects(&empty, w, h, "empty");

        let landing = runtime(fixtures(), (w, h));
        check_rects(&landing, w, h, "landing");

        let mut kv = runtime(fixtures(), (w, h));
        kv.state.home_slot = Some(ConnectionHomeSlotVm {
            connection_id: "cfg-1".to_owned(),
            db_session_id: "conn-1".to_owned(),
            connection_name: "Redis local".to_owned(),
            database_type: "redis".to_owned(),
            initial_database: "db3".to_owned(),
        });
        refresh(&mut kv);
        check_rects(&kv, w, h, "kv");
    }
}

fn check_rects<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
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
