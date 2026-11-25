//! T348 independent audit (tester): fresh vectors — the delete checkbox's
//! both directions under a live options model, an already-selected policy
//! display, the mount with every option off, and the warning's absence
//! for the unset policy with the select still present.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::views::{OptionsBar, SyncConflictPolicy, SyncOptions};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

#[derive(Clone, Default)]
struct Scene {
    options: SyncOptions,
    changed: Vec<SyncOptions>,
    enable_delete_calls: usize,
    policy_open: bool,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        OptionsBar::<Scene>::new(scene.options.clone())
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

fn base() -> SyncOptions {
    SyncOptions {
        insert: true,
        update: true,
        delete: false,
        conflict_policy: None,
        ..Default::default()
    }
}

#[test]
fn checking_delete_twice_asks_twice_and_writes_never() {
    let mut rt = mount(Scene::default(), (960.0, 640.0));
    click(&mut rt, "data-sync-option-delete");
    // The model still says false: the confirm dialog is the only writer.
    assert!(!rt.state.options.delete);
    click(&mut rt, "data-sync-option-delete");
    assert_eq!(rt.state.enable_delete_calls, 2);
    assert!(rt.state.changed.is_empty());
}

#[test]
fn the_delete_round_trip_confirms_then_unchecks() {
    // Host applied the confirmation (delete: true); unchecking writes false.
    let mut scene = Scene {
        options: SyncOptions {
            delete: true,
            ..base()
        },
        ..Scene::default()
    };
    let mut rt = mount(scene.clone(), (960.0, 640.0));
    click(&mut rt, "data-sync-option-delete");
    assert_eq!(rt.state.changed.len(), 1);
    assert!(!rt.state.changed[0].delete);
    assert_eq!(rt.state.enable_delete_calls, 0);
    // And checking again starts over from false: confirmation again.
    scene.options = rt.state.changed[0].clone();
    let mut rt = mount(scene, (960.0, 640.0));
    click(&mut rt, "data-sync-option-delete");
    assert_eq!(rt.state.enable_delete_calls, 1);
    assert_eq!(rt.state.changed.len(), 0);
}

#[test]
fn every_option_off_still_mounts_the_full_bar() {
    let scene = Scene {
        options: SyncOptions {
            insert: false,
            update: false,
            delete: false,
            conflict_policy: None,
            ..Default::default()
        },
        ..Scene::default()
    };
    let rt = mount(scene, (960.0, 640.0));
    for key in [
        "data-sync-option-insert",
        "data-sync-option-update",
        "data-sync-option-delete",
        "data-sync-conflict-policy-trigger",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
}

#[test]
fn a_skip_policy_keeps_the_select_and_its_warning_side_by_side() {
    let scene = Scene {
        options: SyncOptions {
            conflict_policy: Some(SyncConflictPolicy::Skip),
            ..base()
        },
        ..Scene::default()
    };
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, "data-sync-conflict-policy-trigger"));
    assert!(has_key(&rt, "data-sync-options-bar-policy-warning"));
    // The warning sits to the right of the select (a later sibling).
    let select = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-conflict-policy-trigger") && !e.exiting)
        .expect("select mounted");
    let warning = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-options-bar-policy-warning") && !e.exiting)
        .expect("warning mounted");
    assert!(
        warning.rect[0] >= select.rect[0] + select.rect[2] - 1.0,
        "the warning follows the select in the row"
    );
}

#[test]
fn the_unset_policy_defaults_the_trigger_display_to_abort() {
    let scene = Scene::default();
    let rt = mount(scene, (960.0, 640.0));
    // `options.conflictPolicy ?? 'abort'`: the trigger shows the abort
    // label without writing the option.
    let trigger = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-conflict-policy-trigger") && !e.exiting)
        .expect("the trigger is mounted");
    let [tx, ty, tw, th] = trigger.rect;
    let shown = rt
        .elements()
        .into_iter()
        .filter(|e| e.role == "text" && !e.exiting)
        .find(|e| {
            let [x, y, w, h] = e.rect;
            x >= tx - 0.5 && x + w <= tx + tw + 0.5 && y >= ty - 0.5 && y + h <= ty + th + 0.5
        })
        .and_then(|e| e.text)
        .unwrap_or_default();
    assert_eq!(shown, datazen_ui::i18n::t("sync.conflictPolicyAbort"));
    assert!(!has_key(&rt, "data-sync-options-bar-policy-warning"));
}
