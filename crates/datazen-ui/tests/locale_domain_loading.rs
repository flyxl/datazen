//! T140: the LocaleDomainLoading skeleton — both variants render the
//! status role, the 20pt spinner and the hardcoded fallback text (no
//! translation key is read), and stay inside the window.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::locale_domain_loading::{
    LocaleDomainLoading, LocaleDomainLoadingVariant,
};

const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Scene {
    #[default]
    Page,
    Section,
}

fn view(state: &Scene, _cx: &mut Cx<Scene>) -> El {
    let dark = _cx.system_dark();
    let variant = match state {
        Scene::Page => LocaleDomainLoadingVariant::Page,
        Scene::Section => LocaleDomainLoadingVariant::Section,
    };
    col()
        .key("page")
        .w_full()
        .h_full()
        .child(
            LocaleDomainLoading::new("test-domain-loading")
                .variant(variant)
                .build(dark),
        )
}

fn runtime(state: Scene, dark: bool) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        state,
        view,
        DESIGN,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn element_text(rt: &Runtime<Scene>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
}

#[test]
fn the_page_variant_fills_the_window_height() {
    let rt = runtime(Scene::Page, true);
    let root = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("test-domain-loading"))
        .expect("the skeleton is laid out");
    let [_, y, _, h] = root.rect;
    assert!(
        (h - (DESIGN.1)).abs() < 0.5,
        "the page variant is h-screen: {h}"
    );
    assert!((y).abs() < 0.5, "it starts at the top: {y}");
}

#[test]
fn the_section_variant_is_the_bounded_region() {
    let rt = runtime(Scene::Section, true);
    let root = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("test-domain-loading"))
        .expect("the skeleton is laid out");
    let [_, _, _, h] = root.rect;
    assert!((h - 128.0).abs() < 0.5, "the section variant is min-h-8rem: {h}");
}

#[test]
fn the_skeleton_is_a_status_with_the_fallback_text_and_the_spinner() {
    for scene in [Scene::Page, Scene::Section] {
        let rt = runtime(scene, true);
        let root = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("test-domain-loading"))
            .expect("the skeleton is laid out");
        assert_eq!(root.role, "status", "the a11y role survives the port");
        assert_eq!(
            element_text(&rt, "test-domain-loading-fallback").as_deref(),
            Some("Loading…"),
            "the fallback text is hardcoded, no translation key is read"
        );
        assert!(
            rt.find("test-domain-loading-spinner-indicator").is_some(),
            "the spinner indicator is drawn"
        );
    }
}

#[test]
fn both_variants_stay_inside_the_window_in_both_themes() {
    for scene in [Scene::Page, Scene::Section] {
        for dark in [true, false] {
            let rt = runtime(scene, dark);
            let (width, height) = DESIGN;
            for element in rt.elements() {
                let [x, y, w, h] = element.rect;
                let key = element.key.clone().unwrap_or_else(|| "<unnamed>".to_owned());
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= width + 0.5 && y + h <= height + 0.5,
                    "{key} rect [{x}, {y}, {w}, {h}] leaves {width}x{height} (dark={dark})"
                );
            }
        }
    }
}
