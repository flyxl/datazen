//! Independent audit tests for T0-10 overlay base (toast/tooltip/hover-card).
//!
//! Focus: auto-dismiss lifecycle wiring, outward placement flip/clamp behavior, hover-card anchor fit,
//! and toast stacking/interaction invariants beyond the coder's self-tests.

use rune::{Cx, El, Env, Runtime};
use rune_kit::toast_stack::ToastEffect;
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

#[allow(dead_code)]
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

fn rects_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    !(a[0] + a[2] <= b[0] + 0.5
        || b[0] + b[2] <= a[0] + 0.5
        || a[1] + a[3] <= b[1] + 0.5
        || b[1] + b[3] <= a[1] + 0.5)
}

// ---- 1. Auto-dismiss lifecycle -----------------------------------------------------------

/// Integration: a timed toast hosted by ToastHost disappears once its time elapses and fires the
/// close callback exactly once (via ToastEffect::Closed). Further clock advances must not re-fire.
#[test]
fn toast_dismisses_once_its_time_elapses() {
    #[derive(Clone, Debug, Default, PartialEq)]
    struct Opt {
        title: &'static str,
    }

    #[derive(Default)]
    struct S {
        host: ToastHost<Opt>,
        closed: usize,
        now: f64,
        version: usize,
    }

    fn view(s: &S, _cx: &mut Cx<S>) -> El {
        let k = crate::ui::kit::kit(true);
        let cards = s
            .host
            .entries()
            .iter()
            .filter(|e| s.host.stack().place(e.id, |_| 0.0).is_some_and(|p| p.visible))
            .map(|entry| {
                toast::Toast::new(format!("auto-{}", entry.id), entry.options.title)
                    .description("timed")
                    .build(k)
            })
            .collect::<Vec<_>>();
        let region = toast::toast_region(cards).build(k);
        let tick = rune::row()
            .key("tick-toast")
            .role("button")
            .label("tick")
            .w(1.0)
            .h(1.0)
            .on_press(|s: &mut S| {
                s.version += 1;
                for e in s.host.tick(s.now) {
                    if let ToastEffect::Closed(_) = e {
                        s.closed += 1;
                    }
                }
            });
        rune_kit::root(
            rune::col()
                .w_full()
                .h_full()
                .child(rune::el().key(format!("v{}", s.version)).w(0.0).h(0.0))
                .child(tick)
                .child(region),
            Vec::new(),
        )
    }

    let mut state = S::default();
    let _ = state
        .host
        .show("auto", Opt { title: "Saving" }, Some(2.0), 0.0);
    let mut rt = Runtime::new(state, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert_eq!(rt.state.host.entries().len(), 1, "toast mounted initially");
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("auto-"))),
        "toast element present initially"
    );

    // Drive time past due and tick through a state-mutating handler so the view re-renders.
    rt.state.now = 2.1;
    click_center(&mut rt, "tick-toast");
    rt.frame(0.0);

    assert_eq!(rt.state.host.entries().len(), 0, "toast removed after its time elapses");
    // Verify the expired toast is not rendered in a fresh frame of the post-expiry state.
    let mut gone = Runtime::new(
        S {
            host: rt.state.host.clone(),
            closed: rt.state.closed,
            now: rt.state.now,
            version: rt.state.version,
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
            .any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("auto-"))),
        "toast element gone after expiry"
    );
    assert_eq!(rt.state.closed, 1, "auto-close callback/effect fired exactly once");

    // Further ticks must not re-fire.
    rt.state.now = 5.0;
    click_center(&mut rt, "tick-toast");
    rt.frame(0.0);
    assert_eq!(rt.state.closed, 1, "no duplicate close after expiry");
}

/// Vendor mechanism sanity: ToastStack expires a timed toast on Tick and reports Closed/WakeAt.
/// Proves the kit capability exists; integration is the missing piece.
#[test]
fn toast_stack_step_can_close_on_tick() {
    use rune_kit::toast_stack::{ToastEffect, ToastEvent, ToastStack};

    let mut stack: ToastStack<&'static str> = ToastStack::default();
    let effects = stack.step(ToastEvent::Shown {
        key: "k".into(),
        options: "hello",
        secs: Some(2.0),
        now: 0.0,
    });
    let wake = effects
        .into_iter()
        .any(|e| matches!(e, ToastEffect::WakeAt(t) if (t - 2.0).abs() < 1e-6));
    assert!(wake, "showing a timed toast requests WakeAt(2.0)");
    assert_eq!(stack.entries().len(), 1);

    let effects = stack.step(ToastEvent::Tick { now: 2.1 });
    let closed = effects
        .into_iter()
        .any(|e| matches!(e, ToastEffect::Closed(_)));
    assert!(closed, "tick past due closes toast");
    assert_eq!(stack.entries().len(), 0, "expired toast removed from stack");
}

// ---- 2. Outward placement / flip behavior ------------------------------------------------

#[derive(Default, Clone, Copy)]
struct OutTipState {
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

/// Places trigger at an edge/corner and requests a placement that points OUTSIDE the window.
/// Records whether the overlay stays in-bounds (flip/clamp expected for robust overlays).
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
        .open(state.open)
        .build(k, &mut layer);
    // edge: 0=top-left, 1=bottom-right, 2=top-right, 3=bottom-left
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
fn tooltip_outward_placement_flip_or_clamp_observation() {
    // (placement requested, edge, label) - placements chosen to point out of window.
    let cases = [
        (1u8, 0u8, "top-requested-at-top-left"),
        (3u8, 0u8, "left-requested-at-top-left"),
        (0u8, 1u8, "bottom-requested-at-bottom-right"),
        (2u8, 1u8, "right-requested-at-bottom-right"),
    ];
    let mut observations = Vec::new();
    for (width, height) in [(960.0, 640.0)] {
        for (placement, edge, label) in cases {
            let mut rt = Runtime::new(
                OutTipState {
                    open: true,
                    placement,
                    edge,
                },
                outward_tip_view,
                (width, height),
                env(),
            );
            rt.frame(0.0);
            if rt.find("out-tip-tooltip").is_none() {
                observations.push(format!("{label}: tooltip not mounted"));
                continue;
            }
            let tr = element(&rt, "out-anchor").rect;
            let tip = element(&rt, "out-tip-tooltip").rect;
            let inside = tip[0] >= -0.5
                && tip[1] >= -0.5
                && tip[0] + tip[2] <= width + 0.5
                && tip[1] + tip[3] <= height + 0.5;
            observations.push(format!(
                "{label}: trigger={tr:?} tip={tip:?} inside={inside}"
            ));
            assert!(
                inside,
                "{width}x{height} {label}: outward placement should flip/clamp into window: {observations:?}"
            );
        }
    }
    eprintln!("tooltip outward observations: {observations:#?}");
}

// ---- 3. HoverCard trigger fit & outward behavior -----------------------------------------

#[derive(Default)]
struct CardFitState;

fn card_fit_view(_state: &CardFitState, _cx: &mut Cx<CardFitState>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let trigger = rune::row()
        .key("fit-anchor")
        .w(120.0)
        .h(30.0)
        .center()
        .child(rune::text("Trigger").key("fit-t"));
    let card = tooltip::HoverCard::new("fit-card")
        .trigger(trigger)
        .child(rune::text("Body").key("fit-body"))
        .open(true)
        .build(k, &mut layer);
    rune_kit::root(rune::col().w_full().h_full().padding(40.0).child(card), layer)
}

#[test]
fn hover_card_anchor_row_fits_trigger_width() {
    let mut rt = Runtime::new(CardFitState, card_fit_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let anchor = element(&rt, "fit-card-trigger").rect;
    // Anchor row is sized to the trigger content (120.0 wide), not expanded across the column.
    assert!(
        (anchor[2] - 120.0).abs() <= 1.0,
        "anchor width should fit trigger (≈120pt ±1): {anchor:?}"
    );
    eprintln!("hover_card anchor rect (trigger w=120): {anchor:?}");
    assert!(rt.find("fit-card-card").is_some(), "card content mounted");
}

#[derive(Default, Clone, Copy)]
struct OutCardState {
    open: bool,
    placement: u8,
    edge: u8,
}

fn outward_card_view(state: &OutCardState, _cx: &mut Cx<OutCardState>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let trigger = rune::row()
        .key("oc-anchor")
        .w(72.0)
        .h(24.0)
        .center()
        .child(rune::text("C").key("oc-t"));
    let card = tooltip::HoverCard::new("oc")
        .trigger(trigger)
        .child(rune::text("Preview content body").key("oc-body"))
        .placement(placement_from_u8(state.placement))
        .open(state.open)
        .build(k, &mut layer);
    let page = match state.edge {
        1 => rune::col().w_full().h_full().justify_end().child(rune::row().w_full().justify_end().child(card)),
        2 => rune::col().w_full().h_full().child(rune::row().w_full().justify_end().child(card)),
        3 => rune::col().w_full().h_full().justify_end().child(rune::row().w_full().child(card)),
        _ => rune::col().w_full().h_full().child(card),
    };
    rune_kit::root(page, layer)
}

#[test]
fn hover_card_outward_placement_stays_inside_window() {
    let cases = [
        (1u8, 0u8, "top-at-top-left"),
        (3u8, 0u8, "left-at-top-left"),
        (0u8, 1u8, "bottom-at-bottom-right"),
        (2u8, 1u8, "right-at-bottom-right"),
    ];
    for (placement, edge, label) in cases {
        let mut rt = Runtime::new(
            OutCardState { open: true, placement, edge },
            outward_card_view,
            (960.0, 640.0),
            env(),
        );
        rt.frame(0.0);
        let card = element(&rt, "oc-card").rect;
        let inside = card[0] >= -0.5
            && card[1] >= -0.5
            && card[0] + card[2] <= 960.0 + 0.5
            && card[1] + card[3] <= 640.0 + 0.5;
        assert!(inside, "{label}: card should flip/clamp into window, got {card:?}");
    }
}

// ---- 4. Toast stacking/callbacks/geometry ------------------------------------------------

#[derive(Default)]
struct MultiToastState {
    closed: usize,
    actioned: usize,
}

fn multi_toast_view(state: &MultiToastState, _cx: &mut Cx<MultiToastState>) -> El {
    let k = crate::ui::kit::kit(true);
    let long = "A very long toast title and description that should wrap within the fixed toast width instead of overflowing the window bounds when rendered in the bottom-right region.";
    let cards = vec![
        toast::Toast::new("mt1", long)
            .description(long)
            .closable()
            .on_close(|s: &mut MultiToastState| s.closed += 1)
            .action("Act", true, |s: &mut MultiToastState, _tx| s.actioned += 1)
            .build(k),
        toast::Toast::new("mt2", "Second").description("two").build(k),
        toast::Toast::new("mt3", "Third").description("three").build(k),
        toast::Toast::new("mt4", "Fourth").description("four").build(k),
    ];
    let region = toast::toast_region(cards).build(k);
    let _ = state;
    rune_kit::root(rune::col().w_full().h_full(), vec![region])
}

#[test]
fn toast_four_stack_no_overlap_all_inside_callbacks_once() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(MultiToastState::default(), multi_toast_view, (width, height), env());
        rt.frame(0.0);

        let keys = ["mt1", "mt2", "mt3", "mt4"];
        let mut rects = Vec::new();
        for key in keys {
            let r = element(&rt, key).rect;
            assert!(r[2] <= 320.5, "{width}x{height} {key} width exceeds 320: {r:?}");
            rects.push((key, r));
        }
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                assert!(
                    !rects_overlap(rects[i].1, rects[j].1),
                    "{width}x{height} {} and {} overlap: {:?} {:?}",
                    rects[i].0,
                    rects[j].0,
                    rects[i].1,
                    rects[j].1
                );
            }
        }
        assert_inside(&rt, width, height, &format!("toast stack {width}x{height}"));

        // Find and click action button inside mt1
        let action = rt
            .elements()
            .into_iter()
            .find(|e| e.role == "button" && e.path.contains("mt1") && e.path.contains("actions"))
            .map(|e| e.rect)
            .expect("mt1 has action button");
        click_at(&mut rt, action[0] + action[2] * 0.5, action[1] + action[3] * 0.5);
        assert_eq!(rt.state.actioned, 1, "action callback fired once");
        // clicking again should fire again only if clicked again; ensure no duplicate from single click
        assert_eq!(rt.state.actioned, 1);

        // Find and click close button for mt1
        let close = rt
            .elements()
            .into_iter()
            .find(|e| e.role == "button" && e.label.as_deref() == Some("Close") && e.path.contains("mt1"))
            .map(|e| e.rect)
            .expect("mt1 has close button");
        click_at(&mut rt, close[0] + close[2] * 0.5, close[1] + close[3] * 0.5);
        assert_eq!(rt.state.closed, 1, "close callback fired once");
    }
}

/// Records stacking order (vertical positions) for interpretation vs React habits.
#[test]
fn toast_stack_order_is_deterministic() {
    let mut rt = Runtime::new(MultiToastState::default(), multi_toast_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let y1 = element(&rt, "mt1").rect[1];
    let y2 = element(&rt, "mt2").rect[1];
    let y3 = element(&rt, "mt3").rect[1];
    let y4 = element(&rt, "mt4").rect[1];
    // Just assert they form a consistent vertical stack (all different y bands) and record order via assertion message context.
    assert!(y1 != y2 && y2 != y3 && y3 != y4, "stacked vertically: mt1={y1} mt2={y2} mt3={y3} mt4={y4}");
}
