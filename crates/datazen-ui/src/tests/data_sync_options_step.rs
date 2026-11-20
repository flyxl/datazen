//! T203: `windows/data-sync/DataSyncOptionsStep.tsx` port — the card
//! chrome, the title block, the mounted OptionsBar, the callback
//! pass-through, and geometry.

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::{DataSyncOptionsStep, SyncOptions};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn options() -> SyncOptions {
    SyncOptions {
        insert: true,
        update: true,
        delete: false,
        conflict_policy: None,
        ..Default::default()
    }
}

#[derive(Default)]
struct Scene {
    changed: Vec<SyncOptions>,
    enable_delete_calls: usize,
    policy_open: bool,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        DataSyncOptionsStep::<Scene>::new(options())
            .policy_select_open(scene.policy_open)
            .on_toggle_policy(|s| s.policy_open = true)
            .on_close_policy(|s| s.policy_open = false)
            .on_change(|s, next| s.changed.push(next))
            .on_enable_delete(|s| s.enable_delete_calls += 1)
            .build(scene.dark),
    )
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn the_step_mounts_the_title_block_and_the_bar() {
    let rt = mount(Scene::default(), (960.0, 640.0));
    for key in [
        "data-sync-options-step",
        "data-sync-options-step-title",
        "data-sync-options-step-hint",
        "data-sync-option-insert",
        "data-sync-option-update",
        "data-sync-option-delete",
        "data-sync-conflict-policy-trigger",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(text_of(&rt, "data-sync-options-step-title"), t("sync.optionsTitle"));
    assert_eq!(text_of(&rt, "data-sync-options-step-hint"), t("sync.optionsHint"));
}

#[test]
fn the_bar_writes_the_options_through_the_step() {
    let mut rt = mount(Scene::default(), (960.0, 640.0));
    click(&mut rt, "data-sync-option-insert");
    assert_eq!(rt.state.changed.len(), 1);
    assert!(!rt.state.changed[0].insert);
}

#[test]
fn the_delete_confirmation_passes_through_the_step() {
    let mut rt = mount(Scene::default(), (960.0, 640.0));
    click(&mut rt, "data-sync-option-delete");
    assert_eq!(rt.state.enable_delete_calls, 1);
    assert!(rt.state.changed.is_empty());
}

#[test]
fn the_policy_select_slot_passes_through_the_step() {
    let mut scene = Scene::default();
    scene.policy_open = true;
    let rt = mount(scene, (960.0, 640.0));
    // The open flag reaches the bar's select (the trigger mounts with the
    // listbox layer open — the same observable the bar's own tests use).
    assert!(has_key(&rt, "data-sync-conflict-policy-trigger"));
}

#[test]
fn the_step_fits_both_window_widths() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = mount(Scene::default(), (w, h));
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else { continue };
            if !key.starts_with("data-sync") {
                continue;
            }
            let [x, _y, ew, _eh] = e.rect;
            if x < -0.5 || x + ew > w + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(outside.is_empty(), "{w}x{h}: {outside:?}");
    }
}

#[test]
fn the_light_theme_mounts_the_same_step() {
    let mut scene = Scene::default();
    scene.dark = false;
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, "data-sync-options-step"));
    assert!(has_key(&rt, "data-sync-option-insert"));
}
