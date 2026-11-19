//! T074: `windows/data-sync/DataSyncDialogs.tsx` port — the four Data Sync
//! dialogs (error, delete-enable confirm, explain-diff, execute confirm):
//! open/close mounting, titles and bodies, footer wiring, the explain
//! loading/result branches, and the open-epoch body identity.

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::{DataSyncDialogs, DataSyncDialogsVm};

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

/// The same dialogs with no host callbacks wired: every press must be
/// swallowed by the no-op fallback, never panic.
fn view_bare(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col()
        .w_full()
        .h_full()
        .child(DataSyncDialogs::<Scene>::new(scene.vm.clone()).build(scene.dark))
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    mount_with(scene, size, view)
}

fn mount_with(
    scene: Scene,
    size: (f32, f32),
    view: fn(&Scene, &mut Cx<Scene>) -> El,
) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
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

/// A kit button's accessible label: the button element itself carries it
/// (`ElementInfo.label`); its visible text lives on a `label` child.
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

fn error_vm() -> DataSyncDialogsVm {
    DataSyncDialogsVm {
        error_open: true,
        error_msg: "target is read-only".to_string(),
        ..DataSyncDialogsVm::default()
    }
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

const DIALOG_KEYS: [&str; 4] = [
    "data-sync-error-dialog",
    "data-sync-delete-confirm-dialog",
    "data-sync-explain-dialog",
    "data-sync-execute-confirm-dialog",
];

fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let Some(key) = e.key.as_deref() else {
            continue;
        };
        if !key.starts_with("data-sync-") {
            continue;
        }
        let [x, _y, ew, _eh] = e.rect;
        if x < -0.5 || x + ew > w + 0.5 {
            outside.push(format!("{key} {:?}", e.rect));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of {w}x{h}: {outside:?}",
        outside.len()
    );
}

// ---- mounting: closed dialogs mount nothing ----

#[test]
fn all_closed_mounts_only_the_fragment_root() {
    let rt = mount(Scene::with(DataSyncDialogsVm::default()), (800.0, 600.0));
    assert!(has_key(&rt, "data-sync-dialogs"));
    for key in DIALOG_KEYS {
        assert!(!has_key(&rt, key), "{key} must not mount while closed");
    }
    for key in [
        "data-sync-error-ok",
        "data-sync-delete-confirm-enable",
        "data-sync-explain-close",
        "data-sync-confirm-execute",
    ] {
        assert!(!has_key(&rt, key), "{key} must not mount while closed");
    }
}

// ---- error dialog ----

#[test]
fn error_dialog_shows_message_and_ok_closes() {
    let mut rt = mount(Scene::with(error_vm()), (800.0, 600.0));
    assert!(has_key(&rt, "data-sync-error-dialog"));
    assert_eq!(
        text_of(&rt, "data-sync-error-dialog-title"),
        t("common.hint")
    );
    assert_eq!(text_of(&rt, "data-sync-error-text"), "target is read-only");
    assert_eq!(label_of(&rt, "data-sync-error-ok"), t("common.ok"));
    // The other three dialogs stay closed.
    for key in DIALOG_KEYS[1..].iter() {
        assert!(!has_key(&rt, key), "{key} must not mount");
    }
    click(&mut rt, "data-sync-error-ok");
    assert_eq!(rt.state.calls, vec!["close-error"]);
}

#[test]
fn error_dialog_header_close_also_closes() {
    let mut rt = mount(Scene::with(error_vm()), (800.0, 600.0));
    click(&mut rt, "data-sync-error-dialog-close");
    assert_eq!(rt.state.calls, vec!["close-error"]);
}

#[test]
fn error_dialog_renders_an_empty_message() {
    // React renders `<CopyableError message="">` as an empty alert; the
    // dialog still mounts with its OK footer.
    let vm = DataSyncDialogsVm {
        error_open: true,
        ..DataSyncDialogsVm::default()
    };
    let rt = mount(Scene::with(vm), (800.0, 600.0));
    assert!(has_key(&rt, "data-sync-error-dialog"));
    assert_eq!(text_of(&rt, "data-sync-error-text"), "");
    assert!(has_key(&rt, "data-sync-error-ok"));
}

// ---- delete-confirm dialog ----

#[test]
fn delete_confirm_enable_and_cancel_record_distinct_calls() {
    let vm = DataSyncDialogsVm {
        delete_confirm_open: true,
        ..DataSyncDialogsVm::default()
    };
    let mut rt = mount(Scene::with(vm), (800.0, 600.0));
    assert!(has_key(&rt, "data-sync-delete-confirm-dialog"));
    assert_eq!(
        text_of(&rt, "data-sync-delete-confirm-dialog-title"),
        t("sync.deleteConfirmTitle")
    );
    assert_eq!(
        text_of(&rt, "data-sync-delete-confirm-body"),
        t("sync.deleteConfirmBody")
    );
    assert_eq!(
        label_of(&rt, "data-sync-delete-confirm-cancel"),
        t("common.cancel")
    );
    assert_eq!(
        label_of(&rt, "data-sync-delete-confirm-enable"),
        t("sync.enableDelete")
    );
    click(&mut rt, "data-sync-delete-confirm-enable");
    click(&mut rt, "data-sync-delete-confirm-cancel");
    assert_eq!(rt.state.calls, vec!["enable-delete", "close-delete"]);
}

#[test]
fn delete_confirm_header_close_closes() {
    let vm = DataSyncDialogsVm {
        delete_confirm_open: true,
        ..DataSyncDialogsVm::default()
    };
    let mut rt = mount(Scene::with(vm), (800.0, 600.0));
    click(&mut rt, "data-sync-delete-confirm-dialog-close");
    assert_eq!(rt.state.calls, vec!["close-delete"]);
}

// ---- explain dialog ----

#[test]
fn explain_loading_shows_spinner_not_result() {
    let vm = DataSyncDialogsVm {
        explain_open: true,
        explain_loading: true,
        explain_text: "stale result".to_string(),
        ..DataSyncDialogsVm::default()
    };
    let rt = mount(Scene::with(vm), (800.0, 600.0));
    assert!(has_key(&rt, "data-sync-explain-dialog"));
    assert_eq!(
        text_of(&rt, "data-sync-explain-dialog-title"),
        t("sync.explainDiffTitle")
    );
    assert!(has_key(&rt, "data-sync-explain-loading"));
    assert!(has_key(&rt, "data-sync-explain-spinner"));
    assert_eq!(
        text_of(&rt, "data-sync-explain-loading-text"),
        t("sync.executing")
    );
    // While loading the stale text is not shown, exactly as React's
    // ternary renders the spinner branch instead of the result.
    assert!(!has_key(&rt, "data-sync-explain-result"));
}

#[test]
fn explain_loaded_shows_result_with_newlines_not_spinner() {
    let vm = DataSyncDialogsVm {
        explain_open: true,
        explain_text: "line one\nline two".to_string(),
        ..DataSyncDialogsVm::default()
    };
    let mut rt = mount(Scene::with(vm), (800.0, 600.0));
    assert!(has_key(&rt, "data-sync-explain-result"));
    assert_eq!(
        text_of(&rt, "data-sync-explain-result"),
        "line one\nline two"
    );
    assert!(!has_key(&rt, "data-sync-explain-loading"));
    assert!(!has_key(&rt, "data-sync-explain-spinner"));
    assert_eq!(label_of(&rt, "data-sync-explain-close"), t("common.close"));
    click(&mut rt, "data-sync-explain-close");
    assert_eq!(rt.state.calls, vec!["close-explain"]);
}

#[test]
fn explain_header_close_closes() {
    // The footer close button calls the cloned handler directly; only the
    // header close (and Escape) runs the dialog's own `on_close` closure.
    let vm = DataSyncDialogsVm {
        explain_open: true,
        explain_loading: true,
        ..DataSyncDialogsVm::default()
    };
    let mut rt = mount(Scene::with(vm), (800.0, 600.0));
    click(&mut rt, "data-sync-explain-dialog-close");
    assert_eq!(rt.state.calls, vec!["close-explain"]);
}

// ---- execute-confirm dialog ----

#[test]
fn execute_confirm_execute_and_cancel_record_distinct_calls() {
    let vm = DataSyncDialogsVm {
        execute_confirm_open: true,
        ..DataSyncDialogsVm::default()
    };
    let mut rt = mount(Scene::with(vm), (800.0, 600.0));
    assert!(has_key(&rt, "data-sync-execute-confirm-dialog"));
    assert_eq!(
        text_of(&rt, "data-sync-execute-confirm-dialog-title"),
        t("sync.executeDeleteTitle")
    );
    assert_eq!(
        text_of(&rt, "data-sync-execute-confirm-body"),
        t("sync.executeDeleteBody")
    );
    assert_eq!(
        label_of(&rt, "data-sync-execute-confirm-cancel"),
        t("common.cancel")
    );
    assert_eq!(
        label_of(&rt, "data-sync-confirm-execute"),
        t("sync.execute")
    );
    click(&mut rt, "data-sync-confirm-execute");
    click(&mut rt, "data-sync-execute-confirm-cancel");
    assert_eq!(rt.state.calls, vec!["confirm-execute", "close-execute"]);
}

#[test]
fn execute_confirm_header_close_closes() {
    // Same as the explain dialog: only the header close (and Escape) runs
    // the dialog's own `on_close` closure; the footer cancel calls the
    // cloned handler directly.
    let vm = DataSyncDialogsVm {
        execute_confirm_open: true,
        ..DataSyncDialogsVm::default()
    };
    let mut rt = mount(Scene::with(vm), (800.0, 600.0));
    click(&mut rt, "data-sync-execute-confirm-dialog-close");
    assert_eq!(rt.state.calls, vec!["close-execute"]);
}

// ---- all four at once, epochs, themes, geometry ----

#[test]
fn all_four_dialogs_can_be_open_at_once() {
    let rt = mount(Scene::with(all_open_vm()), (800.0, 600.0));
    for key in DIALOG_KEYS {
        assert!(has_key(&rt, key), "{key} must mount");
    }
    assert!(has_key(&rt, "data-sync-error"));
    assert!(has_key(&rt, "data-sync-explain-result"));
}

#[test]
fn open_epoch_restamps_the_dialog_body_content_key() {
    let vm = DataSyncDialogsVm {
        error_open: true,
        error_msg: "boom".to_string(),
        error_epoch: 3,
        ..DataSyncDialogsVm::default()
    };
    let rt = mount(Scene::with(vm), (800.0, 600.0));
    assert!(has_key(&rt, "data-sync-error-dialog-content-3"));
    assert!(!has_key(&rt, "data-sync-error-dialog-content-0"));

    // The default epoch is 0, so a dialog the host never bumped still gets
    // the stable first-generation body.
    let rt = mount(Scene::with(error_vm()), (800.0, 600.0));
    assert!(has_key(&rt, "data-sync-error-dialog-content-0"));
}

#[test]
fn light_theme_mounts_the_error_dialog() {
    let mut scene = Scene::with(error_vm());
    scene.dark = false;
    let mut rt = mount(scene, (800.0, 600.0));
    assert!(has_key(&rt, "data-sync-error-dialog"));
    assert_eq!(text_of(&rt, "data-sync-error-text"), "target is read-only");
    click(&mut rt, "data-sync-error-ok");
    assert_eq!(rt.state.calls, vec!["close-error"]);
}

#[test]
fn all_open_dialogs_fit_in_a_small_window() {
    let rt = mount(Scene::with(all_open_vm()), (480.0, 360.0));
    assert_no_horizontal_overflow(&rt, 480.0, 360.0, "all open, 480x360");
    for key in DIALOG_KEYS {
        assert!(has_key(&rt, key), "{key} must mount");
    }
}

// ---- no host callbacks: presses are swallowed, never panic ----

#[test]
fn presses_without_callbacks_are_swallowed() {
    let mut rt = mount_with(Scene::with(all_open_vm()), (800.0, 600.0), view_bare);
    for key in [
        "data-sync-error-ok",
        "data-sync-delete-confirm-enable",
        "data-sync-delete-confirm-cancel",
        "data-sync-explain-close",
        "data-sync-confirm-execute",
        "data-sync-execute-confirm-cancel",
    ] {
        click(&mut rt, key);
    }
    assert!(rt.state.calls.is_empty());
    // The dialogs stay mounted: a no-op close does not unmount anything,
    // because the open flags live in the (unchanged) view model.
    for key in DIALOG_KEYS {
        assert!(has_key(&rt, key), "{key} must still mount");
    }
}
