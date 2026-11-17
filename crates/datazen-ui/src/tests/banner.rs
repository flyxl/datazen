use rune::{Cx, El, Env, Runtime};

use crate::ui::banner::{Banner, BannerVariant};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
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

#[derive(Default)]
struct Probe {
    dismissed: usize,
}

fn plain_view(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut root = rune::col().w(320.0).h(240.0).padding(8.0);
    if let Some(b) = Banner::new("probe-banner", "boom")
        .variant(BannerVariant::Plain)
        .build(k)
    {
        root = root.child(b);
    }
    root
}

fn boxed_view(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut root = rune::col().w(320.0).h(240.0).padding(8.0);
    if let Some(b) = Banner::new("probe-banner", "boom")
        .variant(BannerVariant::Boxed)
        .build(k)
    {
        root = root.child(b);
    }
    root
}

fn strip_view(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let mut root = rune::col().w(320.0).h(240.0).padding(8.0);
    if let Some(b) = Banner::new("probe-banner", "boom")
        .variant(BannerVariant::Strip)
        .build(k)
    {
        root = root.child(b);
    }
    root
}

fn dismiss_view(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let banner = Banner::new("probe-banner", "boom")
        .variant(BannerVariant::Boxed)
        .on_dismiss("Dismiss error", |s: &mut Probe| s.dismissed += 1)
        .build(k)
        .expect("banner");
    rune::col().w(320.0).h(240.0).padding(8.0).child(banner)
}

fn dismiss_disconnected_view(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    // No dismiss handler wired: control must not exist / cannot reach probe.
    let banner = Banner::new("probe-banner", "boom")
        .variant(BannerVariant::Boxed)
        .build(k)
        .expect("banner");
    rune::col().w(320.0).h(240.0).padding(8.0).child(banner)
}

#[test]
fn each_variant_is_an_alert_with_its_message_and_boxed_strip_are_taller_and_wide() {
    // Each variant mounts as alert with its message key.
    for (variant, view) in [
        (
            BannerVariant::Plain,
            plain_view as fn(&Probe, &mut Cx<Probe>) -> El,
        ),
        (BannerVariant::Boxed, boxed_view),
        (BannerVariant::Strip, strip_view),
    ] {
        let mut rt = Runtime::new(Probe::default(), view, (480.0, 320.0), env());
        rt.frame(0.0);
        let root = element(&rt, "probe-banner");
        assert_eq!(root.role, "alert", "{variant:?}: root role");
        let text = element(&rt, "probe-banner-text");
        assert_eq!(text.text.as_deref(), Some("boom"), "{variant:?}: message text");
        assert!(
            rt.find("probe-banner-text").is_some(),
            "{variant:?}: message key"
        );
    }

    let mut plain = Runtime::new(Probe::default(), plain_view, (480.0, 320.0), env());
    plain.frame(0.0);
    let mut boxed = Runtime::new(Probe::default(), boxed_view, (480.0, 320.0), env());
    boxed.frame(0.0);
    let mut strip = Runtime::new(Probe::default(), strip_view, (480.0, 320.0), env());
    strip.frame(0.0);

    let plain_r = element(&plain, "probe-banner").rect;
    let boxed_r = element(&boxed, "probe-banner").rect;
    let strip_r = element(&strip, "probe-banner").rect;
    assert!(
        boxed_r[3] > plain_r[3] && strip_r[3] > plain_r[3],
        "boxed/strip should be taller than plain: plain={plain_r:?} boxed={boxed_r:?} strip={strip_r:?}"
    );
    // Boxed/strip carry chrome and span the width they are given (container width).
    assert!(
        boxed_r[2] >= plain_r[2],
        "boxed spans given width: plain_w={} boxed_w={}",
        plain_r[2],
        boxed_r[2]
    );
    assert!(
        strip_r[2] >= boxed_r[2] - 1.0,
        "strip spans given width: strip_w={} boxed_w={}",
        strip_r[2],
        boxed_r[2]
    );
}

#[test]
fn dismiss_control_is_focusable_labeled_and_reaches_probe_state() {
    let mut rt = Runtime::new(Probe::default(), dismiss_view, (480.0, 320.0), env());
    rt.frame(0.0);
    let dismiss = element(&rt, "probe-banner-dismiss");
    assert!(dismiss.focusable, "dismiss is focusable");
    assert_eq!(
        dismiss.label.as_deref(),
        Some("Dismiss error"),
        "dismiss has accessible label"
    );
    click(&mut rt, "probe-banner-dismiss");
    assert_eq!(rt.state.dismissed, 1, "press reaches probe state");
}

#[test]
fn dismiss_reverse_verification_disconnects_handler() {
    // Reverse: without on_dismiss, no dismiss control is mounted and probe stays unchanged.
    let mut rt = Runtime::new(
        Probe::default(),
        dismiss_disconnected_view,
        (480.0, 320.0),
        env(),
    );
    rt.frame(0.0);
    assert!(
        rt.find("probe-banner-dismiss").is_none(),
        "no dismiss control without handler"
    );
    assert_eq!(rt.state.dismissed, 0, "disconnected handler cannot fire");
}


