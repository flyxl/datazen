//! Merged-mainline re-audit for T0-10 overlay base (toast/tooltip/hover-card).
//! Supplements existing overlay/tooltip tests with explicit checks for:
//! - tick clock semantics (caller-driven `now`) and ToastEffect::Closed-once
//! - compact tooltip long single-line (~250 chars) capped/wrapped in 960x640
//! - outward Top/Bottom/Left/Right placements stay in-window (flip/clamp)
//! - HoverCard anchor width fits trigger (±1pt)
//! - toast close button role/label and single-fire callbacks

use rune::{Cx, El, Env, Runtime};
use rune_kit::toast_stack::{ToastEffect, ToastEvent, ToastStack};
use rune_kit::Placement;

use crate::ui::toast::ToastHost;
use crate::ui::{toast, tooltip};

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

fn click_at<S>(rt: &mut Runtime<S>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn click_center<S>(rt: &mut Runtime<S>, key: &str) {
    let r = element(rt, key).rect;
    click_at(rt, r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
}

fn assert_inside<S>(rt: &Runtime<S>, width: f32, height: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let (x, y, w, h) = (e.rect[0], e.rect[1], e.rect[2], e.rect[3]);
        if x < -0.5 || y < -0.5 || x + w > width + 0.5 || y + h > height + 0.5 {
            outside.push(format!(
                "{} {:?} role={} label={:?}",
                e.key.clone().unwrap_or_else(|| e.path.clone()),
                e.rect,
                e.role,
                e.label
            ));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) cross the window edge: {outside:?}",
        outside.len()
    );
}

// 1) Toast expiry: caller-driven clock `now`, Closed emitted once on tick past due.

#[test]
fn toast_host_tick_uses_caller_clock_and_emits_closed_once() {
    let mut stack: ToastStack<&'static str> = ToastStack::default();
    let fx = stack.step(ToastEvent::Shown {
        key: "k".into(),
        options: "hello",
        secs: Some(2.0),
        now: 0.0,
    });
    assert!(
        fx.into_iter()
            .any(|e| matches!(e, ToastEffect::WakeAt(t) if (t - 2.0).abs() < 1e-6)),
        "timed toast requests WakeAt at due time (caller clock)"
    );

    let fx = stack.step(ToastEvent::Tick { now: 1.9 });
    assert!(
        !fx.into_iter().any(|e| matches!(e, ToastEffect::Closed(_))),
        "before due time, tick must not close"
    );
    assert_eq!(stack.entries().len(), 1);

    let fx = stack.step(ToastEvent::Tick { now: 2.1 });
    assert!(
        fx.into_iter().any(|e| matches!(e, ToastEffect::Closed(_))),
        "past due time, tick closes toast"
    );
    assert_eq!(stack.entries().len(), 0);

    let fx = stack.step(ToastEvent::Tick { now: 5.0 });
    assert!(
        !fx.into_iter().any(|e| matches!(e, ToastEffect::Closed(_))),
        "Closed must not re-fire after removal"
    );
}

#[test]
fn toast_host_integration_removes_element_after_tick_past_due() {
    #[derive(Clone, Debug, Default, PartialEq)]
    struct Opt {
        title: &'static str,
    }
    #[derive(Default)]
    struct S {
        host: ToastHost<Opt>,
        closed: usize,
        now: f64,
    }
    fn view(s: &S, _cx: &mut Cx<S>) -> El {
        let k = crate::ui::kit::kit(true);
        let cards = s
            .host
            .entries()
            .iter()
            .filter(|e| s.host.stack().place(e.id, |_| 0.0).is_some_and(|p| p.visible))
            .map(|e| toast::Toast::new(format!("audit-auto-{}", e.id), e.options.title).build(k))
            .collect::<Vec<_>>();
        let tick = rune::row()
            .key("audit-tick")
            .role("button")
            .label("tick")
            .w(1.0)
            .h(1.0)
            .on_press(|s: &mut S| {
                for fx in s.host.tick(s.now) {
                    if let ToastEffect::Closed(_) = fx {
                        s.closed += 1;
                    }
                }
            });
        rune_kit::root(
            rune::col()
                .w_full()
                .h_full()
                .child(tick)
                .child(toast::toast_region(cards).build(k)),
            Vec::new(),
        )
    }

    let mut st = S::default();
    let _ = st.host.show("k", Opt { title: "Hi" }, Some(1.0), 0.0);
    let mut rt = Runtime::new(st, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("audit-auto-"))),
        "toast present before expiry"
    );
    rt.state.now = 1.5;
    click_center(&mut rt, "audit-tick");
    rt.frame(0.0);
    assert_eq!(rt.state.host.entries().len(), 0);
    assert_eq!(rt.state.closed, 1);
    let mut gone = Runtime::new(
        S {
            host: rt.state.host.clone(),
            closed: rt.state.closed,
            now: rt.state.now,
        },
        view,
        (960.0, 640.0),
        env(),
    );
    gone.frame(0.0);
    assert!(
        !gone
            .elements()
            .iter()
            .any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("audit-auto-"))),
        "toast element removed from rendered tree after expiry"
    );
}

// 2) Compact tooltip ~250 chars at 960x640 stays in-window, capped at 320pt.

#[derive(Clone, Copy, Default)]
struct LongTipState;

fn long_tip_view(_: &LongTipState, _cx: &mut Cx<LongTipState>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let long = "This is a compact tooltip hint of roughly two hundred and fifty characters that keeps explaining the action, the affected scope, the keyboard shortcut, and an extra caution so that a single-line message would overflow a small 960 by 640 window without wrapping properly indeed.";
    let trigger = rune::row()
        .key("long-tip-anchor")
        .size(120.0, 28.0)
        .center()
        .child(rune::text("Hover"));
    let tip = tooltip::Tooltip::new("long-tip")
        .trigger(trigger)
        .text(long)
        .open(true)
        .placement(Placement::BottomStart)
        .build(k, &mut layer);
    let anchored = rune::col()
        .key("long-tip-wrap")
        .pin(None, None, Some(24.0), Some(24.0))
        .child(tip);
    rune_kit::root(rune::col().w_full().h_full().child(anchored), layer)
}

#[test]
fn compact_tooltip_about_250_chars_stays_inside_960x640_with_cap() {
    let mut rt = Runtime::new(LongTipState, long_tip_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let tip = element(&rt, "long-tip-tooltip").rect;
    assert!(
        tip[2] <= 320.5,
        "compact tooltip width must be capped near 320pt, got {tip:?}"
    );
    assert!(
        tip[3] > 22.5,
        "long single-line content should wrap (multi-line height), got {tip:?}"
    );
    assert_inside(&rt, 960.0, 640.0, "compact ~250-char tooltip at 960x640");
}

// 3) Four outward directions near edges flip/clamp and stay in-window.

#[derive(Default, Clone, Copy)]
struct OutTipState {
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

fn outward_tip_view(state: &OutTipState, _cx: &mut Cx<OutTipState>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let trigger = rune::row()
        .key("out-anchor")
        .w(72.0)
        .h(24.0)
        .center()
        .child(rune::text("T").key("out-t"));
    let tip = tooltip::Tooltip::new("out-tip")
        .trigger(trigger)
        .text("outward tooltip text")
        .placement(placement_from_u8(state.placement))
        .open(true)
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

#[test]
fn tooltip_outward_top_bottom_left_right_stay_inside_window() {
    // Requested placement points outward at the chosen edge/corner.
    for (placement, edge, label) in [
        (1u8, 0u8, "Top requested at top-left"),
        (0u8, 1u8, "Bottom requested at bottom-right"),
        (3u8, 0u8, "Left requested at top-left"),
        (2u8, 1u8, "Right requested at bottom-right"),
    ] {
        let mut rt = Runtime::new(OutTipState { placement, edge }, outward_tip_view, (960.0, 640.0), env());
        rt.frame(0.0);
        assert!(
            rt.find("out-tip-tooltip").is_some(),
            "{label}: tooltip mounted"
        );
        assert_inside(&rt, 960.0, 640.0, label);
    }
}

// 4) HoverCard anchor width fits trigger (±1pt).

#[derive(Default)]
struct CardFitState;

fn card_fit_view(_: &CardFitState, _cx: &mut Cx<CardFitState>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let trigger = rune::row()
        .key("hc-fit-trigger-el")
        .w(120.0)
        .h(30.0)
        .center()
        .child(rune::text("Trigger").key("hc-fit-t"));
    let card = tooltip::HoverCard::new("hc-fit")
        .trigger(trigger)
        .child(rune::text("Body").key("hc-fit-body"))
        .open(true)
        .build(k, &mut layer);
    rune_kit::root(rune::col().w_full().h_full().padding(40.0).child(card), layer)
}

#[test]
fn hover_card_anchor_fits_trigger_width_within_one_point() {
    let mut rt = Runtime::new(CardFitState, card_fit_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let anchor = element(&rt, "hc-fit-trigger").rect;
    assert!(
        (anchor[2] - 120.0).abs() <= 1.0,
        "anchor width must fit trigger (120pt ±1), got {anchor:?}"
    );
}

// 5) Toast close button role/label and on_close/action fire once.

#[derive(Default)]
struct ToastCbState {
    closed: usize,
    actioned: usize,
}

fn toast_cb_view(_: &ToastCbState, _cx: &mut Cx<ToastCbState>) -> El {
    let k = crate::ui::kit::kit(true);
    let t1 = toast::Toast::new("cb1", "Hello")
        .description("desc")
        .closable()
        .on_close(|s: &mut ToastCbState| s.closed += 1)
        .action("Retry", true, |s: &mut ToastCbState, _tx| s.actioned += 1)
        .build(k);
    rune_kit::root(rune::col().w_full().h_full(), vec![toast::toast_region(vec![t1]).build(k)])
}

#[test]
fn toast_close_button_has_role_label_and_callbacks_fire_once() {
    let mut rt = Runtime::new(ToastCbState::default(), toast_cb_view, (960.0, 640.0), env());
    rt.frame(0.0);

    let action = rt
        .elements()
        .into_iter()
        .find(|e| e.role == "button" && e.path.contains("cb1") && e.path.contains("actions"))
        .map(|e| e.rect)
        .expect("cb1 has action button");
    click_at(&mut rt, action[0] + action[2] * 0.5, action[1] + action[3] * 0.5);
    assert_eq!(rt.state.actioned, 1, "action callback fired once");

    let close = rt
        .elements()
        .into_iter()
        .find(|e| e.role == "button" && e.label.as_deref() == Some("Close") && e.path.contains("cb1"))
        .map(|e| e.rect)
        .expect("cb1 has close button with role=button label=Close");
    click_at(&mut rt, close[0] + close[2] * 0.5, close[1] + close[3] * 0.5);
    assert_eq!(rt.state.closed, 1, "on_close callback fired once");
}
