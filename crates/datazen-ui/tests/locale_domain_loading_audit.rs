//! T140 independent audit (tester): fresh vectors for the loading
//! skeleton — a different `testId` (the caller's prop), a narrow window,
//! and the spinner/fallback channels measured apart from the coder's
//! fixtures.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::locale_domain_loading::{
    LocaleDomainLoading, LocaleDomainLoadingVariant,
};

const NARROW: (f32, f32) = (960.0, 640.0);

fn skeleton_view(state: &bool, _cx: &mut Cx<bool>) -> El {
    // `state` picks the variant; the testId is a different one than the
    // coder's (`data-sync-window-loading` — the shape a feature window
    // would pass while its domain imports).
    let dark = _cx.system_dark();
    let variant = match *state {
        true => LocaleDomainLoadingVariant::Page,
        false => LocaleDomainLoadingVariant::Section,
    };
    col()
        .key("page")
        .w_full()
        .h_full()
        .child(
            LocaleDomainLoading::new("data-sync-window-loading")
                .variant(variant)
                .build(dark),
        )
}

fn skeleton_runtime(section: bool, size: (f32, f32)) -> Runtime<bool> {
    let mut rt = Runtime::new(
        section,
        skeleton_view,
        size,
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

#[test]
fn audit_the_page_variant_fills_a_narrow_window_too() {
    let rt = skeleton_runtime(true, NARROW);
    let root = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-window-loading"))
        .expect("the skeleton is laid out");
    let [x, y, w, h] = root.rect;
    let (width, height) = NARROW;
    assert!(
        (h - height).abs() < 0.5,
        "h-screen fills whatever window it is given: {h} vs {height}"
    );
    assert!((w - width).abs() < 0.5, "w-full: {w} vs {width}");
    assert!(x.abs() < 0.5 && y.abs() < 0.5, "it covers from the origin");
}

#[test]
fn audit_the_section_variant_is_bounded_not_the_window() {
    let rt = skeleton_runtime(false, NARROW);
    let root = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-window-loading"))
        .expect("the skeleton is laid out");
    let [_, _, _, h] = root.rect;
    assert!(
        (h - 128.0).abs() < 0.5,
        "the section is a bounded 128pt region, not the window: {h}"
    );
}

#[test]
fn audit_the_channels_are_the_status_role_the_fallback_and_the_indicator() {
    let rt = skeleton_runtime(true, NARROW);
    let root = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-window-loading"))
        .expect("the skeleton is laid out");
    assert_eq!(root.role, "status");
    let fallback = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-window-loading-fallback"))
        .and_then(|e| e.text)
        .expect("the fallback text is laid out");
    assert_eq!(fallback, "Loading…");
    assert!(
        rt.find("data-sync-window-loading-spinner-indicator").is_some(),
        "the spinner indicator is drawn beside the text"
    );
}

#[test]
fn audit_every_element_stays_inside_the_narrow_window() {
    for section in [false, true] {
        let rt = skeleton_runtime(section, NARROW);
        let (width, height) = NARROW;
        for element in rt.elements() {
            let [x, y, w, h] = element.rect;
            let key = element.key.clone().unwrap_or_else(|| "<unnamed>".to_owned());
            assert!(
                x >= -0.5 && y >= -0.5 && x + w <= width + 0.5 && y + h <= height + 0.5,
                "{key} rect [{x}, {y}, {w}, {h}] leaves {width}x{height}"
            );
        }
    }
}
