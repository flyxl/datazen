//! T0-10: Toast / Tooltip / HoverCard base over `rune-kit`.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime};
use rune_kit::Placement;

use crate::ui::{toast, tooltip};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn focused<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    click_at(rt, x, y);
}

fn click_at<S>(rt: &mut Runtime<S>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn assert_inside<S>(rt: &Runtime<S>, width: f32, height: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let (x, y, w, h) = (e.rect[0], e.rect[1], e.rect[2], e.rect[3]);
        if x < -0.5 || y < -0.5 || x + w > width + 0.5 || y + h > height + 0.5 {
            outside.push(format!(
                "{} {:?} role={}",
                e.key.unwrap_or_else(|| e.path.clone()),
                e.rect,
                e.role
            ));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) cross the window edge: {outside:?}",
        outside.len()
    );
}

// ---- Tooltip ------------------------------------------------------------------------------

#[derive(Default)]
struct TipState {
    open: bool,
}

fn tip_center(state: &TipState, _cx: &mut Cx<TipState>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let trigger = rune::row()
        .key("tip-anchor")
        .w(80.0)
        .h(28.0)
        .center()
        .child(rune::text("Save").key("tip-anchor-text"));
    let tip = tooltip::Tooltip::new("probe-tip")
        .trigger(trigger)
        .text("Save changes")
        .open(state.open)
        .build(k, &mut layer);
    rune_kit::root(
        rune::col().w_full().h_full().center().child(tip),
        layer,
    )
}

#[test]
fn tooltip_mounts_only_when_open() {
    let mut closed = Runtime::new(TipState { open: false }, tip_center, (960.0, 640.0), env());
    closed.frame(0.0);
    assert!(
        closed.find("probe-tip-trigger").is_some(),
        "trigger is always laid out"
    );
    assert!(
        closed.find("probe-tip-tooltip").is_none(),
        "closed tooltip does not mount its layer content"
    );

    let mut open = Runtime::new(TipState { open: true }, tip_center, (960.0, 640.0), env());
    open.frame(0.0);
    assert!(
        open.find("probe-tip-tooltip").is_some(),
        "open tooltip mounts its layer content"
    );
}

#[derive(Default, Clone, Copy)]
struct EdgeTipState {
    open: bool,
    placement: u8,
    edge: u8,
}

fn placement_from_u8(v: u8) -> Placement {
    match v {
        1 => Placement::Top,
        2 => Placement::Right,
        3 => Placement::Left,
        _ => Placement::Bottom,
    }
}

fn edge_tip_view(state: &EdgeTipState, _cx: &mut Cx<EdgeTipState>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let trigger = rune::row()
        .key("edge-anchor")
        .w(72.0)
        .h(24.0)
        .center()
        .child(rune::text("T").key("edge-anchor-text"));
    let tip = tooltip::Tooltip::new("edge-tip")
        .trigger(trigger)
        .text("A helpful tooltip near the edge")
        .placement(placement_from_u8(state.placement))
        .open(state.open)
        .build(k, &mut layer);
    let page = match state.edge {
        1 => rune::col()
            .w_full()
            .h_full()
            .justify_end()
            .child(rune::row().w_full().justify_end().child(tip)),
        2 => rune::col()
            .w_full()
            .h_full()
            .child(rune::row().w_full().justify_end().child(tip)),
        3 => rune::col()
            .w_full()
            .h_full()
            .justify_end()
            .child(rune::row().w_full().child(tip)),
        _ => rune::col().w_full().h_full().child(tip),
    };
    rune_kit::root(page, layer)
}

#[derive(Default)]
struct TipTopState {
    open: bool,
}

fn tip_top_view(state: &TipTopState, _cx: &mut Cx<TipTopState>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let trigger = rune::row()
        .key("p-anchor")
        .w(80.0)
        .h(28.0)
        .center()
        .child(rune::text("P").key("p-t"));
    let tip = tooltip::Tooltip::new("p-tip")
        .trigger(trigger)
        .text("Above")
        .placement(Placement::Top)
        .open(state.open)
        .build(k, &mut layer);
    rune_kit::root(
        rune::col().w_full().h_full().center().child(tip),
        layer,
    )
}

#[test]
fn tooltip_stays_inside_the_window_near_edges_and_honours_placement() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for (placement, edge, label) in [
            (0u8, 1u8, "bottom-at-br"),
            (2u8, 1u8, "right-at-br"),
            (1u8, 2u8, "top-at-tr"),
            (2u8, 2u8, "right-at-tr"),
            (0u8, 3u8, "bottom-at-bl"),
            (3u8, 3u8, "left-at-bl"),
        ] {
            let mut rt = Runtime::new(
                EdgeTipState {
                    open: true,
                    placement,
                    edge,
                },
                edge_tip_view,
                (width, height),
                env(),
            );
            rt.frame(0.0);
            assert!(
                rt.find("edge-tip-tooltip").is_some(),
                "{width}x{height} {label}: tooltip mounted"
            );
            assert_inside(&rt, width, height, &format!("{width}x{height} {label}"));
        }

        let mut above = Runtime::new(
            TipTopState { open: true },
            tip_top_view,
            (width, height),
            env(),
        );
        above.frame(0.0);
        let t = element(&above, "p-anchor").rect;
        let b = element(&above, "p-tip-tooltip").rect;
        assert!(
            b[1] + b[3] <= t[1] + 1.0,
            "{width}x{height}: Placement::Top puts tooltip above trigger: trigger={t:?} tip={b:?}"
        );
    }
}

// ---- HoverCard ----------------------------------------------------------------------------

#[derive(Default)]
struct CardState {
    open: bool,
}

fn card_center(state: &CardState, _cx: &mut Cx<CardState>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let trigger = rune::row()
        .key("card-anchor")
        .w(80.0)
        .h(28.0)
        .center()
        .child(rune::text("User").key("card-anchor-text"));
    let card = tooltip::HoverCard::new("probe-card")
        .trigger(trigger)
        .child(rune::text("Jane Doe").key("card-name"))
        .child(rune::text("Maintainer").key("card-role"))
        .open(state.open)
        .build(k, &mut layer);
    rune_kit::root(
        rune::col().w_full().h_full().center().child(card),
        layer,
    )
}

#[test]
fn hover_card_mounts_only_when_open() {
    let mut closed = Runtime::new(CardState { open: false }, card_center, (960.0, 640.0), env());
    closed.frame(0.0);
    assert!(closed.find("probe-card-card").is_none());
    let mut open = Runtime::new(CardState { open: true }, card_center, (960.0, 640.0), env());
    open.frame(0.0);
    assert!(open.find("probe-card-card").is_some());
    assert!(open.find("card-name").is_some());
}

#[derive(Default, Clone, Copy)]
struct EdgeCardState {
    open: bool,
    placement: u8,
    edge: u8,
}

fn edge_card_view(state: &EdgeCardState, _cx: &mut Cx<EdgeCardState>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let trigger = rune::row()
        .key("hc-anchor")
        .w(72.0)
        .h(24.0)
        .center()
        .child(rune::text("H").key("hc-t"));
    let card = tooltip::HoverCard::new("hc")
        .trigger(trigger)
        .child(rune::text("Preview content").key("hc-body"))
        .placement(placement_from_u8(state.placement))
        .open(state.open)
        .build(k, &mut layer);
    let page = match state.edge {
        1 => rune::col()
            .w_full()
            .h_full()
            .justify_end()
            .child(rune::row().w_full().justify_end().child(card)),
        2 => rune::col()
            .w_full()
            .h_full()
            .child(rune::row().w_full().justify_end().child(card)),
        3 => rune::col()
            .w_full()
            .h_full()
            .justify_end()
            .child(rune::row().w_full().child(card)),
        _ => rune::col().w_full().h_full().child(card),
    };
    rune_kit::root(page, layer)
}

#[test]
fn hover_card_stays_inside_the_window_near_edges() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        // Choose placements that open away from the nearest edge so the card stays inside.
        // edge: 1=bottom-right, 2=top-right, 3=bottom-left.
        for (placement, edge, label) in [
            (1u8, 1u8, "top-at-br"),
            (3u8, 1u8, "left-at-br"),
            (0u8, 2u8, "bottom-at-tr"),
            (3u8, 2u8, "left-at-tr"),
            (1u8, 3u8, "top-at-bl"),
            (2u8, 3u8, "right-at-bl"),
        ] {
            let mut rt = Runtime::new(
                EdgeCardState {
                    open: true,
                    placement,
                    edge,
                },
                edge_card_view,
                (width, height),
                env(),
            );
            rt.frame(0.0);
            assert!(rt.find("hc-card").is_some(), "{width}x{height} {label}");
            assert_inside(&rt, width, height, &format!("hover-card {width}x{height} {label}"));
        }
    }
}

// ---- Toast --------------------------------------------------------------------------------

#[derive(Default)]
struct ToastState {
    closed: usize,
    actioned: usize,
}

fn toast_view(state: &ToastState, _cx: &mut Cx<ToastState>) -> El {
    let k = crate::ui::kit::kit(true);
    let long_title = "保存失败：这是一个很长很长的标题用于验证截断行为不会撑破窗口布局";
    let long_desc = "连接远端数据库时发生超时。请检查网络、代理与凭据配置后重试；如果问题持续存在，请导出日志并联系管理员协助定位。This description should wrap within the toast width.";
    let t1 = toast::Toast::new("t1", long_title)
        .description(long_desc)
        .closable()
        .on_close(|s: &mut ToastState| s.closed += 1)
        .action("重试", true, |s: &mut ToastState, _tx| s.actioned += 1)
        .build(k);
    let t2 = toast::Toast::new("t2", "已保存").description("草稿已同步").build(k);
    let t3 = toast::Toast::new("t3", "第三条通知")
        .description("用于验证堆叠")
        .build(k);
    let region = toast::toast_region(vec![t1, t2, t3]).build(k);
    let _ = state;
    rune_kit::root(rune::col().w_full().h_full(), vec![region])
}

#[test]
fn toast_close_and_action_callbacks_fire_and_region_stacks_without_overlap() {
    let mut rt = Runtime::new(ToastState::default(), toast_view, (960.0, 640.0), env());
    rt.frame(0.0);

    // Toast action is a focusable button; Tab should reach it (keyboard path).
    press(&mut rt, Key::Tab);
    assert!(focused(&rt).is_some(), "Tab focuses a control inside the toast region");
    let mut saw_action_focus = false;
    let mut saw_close_focus = false;
    for _ in 0..12 {
        if let Some(f) = focused(&rt) {
            if f.contains("actions") || f.contains("action") {
                saw_action_focus = true;
            }
            if f.contains("close") {
                saw_close_focus = true;
            }
        }
        press(&mut rt, Key::Tab);
    }
    assert!(
        saw_action_focus || saw_close_focus,
        "toast controls (action/close buttons) are reachable by Tab"
    );
    // Activate the action via click to assert callback wiring.
    let action_rect = rt
        .elements()
        .into_iter()
        .find(|e| e.role == "button" && e.path.contains("/actions/"))
        .map(|e| e.rect)
        .expect("toast action button exists");
    click_at(&mut rt, action_rect[0] + action_rect[2] * 0.5, action_rect[1] + action_rect[3] * 0.5);
    assert_eq!(rt.state.actioned, 1, "action callback fired once");
    let _ = saw_action_focus;

    // Close button for t1 is nested under the toast (kit: t1/top/close).
    rt.frame(0.0);
    let close_key = rt
        .elements()
        .into_iter()
        .filter_map(|e| e.key)
        .find(|k| k.ends_with("close") && k.contains("t1"))
        .or_else(|| {
            rt.elements()
                .into_iter()
                .find(|e| e.role == "button" && e.label.as_deref() == Some("Close"))
                .and_then(|e| e.key)
        })
        .expect("closable toast has a close button");
    click(&mut rt, &close_key);
    assert_eq!(rt.state.closed, 1, "on_close callback fired once");

    // Region stacks toasts vertically without overlap.
    let r1 = element(&rt, "t1").rect;
    let r2 = element(&rt, "t2").rect;
    let r3 = element(&rt, "t3").rect;
    for (a, b, name) in [(&r1, &r2, "t1/t2"), (&r2, &r3, "t2/t3")] {
        let overlap = !(a[1] + a[3] <= b[1] + 0.5 || b[1] + b[3] <= a[1] + 0.5);
        assert!(
            !overlap,
            "{name} must not overlap vertically: a={a:?} b={b:?}"
        );
    }

    assert_inside(&rt, 960.0, 640.0, "toast long content");
}

#[test]
fn toast_long_content_stays_inside_at_both_sizes() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(ToastState::default(), toast_view, (width, height), env());
        rt.frame(0.0);
        assert_inside(&rt, width, height, &format!("toast {width}x{height}"));
        // Toast cards have fixed 320 width; region pinned 32 from right.
        let r = element(&rt, "t1").rect;
        assert!(
            r[2] <= 320.5,
            "{width}x{height}: toast width should not exceed 320: {r:?}"
        );
    }
}

// ---- Tooltip/HoverCard are state-driven (no implicit keyboard open) ------------------------
//
// The wrappers expose `open(bool)`; the app owns focus/hover state and flips it. The trigger
// element itself remains whatever the caller passes in, so keyboard reachability of the trigger
// is preserved by the caller (e.g. a focusable button). This matches `ui/select.rs` where
// `on_toggle`/`on_close` are app-owned. No extra keyboard handler is required in this base.

#[test]
fn overlay_triggers_remain_keyboard_reachable_when_focusable() {
    let mut rt = Runtime::new(TipState { open: false }, |state, _cx| {
        let k = crate::ui::kit::kit(true);
        let mut layer = Vec::new();
        let trigger = rune::row()
            .key("focus-anchor")
            .role("button")
            .label("Info")
            .focusable(true)
            .w(80.0)
            .h(28.0)
            .center()
            .child(rune::text("Info").key("focus-anchor-text"));
        let tip = tooltip::Tooltip::new("focus-tip")
            .trigger(trigger)
            .text("Details")
            .open(state.open)
            .build(k, &mut layer);
        rune_kit::root(rune::col().w_full().h_full().padding(16.0).child(tip), layer)
    }, (960.0, 640.0), env());
    rt.frame(0.0);
    press(&mut rt, Key::Tab);
    assert_eq!(
        focused(&rt).as_deref(),
        Some("focus-anchor"),
        "a focusable tooltip trigger is reachable by Tab"
    );
}
