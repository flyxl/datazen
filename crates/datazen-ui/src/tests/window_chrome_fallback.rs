//! T301: the window chrome fallback — the registered chrome slot renders
//! above a centered announcing spinner; an unregistered slot renders no
//! bar; the fallback fills the window.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::window_chrome_fallback::WindowChromeFallback;

const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Scene {
    #[default]
    EmptySlot,
    RegisteredSlot,
}

fn chrome_bar() -> El {
    col().key("title-bar-fixture").w_full().h(28.0)
}

fn view(state: &Scene, _cx: &mut Cx<Scene>) -> El {
    let dark = _cx.system_dark();
    let chrome = match state {
        Scene::EmptySlot => None,
        Scene::RegisteredSlot => Some(chrome_bar()),
    };
    WindowChromeFallback::new().chrome(chrome).build(dark)
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

#[test]
fn the_fallback_fills_the_window() {
    for scene in [Scene::EmptySlot, Scene::RegisteredSlot] {
        let rt = runtime(scene, true);
        let root = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("window-chrome-fallback"))
            .expect("the fallback is laid out");
        let [_, _, w, h] = root.rect;
        assert!(
            (w - DESIGN.0).abs() < 0.5 && (h - DESIGN.1).abs() < 0.5,
            "h-screen fills the window: {w}x{h}"
        );
    }
}

#[test]
fn the_registered_slot_renders_and_the_empty_slot_does_not() {
    let with = runtime(Scene::RegisteredSlot, true);
    assert!(
        with.find("title-bar-fixture").is_some(),
        "the registered chrome renders at the top"
    );
    let without = runtime(Scene::EmptySlot, true);
    assert!(
        without.find("title-bar-fixture").is_none(),
        "an unregistered slot renders no bar"
    );
}

#[test]
fn the_spinner_announces_itself_with_the_literal_label() {
    let rt = runtime(Scene::EmptySlot, true);
    assert!(
        rt.find("chrome-fallback-spinner-indicator").is_some(),
        "the ring spinner is drawn"
    );
    let label = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("chrome-fallback-spinner-label"))
        .and_then(|e| e.text);
    assert_eq!(
        label.as_deref(),
        Some("Loading"),
        "the label is the literal, not a t() call"
    );
}

#[test]
fn the_spinner_is_centered_and_everything_stays_inside() {
    for dark in [true, false] {
        let rt = runtime(Scene::RegisteredSlot, dark);
        let root = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("window-chrome-fallback"))
            .expect("the fallback is laid out");
        let [x, y, w, h] = root.rect;
        let indicator = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("chrome-fallback-spinner"))
            .expect("the spinner is laid out");
        let [ix, iy, iw, ih] = indicator.rect;
        let center_x = ix + iw / 2.0;
        let center_y = iy + ih / 2.0;
        // The centering is the remaining space below the chrome bar
        // (React's flex-1 region), not the raw window.
        let bar = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("title-bar-fixture"))
            .expect("the chrome fixture is laid out");
        let [_, by, _w2, bh] = bar.rect;
        let region_top = by + bh;
        let region_h = y + h - region_top;
        assert!(
            (center_x - (x + w / 2.0)).abs() < 1.0
                && (center_y - (region_top + region_h / 2.0)).abs() < 1.0,
            "the spinner centers the region below the chrome (dark={dark})"
        );
        for element in rt.elements() {
            let [ex, ey, ew, eh] = element.rect;
            let key = element.key.clone().unwrap_or_else(|| "<unnamed>".to_owned());
            assert!(
                ex >= -0.5 && ey >= -0.5 && ex + ew <= DESIGN.0 + 0.5 && ey + eh <= DESIGN.1 + 0.5,
                "{key} rect [{ex}, {ey}, {ew}, {eh}] leaves the window (dark={dark})"
            );
        }
    }
}
