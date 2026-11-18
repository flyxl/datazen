//! `CopyButton` base and the history filter bar metrics it shares a surface with.
//!
//! These are the coder-owned tests for the T029 fix round: the copy control must own
//! the 1.5s feedback contract (copy + wake + caller hook), keep working without a
//! caller hook, and the filter bar must use the React widths.

use rune::{col, Cx, El, Env, Runtime, TextEdit, Tx};

use crate::components::history::{
    HistoryConnectionOption, HistoryFilterBar, HistoryFilterOpen, HistoryQueryState,
};
use crate::i18n::t;
use crate::ui::copyable::{CopyButton, BUTTON_SM};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[derive(Default)]
struct Probe {
    copies: usize,
    press_clock: Option<f64>,
    copied_until: Option<f64>,
    now: f64,
}

fn feedback_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let copied = state.copied_until.map_or(false, |u| state.now < u);
    let copy = CopyButton::new("cb", "exact text")
        .copied(copied)
        .label(t("query.historyCopy"))
        // No `.help()` here: a tooltip keeps its own presentation animation alive, and the
        // pending animation wake would mask the 1500ms feedback wake under test.
        .on_copy_tx(|s: &mut Probe, tx: &mut Tx| {
            s.copies += 1;
            s.press_clock = Some(tx.clock());
            s.now = tx.clock();
            s.copied_until = Some(tx.clock() + 1.5);
        })
        .build::<Probe>(true);
    let sync = rune::row()
        .key("sync")
        .role("button")
        .label("sync")
        .w(1.0)
        .h(1.0)
        .on_press(|_s: &mut Probe| {});
    col().w(480.0).h(120.0).child(sync).child(copy)
}

fn no_callback_view(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let copy = CopyButton::new("cb", "exact text").label(t("query.historyCopy")).build::<Probe>(true);
    col().w(480.0).h(120.0).child(copy)
}

#[test]
fn copied_state_switches_label_for_the_1_5s_feedback() {
    let mut rt = Runtime::new(Probe::default(), feedback_view, (480.0, 240.0), env());
    rt.frame(0.0);
    let trigger = element(&rt, "cb");
    assert_eq!(trigger.role, "button", "copy control is a real button");
    assert!(
        (trigger.rect[3] - BUTTON_SM).abs() <= 0.5,
        "copy control uses the small button height {BUTTON_SM}: rect={:?}",
        trigger.rect
    );
    assert_eq!(trigger.label.as_deref(), Some(t("query.historyCopy")));

    click(&mut rt, "cb");
    assert_eq!(rt.state.copies, 1, "copy press runs the caller hook");
    let press_clock = rt.state.press_clock.expect("hook observed tx.clock()");
    let deadline = rt.wake_deadline().expect("copy press must schedule the feedback wake");
    assert!(
        (deadline - press_clock - 1.5).abs() <= 0.01,
        "feedback wake must be ~1500ms after press: press={press_clock} deadline={deadline}"
    );
    assert_eq!(
        element(&rt, "cb").label.as_deref(),
        Some(t("common.copied")),
        "copied state must show the copied label"
    );

    // Before the deadline the feedback stays; at the deadline it reverts. The kit button's
    // icon flips Copy -> Check on the same `copied` flag, but icon identity is not part of
    // ElementInfo, so the label is the observable assertion here.
    let until = rt.state.copied_until.expect("copied_until recorded");
    rt.state.now = until - 0.001;
    click(&mut rt, "sync");
    assert_eq!(element(&rt, "cb").label.as_deref(), Some(t("common.copied")));
    rt.state.now = until;
    click(&mut rt, "sync");
    assert_eq!(
        element(&rt, "cb").label.as_deref(),
        Some(t("query.historyCopy")),
        "at the 1.5s deadline the label reverts"
    );
}

#[test]
fn copy_without_a_caller_hook_still_copies_and_schedules_the_wake() {
    let clipboard = rune::clipboard::memory();
    let mut rt = Runtime::new(Probe::default(), no_callback_view, (480.0, 240.0), env());
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);
    let before = rt.clock.max(0.0);
    click(&mut rt, "cb");
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("exact text"),
        "press without a hook must still copy the exact text"
    );
    let deadline = rt.wake_deadline().expect("press without a hook must still schedule the wake");
    let delta = deadline - before;
    assert!(
        delta > 1.4 && delta < 1.7,
        "wake without a hook must be ~1.5s out: before={before} deadline={deadline}"
    );
}

#[derive(Clone)]
struct FilterState {
    query: HistoryQueryState,
    search: TextEdit,
    open: HistoryFilterOpen,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            query: HistoryQueryState {
                connection_id: "c1".to_string(),
                ..HistoryQueryState::default()
            },
            search: TextEdit::new(String::new()),
            open: HistoryFilterOpen::None,
        }
    }
}

fn filter_bar(state: &FilterState, width: f32) -> El {
    let mut layer = Vec::new();
    let bar = HistoryFilterBar::<FilterState>::new(state.query.clone(), &state.search)
        .connections(vec![HistoryConnectionOption {
            id: "c1".to_string(),
            name: "Conn A".to_string(),
        }])
        .databases(vec!["db1".to_string()])
        .schemas(vec!["public".to_string()])
        .open(state.open)
        .on_search_edit(|s: &mut FilterState| &mut s.search)
        .on_toggle_open(|s: &mut FilterState, which| s.open = which)
        .on_close(|s: &mut FilterState| s.open = HistoryFilterOpen::None)
        .build(true, &mut layer);
    let mut root = col().w(width).h(600.0).child(bar);
    for l in layer {
        root = root.child(l);
    }
    root
}

fn filter_view(state: &FilterState, _cx: &mut Cx<FilterState>) -> El {
    filter_bar(state, 960.0)
}

fn narrow_filter_view(state: &FilterState, _cx: &mut Cx<FilterState>) -> El {
    filter_bar(state, 120.0)
}

#[test]
fn filter_bar_metrics_match_the_react_source() {
    let mut rt = Runtime::new(FilterState::default(), filter_view, (960.0, 640.0), env());
    rt.frame(0.0);

    let search = element(&rt, "history-filter-bar-search");
    let icon = element(&rt, "history-filter-bar-search-icon");
    let inset = icon.rect[0] - search.rect[0];
    assert!(
        (inset - 10.0).abs() <= 0.5,
        "search icon uses left-2.5 (10pt) inset: icon={:?} search={:?}",
        icon.rect,
        search.rect
    );
    assert!(
        icon.rect[1] >= search.rect[1] - 0.5
            && icon.rect[1] + icon.rect[3] <= search.rect[1] + search.rect[3] + 0.5,
        "search icon stays vertically centered inside the field: icon={:?} search={:?}",
        icon.rect,
        search.rect
    );

    for (key, expected) in [
        ("history-filter-bar-connection-trigger", 128.0),
        ("history-filter-bar-database-trigger", 112.0),
        ("history-filter-bar-schema-trigger", 112.0),
        ("history-filter-bar-status-trigger", 112.0),
        ("history-filter-bar-range-trigger", 128.0),
        ("history-filter-bar-sort-trigger", 128.0),
    ] {
        let rect = element(&rt, key).rect;
        assert!(
            (rect[2] - expected).abs() <= 0.5,
            "{key} must use the React width {expected}: rect={rect:?}"
        );
    }
}

#[test]
fn filter_bar_search_keeps_the_react_min_width() {
    let mut rt = Runtime::new(FilterState::default(), narrow_filter_view, (160.0, 640.0), env());
    rt.frame(0.0);
    let search = element(&rt, "history-filter-bar-search");
    assert!(
        (search.rect[2] - 180.0).abs() <= 0.5,
        "search must keep min-w-[180px] even in a 120pt column: rect={:?}",
        search.rect
    );
}
