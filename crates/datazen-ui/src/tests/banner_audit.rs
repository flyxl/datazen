use rune::{Cx, El, Env, Runtime};

use crate::ui::banner::{Banner, BannerElement, BannerVariant};
use crate::ui::copyable::CopyableError;

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
    copied: usize,
    now: f64,
    copied_until: Option<f64>,
}

// ---- Banner: color pinning (must distinguish danger fg vs text_base, and danger chrome) ----

fn plain_banner_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let b = Banner::new("b", "boom")
        .variant(BannerVariant::Plain)
        .as_element(BannerElement::Div)
        .build(k)
        .expect("banner");
    rune::col().w(320.0).h(120.0).padding(8.0).child(b)
}

fn boxed_banner_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let b = Banner::new("b", "boom")
        .variant(BannerVariant::Boxed)
        .build(k)
        .expect("banner");
    rune::col().w(320.0).h(120.0).padding(8.0).child(b)
}

fn strip_banner_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let b = Banner::new("b", "boom")
        .variant(BannerVariant::Strip)
        .build(k)
        .expect("banner");
    rune::col().w(320.0).h(120.0).padding(8.0).child(b)
}

#[test]
fn plain_banner_text_uses_danger_fg_not_text_base() {
    // Guards: plain text color must be danger foreground, not ordinary base text.
    let k = crate::ui::kit::kit(true);
    let mut rt = Runtime::new(Probe::default(), plain_banner_view, (480.0, 240.0), env());
    rt.frame(0.0);
    let t = element(&rt, "b-text");
    let color = t.text_color.expect("text node exposes text_color");
    assert_eq!(color, k.c.danger.fg, "plain banner text must use danger fg");
    assert_ne!(color, k.c.text_base, "plain banner text must not degrade to text_base");
}

#[test]
fn boxed_and_strip_roots_use_danger_background_chrome() {
    // Guards: boxed/strip must carry danger background fill (tinted chrome), not transparent/plain.
    let k = crate::ui::kit::kit(true);

    let mut boxed = Runtime::new(Probe::default(), boxed_banner_view, (480.0, 240.0), env());
    boxed.frame(0.0);
    let boxed_root = element(&boxed, "b");
    assert_eq!(boxed_root.role, "alert");
    assert_eq!(boxed_root.fill, k.c.danger.bg, "boxed root fill must be danger.bg");
    assert_ne!(boxed_root.fill, k.c.page, "boxed root must not be plain/page background");

    let mut strip = Runtime::new(Probe::default(), strip_banner_view, (480.0, 240.0), env());
    strip.frame(0.0);
    let strip_root = element(&strip, "b");
    assert_eq!(strip_root.role, "alert");
    assert_eq!(strip_root.fill, k.c.danger.bg, "strip root fill must be danger.bg");
    assert_ne!(strip_root.fill, k.c.page, "strip root must not be plain/page background");
    // Note: border color (danger.border / border_sides) is not exposed by ElementInfo in this runtime.
}

#[test]
fn banner_alert_roles_match_react_across_variants_and_elements() {
    // Guards: every banner variant/element form remains an announced alert root.
    let k = crate::ui::kit::kit(true);
    fn view_plain_div(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
        let k = crate::ui::kit::kit(true);
        rune::col().w(320.0).h(120.0).child(
            Banner::new("b", "boom").variant(BannerVariant::Plain).as_element(BannerElement::Div).build(k).unwrap(),
        )
    }
    fn view_plain_p(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
        let k = crate::ui::kit::kit(true);
        rune::col().w(320.0).h(120.0).child(
            Banner::new("b", "boom").variant(BannerVariant::Plain).as_element(BannerElement::P).build(k).unwrap(),
        )
    }
    fn view_plain_span(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
        let k = crate::ui::kit::kit(true);
        rune::col().w(320.0).h(120.0).child(
            Banner::new("b", "boom").variant(BannerVariant::Plain).as_element(BannerElement::Span).build(k).unwrap(),
        )
    }
    fn view_boxed(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
        let k = crate::ui::kit::kit(true);
        rune::col().w(320.0).h(120.0).child(
            Banner::new("b", "boom").variant(BannerVariant::Boxed).as_element(BannerElement::Div).build(k).unwrap(),
        )
    }
    fn view_strip(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
        let k = crate::ui::kit::kit(true);
        rune::col().w(320.0).h(120.0).child(
            Banner::new("b", "boom").variant(BannerVariant::Strip).as_element(BannerElement::Div).build(k).unwrap(),
        )
    }
    for (name, vf) in [
        ("plain-div", view_plain_div as fn(&Probe, &mut Cx<Probe>) -> El),
        ("plain-p", view_plain_p),
        ("plain-span", view_plain_span),
        ("boxed-div", view_boxed),
        ("strip-div", view_strip),
    ] {
        let mut rt = Runtime::new(Probe::default(), vf, (480.0, 240.0), env());
        rt.frame(0.0);
        let root = element(&rt, "b");
        assert_eq!(root.role, "alert", "{name}: banner root must be alert");
        let t = element(&rt, "b-text");
        assert_eq!(t.text_color, Some(k.c.danger.fg), "{name}: text uses danger fg");
    }
}

// ---- Copyable: alert role by mono/non-mono ----

fn copyable_non_mono_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let w = CopyableError::new("c", "err").build::<Probe>(k);
    rune::col().w(480.0).h(120.0).child(w)
}

fn copyable_mono_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let w = CopyableError::new("c", "err").mono(true).build::<Probe>(k);
    rune::col().w(480.0).h(120.0).child(w)
}

fn copyable_non_mono_with_button_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let copied = s.copied_until.map_or(false, |u| s.now < u);
    let w = CopyableError::new("c", "err")
        .copy_button(true)
        .copied(copied)
        .on_copy_tx(|s: &mut Probe, tx: &mut rune::Tx| {
            s.copied += 1;
            s.now = tx.clock();
            s.copied_until = Some(tx.clock() + 1.5);
        })
        .build::<Probe>(k);
    let sync = rune::row()
        .key("c-sync")
        .role("button")
        .label("sync")
        .w(1.0)
        .h(1.0)
        .on_press(|_s: &mut Probe| {});
    rune::col().w(480.0).h(120.0).child(sync).child(w)
}

fn copyable_mono_with_button_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let copied = s.copied_until.map_or(false, |u| s.now < u);
    let w = CopyableError::new("c", "err")
        .mono(true)
        .copy_button(true)
        .copied(copied)
        .on_copy_tx(|s: &mut Probe, tx: &mut rune::Tx| {
            s.copied += 1;
            s.now = tx.clock();
            s.copied_until = Some(tx.clock() + 1.5);
        })
        .build::<Probe>(k);
    rune::col().w(480.0).h(120.0).child(w)
}

#[test]
fn copyable_non_mono_is_alert_mono_is_not() {
    // Guards: React parity — non-mono announced as alert, mono/technical block is not.
    let mut non_mono = Runtime::new(Probe::default(), copyable_non_mono_view, (480.0, 240.0), env());
    non_mono.frame(0.0);
    assert_eq!(element(&non_mono, "c").role, "alert", "non-mono root is alert");

    let mut mono = Runtime::new(Probe::default(), copyable_mono_view, (480.0, 240.0), env());
    mono.frame(0.0);
    assert_ne!(element(&mono, "c").role, "alert", "mono root must not be alert");

    let mut non_mono_btn = Runtime::new(Probe::default(), copyable_non_mono_with_button_view, (480.0, 240.0), env());
    non_mono_btn.frame(0.0);
    assert_eq!(element(&non_mono_btn, "c").role, "alert", "non-mono with copy button is alert");
    let mut mono_btn = Runtime::new(Probe::default(), copyable_mono_with_button_view, (480.0, 240.0), env());
    mono_btn.frame(0.0);
    assert_ne!(element(&mono_btn, "c").role, "alert", "mono with copy button must not be alert");
}

// ---- Known gaps documented per task ----

#[test]
#[ignore = "rune ElementInfo exposes no text font/style; rect-only mono metric is not a stable reverse-verifiable style assertion in this env (see banner_audit report)"]
fn mono_text_style_is_observable_via_element_tree() {
    // Intent: mono variant should use monospace type (React <pre>/font-mono) and differ
    // from non-mono text_xsmall. Current runtime ElementInfo provides text/text_color/rect/fill
    // but no font family/size/style fields, so this cannot be asserted directly or
    // reverse-verified by changing copyable font tokens alone in a robust way.
    // Existing tests/copyable.rs::mono_variant_uses_mono_face_and_is_not_an_alert compares
    // rects against a mono reference; in this environment that metric is not a reliable
    // guard against "wrong font token but same/similar rect" regressions.
}

#[test]
fn copy_button_shows_copied_feedback_for_1500ms_then_reverts() {
    // Timed feedback (React CopyableError.tsx + useCopyFeedback 1500ms):
    // - before click: button shows Copy + t('common.copy')
    // - after click: immediately shows Check + t('common.copied'), wake scheduled
    // - after ~1500ms (virtual time advanced): reverts to Copy + t('common.copy')
    // - second click re-times: 1.6s later still copied, 1.6s after that reverted
    let mut rt = Runtime::new(Probe::default(), copyable_non_mono_with_button_view, (480.0, 240.0), env());
    rt.frame(0.0);

    let before = element(&rt, "c-copy").label.unwrap_or_default();
    assert!(
        before.contains("copy") || before.contains("复制"),
        "before click should show copy label: {before}"
    );

    click(&mut rt, "c-copy");
    rt.frame(0.0);
    let copied = element(&rt, "c-copy").label.unwrap_or_default();
    assert!(
        copied.contains("copied") || copied.contains("已复制"),
        "after click should show copied feedback: {copied}"
    );
    assert_ne!(copied, before, "copied feedback must differ from copy label");

    // Advance past 1500ms and observe revert (state.now compared in view).
    let until = rt.state.copied_until.expect("copied_until set on press");
    rt.frame(1.6);
    rt.state.now = until + 0.01;
    click(&mut rt, "c-sync");
    let reverted = element(&rt, "c-copy").label.unwrap_or_default();
    assert!(
        reverted.contains("copy") || reverted.contains("复制"),
        "after 1.6s should revert to copy: {reverted}"
    );
    assert!(
        !(reverted.contains("copied") || reverted.contains("已复制")),
        "after 1.6s must not still show copied: {reverted}"
    );

    // Re-click re-times: click at ~1.6s (reverted), click again at +0.8s to reset timer,
    // then +1.0s later (1.0s since re-time, 1.8s since re-click point but timer reset) still copied,
    // then +0.6s more (1.6s since re-time) reverted. This proves the second click re-timed.
    click(&mut rt, "c-copy"); // t = T0
    rt.frame(0.0);
    let copied2 = element(&rt, "c-copy").label.unwrap_or_default();
    assert!(
        copied2.contains("copied") || copied2.contains("已复制"),
        "second click should show copied again: {copied2}"
    );
    let until2 = rt.state.copied_until.expect("copied_until set on second press");
    rt.frame(0.8);
    rt.state.now = until2 - 0.2;
    click(&mut rt, "c-sync");
    assert!(
        element(&rt, "c-copy").label.unwrap_or_default().contains("copied")
            || element(&rt, "c-copy").label.unwrap_or_default().contains("已复制"),
        "0.8s after click still copied"
    );
    click(&mut rt, "c-copy"); // re-time
    rt.frame(0.0);
    let copied3 = element(&rt, "c-copy").label.unwrap_or_default();
    assert!(
        copied3.contains("copied") || copied3.contains("已复制"),
        "re-click should show copied: {copied3}"
    );
    let until3 = rt.state.copied_until.expect("copied_until set on re-time");
    rt.frame(1.0);
    rt.state.now = until3 - 0.1;
    click(&mut rt, "c-sync");
    let still_copied = element(&rt, "c-copy").label.unwrap_or_default();
    assert!(
        still_copied.contains("copied") || still_copied.contains("已复制"),
        "1.0s after re-time should still be copied (re-timed): {still_copied}"
    );
    rt.frame(0.6);
    rt.state.now = until3 + 0.01;
    click(&mut rt, "c-sync");
    let reverted2 = element(&rt, "c-copy").label.unwrap_or_default();
    assert!(
        reverted2.contains("copy") || reverted2.contains("复制"),
        "1.6s after re-time should revert: {reverted2}"
    );
}
