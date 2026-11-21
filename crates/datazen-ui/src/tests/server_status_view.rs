//! T528: `ServerStatusView` port — the React test cases as view/state tests:
//! data-driven cards/charts, the three tabs, header actions, loading/error/empty
//! states and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::server_status_view::{
    build, chart_cards, detail_rows, format_byte_rate, format_bytes, format_clock, format_rate,
    format_uptime, label_for_key, metric_cards, number_from, ServerStatusAction,
    ServerStatusVm, StatusValue, StatusVariableVm, TrendPointVm, ViewTab, EMPTY_KEY, ERROR_KEY,
    LAST_UPDATED_KEY, LOADING_KEY, REFRESH_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: ServerStatusVm,
    actions: Rc<RefCell<Vec<ServerStatusAction>>>,
    dark: bool,
}

fn status_value_str(value: &str) -> StatusValue {
    StatusValue::Str(value.to_owned())
}

fn status(pairs: &[(&str, StatusValue)]) -> BTreeMap<String, StatusValue> {
    pairs.iter().map(|(key, value)| ((*key).to_owned(), value.clone())).collect()
}

fn payload_status() -> BTreeMap<String, StatusValue> {
    status(&[
        ("version", status_value_str("9.7.1")),
        ("database", status_value_str("db")),
        ("uptimeSeconds", StatusValue::Num(100.0)),
        ("connections", StatusValue::Num(3.0)),
        ("maxConnections", StatusValue::Num(151.0)),
        ("activeQueries", StatusValue::Num(2.0)),
        ("qps", status_value_str("0.34")),
    ])
}

fn series(base: i64, values: &[f64]) -> Vec<TrendPointVm> {
    values.iter().enumerate().map(|(i, v)| TrendPointVm { t: base + i as i64 * 5000, v: *v }).collect()
}

fn ready() -> ServerStatusVm {
    ServerStatusVm {
        db_session_id: "conn-mysql".to_owned(),
        connection_name: "mysql".to_owned(),
        has_initial_status: true,
        status: Some(payload_status()),
        variables: vec![StatusVariableVm { name: "X".to_owned(), value: Some("y".to_owned()) }],
        auto_refresh: 5000,
        last_updated_at: Some(1_700_000_000_000),
        ..ServerStatusVm::default()
    }
}

fn scene(vm: ServerStatusVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action: ServerStatusAction| {
        match &action {
            ServerStatusAction::SetAutoRefresh(value) => {
                state.vm.auto_refresh = *value;
                state.vm.auto_refresh_open = false;
            }
            ServerStatusAction::ToggleAutoRefreshOptions => {
                state.vm.auto_refresh_open = !state.vm.auto_refresh_open;
            }
            ServerStatusAction::SelectTab(tab) => state.vm.view_tab = *tab,
            _ => {}
        }
        actions.borrow_mut().push(action);
    });
    col().w_full().h_full().child(build(&s.vm, Some(callback), s.dark))
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn texts(rt: &Runtime<Scene>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside(rt: &Runtime<Scene>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: outside {outside:?}");
}

// ── pure helpers ──────────────────────────────────────────────────────────────

#[test]
fn formatting_helpers_match_react() {
    assert_eq!(format_uptime(100.0), t("serverStatus.uptimeFormat")
        .replace("{hours}", "0").replace("{minutes}", "1").replace("{seconds}", "40"));
    let expected_days = t("serverStatus.uptimeFormatDays")
        .replace("{days}", "1").replace("{hours}", "1").replace("{minutes}", "1");
    assert_eq!(format_uptime(90061.0), expected_days, "days form when days > 0");
    assert_eq!(format_bytes(512.0), "512.0 B");
    assert_eq!(format_bytes(2048.0), "2.0 KB");
    assert_eq!(format_byte_rate(2048.0), "2.0 KB/s");
    assert_eq!(format_rate(0.34), "0.34");
    assert_eq!(format_rate(12.3), "12.3");
    assert_eq!(format_rate(1500.0), "1.5K");
    assert_eq!(format_clock(1_700_000_000_000), "22:13:20");
    assert_eq!(number_from(&status_value_str("0.34")), Some(0.34));
    assert_eq!(number_from(&status_value_str("")), None);
    assert_eq!(number_from(&StatusValue::Bool(true)), None);
    assert_eq!(label_for_key("qps"), t("common.queriesPerSec"));
    assert_eq!(label_for_key("custom"), "custom");
}

// ── data-driven cards / charts / details ─────────────────────────────────────

#[test]
fn stat_cards_render_only_available_fields() {
    let vm = ready();
    let cards = metric_cards(&vm);
    let keys: Vec<&str> = cards.iter().map(|card| card.key).collect();
    assert!(keys.contains(&"connections"), "connections field present");
    assert!(!keys.contains(&"deadlocks"), "absent field hidden");
    assert!(!keys.contains(&"cacheHit"), "absent field hidden");
    assert_eq!(label_for_key("connections"), t("serverStatus.connections"));
}

#[test]
fn cache_hit_card_appears_when_present() {
    let mut vm = ready();
    vm.status.as_mut().unwrap().insert("cacheHitRatio".to_owned(), status_value_str("99.86%"));
    let keys: Vec<&str> = metric_cards(&vm).iter().map(|card| card.key).collect();
    assert!(keys.contains(&"cacheHit"));
}

#[test]
fn chart_cards_render_from_backing_series_only() {
    let mut vm = ready();
    vm.history.insert("qps".to_owned(), series(1000, &[1.0, 2.0, 3.0]));
    vm.history.insert("netIn".to_owned(), series(1000, &[10.0, 20.0]));
    vm.history.insert("netOut".to_owned(), series(1000, &[5.0, 6.0]));
    let keys: Vec<&str> = chart_cards(&vm).iter().map(|card| card.key).collect();
    assert!(keys.contains(&"transactions"));
    assert!(keys.contains(&"netTraffic"));
    assert!(!keys.contains(&"commands"), "no cmd_* series");
    let rt = runtime(scene(vm, true), DESIGN);
    assert!(has_key(&rt, "server-status-chart-transactions"));
    assert!(has_key(&rt, "server-status-chart-netTraffic"));
    assert!(!has_key(&rt, "server-status-chart-commands"));
}

#[test]
fn details_order_and_internal_keys_are_hidden() {
    let mut vm = ready();
    vm.status.as_mut().unwrap().insert("sessionTotal".to_owned(), StatusValue::Num(9.0));
    vm.status.as_mut().unwrap().insert("customField".to_owned(), status_value_str("x"));
    let rows = detail_rows(&vm);
    let keys: Vec<&str> = rows.iter().map(|(label, _)| label.as_str()).collect();
    assert!(!rows.iter().any(|(label, _)| label == "sessionTotal"), "internal key hidden");
    assert!(keys.contains(&"customField"), "extra field shown with raw key");
    let labels: Vec<&str> = rows.iter().map(|(label, _)| label.as_str()).collect();
    let version = labels.iter().position(|label| *label == t("serverStatus.version")).unwrap();
    let connections = labels.iter().position(|label| *label == t("serverStatus.connections")).unwrap();
    assert!(version < connections, "React's preferred order is kept");
}

// ── keys, states and host actions ────────────────────────────────────────────

#[test]
fn header_and_tab_keys_are_exact() {
    let rt = runtime(scene(ready(), true), DESIGN);
    for key in [
        "server-status-view",
        "server-status-header",
        LAST_UPDATED_KEY,
        REFRESH_KEY,
        "server-view-tablist",
        "server-view-tab-dashboard",
        "server-view-tab-variables",
        "server-view-tab-details",
        "server-status-cards",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|text| text == t("serverStatus.dashboardTitle")));
    assert!(rendered.iter().any(|text| text.as_str() == format_clock(1_700_000_000_000)));
}

#[test]
fn refresh_and_auto_refresh_route_through_the_host() {
    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, REFRESH_KEY);
    assert_eq!(*rt.state.actions.borrow(),
        [ServerStatusAction::Refresh { db_session_id: "conn-mysql".to_owned() }]);

    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, "server-status-auto-refresh-trigger");
    assert_eq!(*rt.state.actions.borrow(), [ServerStatusAction::ToggleAutoRefreshOptions]);
    assert!(has_key(&rt, "server-status-auto-refresh-menu"));
    click(&mut rt, "server-status-auto-refresh-10000");
    assert_eq!(rt.state.actions.borrow().last(), Some(&ServerStatusAction::SetAutoRefresh(10000)));
    assert_eq!(rt.state.vm.auto_refresh, 10000);
}

#[test]
fn busy_button_and_manual_load_are_distinct_intents() {
    let mut busy = ready();
    busy.button_loading = true;
    let mut rt = runtime(scene(busy, true), DESIGN);
    click(&mut rt, REFRESH_KEY);
    assert!(rt.state.actions.borrow().is_empty(), "a disabled refresh emits nothing");

    let mut vm = ServerStatusVm::default();
    vm.view_tab = ViewTab::Details;
    let mut rt = runtime(scene(vm, true), DESIGN);
    // No status: the empty guard body has no header, so the action is not
    // reachable — the host owns the initial load from the session id.
    assert!(has_key(&rt, EMPTY_KEY));
}

#[test]
fn tab_switch_and_tab_bodies() {
    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, "server-view-tab-variables");
    assert_eq!(*rt.state.actions.borrow(), [ServerStatusAction::SelectTab(ViewTab::Variables)]);
    assert!(has_key(&rt, "server-status-variable-X"), "variables body mounted after switch");

    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, "server-view-tab-details");
    assert_eq!(*rt.state.actions.borrow(), [ServerStatusAction::SelectTab(ViewTab::Details)]);
    assert!(has_key(&rt, "server-status-details-table"));
}

#[test]
fn variables_empty_and_loading_error_states() {
    let mut empty_vars = ready();
    empty_vars.view_tab = ViewTab::Variables;
    empty_vars.variables.clear();
    let rt = runtime(scene(empty_vars, true), DESIGN);
    assert!(has_key(&rt, "server-status-empty-variables"));

    let mut loading = ServerStatusVm::default();
    loading.loading = true;
    let rt = runtime(scene(loading, true), DESIGN);
    assert!(has_key(&rt, LOADING_KEY));
    assert!(!has_key(&rt, REFRESH_KEY), "loading body has no header");

    let mut errored = ServerStatusVm::default();
    errored.error = "failed".to_owned();
    let rt = runtime(scene(errored, false), DESIGN);
    assert!(has_key(&rt, ERROR_KEY));
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut vm = ready();
            vm.history.insert("qps".to_owned(), series(1000, &[1.0, 2.0, 3.0]));
            vm.history.insert("netIn".to_owned(), series(1000, &[10.0, 20.0]));
            vm.history.insert("netOut".to_owned(), series(1000, &[5.0, 6.0]));
            vm.auto_refresh_open = true;
            for tab in [ViewTab::Dashboard, ViewTab::Variables, ViewTab::Details] {
                vm.view_tab = tab;
                let rt = runtime(scene(vm.clone(), dark), size);
                assert_inside(&rt, size, &format!("{tab:?} {size:?} dark={dark}"));
            }
        }
    }
}
