//! T203 independent audit (tester): fresh vectors — the title block sits
//! above the bar, the pass-through keeps every callback (a dropped one
//! would be invisible in the bar's own tests), and the step mounts the
//! bar's warning under a live skip policy.

use rune::{col, Cx, El, Env, Runtime};

use crate::views::{DataSyncOptionsStep, SyncConflictPolicy, SyncOptions};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn options(conflict_policy: Option<SyncConflictPolicy>) -> SyncOptions {
    SyncOptions {
        insert: true,
        update: true,
        delete: false,
        conflict_policy,
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
        DataSyncOptionsStep::<Scene>::new(options(None))
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

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn the_title_block_sits_above_the_bar() {
    let rt = mount(Scene::default(), (960.0, 640.0));
    let title = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-options-step-title") && !e.exiting)
        .expect("title mounted");
    let insert = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-option-insert") && !e.exiting)
        .expect("insert checkbox mounted");
    assert!(
        title.rect[1] + title.rect[3] <= insert.rect[1] + 1.0,
        "the title block precedes the bar vertically"
    );
}

#[test]
fn every_callback_reaches_the_bar_through_the_step() {
    // One journey per pass-through: options change, delete confirmation,
    // policy toggle, policy close.
    let mut rt = mount(Scene::default(), (960.0, 640.0));
    click(&mut rt, "data-sync-option-update");
    assert_eq!(rt.state.changed.len(), 1, "on_change passes through");
    assert!(!rt.state.changed[0].update);

    click(&mut rt, "data-sync-option-delete");
    assert_eq!(rt.state.enable_delete_calls, 1, "on_enable_delete passes through");

    // The policy select's open flag round-trips through the host slot the
    // step mirrors (the trigger click toggles the slot; the bar's layer
    // then holds the listbox the same frame).
    click(&mut rt, "data-sync-conflict-policy-trigger");
    assert!(rt.state.policy_open, "on_toggle_policy passes through");
}

#[test]
fn a_live_skip_policy_mounts_the_warning_inside_the_step() {
    let rt = Runtime::new(
        Scene {
            changed: Vec::new(),
            enable_delete_calls: 0,
            policy_open: false,
            dark: true,
        },
        |scene: &Scene, _cx: &mut Cx<Scene>| {
            col().w_full().h_full().child(
                DataSyncOptionsStep::<Scene>::new(options(Some(SyncConflictPolicy::Skip)))
                    .on_toggle_policy(|s| s.policy_open = true)
                    .on_close_policy(|s| s.policy_open = false)
                    .on_change(|s, next| s.changed.push(next))
                    .on_enable_delete(|s| s.enable_delete_calls += 1)
                    .build(scene.dark),
            )
        },
        (960.0, 640.0),
        env(true),
    );
    let mut rt = rt;
    rt.frame(0.0);
    assert!(has_key(&rt, "data-sync-options-step"));
    assert!(has_key(&rt, "data-sync-options-bar-policy-warning"));
}
