//! T022: the `InstallWappDialog` port — the two-step flow observed through
//! the real `crate::view` (the root modal layer mounts the dialog, exactly
//! as React's portal does), the zh-CN labels, the native pickers the two
//! browse presses open, the honest host seams, React's disabled gates, the
//! close/reset path, and the geometry at both window sizes.

use super::*;
use crate::i18n::t;
use crate::views::{InstallWappStep, WappManifestVm, WappPackagePreview};
use rune::{Key, KeyEvent, Mods};

fn element(rt: &Runtime<AppState>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<AppState>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

fn escape(rt: &mut Runtime<AppState>) {
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.frame(0.0);
    // The dismissed layer rides an exit transition; real time completes it
    // (a zero-delta frame holds the fade at its start).
    rt.frame(0.5);
}

fn dialog_runtime(state: AppState, size: (f32, f32), dark: bool) -> Runtime<AppState> {
    let mut rt = Runtime::new(
        state,
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

/// A runtime with the dialog open on the select step.
fn opened(size: (f32, f32)) -> Runtime<AppState> {
    let mut state = AppState::default();
    state.open_install_wapp_dialog();
    dialog_runtime(state, size, true)
}

/// One inspected package, per the fields the review step renders.
fn preview(
    permissions: &[&str],
    author: Option<&str>,
    description: Option<&str>,
) -> WappPackagePreview {
    WappPackagePreview {
        pick_token: "pick-7".into(),
        package_label: "datazen.demo.zip".into(),
        manifest: WappManifestVm {
            name: "Demo Wapp".into(),
            version: "1.2.3".into(),
            author: author.map(str::to_owned),
            description: description.map(str::to_owned),
            permissions: permissions.iter().map(|p| (*p).to_owned()).collect(),
        },
    }
}

/// Advance to the review step through the inspect seam's transition (the
/// call the future host wiring makes once the adapter answers).
fn reviewed(rt: &mut Runtime<AppState>) {
    rt.state.install_wapp_inspected(preview(
        &["context:connections", "storage:local"],
        Some("DataZen Labs"),
        Some("A demo package."),
    ));
    rt.settle_all();
    rt.frame(0.0);
}

#[test]
fn the_select_step_renders_reacts_structure_with_the_zh_table() {
    let rt = opened((960.0, 640.0));
    // The panel is the dialog, named by its title, at React's max-w-xl.
    let panel = element(&rt, "install-wapp-dialog");
    assert_eq!(panel.role, "dialog");
    assert_eq!(
        panel.label.as_deref(),
        Some(t("extensions.install.title")),
        "the panel carries the React dialog's accessible name"
    );
    assert!(
        (panel.rect[2] - 576.0).abs() < 0.5,
        "React's max-w-xl panel: {:?}",
        panel.rect
    );
    assert_eq!(
        element(&rt, "install-wapp-dialog-title").text.as_deref(),
        Some(t("extensions.install.title"))
    );
    // The description line under the title renders its zh-CN value.
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some(t("extensions.install.description"))),
        "the description renders the zh-CN value"
    );

    // The pick prompt and the two picks, in React's order.
    assert_eq!(
        element(&rt, "extension-install-prompt").text.as_deref(),
        Some(t("extensions.install.pickPrompt"))
    );
    let zip = element(&rt, "extension-install-browse-zip");
    assert_eq!(zip.role, "button");
    assert_eq!(
        zip.label.as_deref(),
        Some(t("extensions.install.browseZip")),
        "the zip pick reads the zh-CN table"
    );
    let folder = element(&rt, "extension-install-browse-folder");
    assert_eq!(
        folder.label.as_deref(),
        Some(t("extensions.install.browseFolder")),
        "the folder pick reads the zh-CN table"
    );
    assert!(zip.rect[0] < folder.rect[0], "zip precedes folder, as in React");
    // React's md buttons (h-9) sit gap-2 (8pt) apart.
    assert!(
        (zip.rect[3] - 36.0).abs() < 0.5 && (folder.rect[3] - 36.0).abs() < 0.5,
        "both picks are 36pt tall: zip {:?}, folder {:?}",
        zip.rect,
        folder.rect
    );
    assert!(
        (folder.rect[0] - (zip.rect[0] + zip.rect[2]) - 8.0).abs() < 0.5,
        "gap-2 between the picks: zip {:?}, folder {:?}",
        zip.rect,
        folder.rect
    );

    // The select footer's one button: the shared cancel.
    let cancel = element(&rt, "extension-install-cancel");
    assert_eq!(cancel.role, "button");
    assert_eq!(
        cancel.label.as_deref(),
        Some(t("common.cancel")),
        "the cancel reads the shared zh-CN table"
    );

    // No review content and no error before anything happened.
    assert!(!has_key(&rt, "extension-install-review"));
    assert!(!has_key(&rt, "extension-install-error"));
    assert!(!has_key(&rt, "extension-install-back"));
    assert!(!has_key(&rt, "extension-install-confirm"));
}

#[test]
fn the_pick_presses_open_the_native_pickers_and_the_seam_answers_the_error() {
    let mut rt = opened((960.0, 640.0));
    click(&mut rt, "extension-install-browse-zip");

    // The native picker: an Open dialog with the host's own zip filter
    // (`commands/wapps/mod.rs`: `("Wapp package", ["zip"])`).
    let (kind, options) = rt
        .file_dialog()
        .unwrap_or_else(|| panic!("the zip pick asked for a dialog"));
    assert_eq!(kind, rune::FileDialogKind::Open);
    assert_eq!(options.filters.len(), 1, "the zip pick offers one filter");
    assert_eq!(options.filters[0].0, "Wapp package");
    assert_eq!(options.filters[0].1, vec!["zip".to_owned()]);

    // While the pick is in flight the buttons swap to the inspecting label.
    let busy = element(&rt, "extension-install-browse-zip");
    assert_eq!(
        busy.label.as_deref(),
        Some(t("extensions.install.inspecting")),
        "the in-flight pick says so"
    );
    assert!(!busy.focusable, "an in-flight pick refuses another press");

    // A second pick while one is in flight is a silent no-op (React:
    // `if (inspecting || installing) return;`), not a second dialog.
    click(&mut rt, "extension-install-browse-folder");
    let (kind, _) = rt
        .file_dialog()
        .unwrap_or_else(|| panic!("the first pick is still the one open"));
    assert_eq!(kind, rune::FileDialogKind::Open);
    assert!(
        !has_key(&rt, "extension-install-error"),
        "the refused second press stays silent"
    );

    // The picked path hands the inspect seam the package: the host use case
    // is not wired (T073), so the answer is React's catch branch.
    rt.resolve_file_dialog(Some("/packages/datazen.demo.zip".into()))
        .unwrap_or_else(|error| panic!("the pick answers: {error}"));
    rt.frame(0.0);
    rt.frame(0.0);
    let error = element(&rt, "extension-install-error");
    assert_eq!(error.role, "alert", "the error branch is an alert");
    let sentence = element(&rt, "extension-install-error-text")
        .text
        .clone()
        .unwrap_or_default();
    assert!(
        sentence.contains("needs the live host"),
        "the seam names the missing host use case: {sentence}"
    );
    assert!(
        !sentence.contains("/packages/datazen.demo.zip"),
        "the picked path is not echoed into the error"
    );
    assert!(!rt.state.install_wapp_inspecting, "the finally cleared the mark");
    assert!(rt.state.install_wapp_preview.is_none());
    // The picks come back enabled with their own labels.
    assert_eq!(
        element(&rt, "extension-install-browse-zip").label.as_deref(),
        Some(t("extensions.install.browseZip"))
    );

    // The folder pick asks for the folder flavor instead.
    let mut rt = opened((960.0, 640.0));
    click(&mut rt, "extension-install-browse-folder");
    let (kind, options) = rt
        .file_dialog()
        .unwrap_or_else(|| panic!("the folder pick asked for a dialog"));
    assert_eq!(kind, rune::FileDialogKind::Folder);
    assert!(
        options.filters.is_empty(),
        "a folder pick carries no file filters"
    );
}

#[test]
fn a_cancelled_pick_stays_on_the_select_step_with_no_error() {
    let mut rt = opened((960.0, 640.0));
    click(&mut rt, "extension-install-browse-zip");
    rt.resolve_file_dialog(None)
        .unwrap_or_else(|error| panic!("the pick was cancelled: {error}"));
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(
        !has_key(&rt, "extension-install-error"),
        "React's `if (!preview) return` leaves no error"
    );
    assert!(has_key(&rt, "extension-install-prompt"));
    assert!(!rt.state.install_wapp_inspecting);
    assert_eq!(
        rt.state.install_wapp_step,
        InstallWappStep::Select,
        "the cancelled pick stays on the select step"
    );
}

#[test]
fn an_unavailable_picker_leaves_the_error_branch_and_clears_the_mark() {
    let mut rt = opened((960.0, 640.0));
    // A picker the desktop cannot show answers Err — React's catch.
    rt.state.install_wapp_inspecting = true;
    rt.state.answer_install_wapp_pick(Err(rune::FileDialogError(
        "the desktop has no portal".into(),
    )));
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "extension-install-error").role,
        "alert",
        "the error branch is an alert"
    );
    let sentence = element(&rt, "extension-install-error-text")
        .text
        .clone()
        .unwrap_or_default();
    assert!(
        sentence.contains("the package picker is unavailable (the desktop has no portal)"),
        "the picker failure names itself: {sentence}"
    );
    assert!(
        !rt.state.install_wapp_inspecting,
        "the finally clause cleared the in-flight mark"
    );
}

#[test]
fn the_inspecting_marks_disable_both_picks_and_the_cancel() {
    let mut rt = opened((960.0, 640.0));
    rt.state.install_wapp_inspecting = true;
    rt.settle_all();
    rt.frame(0.0);
    for key in [
        "extension-install-browse-zip",
        "extension-install-browse-folder",
        "extension-install-cancel",
    ] {
        let e = element(&rt, key);
        assert!(
            !e.focusable,
            "{key} is disabled while the inspect is in flight"
        );
    }
    for key in [
        "extension-install-browse-zip",
        "extension-install-browse-folder",
    ] {
        assert_eq!(
            element(&rt, key).label.as_deref(),
            Some(t("extensions.install.inspecting")),
            "{key} swaps to the inspecting label"
        );
    }
}

#[test]
fn the_review_step_renders_the_inspected_manifest() {
    let mut rt = opened((960.0, 640.0));
    reviewed(&mut rt);
    assert_eq!(
        rt.state.install_wapp_step,
        InstallWappStep::Review,
        "the inspect transition advanced the step"
    );

    assert_eq!(
        element(&rt, "extension-install-package-label").text.as_deref(),
        Some("datazen.demo.zip"),
        "the package label from the preview"
    );
    assert_eq!(
        element(&rt, "extension-install-name").text.as_deref(),
        Some("Demo Wapp")
    );
    assert_eq!(
        element(&rt, "extension-install-version").text.as_deref(),
        Some("v1.2.3 · DataZen Labs"),
        "React's v{{version}} · {{author}} line"
    );
    assert_eq!(
        element(&rt, "extension-install-description").text.as_deref(),
        Some("A demo package.")
    );
    assert_eq!(
        element(&rt, "extension-install-permissions-label").text.as_deref(),
        Some(t("extensions.install.permissions"))
    );
    // One neutral badge per requested permission.
    for perm in ["context:connections", "storage:local"] {
        let badge = element(&rt, &format!("extension-install-permission-{perm}"));
        assert_eq!(badge.role, "status", "the badge base's role");
        assert_eq!(
            badge.label.as_deref(),
            Some(perm),
            "the badge names the permission string"
        );
    }
    assert!(
        !has_key(&rt, "extension-install-no-permissions"),
        "a manifest with permissions shows no empty sentence"
    );

    // The review footer: back (ghost) before confirm (primary).
    let back = element(&rt, "extension-install-back");
    assert_eq!(back.role, "button");
    assert_eq!(back.label.as_deref(), Some(t("extensions.install.back")));
    let confirm = element(&rt, "extension-install-confirm");
    assert_eq!(confirm.role, "button");
    assert_eq!(
        confirm.label.as_deref(),
        Some(t("extensions.install.confirm"))
    );
    assert!(
        back.rect[0] < confirm.rect[0],
        "back precedes confirm, as in React"
    );
    assert!(
        (confirm.rect[3] - 36.0).abs() < 0.5,
        "the confirm is a 36pt md button: {:?}",
        confirm.rect
    );

    // The select body left with the review.
    assert!(!has_key(&rt, "extension-install-picks"));
    assert!(!has_key(&rt, "extension-install-cancel"));
    assert!(!has_key(&rt, "extension-install-prompt"));
}

#[test]
fn the_review_step_without_author_description_or_permissions() {
    let mut rt = opened((960.0, 640.0));
    rt.state
        .install_wapp_inspected(preview(&[], None, None));
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "extension-install-version").text.as_deref(),
        Some("v1.2.3"),
        "no author suffix when the manifest carries none"
    );
    assert!(
        !has_key(&rt, "extension-install-description"),
        "no description line without a manifest description"
    );
    // React's empty branch: the sentence, not a badge row.
    assert_eq!(
        element(&rt, "extension-install-no-permissions").text.as_deref(),
        Some(t("extensions.install.noPermissions"))
    );
    let badges = rt
        .elements()
        .iter()
        .filter(|e| {
            e.key
                .as_deref()
                .is_some_and(|key| key.starts_with("extension-install-permission-"))
        })
        .count();
    assert_eq!(badges, 0, "no permission badges in the empty branch");
    // The confirm stays enabled without permissions: React's gate is the
    // manifest and the pick token, not the permission list.
    assert!(element(&rt, "extension-install-confirm").focusable);
}

#[test]
fn the_back_press_returns_to_select_and_clears_the_pick() {
    let mut rt = opened((960.0, 640.0));
    reviewed(&mut rt);
    click(&mut rt, "extension-install-back");
    assert_eq!(
        rt.state.install_wapp_step,
        InstallWappStep::Select,
        "back returns to the select step"
    );
    assert!(rt.state.install_wapp_preview.is_none(), "the pick data left");
    assert!(rt.state.install_wapp_error.is_none());
    assert!(has_key(&rt, "extension-install-picks"));
    assert!(has_key(&rt, "extension-install-cancel"));
    assert!(!has_key(&rt, "extension-install-review"));
    assert!(!has_key(&rt, "extension-install-back"));
    assert!(!has_key(&rt, "extension-install-confirm"));
}

#[test]
fn the_confirm_press_runs_reacts_failure_branch_back_to_select() {
    let mut rt = opened((960.0, 640.0));
    reviewed(&mut rt);
    click(&mut rt, "extension-install-confirm");
    // React's catch: `backToSelect()` then `setError(...)` — the error
    // renders on the select step, the review left behind.
    assert_eq!(rt.state.install_wapp_step, InstallWappStep::Select);
    assert!(rt.state.install_wapp_preview.is_none());
    assert!(!rt.state.install_wapp_installing);
    assert!(!has_key(&rt, "extension-install-review"));
    assert!(has_key(&rt, "extension-install-error"));
    assert!(has_key(&rt, "extension-install-cancel"), "the select footer");
    let error = element(&rt, "extension-install-error");
    assert_eq!(error.role, "alert");
    let sentence = element(&rt, "extension-install-error-text")
        .text
        .clone()
        .unwrap_or_default();
    assert!(
        sentence.contains("wapp install needs the live host"),
        "the seam names the missing install use case: {sentence}"
    );
}

#[test]
fn the_installing_marks_swap_the_confirm_label_and_disable_both_actions() {
    let mut rt = opened((960.0, 640.0));
    reviewed(&mut rt);
    rt.state.install_wapp_installing = true;
    rt.settle_all();
    rt.frame(0.0);
    let confirm = element(&rt, "extension-install-confirm");
    assert_eq!(
        confirm.label.as_deref(),
        Some(t("extensions.install.installing")),
        "the confirm swaps to the installing label"
    );
    assert!(!confirm.focusable, "React disables the confirm while installing");
    // React: `disabled={installing}` on the back button too.
    assert!(
        !element(&rt, "extension-install-back").focusable,
        "back is disabled while installing"
    );

    // A disabled press does nothing: the review stays put.
    click(&mut rt, "extension-install-confirm");
    assert_eq!(
        rt.state.install_wapp_step,
        InstallWappStep::Review,
        "the disabled confirm refused the press"
    );
    assert!(rt.state.install_wapp_preview.is_some());
}

#[test]
fn escape_closes_the_dialog_and_resets_the_flow() {
    let mut rt = opened((960.0, 640.0));
    reviewed(&mut rt);
    rt.state.install_wapp_error = Some("a leftover error".into());
    rt.settle_all();
    rt.frame(0.0);
    escape(&mut rt);
    assert!(!rt.state.install_wapp_open, "escape closed the dialog");
    // React unmounts the component, so every local field starts over.
    assert_eq!(rt.state.install_wapp_step, InstallWappStep::Select);
    assert!(rt.state.install_wapp_preview.is_none());
    assert!(rt.state.install_wapp_error.is_none());
    assert!(!rt.state.install_wapp_inspecting);
    assert!(!rt.state.install_wapp_installing);
    // The whole layer unmounts — a leftover scrim would swallow the page.
    for key in [
        "install-wapp-dialog",
        "install-wapp-dialog-scrim",
        "install-wapp-dialog-footer",
    ] {
        assert!(
            !rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some(key)),
            "{key} unmounted"
        );
    }
}

#[test]
fn the_install_success_transition_resets_and_closes() {
    // The seam's success answer (`install_wapp_succeeded`) is the transition
    // the future host wiring calls after the store refresh; React's success
    // branch closes the dialog with the pick data gone.
    let mut rt = opened((960.0, 640.0));
    reviewed(&mut rt);
    rt.state.install_wapp_succeeded();
    rt.settle_all();
    rt.frame(0.0);
    assert!(!rt.state.install_wapp_open);
    assert_eq!(rt.state.install_wapp_step, InstallWappStep::Select);
    assert!(rt.state.install_wapp_preview.is_none());
    assert!(rt.state.install_wapp_error.is_none());
    assert!(!rt.state.install_wapp_installing);
    assert!(!has_key(&rt, "install-wapp-dialog"));
}

#[test]
fn the_dialog_fits_both_window_sizes_and_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut state = AppState::default();
            state.open_install_wapp_dialog();
            let mut rt = dialog_runtime(state, (w, h), dark);
            // The review step is the taller body: fit it in both skins.
            rt.state.install_wapp_inspected(preview(
                &["context:connections", "storage:local", "ui:notify"],
                Some("DataZen Labs"),
                Some("A demo package with a description."),
            ));
            rt.settle_all();
            rt.frame(0.0);
            for key in [
                "install-wapp-dialog",
                "install-wapp-dialog-title",
                "install-wapp-dialog-body",
                "install-wapp-dialog-footer",
                "extension-install-review",
                "extension-install-package-label",
                "extension-install-name",
                "extension-install-version",
                "extension-install-description",
                "extension-install-permissions",
                "extension-install-permission-context:connections",
                "extension-install-back",
                "extension-install-confirm",
            ] {
                let e = element(&rt, key);
                assert!(
                    e.rect[0] >= -0.5
                        && e.rect[0] + e.rect[2] <= w + 0.5
                        && e.rect[1] >= -0.5
                        && e.rect[1] + e.rect[3] <= h + 0.5,
                    "{key} out of {w}x{h} (dark={dark}): {:?}",
                    e.rect
                );
            }
        }
    }
}

#[test]
fn no_i18n_key_echoes_into_the_rendered_tree() {
    // `t()` falls back to the key itself when a translation is missing; the
    // rendered tree must show the zh-CN values, never a dotted key.
    let mut rt = opened((960.0, 640.0));
    reviewed(&mut rt);
    for e in rt.elements() {
        for observed in [e.text.as_deref(), e.label.as_deref()] {
            if let Some(observed) = observed {
                assert!(
                    !observed.starts_with("extensions.")
                        && !observed.starts_with("common."),
                    "a dotted key rendered: {observed}"
                );
            }
        }
    }
}
