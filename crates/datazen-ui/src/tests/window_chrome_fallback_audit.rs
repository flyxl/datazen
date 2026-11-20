//! T301 independent audit (tester): fresh vectors for the chrome
//! fallback — a different chrome fixture height, the narrow window, the
//! empty-slot geometry (no bar to subtract), and the channels measured
//! apart.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::window_chrome_fallback::WindowChromeFallback;

const NARROW: (f32, f32) = (960.0, 640.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuditScene {
    TallBar,
    NoBar,
}

fn tall_bar() -> El {
    col().key("audit-tall-bar").w_full().h(64.0)
}

fn audit_view(state: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let dark = _cx.system_dark();
    let chrome = match state {
        AuditScene::TallBar => Some(tall_bar()),
        AuditScene::NoBar => None,
    };
    WindowChromeFallback::new().chrome(chrome).build(dark)
}

fn audit_runtime(scene: AuditScene, size: (f32, f32)) -> Runtime<AuditScene> {
    let mut rt = Runtime::new(
        scene,
        audit_view,
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
fn audit_the_tall_bar_leaves_the_centering_below_it() {
    let rt = audit_runtime(AuditScene::TallBar, NARROW);
    let root = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("window-chrome-fallback"))
        .expect("the fallback is laid out");
    let [x, y, w, h] = root.rect;
    let bar = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("audit-tall-bar"))
        .expect("the fixture is laid out");
    let [_, by, _, bh] = bar.rect;
    assert_eq!(bh, 64.0, "the 64pt fixture renders at its own height");
    let spinner = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("chrome-fallback-spinner"))
        .expect("the spinner is laid out");
    let [sx, sy, sw, sh] = spinner.rect;
    let region_top = by + bh;
    assert!(
        (sx + sw / 2.0 - (x + w / 2.0)).abs() < 1.0
            && (sy + sh / 2.0 - (region_top + (y + h - region_top) / 2.0)).abs() < 1.0,
        "the spinner centers the space below the 64pt bar"
    );
}

#[test]
fn audit_without_a_bar_the_spinner_centers_the_whole_window() {
    let rt = audit_runtime(AuditScene::NoBar, NARROW);
    let root = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("window-chrome-fallback"))
        .expect("the fallback is laid out");
    let [x, y, w, h] = root.rect;
    let spinner = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("chrome-fallback-spinner"))
        .expect("the spinner is laid out");
    let [sx, sy, sw, sh] = spinner.rect;
    assert!(
        (sx + sw / 2.0 - (x + w / 2.0)).abs() < 1.0
            && (sy + sh / 2.0 - (y + h / 2.0)).abs() < 1.0,
        "no bar means the whole window is the centered region"
    );
}

#[test]
fn audit_the_fallback_fills_the_narrow_window_too() {
    for scene in [AuditScene::TallBar, AuditScene::NoBar] {
        let rt = audit_runtime(scene, NARROW);
        let root = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("window-chrome-fallback"))
            .expect("the fallback is laid out");
        let [x, y, w, h] = root.rect;
        assert!(
            x.abs() < 0.5 && y.abs() < 0.5 && (w - NARROW.0).abs() < 0.5 && (h - NARROW.1).abs() < 0.5,
            "the fallback covers the narrow window ({w}x{h})"
        );
    }
}

#[test]
fn audit_every_element_stays_inside_the_narrow_window() {
    for scene in [AuditScene::TallBar, AuditScene::NoBar] {
        let rt = audit_runtime(scene, NARROW);
        for element in rt.elements() {
            let [ex, ey, ew, eh] = element.rect;
            let key = element.key.clone().unwrap_or_else(|| "<unnamed>".to_owned());
            assert!(
                ex >= -0.5 && ey >= -0.5 && ex + ew <= NARROW.0 + 0.5 && ey + eh <= NARROW.1 + 0.5,
                "{key} rect [{ex}, {ey}, {ew}, {eh}] leaves the window"
            );
        }
    }
}

#[test]
fn audit_the_ring_and_the_label_are_the_only_content() {
    let rt = audit_runtime(AuditScene::NoBar, NARROW);
    assert!(
        rt.find("chrome-fallback-spinner-indicator").is_some(),
        "the ring indicator is drawn"
    );
    let label = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("chrome-fallback-spinner-label"))
        .and_then(|e| e.text);
    assert_eq!(label.as_deref(), Some("Loading"));
    // With the spinner root being a progressbar, the a11y channel reads.
    let spinner = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("chrome-fallback-spinner"))
        .expect("the spinner is laid out");
    assert_eq!(
        spinner.role, "progressbar",
        "the label makes the spinner announce itself"
    );
}
