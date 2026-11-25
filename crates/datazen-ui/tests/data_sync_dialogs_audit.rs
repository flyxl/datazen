//! T074 independent audit: `windows/data-sync/DataSyncDialogs.tsx` port.
//!
//! Gaps the coder tests leave open, checked fresh here:
//! - empty explain text (deviation 9, explain side; the coder only pinned
//!   the empty error message),
//! - all four dialogs open at once *and* their footers pressed in one
//!   session, in order (the coder only asserted they mount together),
//! - open-epoch restamping on a dialog other than the error dialog
//!   (the coder only pinned the error epoch),
//! - the header close button label (deviation 8: the Dialog base hardcodes
//!   "Close" where React localizes `common.close`),
//! - light theme with all four dialogs open (the coder only light-tested
//!   the error dialog),
//! - the error panel width (deviation 2: the Dialog default 576pt, the
//!   React `max-w-xl` figure).

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::views::{DataSyncDialogs, DataSyncDialogsVm};

#[derive(Default)]
struct Scene {
    vm: DataSyncDialogsVm,
    calls: Vec<&'static str>,
    dark: bool,
}

impl Scene {
    fn with(vm: DataSyncDialogsVm) -> Self {
        Self {
            vm,
            dark: true,
            ..Self::default()
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        DataSyncDialogs::<Scene>::new(scene.vm.clone())
            .on_close_error(|s: &mut Scene| s.calls.push("close-error"))
            .on_close_delete_confirm(|s: &mut Scene| s.calls.push("close-delete"))
            .on_confirm_enable_delete(|s: &mut Scene| s.calls.push("enable-delete"))
            .on_close_explain(|s: &mut Scene| s.calls.push("close-explain"))
            .on_close_execute_confirm(|s: &mut Scene| s.calls.push("close-execute"))
            .on_confirm_execute(|s: &mut Scene| s.calls.push("confirm-execute"))
            .build(scene.dark),
    )
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(
        scene,
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key)
        .and_then(|e| e.label)
        .unwrap_or_else(|| panic!("{key} has a label"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn all_open_vm() -> DataSyncDialogsVm {
    DataSyncDialogsVm {
        error_open: true,
        error_msg: "target is read-only".to_string(),
        delete_confirm_open: true,
        explain_open: true,
        explain_text: "3 inserts, 1 delete".to_string(),
        execute_confirm_open: true,
        ..DataSyncDialogsVm::default()
    }
}

#[test]
fn empty_explain_text_mounts_empty_result() {
    // React renders `<p data-testid="data-sync-explain-result">` with an
    // empty string; the dialog still mounts its result node and close
    // footer, with no spinner.
    let vm = DataSyncDialogsVm {
        explain_open: true,
        ..DataSyncDialogsVm::default()
    };
    let rt = mount(Scene::with(vm), (800.0, 600.0));
    assert!(has_key(&rt, "data-sync-explain-result"));
    assert_eq!(text_of(&rt, "data-sync-explain-result"), "");
    assert!(!has_key(&rt, "data-sync-explain-loading"));
    assert!(!has_key(&rt, "data-sync-explain-spinner"));
    assert!(has_key(&rt, "data-sync-explain-close"));
}

#[test]
fn all_open_topmost_dialog_captures_footer_clicks() {
    // React renders each Dialog as `fixed inset-0 z-50` with a full-screen
    // scrim (packages/ui/src/Dialog.tsx:146-147), so with all four open the
    // last in DOM order — the execute-confirm — paints on top and its
    // scrim/panel intercepts pointer events to the three dialogs below.
    // The same stacking must hold in Rune: a press aimed at a lower
    // dialog's footer is captured by the topmost dialog instead.
    let mut rt = mount(Scene::with(all_open_vm()), (800.0, 600.0));
    click(&mut rt, "data-sync-delete-confirm-enable");
    click(&mut rt, "data-sync-explain-close");
    click(&mut rt, "data-sync-error-ok");
    assert_eq!(
        rt.state.calls,
        vec!["confirm-execute", "confirm-execute", "confirm-execute"]
    );
    // All four stay mounted: the open flags live in the (unchanged) view
    // model, exactly as React keeps all four mounted until the host flips
    // its own flags.
    for key in [
        "data-sync-error-dialog",
        "data-sync-delete-confirm-dialog",
        "data-sync-explain-dialog",
        "data-sync-execute-confirm-dialog",
    ] {
        assert!(has_key(&rt, key), "{key} must still mount");
    }
    // The topmost dialog's own footer is reachable and records its call.
    let mut rt = mount(Scene::with(all_open_vm()), (800.0, 600.0));
    click(&mut rt, "data-sync-confirm-execute");
    assert_eq!(rt.state.calls, vec!["confirm-execute"]);
}

#[test]
fn execute_confirm_epoch_restamps_body_content_key() {
    let vm = DataSyncDialogsVm {
        execute_confirm_open: true,
        execute_confirm_epoch: 7,
        ..DataSyncDialogsVm::default()
    };
    let rt = mount(Scene::with(vm), (800.0, 600.0));
    assert!(has_key(
        &rt,
        "data-sync-execute-confirm-dialog-content-7"
    ));
    assert!(!has_key(
        &rt,
        "data-sync-execute-confirm-dialog-content-0"
    ));
}

#[test]
fn header_close_button_label_is_close() {
    // Deviation 8, pinned: the Dialog base hardcodes the English "Close"
    // (ui/dialog.rs), where React localizes `common.close`
    // (components/ui/Dialog.tsx: `closeLabel ?? t('common.close')`).
    // In the English locale the two agree; other locales diverge at the
    // base, not in this slice.
    let vm = DataSyncDialogsVm {
        error_open: true,
        error_msg: "boom".to_string(),
        ..DataSyncDialogsVm::default()
    };
    let rt = mount(Scene::with(vm), (800.0, 600.0));
    assert_eq!(label_of(&rt, "data-sync-error-dialog-close"), "Close");
}

#[test]
fn light_theme_all_four_dialogs_mount() {
    let mut scene = Scene::with(all_open_vm());
    scene.dark = false;
    let rt = mount(scene, (800.0, 600.0));
    for key in [
        "data-sync-error-dialog",
        "data-sync-delete-confirm-dialog",
        "data-sync-explain-dialog",
        "data-sync-execute-confirm-dialog",
    ] {
        assert!(has_key(&rt, key), "{key} must mount in light theme");
    }
}

#[test]
fn error_dialog_panel_width_is_576() {
    // Deviation 2: the Dialog default width is 576pt, React's `max-w-xl`.
    let vm = DataSyncDialogsVm {
        error_open: true,
        error_msg: "boom".to_string(),
        ..DataSyncDialogsVm::default()
    };
    let rt = mount(Scene::with(vm), (800.0, 600.0));
    let panel = maybe(&rt, "data-sync-error-dialog").expect("error panel mounts");
    assert!(
        (panel.rect[2] - 576.0).abs() < 0.5,
        "error panel width is {}, expected 576",
        panel.rect[2]
    );
}
