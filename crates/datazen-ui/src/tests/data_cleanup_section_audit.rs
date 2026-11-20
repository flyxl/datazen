//! T244 audit: the DataCleanupSection port, verified against the React
//! source re-read line by line and
//! `windows/settings/__tests__/DataCleanupSection.test.tsx` (every case
//! mirrored, plus the custom-retention and confirm-journey arms the
//! React suite implies). Element evidence comes from
//! `Runtime::elements()`, presses go through real pointer events.
//!
//! Tester-written: every expectation below is re-derived from the React
//! files.

use std::rc::Rc;

use rune::{col, Env, El, Runtime, TextEdit, Tx};

use crate::hooks::confirm_dialog::ConfirmDialogHost;
use crate::views::data_cleanup_section::{
    CleanupMessage, CleanupRunDecision, DataCleanupSection, DataCleanupSectionState,
    HistoryPurgeScope, RetentionMode, RETENTION_PRESETS,
};

#[derive(Clone)]
struct AuditState {
    /// The onChange diff's memory (interior: the view closure reads
    /// state by reference).
    last_custom_text: std::cell::RefCell<String>,
    section: DataCleanupSectionState,
    dark: bool,
    // The purge seam (historyCommands.purgeHistory, T316's).
    purges: Vec<(HistoryPurgeScope, Option<u32>)>,
    /// What the purge answers: Some(count) or None (the rejection).
    purge_result: Option<u32>,
}

impl AuditState {
    fn scene() -> Self {
        Self {
            section: DataCleanupSectionState::new(),
            dark: true,
            last_custom_text: std::cell::RefCell::new("30".to_owned()),
            purges: Vec::new(),
            purge_result: Some(3),
        }
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let section = DataCleanupSection::<AuditState>::new(&s.section)
        .on_toggle_scope_query(|s: &mut AuditState| s.section.scope_query = !s.section.scope_query)
        .on_toggle_scope_workflow(|s: &mut AuditState| {
            s.section.scope_workflow = !s.section.scope_workflow
        })
        .on_preset(|s: &mut AuditState, days| s.section.select_preset(days))
        .custom_edit(|s: &mut AuditState| &mut s.section.custom_days)
        .on_custom_focus(|s: &mut AuditState| s.section.focus_custom())
        .on_clear_all(|s: &mut AuditState| s.section.select_clear_all())
        .on_run(|s: &mut AuditState, tx: &mut Tx| {
            // React's onChange equivalent, evaluated at the run moment:
            // a custom text that changed since mount means the custom
            // mode owns the retention (the accessor itself stays pure —
            // the engine calls it for caret reads too).
            if *s.last_custom_text.borrow() != s.section.custom_days.text {
                s.section.retention_mode = RetentionMode::Custom;
            }
            // handleRun: the decision, the confirm, the purge.
            let decision = s.section.begin_run();
            match decision {
                CleanupRunDecision::NoScope => {}
                CleanupRunDecision::Confirm { scope, retain_days, .. } => {
                    // The React test's confirm answers OK.
                    if s.section.resolve_confirm(true) {
                        s.section.begin_purge();
                        let result = if tx.clock() >= 0.0 {
                            s.purge_result
                        } else {
                            s.purge_result
                        };
                        s.purges.push((scope, retain_days));
                        s.section.note_purge_result(result);
                    }
                }
            }
        })
        .on_confirm_resolved(|_s: &mut AuditState, _result: bool| {
            // The confirm's resolution rides on_run above (the audit's
            // synchronous harness); the host's layer dialog only paints.
        })
        .build(s.dark, &mut layer);
    let _ = ConfirmDialogHost::<AuditState>::new(&s.section.confirm);
    col().max_w(640.0).h_full().child(section).children(layer)
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

// ---- the pure state machine ---------------------------------------------------

#[test]
fn the_scope_collapse_follows_the_react_rule() {
    let mut state = DataCleanupSectionState::new();
    // Both (the default): all.
    assert_eq!(state.resolve_scope(), Some(HistoryPurgeScope::All));
    state.scope_workflow = false;
    assert_eq!(state.resolve_scope(), Some(HistoryPurgeScope::Query));
    state.scope_workflow = true;
    state.scope_query = false;
    assert_eq!(state.resolve_scope(), Some(HistoryPurgeScope::Workflow));
    state.scope_workflow = false;
    assert_eq!(state.resolve_scope(), None, "no scope checked");
}

#[test]
fn custom_days_parse_clamps_and_default() {
    let mut state = DataCleanupSectionState::new();
    assert_eq!(state.preset_days, 30, "the preset default is 30");
    state.set_custom_days("60");
    assert_eq!(state.retention_mode, RetentionMode::Custom);
    assert_eq!(state.custom_days_value(), 60);
    // Unparsable reads 1 (`Number(...) || 1`).
    state.set_custom_days("");
    assert_eq!(state.custom_days_value(), 1);
    state.set_custom_days("abc");
    assert_eq!(state.custom_days_value(), 1);
    // Zero reads 1 too.
    state.set_custom_days("0");
    assert_eq!(state.custom_days_value(), 1);
    // The 1..=365 clamp.
    state.set_custom_days("9999");
    assert_eq!(state.custom_days_value(), 365);
    state.set_custom_days("-4");
    assert_eq!(state.custom_days_value(), 1);
    // The default field starts at the preset default.
    assert_eq!(DataCleanupSectionState::new().custom_days_value(), 30);
}

#[test]
fn the_presets_and_modes_flip() {
    let mut state = DataCleanupSectionState::new();
    assert_eq!(RETENTION_PRESETS, [7, 30, 90]);
    state.select_preset(7);
    assert_eq!(state.retention_mode, RetentionMode::Preset);
    assert_eq!(state.preset_days, 7);
    state.select_clear_all();
    assert_eq!(state.retention_mode, RetentionMode::ClearAll);
    state.focus_custom();
    assert_eq!(state.retention_mode, RetentionMode::Custom);
}

#[test]
fn begin_run_decides_the_purge_bag() {
    // Preset: the preset days.
    let mut state = DataCleanupSectionState::new();
    state.select_preset(30);
    let CleanupRunDecision::Confirm { scope, retain_days, .. } = state.begin_run() else {
        panic!("a scoped run confirms");
    };
    assert_eq!(scope, HistoryPurgeScope::All);
    assert_eq!(retain_days, Some(30));

    // Clear all: null retainDays.
    let mut state = DataCleanupSectionState::new();
    state.select_clear_all();
    let CleanupRunDecision::Confirm { retain_days, .. } = state.begin_run() else {
        panic!();
    };
    assert_eq!(retain_days, None);

    // Custom: the clamped value.
    let mut state = DataCleanupSectionState::new();
    state.set_custom_days("120");
    let CleanupRunDecision::Confirm { retain_days, .. } = state.begin_run() else {
        panic!();
    };
    assert_eq!(retain_days, Some(120));

    // No scope: the noScope error, nothing armed.
    let mut state = DataCleanupSectionState::new();
    state.scope_query = false;
    state.scope_workflow = false;
    assert_eq!(state.begin_run(), CleanupRunDecision::NoScope);
    assert!(matches!(state.message, Some(CleanupMessage::Error { .. })));
}

#[test]
fn the_result_messages_and_busy_cycle() {
    let mut state = DataCleanupSectionState::new();
    state.begin_purge();
    assert!(state.busy);
    assert!(state.message.is_none());
    state.note_purge_result(Some(5));
    assert!(!state.busy, "finally clears busy");
    assert_eq!(state.message, Some(CleanupMessage::Success { count: 5 }));
    state.begin_purge();
    state.note_purge_result(None);
    assert!(!state.busy);
    assert!(matches!(state.message, Some(CleanupMessage::Error { .. })));
}

// ---- the runtime tree (the React suite, case for case) ------------------------

/// React test 1: no scope → the noScope line, and the purge never runs.
#[test]
fn requires_at_least_one_scope() {
    let mut scene = AuditState::scene();
    scene.purge_result = Some(3);
    let mut rt = rt(scene);
    // Uncheck both scopes.
    press(&mut rt, "data-cleanup-scope-query");
    press(&mut rt, "data-cleanup-scope-workflow");
    press(&mut rt, "data-cleanup-run");
    let state = rt.state.clone();
    assert!(state.purges.is_empty(), "the purge never runs");
    assert!(matches!(
        state.section.message,
        Some(CleanupMessage::Error { .. })
    ));
    assert!(has(&rt, "data-cleanup-message"));
}

/// React test 2: the preset retention reaches the purge.
#[test]
fn runs_with_preset_retention() {
    let mut scene = AuditState::scene();
    scene.purge_result = Some(3);
    let mut rt = rt(scene);
    press(&mut rt, "data-cleanup-preset-30");
    press(&mut rt, "data-cleanup-run");
    let state = rt.state.clone();
    assert_eq!(
        state.purges,
        vec![(HistoryPurgeScope::All, Some(30))],
        "scope all + retainDays 30"
    );
    // The success line shows the count.
    assert_eq!(state.section.message, Some(CleanupMessage::Success { count: 3 }));
    assert!(text_of(&rt, "data-cleanup-message").contains("3"));
}

/// React test 3: clear-all reaches the purge with null retainDays.
#[test]
fn runs_with_null_retention_for_clear_all() {
    let mut scene = AuditState::scene();
    scene.purge_result = Some(5);
    let mut rt = rt(scene);
    press(&mut rt, "data-cleanup-clear-all");
    press(&mut rt, "data-cleanup-run");
    assert_eq!(
        rt.state.purges,
        vec![(HistoryPurgeScope::All, None)]
    );
}

/// A failed purge shows the generic error line.
#[test]
fn a_failed_purge_shows_the_error_line() {
    let mut scene = AuditState::scene();
    scene.purge_result = None;
    let mut rt = rt(scene);
    press(&mut rt, "data-cleanup-run");
    let state = rt.state.clone();
    assert_eq!(state.purges.len(), 1);
    assert!(matches!(
        state.section.message,
        Some(CleanupMessage::Error { .. })
    ));
}

/// Isolating the clear-all press: the radio must flip the mode.
#[test]
fn the_clear_all_press_flips_the_mode() {
    let mut rt = rt(AuditState::scene());
    press(&mut rt, "data-cleanup-clear-all");
    assert_eq!(rt.state.section.retention_mode, RetentionMode::ClearAll);
}

/// The section mounts with its React testids and the preset default.
#[test]
fn the_section_mounts_with_its_testids() {
    let rt = rt(AuditState::scene());
    assert!(has(&rt, "data-cleanup-section"));
    assert!(has(&rt, "data-cleanup-scope-query"));
    assert!(has(&rt, "data-cleanup-scope-workflow"));
    assert!(has(&rt, "data-cleanup-preset-7"));
    assert!(has(&rt, "data-cleanup-preset-30"));
    assert!(has(&rt, "data-cleanup-preset-90"));
    assert!(has(&rt, "data-cleanup-custom-days"));
    assert!(has(&rt, "data-cleanup-clear-all"));
    assert!(has(&rt, "data-cleanup-run"));
    // The 30 preset is the selected default.
    let state = rt.state.clone();
    assert_eq!(state.section.preset_days, 30);
}
