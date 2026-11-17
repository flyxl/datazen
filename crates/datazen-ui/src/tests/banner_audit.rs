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

fn copyable_non_mono_with_button_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let w = CopyableError::new("c", "err")
        .copy_button(true)
        .on_copy(|s: &mut Probe| s.copied += 1)
        .build::<Probe>(k);
    rune::col().w(480.0).h(120.0).child(w)
}

fn copyable_mono_with_button_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let w = CopyableError::new("c", "err")
        .mono(true)
        .copy_button(true)
        .on_copy(|s: &mut Probe| s.copied += 1)
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
#[ignore = "DEFECT: copyable has no copied-state timer (no 1500ms common.copied feedback); needs timed state/clock mechanism in runtime/view"]
fn copy_button_shows_copied_feedback_for_1500ms_then_reverts() {
    // Expected (React CopyableError.tsx + useCopyFeedback 1500ms):
    // - before click: button shows Copy icon + t('common.copy')
    // - after click: immediately shows Check icon + t('common.copied')
    // - after ~1500ms: reverts to Copy + t('common.copy')
    // Current implementation (ui/copyable.rs): label is static i18n::t("common.copy")
    // and press only tx.copy + optional callback; no transient "copied" state/timer.
    // To implement: add timed copied state (e.g. store timestamp/flag in app state and
    // drive revert via frame clock/tick or ToastTimer-like runtime timer) and swap
    // glyph/label while active.
    let mut rt = Runtime::new(Probe::default(), copyable_non_mono_with_button_view, (480.0, 240.0), env());
    rt.frame(0.0);
    click(&mut rt, "c-copy");
    // When implemented, assert copied label/glyph here and after 1.5s revert.
    let btn = element(&rt, "c-copy");
    let label = btn.label.unwrap_or_default();
    assert!(label.contains("copied") || label.contains("已复制"), "should show copied feedback: {label}");
}
