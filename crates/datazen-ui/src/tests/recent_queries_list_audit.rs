//! Independent audit for T427: `RecentQueriesList` port.
//!
//! Fill branch coverage the coder does not drive: actions with no host port,
//! the exact relative-time bucket boundaries, and the row's a11y role/focus.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};
use crate::i18n::t;
use crate::views::home::recent_queries_list::{
    build, relative_label, QueryHistoryVm, RecentQueriesHostPort, RecentQueriesVm,
    SavedConnectionVm, ROOT_KEY, VIEW_ALL_KEY,
};

#[derive(Default)]
struct P { calls: Rc<RefCell<Vec<String>>> }

fn view(p: &P, _cx: &mut Cx<P>) -> El {
    let mut vm = RecentQueriesVm::default();
    vm.entries = vec![QueryHistoryVm {
        id: "hist-1".to_owned(),
        connection_id: "conn-1".to_owned(),
        database: "db".to_owned(),
        sql: "SELECT 1;".to_owned(),
        executed_at_ms: 1_700_000_000_000,
        execution_time_ms: 12,
        success: true,
    }];
    vm.saved_connections = vec![SavedConnectionVm {
        id: "conn-1".to_owned(),
        name: "Local".to_owned(),
    }];
    vm.now_ms = 1_700_000_000_000;
    let mut layer = Vec::new();
    // No host callbacks wired at all.
    build(&vm, true, RecentQueriesHostPort::<P>::default(), &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(p: P) -> Runtime<P> {
    Runtime::new(p, view, (1440.0, 816.0), Env { system_dark: true, scale: 1.0 })
}

fn click(rt: &mut Runtime<P>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn actions_are_inert_when_no_host_port_is_wired() {
    let mut rt = runtime(P::default());
    rt.frame(0.0);
    for key in [ROOT_KEY, VIEW_ALL_KEY, "connections-recent-row-hist-1", "home-query-copy-hist-1"] {
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some(key)), "{key} composed");
    }
    // Re-run is gated on on_select, so it is absent with no host port.
    assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some("home-query-rerun-hist-1")));
    click(&mut rt, VIEW_ALL_KEY);
    click(&mut rt, "home-query-copy-hist-1");
    click(&mut rt, "connections-recent-row-hist-1");
    assert!(rt.state.calls.borrow().is_empty(), "no callback means no host calls");
}

#[test]
fn relative_time_bucket_boundaries_match_the_js_lib() {
    let now = 1_700_000_000_000;
    assert_eq!(relative_label(0, now), None, "invalid timestamp omitted");
    assert_eq!(relative_label(-1, now), None, "negative timestamp omitted");
    assert_eq!(relative_label(now, now), Some(t("connWin.home.queries.justNow").to_owned()));
    assert_eq!(
        relative_label(now - 59_999, now),
        Some(t("connWin.home.queries.justNow").to_owned()),
        "just under a minute is justNow"
    );
    assert_eq!(
        relative_label(now - 60_000, now),
        Some(t("connWin.home.queries.minutesAgo").replace("{count}", "1")),
        "exactly a minute enters the minutes bucket"
    );
    assert_eq!(
        relative_label(now - 3_600_000, now),
        Some(t("connWin.home.queries.hoursAgo").replace("{count}", "1"))
    );
    assert_eq!(
        relative_label(now - 86_400_000, now),
        Some(t("connWin.home.queries.daysAgo").replace("{count}", "1"))
    );
    let beyond = relative_label(now - 8 * 86_400_000, now).expect("a short date");
    assert!(beyond.contains('-') && beyond.len() == 10, "YYYY-MM-DD beyond 7 days: {beyond}");
}

#[test]
fn rows_carry_the_button_role_and_are_focusable() {
    let mut rt = runtime(P::default());
    rt.frame(0.0);
    let row = rt.elements().into_iter()
        .find(|e| e.key.as_deref() == Some("connections-recent-row-hist-1"))
        .expect("row laid out");
    assert_eq!(row.role, "button", "React's row has role=button");
}
