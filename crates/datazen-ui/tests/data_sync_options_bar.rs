//! T348: `windows/data-sync/OptionsBar.tsx` port — the three operation
//! checkboxes (the delete one's confirm-first / uncheck-direct split), the
//! conflict-policy select with its warnings, the hint, and geometry.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::{OptionsBar, SyncConflictPolicy, SyncOptions};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

#[derive(Default)]
struct Scene {
    options: Option<SyncOptions>,
    changed: Vec<SyncOptions>,
    enable_delete_calls: usize,
    policy_open: bool,
    dark: bool,
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

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let options = scene.options.clone().unwrap_or_else(options);
    col().w_full().h_full().child(
        OptionsBar::<Scene>::new(options)
            .policy_select_open(scene.policy_open)
            .on_toggle_policy(|s| s.policy_open = true)
            .on_close_policy(|s| s.policy_open = false)
            .on_change(|s, next| {
                s.options = Some(next.clone());
                s.changed.push(next);
            })
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
fn the_bar_mounts_its_five_controls_and_the_hint() {
    let rt = mount(Scene::default(), (960.0, 640.0));
    for key in [
        "data-sync-options-bar-title",
        "data-sync-option-insert",
        "data-sync-option-update",
        "data-sync-option-delete",
        "data-sync-conflict-policy-trigger",
        "data-sync-options-bar-hint",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(text_of(&rt, "data-sync-options-bar-title"), t("sync.optionsTitle"));
    assert_eq!(text_of(&rt, "data-sync-options-bar-hint"), t("sync.optionsHint"));
    assert!(!has_key(&rt, "data-sync-options-bar-policy-warning"));
}

#[test]
fn toggling_insert_and_update_writes_the_options() {
    let mut rt = mount(Scene::default(), (960.0, 640.0));
    click(&mut rt, "data-sync-option-insert");
    assert_eq!(rt.state.changed.len(), 1);
    assert!(!rt.state.changed[0].insert, "insert toggled off");
    assert!(rt.state.changed[0].update);
    click(&mut rt, "data-sync-option-update");
    assert_eq!(rt.state.changed.len(), 2);
    assert!(!rt.state.changed[1].update);
}

#[test]
fn checking_delete_asks_for_the_confirmation_instead_of_writing() {
    let mut rt = mount(Scene::default(), (960.0, 640.0));
    click(&mut rt, "data-sync-option-delete");
    assert_eq!(rt.state.enable_delete_calls, 1, "the confirm dialog is requested");
    assert!(
        rt.state.changed.is_empty(),
        "the option itself is never flipped by a check"
    );
}

#[test]
fn unchecking_delete_writes_false_directly() {
    let mut scene = Scene::default();
    scene.options = Some(SyncOptions {
        delete: true,
        ..options()
    });
    let mut rt = mount(scene, (960.0, 640.0));
    click(&mut rt, "data-sync-option-delete");
    assert_eq!(rt.state.changed.len(), 1);
    assert!(!rt.state.changed[0].delete);
    assert_eq!(rt.state.enable_delete_calls, 0, "no confirmation on the way out");
}

#[test]
fn a_skip_policy_mounts_its_warning() {
    let mut scene = Scene::default();
    scene.options = Some(SyncOptions {
        conflict_policy: Some(SyncConflictPolicy::Skip),
        ..options()
    });
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, "data-sync-options-bar-policy-warning"));
    assert_eq!(
        text_of(&rt, "data-sync-options-bar-policy-warning"),
        t("sync.conflictPolicySkipWarning")
    );
}

#[test]
fn a_force_policy_mounts_its_warning() {
    let mut scene = Scene::default();
    scene.options = Some(SyncOptions {
        conflict_policy: Some(SyncConflictPolicy::Force),
        ..options()
    });
    let rt = mount(scene, (960.0, 640.0));
    assert_eq!(
        text_of(&rt, "data-sync-options-bar-policy-warning"),
        t("sync.conflictPolicyForceWarning")
    );
}

#[test]
fn an_abort_or_unset_policy_mounts_no_warning() {
    let mut scene = Scene::default();
    scene.options = Some(SyncOptions {
        conflict_policy: Some(SyncConflictPolicy::Abort),
        ..options()
    });
    let rt = mount(scene, (960.0, 640.0));
    assert!(!has_key(&rt, "data-sync-options-bar-policy-warning"));
}

#[test]
fn the_bar_fits_both_window_widths() {
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
fn the_light_theme_mounts_the_same_bar() {
    let mut scene = Scene::default();
    scene.dark = false;
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, "data-sync-option-insert"));
    assert!(has_key(&rt, "data-sync-conflict-policy-trigger"));
}
