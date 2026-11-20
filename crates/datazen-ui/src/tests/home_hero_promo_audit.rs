//! T418 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 5 tests do not pin: a host-supplied
//! CLI command being copied, re-copy re-timing the deadline, the bar-keyed
//! copied marker, badge-cap boundaries, an exact 2000ms wake pin, and a
//! non-vacuous `visible`-rect geometry channel.

use std::cell::RefCell;

use rune::{clipboard, col, tok, Cx, El, Env, Runtime, Tx};

use crate::i18n::t;
use crate::state_def::AppState;
use crate::views::home::home_hero::{build_home_hero, HomeHeroVm};
use crate::views::home::mcp_promo_bar::{
    build_mcp_promo_bar, mcp_copied, McpPromoVm, MCP_COPY_ROW_ID,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn promo_view(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);
    col()
        .w_full()
        .min_h(0.0)
        .child(build_mcp_promo_bar(
            &McpPromoVm { cli_command: "/opt/app --mcp".to_owned() },
            s,
            theme,
            dark,
        ))
}

thread_local! {
    static HERO_TYPES: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

fn hero_view(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = cx.system_dark();
    let types = HERO_TYPES.with(|f| f.borrow().clone());
    col()
        .w_full()
        .min_h(0.0)
        .child(build_mcp_promo_bar(&McpPromoVm::with_default_command(), s, tok::theme(dark), dark))
        .child(build_home_hero(
            &HomeHeroVm { connection_count: 5, group_count: 2, db_types: types },
            tok::theme(dark),
            dark,
        ))
}

fn hero_runtime(types: Vec<String>, size: (f32, f32), dark: bool) -> Runtime<AppState> {
    HERO_TYPES.with(|f| *f.borrow_mut() = types);
    let mut rt = Runtime::new(AppState::default(), hero_view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn copy_writes_the_host_supplied_command() {
    let clip = clipboard::memory();
    let mut rt = Runtime::new(AppState::default(), promo_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.use_clipboard(&clip);
    rt.frame(0.0);
    click(&mut rt, "home-mcp-copy-button");
    assert_eq!(clip.text().as_deref().map(|s| s.trim()), Some("/opt/app --mcp"));
}

#[test]
fn a_second_copy_retimes_the_feedback_deadline() {
    let mut rt = Runtime::new(AppState::default(), promo_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    click(&mut rt, "home-mcp-copy-button");
    let first = rt.state.home_copied_until.expect("first deadline");
    rt.frame(0.8);
    click(&mut rt, "home-mcp-copy-button");
    let second = rt.state.home_copied_until.expect("second deadline");
    assert!(second > first, "a repeat copy extends the deadline: {first} -> {second}");
    assert!((second - first - 0.8).abs() <= 0.02, "the extension tracks the second press clock");
}

#[test]
fn the_copied_marker_is_keyed_to_the_promo_bar() {
    let mut state = AppState::default();
    state.home_copied_until = Some(100.0);
    state.home_copied_id = Some("some-other-row".to_owned());
    assert!(!mcp_copied(&state), "a marker for another row must not flip this bar");
    state.home_copied_id = Some(MCP_COPY_ROW_ID.to_owned());
    assert!(mcp_copied(&state));
    let mut tx = Tx::default();
    crate::app::app_poll(&mut state, 100.0, &mut tx);
    assert!(!mcp_copied(&state), "the marker reverts at its deadline");
}

#[test]
fn badge_cluster_caps_at_four_but_keeps_fewer() {
    for (count, expected) in [(3usize, 3usize), (4, 4), (5, 4)] {
        let types: Vec<String> = (0..count).map(|i| format!("engine{i}")).collect();
        let keys = types.clone();
        let rt = hero_runtime(types, DESIGN, true);
        let shown = keys
            .iter()
            .filter(|e| rt.elements().iter().any(|el| el.key.as_deref() == Some(&format!("home-hero-badge-{e}"))))
            .count();
        assert_eq!(shown, expected, "count {count} shows {expected} badges");
    }
}

#[test]
fn feedback_wake_is_exactly_2000ms() {
    let mut rt = Runtime::new(AppState::default(), promo_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    let before = rt.clock.max(0.0);
    click(&mut rt, "home-mcp-copy-button");
    let until = rt.state.home_copied_until.expect("deadline");
    let wake = rt.wake_deadline().expect("real wake");
    assert!((wake - until).abs() <= 1e-6, "wake must equal the recorded deadline");
    // The press handler runs after click()'s 1/60s frame advance.
    assert!(
        (wake - before - 2.0 - 1.0 / 60.0).abs() <= 0.01,
        "feedback must be exactly 2000ms after the press: wake={wake} before={before}"
    );
}

#[test]
fn hero_and_promo_stay_inside_via_visible_rects() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = hero_runtime(vec!["postgres".to_owned(), "mysql".to_owned()], size, dark);
            assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("connections-home-hero")));
            for e in rt.elements() {
                let [x, y, w, h] = e.visible;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                    "{size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                    e.key.as_deref().unwrap_or(&e.path), e.visible, e.rect
                );
            }
        }
    }
}

#[test]
fn exact_locale_keys_are_used_for_copy_and_copied() {
    let mut rt = Runtime::new(AppState::default(), promo_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    let idle = rt.elements().iter().find(|e| e.key.as_deref() == Some("home-mcp-copy-button"))
        .and_then(|e| e.label.clone()).unwrap_or_default();
    assert_eq!(idle, t("connWin.home.aiIntegration.copy"));
    click(&mut rt, "home-mcp-copy-button");
    let copied = rt.elements().iter().find(|e| e.key.as_deref() == Some("home-mcp-copy-button"))
        .and_then(|e| e.label.clone()).unwrap_or_default();
    assert_eq!(copied, t("connWin.home.aiIntegration.copied"));
}
