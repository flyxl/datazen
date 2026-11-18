//! T071 (segment-2's T070): the `RunningExecutionCloseHost` port — the host
//! half of the gate behind "close a tab whose query is still running".
//!
//! The store asks a gate before it takes away a tab whose query is still
//! running; this is the other half of that contract: the ask must actually
//! reach a human, name the tab and show the SQL so the question is
//! answerable, and hand back their answer. The observation channel is the
//! a11y tree (keys, rects, roles, labels and text) of the real
//! `crate::view` — nothing is asked of the module under test directly.

use super::*;
use crate::i18n::t;
use crate::views::RunningExecutionCloseRequest;
use rune::{Key, KeyEvent, Mods};

/// React's test fixture (`RunningExecutionCloseHost.test.tsx:22-31`), the
/// same ask the store's guard would hand the gate.
fn request() -> RunningExecutionCloseRequest {
    RunningExecutionCloseRequest {
        panel_id: "panel-1".into(),
        title: "local · app · SQL".into(),
        pane_id: "main".into(),
        sql: "SELECT pg_sleep(10)".into(),
        running_panes: 1,
    }
}

fn main_runtime(state: AppState, size: (f32, f32), dark: bool) -> Runtime<AppState> {
    let mut rt = Runtime::new(
        state,
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

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
    rt.frame(1.0 / 60.0);
}

fn escape(rt: &mut Runtime<AppState>) {
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.frame(0.0);
}

/// The ask opens the prompt on a running runtime, the way the store's guard
/// will hand the gate an ask while a frame is on screen.
fn ask(rt: &mut Runtime<AppState>, request: RunningExecutionCloseRequest) {
    rt.state.ask_running_execution_close(request);
    rt.settle_all();
    rt.frame(0.0);
}

#[test]
fn the_host_mounts_nothing_while_no_ask_is_pending_and_after_it_settles() {
    // React returns `dialog` — null while `useConfirmDialog` holds no pending
    // request — so the idle host renders nothing over the workspace.
    let rt = main_runtime(AppState::default(), (1440.0, 900.0), true);
    for key in [
        "running-close-dialog",
        "running-close-dialog-layer",
        "running-close-dialog-scrim",
        "confirm-dialog-ok",
        "confirm-dialog-cancel",
    ] {
        assert!(!has_key(&rt, key), "{key} mounts nothing while idle");
    }
    assert!(
        rt.state.running_close_request.is_none(),
        "no pending ask in the default state"
    );

    // And the prompt leaves with the answer, exactly as React's
    // `handleClose` clears the hook's state.
    let mut rt = main_runtime(AppState::default(), (1440.0, 900.0), true);
    ask(&mut rt, request());
    assert!(has_key(&rt, "running-close-dialog"), "the ask opened it");
    click(&mut rt, "confirm-dialog-ok");
    assert!(
        !has_key(&rt, "running-close-dialog"),
        "the settled prompt unmounts"
    );
    assert!(rt.state.running_close_request.is_none());
}

#[test]
fn an_ask_opens_the_prompt_naming_the_tab_and_its_sql() {
    // React's own contract: the prompt shows which tab and which SQL it is
    // asking about (`RunningExecutionCloseHost.test.tsx:34-50`).
    let mut rt = main_runtime(AppState::default(), (1440.0, 900.0), true);
    ask(&mut rt, request());

    let title = element(&rt, "running-close-dialog-title");
    assert_eq!(
        title.text.as_deref(),
        Some(t("query.session.closeTabActiveExecutionTitle")),
        "the title reads the zh-CN table"
    );
    let message = element(&rt, "running-close-dialog-message");
    assert_eq!(
        message.text.as_deref(),
        Some(t("query.session.closeTabActiveExecution")),
        "one running pane takes the singular sentence"
    );
    assert_eq!(
        element(&rt, "running-close-dialog-badge-text").text.as_deref(),
        Some("local · app · SQL"),
        "the badge names the tab (the request's title)"
    );
    assert_eq!(
        element(&rt, "running-close-dialog-code-text").text.as_deref(),
        Some("SELECT pg_sleep(10)"),
        "the code preview shows the pane's SQL"
    );

    // The two answers: a danger "stop and close" and a ghost "keep the tab".
    let ok = element(&rt, "confirm-dialog-ok");
    assert_eq!(ok.role, "button");
    assert_eq!(
        ok.label.as_deref(),
        Some(t("query.session.closeTabActiveExecutionConfirm")),
        "the confirm label is React's stop-and-close sentence"
    );
    let cancel = element(&rt, "confirm-dialog-cancel");
    assert_eq!(cancel.role, "button");
    assert_eq!(
        cancel.label.as_deref(),
        Some(t("common.cancel")),
        "the cancel label is the dialog default"
    );

    // The panel is a real dialog over the window, in both window sizes and
    // both themes: inside the window, centered, and no wider than React's
    // `max-w-lg` (512pt).
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let rt = main_runtime(
                {
                    let mut state = AppState::default();
                    state.ask_running_execution_close(request());
                    state
                },
                (w, h),
                dark,
            );
            let panel = element(&rt, "running-close-dialog");
            assert_eq!(panel.role, "dialog", "the panel carries the dialog role");
            assert!(
                panel.rect[0] >= -0.5 && panel.rect[0] + panel.rect[2] <= w + 0.5,
                "panel out of {w}x{h} (dark={dark}): {:?}",
                panel.rect
            );
            assert!(
                panel.rect[1] >= -0.5 && panel.rect[1] + panel.rect[3] <= h + 0.5,
                "panel out of {w}x{h} vertically (dark={dark}): {:?}",
                panel.rect
            );
            assert!(
                panel.rect[2] <= 512.5,
                "the panel keeps React's max-w-lg: {:?}",
                panel.rect
            );
            let center = panel.rect[0] + panel.rect[2] / 2.0;
            assert!(
                (center - w / 2.0).abs() < 0.5,
                "the layer centers the panel at {center}, want {}: {:?}",
                w / 2.0,
                panel.rect
            );
        }
    }
}

#[test]
fn confirm_routes_true_and_a_new_ask_reopens_the_prompt() {
    // React: the gate resolves `true` when the user chooses stop-and-close;
    // a later ask is a fresh prompt (the hook's state was cleared, and the
    // dialog remounts — here the epoch, so the body starts at the top).
    let mut rt = main_runtime(AppState::default(), (1440.0, 900.0), true);
    let first = request();
    let expected = first.clone();
    ask(&mut rt, first);
    assert!(
        has_key(&rt, "running-close-dialog-content-1"),
        "the first ask is the first open generation"
    );

    click(&mut rt, "confirm-dialog-ok");
    match rt.state.running_close_outcome.as_ref() {
        Some((answered, allowed)) => {
            assert_eq!(*answered, expected, "the outcome carries the ask back");
            assert!(*allowed, "stop and close answers true");
        }
        None => panic!("the OK press settled the ask"),
    }

    // A second ask while nobody is pending opens a fresh prompt and clears
    // the previous outcome, as the store's next queued close takes its turn.
    let second = RunningExecutionCloseRequest {
        title: "remote · analytics".into(),
        running_panes: 1,
        ..request()
    };
    let expected_second = second.clone();
    ask(&mut rt, second);
    assert!(has_key(&rt, "running-close-dialog"), "the new ask reopened it");
    assert!(
        has_key(&rt, "running-close-dialog-content-2"),
        "each ask bumps the open generation"
    );
    assert_eq!(
        rt.state.running_close_outcome, None,
        "a new ask clears the settled outcome"
    );
    assert_eq!(
        element(&rt, "running-close-dialog-badge-text").text.as_deref(),
        Some("remote · analytics"),
        "the reopened prompt asks about the new tab"
    );
    match rt.state.running_close_request.as_ref() {
        Some(pending) => assert_eq!(*pending, expected_second),
        None => panic!("the new ask is pending"),
    }
}

#[test]
fn cancel_and_dismiss_both_keep_the_tab() {
    // React: the gate resolves `false` when the user keeps the tab — and the
    // dialog's own dismissal channels (the header close, Escape) are wired
    // to `onCancel`, the same answer.
    let mut rt = main_runtime(AppState::default(), (1440.0, 900.0), true);
    let expected = request();
    ask(&mut rt, request());
    click(&mut rt, "confirm-dialog-cancel");
    match rt.state.running_close_outcome.as_ref() {
        Some((answered, allowed)) => {
            assert_eq!(*answered, expected, "the outcome carries the ask back");
            assert!(!allowed, "keeping the tab answers false");
        }
        None => panic!("the cancel press settled the ask"),
    }
    assert!(
        !has_key(&rt, "running-close-dialog"),
        "the answered prompt left the layer"
    );

    // The next ask, dismissed with Escape (React `onClose={onCancel}`).
    let expected = request();
    ask(&mut rt, request());
    escape(&mut rt);
    match rt.state.running_close_outcome.as_ref() {
        Some((answered, allowed)) => {
            assert_eq!(*answered, expected);
            assert!(!allowed, "Escape is the keep-the-tab answer");
        }
        None => panic!("Escape settled the ask"),
    }
    assert!(!has_key(&rt, "running-close-dialog"), "Escape took it down");

    // A settled ask cannot settle twice: React's promise resolves one value
    // only, so a stray second answer changes nothing.
    rt.state.answer_running_execution_close(true);
    match rt.state.running_close_outcome.as_ref() {
        Some((_, allowed)) => assert!(!allowed, "the second press landed on nothing"),
        None => panic!("the outcome survives"),
    }
}

#[test]
fn several_running_panes_say_how_many() {
    // React's plural branch: a tab with several running panes counts them
    // (`RunningExecutionCloseHost.tsx:29-31`, the {count} placeholder).
    let mut rt = main_runtime(AppState::default(), (1440.0, 900.0), true);
    let expected = t("query.session.closeTabActiveExecutionMany").replace("{count}", "3");
    ask(
        &mut rt,
        RunningExecutionCloseRequest { running_panes: 3, ..request() },
    );
    assert_eq!(
        element(&rt, "running-close-dialog-message").text.as_deref(),
        Some(expected.as_str()),
        "the sentence names the count"
    );
    // The singular boundary: exactly one pane keeps the singular sentence.
    let expected = t("query.session.closeTabActiveExecutionMany").replace("{count}", "2");
    ask(
        &mut rt,
        RunningExecutionCloseRequest { running_panes: 2, ..request() },
    );
    assert_eq!(
        element(&rt, "running-close-dialog-message").text.as_deref(),
        Some(expected.as_str()),
        "two panes are already many"
    );
}

#[test]
fn an_ask_without_sql_leaves_the_code_preview_out() {
    // React renders the code section only for a truthy `codePreview`: an
    // empty SQL leaves it out (`{preview && ...}` with `codePreview: ''`).
    let mut rt = main_runtime(AppState::default(), (1440.0, 900.0), true);
    ask(
        &mut rt,
        RunningExecutionCloseRequest {
            sql: String::new(),
            ..request()
        },
    );
    assert!(
        !has_key(&rt, "running-close-dialog-code"),
        "no code section for an empty SQL"
    );
    assert!(
        has_key(&rt, "running-close-dialog-message"),
        "the question itself still mounts"
    );
    let panel = element(&rt, "running-close-dialog");
    assert!(
        panel.rect[0] >= -0.5 && panel.rect[0] + panel.rect[2] <= 1440.5,
        "the prompt fits without its code section: {:?}",
        panel.rect
    );
}

#[test]
fn the_copy_control_copies_the_sql_and_marks_itself() {
    // React's code section carries a copy affordance (`confirm-dialog-copy-sql`):
    // pressing it copies the SQL and shows the copied mark; the next ask
    // starts with a clean mark.
    let mut rt = main_runtime(AppState::default(), (1440.0, 900.0), true);
    ask(&mut rt, request());
    assert_eq!(
        element(&rt, "confirm-dialog-copy-sql").label.as_deref(),
        Some(t("query.editor.executionConfirm.copySql")),
        "the control starts as the copy label"
    );

    click(&mut rt, "confirm-dialog-copy-sql");
    assert!(
        rt.state.running_close_sql_copied,
        "the copy press marked the state"
    );
    assert_eq!(
        element(&rt, "confirm-dialog-copy-sql").label.as_deref(),
        Some("✓"),
        "the control shows the copied mark"
    );
    assert_eq!(
        element(&rt, "running-close-dialog-code-text").text.as_deref(),
        Some("SELECT pg_sleep(10)"),
        "copying does not answer the prompt"
    );
    assert!(
        rt.state.running_close_request.is_some(),
        "the prompt stays up after a copy"
    );

    // A new ask resets the mark (React's hook state is per confirm call).
    ask(&mut rt, request());
    assert_eq!(
        element(&rt, "confirm-dialog-copy-sql").label.as_deref(),
        Some(t("query.editor.executionConfirm.copySql")),
        "the next ask starts with the copy label"
    );
}

#[test]
fn a_long_sql_truncates_the_preview_and_shows_the_omission_hint() {
    // T070-tester S1 pin: >12 lines swaps in the first-12-lines preview and
    // mounts the `仅显示前 12 行` hint; the full SQL stays out of the tree.
    let long_sql = (1..=20u32)
        .map(|line| format!("SELECT {line}, 'x' AS pad_{line}"))
        .collect::<Vec<_>>()
        .join(";\n");
    let mut rt = main_runtime(AppState::default(), (1440.0, 900.0), true);
    ask(
        &mut rt,
        RunningExecutionCloseRequest { sql: long_sql, ..request() },
    );
    let hint = element(&rt, "running-close-dialog-code-truncated");
    assert_eq!(hint.text.as_deref(), Some("仅显示前 12 行"));
    let code_text = element(&rt, "running-close-dialog-code-text")
        .text
        .as_deref()
        .unwrap_or_default()
        .to_string();
    assert!(code_text.contains("SELECT 12"), "the first 12 lines are shown");
    assert!(
        !code_text.contains("SELECT 13"),
        "line 13 onward is cut off"
    );
}

#[test]
fn the_failure_branch_of_the_main_page_does_not_mount_the_host() {
    // T070-tester S2 pin: React renders the host only in the healthy
    // branch (MainPage.tsx:140); the load-failed window leaves it out.
    let mut state = AppState::default();
    state.connections_load_error = Some("load failed".to_string());
    let mut rt = main_runtime(state, (960.0, 640.0), true);
    ask(&mut rt, request());
    assert!(
        !has_key(&rt, "running-close-dialog-title"),
        "the host layer stays off the failure screen"
    );
}
