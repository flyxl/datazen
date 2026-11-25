//! T285 audit: the InstallLanguagePackDialog port, verified against
//! the React source re-read line by line and
//! `InstallLanguagePackDialog.test.tsx` (all five cases mirrored).
//! Element evidence comes from `Runtime::elements()`, presses go
//! through real pointer events.
//!
//! Tester-written: every expectation below is re-derived from the React
//! files.

use std::rc::Rc;

use rune::{col, Env, El, Runtime};

use datazen_ui::views::install_language_pack_dialog::{
    InstallLanguagePackDialog, InstallLanguagePackDialogState, InstallStep, LanguagePackDomainStat,
    LanguagePackManifest, LanguagePackPreview,
};

fn preview() -> LanguagePackPreview {
    LanguagePackPreview {
        pick_token: "pick-token-1".to_owned(),
        package_label: "/Downloads/lang-pack.zip".to_owned(),
        manifest: LanguagePackManifest {
            code: "ja-JP".to_owned(),
            label: "日本語".to_owned(),
            version: "1.2.0".to_owned(),
            author: Some("Acme".to_owned()),
        },
        message_count: 120,
        domains: vec![
            LanguagePackDomainStat { domain: "core".to_owned(), message_count: 7 },
            LanguagePackDomainStat { domain: "settings".to_owned(), message_count: 113 },
        ],
    }
}

#[derive(Clone)]
struct AuditState {
    dialog: InstallLanguagePackDialogState,
    open: bool,
    dark: bool,
    // The inspect/install seams' answers.
    pick_answer: Option<Option<LanguagePackPreview>>,
    pick_error: Option<String>,
    install_error: Option<String>,
    // recording
    closes: usize,
    installs: usize,
    installs_with: Vec<String>,
}

impl AuditState {
    fn scene() -> Self {
        Self {
            dialog: InstallLanguagePackDialogState::new(),
            open: true,
            dark: true,
            pick_answer: None,
            pick_error: None,
            install_error: None,
            closes: 0,
            installs: 0,
            installs_with: Vec::new(),
        }
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    InstallLanguagePackDialog::<AuditState>::new(&s.dialog, s.open)
        .on_close(|s: &mut AuditState| s.closes += 1)
        .on_pick(|s: &mut AuditState| {
            if !s.dialog.begin_pick() {
                return;
            }
            if let Some(error) = s.pick_error.clone() {
                s.dialog.note_pick_error(error);
            } else {
                s.dialog.note_pick(s.pick_answer.clone().flatten().as_ref().cloned());
            }
        })
        .on_back(|s: &mut AuditState| s.dialog.back_to_select())
        .on_install(|s: &mut AuditState| {
            let token = s.dialog.pick_token.clone().unwrap_or_default();
            if !s.dialog.begin_install() {
                return;
            }
            s.installs += 1;
            s.installs_with.push(token);
            match s.install_error.clone() {
                Some(error) => s.dialog.note_install_error(error),
                None => {
                    s.dialog.note_install_ok();
                    s.closes += 1; // the host closes on success
                }
            }
        })
        .build(s.dark, &mut layer);
    col().max_w(640.0).h_full().children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt =
        Runtime::new(scene, view, (960.0, 640.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

/// React test 1: cancelling the native picker stays on the select step
/// (and never closes).
#[test]
fn a_cancelled_picker_stays_on_select() {
    let mut scene = AuditState::scene();
    scene.pick_answer = Some(None);
    let mut rt = rt(scene);
    press(&mut rt, "language-pack-install-pick");
    let state = rt.state.clone();
    assert_eq!(state.dialog.step, InstallStep::Select);
    assert!(state.dialog.pick_token.is_none());
    assert!(state.dialog.error.is_none(), "a cancel is silent");
    assert_eq!(state.closes, 0);
    assert!(has(&rt, "language-pack-install-pick"));
}

/// React test 2: the review step shows the manifest, the per-domain
/// counts and the file label.
#[test]
fn the_review_shows_manifest_domains_and_file() {
    let mut scene = AuditState::scene();
    scene.pick_answer = Some(Some(preview()));
    let mut rt = rt(scene);
    press(&mut rt, "language-pack-install-pick");
    let state = rt.state.clone();
    assert_eq!(state.dialog.step, InstallStep::Review);
    assert!(has(&rt, "language-pack-review"));
    assert!(has(&rt, "language-pack-review-domains"));
    assert!(text_of(&rt, "language-pack-domain-core").contains('7'));
    assert!(text_of(&rt, "language-pack-review-file").contains("lang-pack.zip"));
    // The label and version render.
    assert!(text_of(&rt, "language-pack-review-code").contains("ja-JP"));
    assert!(text_of(&rt, "language-pack-review-version").contains("1.2.0"));
}

/// React test 3: the confirm installs with the pick token, refreshes
/// (the host's half) and closes.
#[test]
fn the_confirm_installs_with_the_token_and_closes() {
    let mut scene = AuditState::scene();
    scene.pick_answer = Some(Some(preview()));
    let mut rt = rt(scene);
    press(&mut rt, "language-pack-install-pick");
    press(&mut rt, "language-pack-install-confirm");
    let state = rt.state.clone();
    assert_eq!(state.installs, 1);
    assert_eq!(state.installs_with, vec!["pick-token-1".to_owned()]);
    assert!(state.closes >= 1, "the host closed on success");
    assert_eq!(state.dialog.step, InstallStep::Select, "back to select before closing");
}

/// React test 4: a rejected inspection shows the error inline (still on
/// select).
#[test]
fn a_rejected_inspection_shows_the_error_inline() {
    let mut scene = AuditState::scene();
    scene.pick_error = Some("invalid language pack: empty label".to_owned());
    let mut rt = rt(scene);
    press(&mut rt, "language-pack-install-pick");
    let state = rt.state.clone();
    assert_eq!(state.dialog.step, InstallStep::Select);
    assert!(state.dialog.error.as_deref().unwrap_or_default().contains("empty label"));
    assert!(has(&rt, "language-pack-install-pick"), "the pick button remains");
    // The error element mounts with its copyable form.
    assert!(rt
        .elements()
        .into_iter()
        .any(|e| !e.exiting
            && e.key.as_deref() == Some("language-pack-install-error")));
}

/// React test 5: a rejected install returns to select and shows the
/// error (the dialog stays open).
#[test]
fn a_rejected_install_returns_to_select_with_the_error() {
    let mut scene = AuditState::scene();
    scene.pick_answer = Some(Some(preview()));
    scene.install_error = Some("conflicts with built-in".to_owned());
    let mut rt = rt(scene);
    press(&mut rt, "language-pack-install-pick");
    press(&mut rt, "language-pack-install-confirm");
    let state = rt.state.clone();
    assert_eq!(state.dialog.step, InstallStep::Select);
    assert!(state.dialog.error.as_deref().unwrap_or_default().contains("conflicts with built-in"));
    assert_eq!(state.closes, 0, "the dialog stays open");
    assert!(has(&rt, "language-pack-install-pick"));
}

/// The back press returns to select whole (the review never leaks into
/// a fresh pick).
#[test]
fn the_back_press_resets_everything() {
    let mut scene = AuditState::scene();
    scene.pick_answer = Some(Some(preview()));
    let mut rt = rt(scene);
    press(&mut rt, "language-pack-install-pick");
    press(&mut rt, "language-pack-install-back");
    let state = rt.state.clone();
    assert_eq!(state.dialog.step, InstallStep::Select);
    assert!(state.dialog.pick_token.is_none());
    assert!(state.dialog.preview.is_none());
    assert!(state.dialog.package_label.is_empty());
    assert!(state.dialog.error.is_none());
}

/// A closed dialog mounts nothing.
#[test]
fn closed_mounts_nothing() {
    let mut scene = AuditState::scene();
    scene.open = false;
    let rt = rt(scene);
    assert!(!has(&rt, "language-pack-install-pick"));
    assert!(rt.elements().into_iter().all(|e| e.key != Some("install-language-pack-dialog".to_string().into())));
    let _ = Rc::new(0); // keep Rc imported for the view signature
}
