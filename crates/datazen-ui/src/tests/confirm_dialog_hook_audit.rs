//! Independent audit for T038 (the useConfirmDialog Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/hooks/useConfirmDialog.tsx`, the
//! `packages/ui/src/ConfirmDialog.tsx` base it renders, and the React test
//! `__tests__/useConfirmDialog.test.tsx`: nothing paints before a request,
//! the options reach the dialog (title/message/badge/preview/description),
//! OK resolves true and cancel false, and a superseded request can never
//! be answered. Element evidence comes from `Runtime::elements()` (runtime
//! layout), never from a value the component reports about itself; presses
//! go through real pointer events.
//!
//! The state machine and the truncation helper are additionally
//! mutation-proved out-of-tree (the scratch crate, runbook §3.4).

use rune::{Cx, El, Env, Runtime};

use crate::hooks::confirm_dialog::{
    ConfirmDialogHost, ConfirmDialogState, ConfirmOptionsVm,
};
use crate::ui::confirm_dialog::{
    CODE_PREVIEW_MAX_CHARS, CODE_PREVIEW_MAX_LINES, ConfirmKind, truncate_code_preview,
};

fn options(title: &str, message: &str) -> ConfirmOptionsVm {
    ConfirmOptionsVm {
        title: title.to_owned(),
        message: message.to_owned(),
        confirm_label: None,
        cancel_label: None,
        kind: ConfirmKind::Warning,
        badge: None,
        code_preview: None,
        description: None,
    }
}

// ---- the state machine (React's useState + the resolve identity) ---------

#[test]
fn open_hands_out_fresh_ids_and_paints() {
    let mut state = ConfirmDialogState::default();
    assert!(!state.is_open(), "nothing before a request");
    let first = state.open(options("Test Title", "Test Message"));
    assert_eq!(first, 1);
    assert!(state.is_open());
    let second = state.open(options("Again", "Again"));
    assert_eq!(second, 2, "ids increase");
}

#[test]
fn answering_the_pending_request_resolves_and_closes() {
    let mut state = ConfirmDialogState::default();
    let id = state.open(options("T", "M"));
    assert_eq!(state.answer(id, true), Some(true), "OK resolves true");
    assert!(!state.is_open(), "the dialog closes");
    // And it can reopen afterwards with a fresh id.
    let next = state.open(options("T2", "M2"));
    assert_eq!(state.answer(next, false), Some(false), "cancel resolves false");
}

#[test]
fn a_superseded_request_can_never_be_answered() {
    // React's setState replacement: the first resolver leaks unresolved.
    let mut state = ConfirmDialogState::default();
    let first = state.open(options("A", "a"));
    let second = state.open(options("B", "b"));
    assert_ne!(first, second);
    assert_eq!(state.answer(first, true), None, "the stale id lands nothing");
    assert!(state.is_open(), "the live request stays open");
    assert_eq!(state.answer(second, true), Some(true));
    assert!(!state.is_open());
}

#[test]
fn answering_with_nothing_pending_lands_nothing() {
    let mut state = ConfirmDialogState::default();
    assert_eq!(state.answer(1, true), None);
    assert_eq!(state.answer_pending(true), None, "no live request");
}

#[test]
fn answer_pending_targets_the_live_request() {
    let mut state = ConfirmDialogState::default();
    state.open(options("A", "a"));
    state.open(options("B", "b"));
    assert_eq!(state.answer_pending(false), Some(false));
    assert!(!state.is_open());
}

// ---- the preview truncation (packages/ui's truncateCodePreview) ----------

#[test]
fn short_previews_pass_through_verbatim() {
    let (text, truncated) = truncate_code_preview("DELETE FROM users");
    assert_eq!(text, "DELETE FROM users");
    assert!(!truncated);
}

#[test]
fn thirteen_lines_truncate_to_twelve() {
    let raw = (0..CODE_PREVIEW_MAX_LINES + 1).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    let (text, truncated) = truncate_code_preview(&raw);
    assert!(truncated);
    assert_eq!(text.lines().count(), CODE_PREVIEW_MAX_LINES);
    assert!(text.starts_with("line 0"));
    assert!(text.contains("line 11"));
    assert!(!text.contains("line 12"));
}

#[test]
fn a_long_single_line_stays_but_flags_truncated() {
    // React quirk kept byte-for-byte: the slice keeps the (single) whole
    // line — only the flag and the ellipsis mark the cut.
    let raw = "x".repeat(CODE_PREVIEW_MAX_CHARS + 1);
    let (text, truncated) = truncate_code_preview(&raw);
    assert!(truncated);
    assert_eq!(text, raw, "one line: slice(0, 12) keeps it whole");
}

#[test]
fn exactly_at_the_limits_is_not_truncated() {
    let raw = (0..CODE_PREVIEW_MAX_LINES).map(|i| format!("l{i}")).collect::<Vec<_>>().join("\n");
    let (text, truncated) = truncate_code_preview(&raw);
    assert!(!truncated);
    assert_eq!(text, raw);
    let exact = "x".repeat(CODE_PREVIEW_MAX_CHARS);
    let (text, truncated) = truncate_code_preview(&exact);
    assert!(!truncated);
    assert_eq!(text, exact);
}

// ---- runtime evidence -------------------------------------------------------

#[derive(Clone)]
struct Scene {
    confirm: ConfirmDialogState,
    dark: bool,
    results: Vec<(u64, bool)>,
}

impl Scene {
    fn base() -> Self {
        Self {
            confirm: ConfirmDialogState::default(),
            dark: true,
            results: Vec::new(),
        }
    }

    fn with(title: &str, message: &str) -> (Self, u64) {
        let mut scene = Self::base();
        let id = scene.confirm.open(options(title, message));
        (scene, id)
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    ConfirmDialogHost::<Scene>::new(&s.confirm)
        .on_resolved(|s: &mut Scene, result: bool| {
            // The host's resolver: answer by the live id (the dialog's own
            // buttons always target the pending request).
            if let Some(resolved) = s.confirm.answer_pending(result) {
                let id = s.confirm.pending_id;
                s.results.push((id, resolved));
            }
        })
        .build(s.dark, &mut layer);
    let mut root = rune::col().key("probe");
    for child in layer {
        root = root.child(child);
    }
    root
}

fn rt(scene: Scene) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<Scene>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn press(rt: &mut Runtime<Scene>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    rt.frame(0.0);
    rt.frame(0.0);
}

#[test]
fn nothing_paints_before_a_request() {
    // React test: queryByText('Test Title') is null.
    let rt = rt(Scene::base());
    assert!(maybe(&rt, "confirm-dialog").is_none());
    assert!(maybe(&rt, "confirm-dialog-ok").is_none());
}

#[test]
fn the_request_options_reach_the_dialog() {
    let (scene, _id) = Scene::with("Test Title", "Test Message");
    let rt = rt(scene);
    assert!(maybe(&rt, "confirm-dialog").is_some());
    let title = maybe(&rt, "confirm-dialog-title").and_then(|e| e.text).unwrap_or_default();
    assert!(title.contains("Test Title"), "title: {title}");
    assert!(text_of(&rt, "confirm-dialog-message").contains("Test Message"));
}

#[test]
fn ok_resolves_true_and_cancel_resolves_false() {
    // React tests: the two button presses forward the two results.
    let (scene, _id) = Scene::with("T", "M");
    let mut ok_rt = rt(scene);
    press(&mut ok_rt, "confirm-dialog-ok");
    assert_eq!(ok_rt.state.results, vec![(1, true)], "OK resolved true");
    assert!(!ok_rt.state.confirm.is_open());

    let (scene, _id) = Scene::with("T", "M");
    let mut cancel_rt = rt(scene);
    press(&mut cancel_rt, "confirm-dialog-cancel");
    assert_eq!(cancel_rt.state.results, vec![(1, false)], "cancel resolved false");
    assert!(!cancel_rt.state.confirm.is_open());
}

#[test]
fn the_advanced_options_render_their_parts() {
    // React tests: badge, codePreview and description all pass through.
    let mut scene = Scene::base();
    scene.confirm.open(ConfirmOptionsVm {
        title: "Production SQL".to_owned(),
        message: "Review before executing".to_owned(),
        confirm_label: None,
        cancel_label: None,
        kind: ConfirmKind::Warning,
        badge: Some("Production".to_owned()),
        code_preview: Some("DELETE FROM users".to_owned()),
        description: Some("This will affect all rows.".to_owned()),
    });
    let rt = rt(scene);
    assert!(text_of(&rt, "confirm-dialog-badge").contains("Production"));
    assert!(
        text_of(&rt, "confirm-dialog-preview-text").contains("DELETE FROM users"),
        "the preview renders the SQL"
    );
    assert!(maybe(&rt, "confirm-dialog-copy-sql").is_some(), "the copy affordance");
    assert!(text_of(&rt, "confirm-dialog-body").contains("This will affect all rows."));
}

#[test]
fn a_truncated_preview_shows_the_note_and_ellipsis() {
    let mut scene = Scene::base();
    let preview = (0..20).map(|i| format!("SELECT {i}")).collect::<Vec<_>>().join("\n");
    scene.confirm.open(ConfirmOptionsVm {
        code_preview: Some(preview),
        ..options("T", "M")
    });
    let rt = rt(scene);
    let text = text_of(&rt, "confirm-dialog-preview-text");
    assert!(text.contains("…"), "the ellipsis marks the cut: {text}");
    assert!(!text.contains("SELECT 13"), "past the 12-line cut");
    assert!(
        maybe(&rt, "confirm-dialog-preview-head")
            .and_then(|e| e.text)
            .unwrap_or_default()
            .contains("12"),
        "the truncation note names the line count"
    );
}
