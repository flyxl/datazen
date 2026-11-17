//! Second-round independent recheck for T0-10 fixes (toast lifecycle host, hover-card anchor fit).

use rune::{Cx, El, Env, Runtime};
use rune_kit::toast_stack::ToastEffect;

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

/// Key assertion: timed toast rendered from ToastHost is removed from the element tree after tick past due.
#[test]
fn retest_toast_element_removed_from_tree_after_expiry() {
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
            .map(|e| toast::Toast::new(format!("rt-auto-{}", e.id), e.options.title).build(k))
            .collect::<Vec<_>>();
        let tick = rune::row()
            .key("rt-tick")
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
        rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("rt-auto-"))),
        "toast present before expiry"
    );

    rt.state.now = 1.5;
    click_center(&mut rt, "rt-tick");
    rt.frame(0.0);
    assert_eq!(rt.state.host.entries().len(), 0, "host entry removed on expiry");
    assert_eq!(rt.state.closed, 1, "Closed effect handled once");

    // Verify removal from the rendered element tree with post-expiry state.
    let mut gone = Runtime::new(
        S { host: rt.state.host.clone(), closed: rt.state.closed, now: rt.state.now },
        view,
        (960.0, 640.0),
        env(),
    );
    gone.frame(0.0);
    assert!(
        !gone.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("rt-auto-"))),
        "toast element removed from rendered tree after expiry"
    );

    rt.state.now = 3.0;
    click_center(&mut rt, "rt-tick");
    rt.frame(0.0);
    assert_eq!(rt.state.closed, 1, "Closed effect not re-fired on later ticks");
}

/// Key assertion: hover-card anchor row width matches trigger width (fit), not expanded across container.
#[test]
fn retest_hover_card_anchor_fits_trigger_width() {
    #[derive(Default)]
    struct S;
    fn view(_: &S, _cx: &mut Cx<S>) -> El {
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
    let mut rt = Runtime::new(S, view, (960.0, 640.0), env());
    rt.frame(0.0);
    let anchor = element(&rt, "hc-fit-trigger").rect;
    assert!(
        (anchor[2] - 120.0).abs() <= 1.0,
        "anchor width must fit trigger (~120), got {anchor:?}"
    );
}

/// Outward placements remain inside window (independent check, four directions).
#[test]
fn retest_hover_card_outward_four_directions_inside_window() {
    #[derive(Default, Clone, Copy)]
    struct S { placement: u8, edge: u8 }
    fn placement(v: u8) -> rune_kit::Placement {
        match v { 1 => rune_kit::Placement::Top, 2 => rune_kit::Placement::Right, 3 => rune_kit::Placement::Left, _ => rune_kit::Placement::Bottom }
    }
    fn view(s: &S, _cx: &mut Cx<S>) -> El {
        let k = crate::ui::kit::kit(true);
        let mut layer = Vec::new();
        let trigger = rune::row().key("oa").w(72.0).h(24.0).center().child(rune::text("C").key("oat"));
        let card = tooltip::HoverCard::new("oc")
            .trigger(trigger)
            .child(rune::text("body").key("ocb"))
            .placement(placement(s.placement))
            .open(true)
            .build(k, &mut layer);
        let page = match s.edge {
            1 => rune::col().w_full().h_full().justify_end().child(rune::row().w_full().justify_end().child(card)),
            2 => rune::col().w_full().h_full().child(rune::row().w_full().justify_end().child(card)),
            3 => rune::col().w_full().h_full().justify_end().child(rune::row().w_full().child(card)),
            _ => rune::col().w_full().h_full().child(card),
        };
        rune_kit::root(page, layer)
    }
    for (p, e, label) in [(1u8,0u8,"top-at-tl"),(3u8,0u8,"left-at-tl"),(0u8,1u8,"bottom-at-br"),(2u8,1u8,"right-at-br")] {
        let mut rt = Runtime::new(S{placement:p,edge:e}, view, (960.0,640.0), env());
        rt.frame(0.0);
        let r = element(&rt, "oc-card").rect;
        assert!(r[0]>= -0.5 && r[1]>= -0.5 && r[0]+r[2] <= 960.5 && r[1]+r[3] <= 640.5, "{label}: {r:?}");
    }
}
