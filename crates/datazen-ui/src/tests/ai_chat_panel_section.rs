//! T009-S7: AiChatPanel `Section` wiring (the panel shell mounted in
//! the connection workspace, the toolbar toggle, and the host sync).
//!
//! Runtime tests render the real app view (`view` + `app_poll`) and
//! read back the laid-out element tree — never the code's own report
//! of itself. Geometry is asserted on `rt.elements()` rects, which
//! come from the layout engine; the memory seed and the draft
//! re-issue are asserted on the panel state the sync writes, with the
//! host-owned field flipped in between to prove the seed (not the VM
//! default) is what lands.

use super::*;
use crate::app_poll;
use crate::components::ai::{AiMemoryMode, AiPendingDraft};
use crate::data_grid::DataGrid;
use crate::sql_editor::SqlEditor;
use rune::{Env, Key, KeyEvent, Mods, Runtime};
use serde_json::Value;

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn drag<S>(rt: &mut Runtime<S>, key: &str, dx: f32) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_move(x + dx, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x + dx, y);
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn focused_key<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

/// Tab through the focus order until `key` is focused (at most 120 stops),
/// so keyboard tests do not depend on the exact tab order.
fn focus_by_tab<S>(rt: &mut Runtime<S>, key: &str) {
    for _ in 0..120 {
        if focused_key(rt).as_deref() == Some(key) {
            return;
        }
        press(rt, Key::Tab);
    }
    panic!("{key} never took focus after 120 Tabs");
}

/// Every laid-out element stays inside the window horizontally.
fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let r = e.rect;
        if r[0] < -0.5 || r[0] + r[2] > w + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) horizontally out of window {w}: {outside:?}",
        outside.len()
    );
}

/// The identity of an overflowing element for the open-vs-closed
/// comparison: its key when it has one, else its path with the
/// `workspace-with-ai-chat/workspace-content/` segment stripped —
/// opening the panel wraps the content in that row, so the same
/// anonymous node reports a longer path with the panel open.
fn overflow_id(e: &rune::ElementInfo) -> String {
    match &e.key {
        Some(key) => key.clone(),
        None => e
            .path
            .replace("workspace-with-ai-chat/workspace-content/", ""),
    }
}

/// The keys (or normalized paths, for anonymous nodes) of the elements
/// that leave the window horizontally. The SQL editor is a horizontal scroller by
/// design — `sql_editor_scrolls_a_surface_wider_than_the_viewport`
/// pins its field wider than its viewport — so its scroll content is
/// excluded here: the sweep's invariant is that opening the aside adds
/// no *new* overflow, not that the editor's document fits the window.
fn overflowing_keys<S>(rt: &Runtime<S>, w: f32) -> std::collections::BTreeSet<String> {
    let mut outside = std::collections::BTreeSet::new();
    let editor_path = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("sql-editor"))
        .map(|e| e.path);
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        if let Some(prefix) = &editor_path {
            // The editor's own box and its scroll viewport stay
            // asserted; only the scroll *content* under it is exempt.
            if e.path.starts_with(&format!("{prefix}/"))
                && e.key.as_deref() != Some("sql-editor-scroll")
            {
                continue;
            }
        }
        let r = e.rect;
        if r[0] < -0.5 || r[0] + r[2] > w + 0.5 {
            outside.insert(overflow_id(&e));
        }
    }
    outside
}

/// A runtime with the real poll attached, so the host sync runs on frames.
fn polled() -> Runtime<AppState> {
    let mut rt = runtime();
    rt.poll = Some(app_poll);
    rt
}

/// Open the workspace with the panel mounted. The `configured` flag is
/// the host seed: production leaves it `false` (no aiStore yet), so the
/// unconfigured view mounts; tests seed `true` for the full panel.
fn open_panel(rt: &mut Runtime<AppState>, configured: bool) {
    rt.state.active = Section::Workspace;
    rt.state.ai_chat_panel.set_is_configured(configured);
    rt.state.ai_chat_open = true;
    rt.frame(0.0);
}

#[test]
fn the_ai_toggle_mounts_and_unmounts_the_panel_in_the_workspace_section() {
    let mut rt = polled();
    rt.state.active = Section::Workspace;
    rt.frame(0.0);
    assert!(
        has_key(&rt, "conn-toolbar-ai"),
        "the workspace toolbar exposes the AI toggle"
    );
    assert!(!has_key(&rt, "ai-chat-aside"), "closed: no aside");
    assert!(
        !has_key(&rt, "ai-not-configured"),
        "closed: no unconfigured view"
    );

    // Production has no aiStore yet, so the mounted panel is the
    // unconfigured view — the mount itself is what this pins.
    click(&mut rt, "conn-toolbar-ai");
    assert!(rt.state.ai_chat_open, "the toggle flips the host flag");
    assert!(has_key(&rt, "ai-chat-aside"), "open: aside mounts");
    assert!(
        has_key(&rt, "ai-not-configured"),
        "open: unconfigured view mounts while AI is unconfigured"
    );
    assert!(
        !has_key(&rt, "ai-chat-panel"),
        "open: the full panel does not mount while unconfigured"
    );
    let body = element(&rt, "ai-chat-aside-body");
    assert!(
        (body.rect[2] - 320.0).abs() < 0.5,
        "aside body is React's 320pt default, got {:?}",
        body.rect
    );
    assert!(
        has_key(&rt, "splitter:ai-sidebar-width"),
        "the resize handle mounts with the aside"
    );

    click(&mut rt, "conn-toolbar-ai");
    assert!(!rt.state.ai_chat_open, "the toggle flips back");
    assert!(
        !has_key(&rt, "ai-chat-aside"),
        "closed again: aside unmounts"
    );
    assert!(
        !has_key(&rt, "ai-not-configured"),
        "closed again: unconfigured view unmounts"
    );

    // The toggle is a workspace action: other sections have no AI button.
    rt.state.active = Section::Connections;
    rt.frame(0.0);
    assert!(
        !has_key(&rt, "conn-toolbar-ai"),
        "the connections toolbar has no AI toggle"
    );
}

#[test]
fn the_configured_panel_mounts_with_its_react_key() {
    let mut rt = polled();
    open_panel(&mut rt, true);
    assert!(has_key(&rt, "ai-chat-panel"), "ai-chat-panel mounts");
    assert!(
        !has_key(&rt, "ai-not-configured"),
        "configured: no unconfigured view"
    );
    let body = element(&rt, "ai-chat-aside-body");
    let panel = element(&rt, "ai-chat-panel");
    assert!(
        (body.rect[2] - 320.0).abs() < 0.5,
        "aside body is 320pt, got {:?}",
        body.rect
    );
    assert!(
        panel.rect[0] >= body.rect[0] - 0.5
            && panel.rect[0] + panel.rect[2] <= body.rect[0] + body.rect[2] + 0.5,
        "panel {:?} stays inside the aside body {:?}",
        panel.rect,
        body.rect
    );
    assert!(
        panel.rect[0] + panel.rect[2] <= 1440.5,
        "panel {:?} stays inside the window",
        panel.rect
    );
    let handle = element(&rt, "splitter:ai-sidebar-width");
    assert_eq!(handle.role, "separator", "the handle is a separator");
    assert!(
        (handle.rect[2] - 1.0).abs() < 0.5,
        "the handle is React's 1pt bar, got {:?}",
        handle.rect
    );
}

#[test]
fn the_panel_does_not_mount_outside_the_workspace_section() {
    let mut rt = polled();
    rt.state.ai_chat_panel.set_is_configured(true);
    rt.state.ai_chat_open = true;
    rt.state.active = Section::Connections;
    rt.frame(0.0);
    assert!(!has_key(&rt, "ai-chat-aside"), "no aside on connections");
    assert!(!has_key(&rt, "ai-chat-panel"), "no panel on connections");
}

#[test]
fn the_host_seeds_memory_enabled_from_its_own_setting() {
    // Tester S6 non-blocking #3: the VM defaults `memory_enabled` to
    // `true` (effective 'auto'), but React's memory is opt-in, so the
    // host must seed from its own setting — never the VM default.
    let mut rt = polled();
    open_panel(&mut rt, true);
    assert_eq!(
        rt.state.ai_chat_panel.effective_memory_mode(),
        AiMemoryMode::Off,
        "host default false seeds 'off' over the VM's true default"
    );
    assert!(
        !rt.state.ai_chat_panel.memory_enabled(),
        "the panel carries the host value, not its own default"
    );

    rt.state.ai_memory_enabled = true;
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.effective_memory_mode(),
        AiMemoryMode::Auto,
        "flipping the host setting flips the effective mode"
    );

    rt.state.ai_memory_enabled = false;
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.effective_memory_mode(),
        AiMemoryMode::Off,
        "the seed is re-applied every poll, not written once"
    );
}

#[test]
fn the_draft_bridge_reissues_when_the_input_clears() {
    // Tester S6 non-blocking #4: React's draft effect re-runs when
    // `input` changes, so a draft parked as a conflict prefills as
    // soon as the input clears.
    let mut rt = polled();
    open_panel(&mut rt, true);
    rt.state
        .ai_chat_panel
        .area_mut()
        .set_input("user is typing");
    rt.state
        .open_ai_chat_draft(AiPendingDraft::new("draft-1", "explain this error"));
    rt.frame(0.0);
    assert_eq!(
        rt.state
            .ai_chat_panel
            .area()
            .pending_conflict()
            .map(|draft| draft.request_id.as_str()),
        Some("draft-1"),
        "non-empty input parks the draft as a conflict"
    );
    assert!(
        rt.state.ai_chat_panel.consumed_draft_ids().is_empty(),
        "parked: nothing acked yet"
    );
    assert!(
        rt.state.ai_pending_draft.is_some(),
        "parked: the host still holds the draft"
    );

    // Dismissing the conflict while the input stays non-empty must
    // stick: the issue key did not change, so the host re-issues
    // nothing (React's effect deps did not change either).
    rt.state.ai_chat_panel.dismiss_conflict();
    rt.frame(0.0);
    assert!(
        rt.state.ai_chat_panel.area().pending_conflict().is_none(),
        "dismissed conflict stays dismissed while the input is non-empty"
    );

    // Clearing the input flips the issue key: the host re-issues, the
    // draft prefills, the ack lands, and the host clears its hold —
    // all in the same poll pass.
    rt.state.ai_chat_panel.area_mut().set_input("");
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.area().input_text(),
        "explain this error",
        "cleared input: the draft prefills"
    );
    assert_eq!(
        rt.state.ai_chat_panel.consumed_draft_ids(),
        ["draft-1"],
        "prefill acks the draft"
    );
    assert!(
        rt.state.ai_pending_draft.is_none(),
        "acked: the host clears its pending draft"
    );
    assert!(
        !rt.state.ai_chat_panel.take_focus_input_request(),
        "the prefill's focus request was consumed by the host sync"
    );
    // The caret lands in the input's inner editable: kit's TextInput
    // keys it `input` under the `ai-input-text` field, and `tx.focus`
    // takes the first focusable child — so the focused element's path
    // must run through the field, not the field key itself.
    let focused = rt
        .elements()
        .into_iter()
        .find(|e| e.focused)
        .expect("something took focus after the prefill");
    assert!(
        focused.path.contains("ai-input-text"),
        "the prefill focuses the input field, got key {:?} path {:?}",
        focused.key,
        focused.path
    );
}

#[test]
fn closing_the_panel_cancels_the_in_flight_run_once() {
    let mut rt = polled();
    open_panel(&mut rt, true);
    rt.state
        .ai_chat_panel
        .set_request_id(Some("req-1".to_string()));
    rt.frame(0.0);
    assert!(
        rt.state.ai_chat_panel.cancelled_request_ids().is_empty(),
        "mounted: nothing cancelled"
    );

    click(&mut rt, "conn-toolbar-ai");
    assert!(!rt.state.ai_chat_open, "the toggle closed the panel");
    // The poll runs before the frame's queued input is processed, so
    // the unmount cancel lands on the next poll, not the click's own
    // frame (production polls every frame, so the delay is one frame).
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.cancelled_request_ids(),
        ["req-1"],
        "closing cancels the in-flight run (React's unmount effect)"
    );
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.cancelled_request_ids(),
        ["req-1"],
        "hidden: the cancel does not repeat on later polls"
    );

    // Leaving the section while open cancels the same way.
    click(&mut rt, "conn-toolbar-ai");
    rt.state
        .ai_chat_panel
        .set_request_id(Some("req-2".to_string()));
    rt.frame(0.0);
    rt.state.active = Section::Connections;
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.cancelled_request_ids(),
        ["req-1", "req-2"],
        "leaving the workspace cancels the new run once"
    );
}

#[test]
fn the_unconfigured_settings_button_opens_the_ai_settings_section() {
    let mut rt = polled();
    open_panel(&mut rt, false);
    click(&mut rt, "ai-not-configured-settings");
    assert_eq!(
        rt.state.active,
        Section::Settings,
        "the button leaves the workspace for settings"
    );
    assert_eq!(
        rt.state.settings_section(),
        "ai",
        "the button deep-links the AI section"
    );
}

#[test]
fn the_auto_context_snapshot_follows_the_live_query_panel() {
    let mut rt = polled();
    open_panel(&mut rt, true);
    assert!(
        rt.state
            .open_query_tab("Query")
            .is_some_and(|_| query_selected(&rt.state)),
        "a query tab is selected"
    );
    rt.state.sql = SqlEditor::from_doc("SELECT id FROM customers");
    let mut grid = DataGrid::new();
    grid.load_page(&datazen_ui_api::TablePage {
        table: "customers".into(),
        columns: vec!["id".into(), "name".into()],
        rows: vec![
            vec!["1".into(), "Ada".into()],
            vec!["2".into(), "Bob".into()],
        ],
        page: 0,
        page_size: 25,
        total_rows: 2,
        sort: None,
        filter: None,
        primary_key: vec!["id".into()],
        read_only: None,
    });
    rt.state.result_grid = Some(grid);
    rt.frame(0.0);
    let snapshot = rt
        .state
        .ai_chat_panel
        .area()
        .snapshot()
        .expect("the live query panel seeds a snapshot");
    assert_eq!(
        snapshot.editor_sql.as_deref(),
        Some("SELECT id FROM customers"),
        "the editor SQL is the snapshot's editor_sql"
    );
    let preview = snapshot
        .result_preview
        .as_ref()
        .expect("the result grid seeds a preview");
    assert_eq!(preview.columns, ["id", "name"]);
    assert_eq!(preview.rows.len(), 2, "the preview keeps the head rows");
    assert_eq!(preview.rows[0][0], Value::String("1".into()));
    assert_eq!(preview.rows[1][1], Value::String("Bob".into()));

    // Leaving the query tab clears the host snapshot (React passes
    // `undefined` when no query panel is active).
    let _ = rt
        .state
        .tabs
        .open(datazen_ui_api::TabKind::Table, "customers");
    rt.frame(0.0);
    assert!(!query_selected(&rt.state), "a table tab is selected now");
    assert!(
        rt.state.ai_chat_panel.area().snapshot().is_none(),
        "no query panel: the snapshot clears"
    );
}

#[test]
fn dragging_the_handle_left_widens_the_aside() {
    // The aside sits on the right edge, so dragging left grows it —
    // React's `useResizable({ reverse: true })`.
    let mut rt = polled();
    open_panel(&mut rt, true);
    let before = element(&rt, "ai-chat-aside-body");
    assert!(
        (before.rect[2] - 320.0).abs() < 0.5,
        "starts at the 320pt default, got {:?}",
        before.rect
    );
    drag(&mut rt, "splitter:ai-sidebar-width", -40.0);
    let after = element(&rt, "ai-chat-aside-body");
    assert!(
        (after.rect[2] - 360.0).abs() < 1.5,
        "40pt left drag widens 320 to 360, got {:?}",
        after.rect
    );
    drag(&mut rt, "splitter:ai-sidebar-width", 40.0);
    let back = element(&rt, "ai-chat-aside-body");
    assert!(
        (back.rect[2] - 320.0).abs() < 1.5,
        "40pt right drag narrows back to 320, got {:?}",
        back.rect
    );
}

#[test]
fn the_left_arrow_grows_the_reversed_handle() {
    // Keyboard twin of the drag: on a right-edge handle Left grows.
    let mut rt = polled();
    open_panel(&mut rt, true);
    focus_by_tab(&mut rt, "splitter:ai-sidebar-width");
    press(&mut rt, Key::Left);
    let grown = element(&rt, "ai-chat-aside-body");
    assert!(
        (grown.rect[2] - 321.0).abs() < 1.5,
        "Left grows the aside by one point, got {:?}",
        grown.rect
    );
    press(&mut rt, Key::Right);
    let back = element(&rt, "ai-chat-aside-body");
    assert!(
        (back.rect[2] - 320.0).abs() < 1.5,
        "Right shrinks it back, got {:?}",
        back.rect
    );
}

#[test]
fn the_panel_fits_both_window_sizes_in_both_themes() {
    // §5 DoD: 960×640 and 1440×900, dark and light, configured and
    // unconfigured. Two invariants: the aside itself stays inside the
    // window, and opening it adds no *new* horizontal overflow — the
    // query workspace already overflows at 960 without the panel (its
    // toolbar row does not wrap) and the editor's scroll content is
    // wider than its viewport by design, so the sweep compares the
    // overflowing-key set against the panel-closed control instead of
    // demanding zero overflow where the product never had it.
    for (width, height) in [(1440.0, 900.0), (960.0, 640.0)] {
        for dark in [true, false] {
            for configured in [true, false] {
                let mut rt = Runtime::new(
                    AppState::default(),
                    view,
                    (width, height),
                    Env {
                        system_dark: dark,
                        scale: 1.0,
                    },
                );
                rt.poll = Some(app_poll);
                rt.state.active = Section::Workspace;
                let _ = rt.state.open_query_tab("Query");
                rt.state.ai_chat_panel.set_is_configured(configured);
                rt.frame(0.0);
                let label = format!("{width}x{height} dark={dark} configured={configured}");
                let closed = overflowing_keys(&rt, width);
                rt.state.ai_chat_open = true;
                rt.frame(0.0);
                let open = overflowing_keys(&rt, width);
                assert!(
                    open.is_subset(&closed),
                    "{label}: opening the aside adds overflow: {:?} (closed: {:?})",
                    open.difference(&closed).collect::<Vec<_>>(),
                    closed
                );
                let body = element(&rt, "ai-chat-aside-body");
                assert!(
                    body.rect[0] + body.rect[2] <= width + 0.5,
                    "{label}: aside body {:?} hangs past the right edge",
                    body.rect
                );
                assert!(
                    (body.rect[2] - 320.0).abs() < 0.5,
                    "{label}: aside body is 320pt, got {:?}",
                    body.rect
                );
                if configured {
                    assert!(has_key(&rt, "ai-chat-panel"), "{label}: panel mounts");
                } else {
                    assert!(
                        has_key(&rt, "ai-not-configured"),
                        "{label}: unconfigured view mounts"
                    );
                }
            }
        }
    }

    // Empty workspace (no tab open) at both sizes: no editor, no
    // toolbar, so the strict form holds — nothing leaves the window.
    for (width, height) in [(1440.0, 900.0), (960.0, 640.0)] {
        let mut rt = Runtime::new(
            AppState::default(),
            view,
            (width, height),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.poll = Some(app_poll);
        rt.state.active = Section::Workspace;
        rt.state.ai_chat_panel.set_is_configured(true);
        rt.state.ai_chat_open = true;
        rt.frame(0.0);
        assert_no_horizontal_overflow(&rt, width, &format!("{width}x{height} empty workspace"));
        assert!(
            has_key(&rt, "workspace-empty"),
            "the empty state still shows"
        );
        assert!(has_key(&rt, "ai-chat-panel"), "the panel mounts beside it");
    }
}

#[test]
fn insert_ai_sql_replaces_the_query_editor_only_with_a_query_tab() {
    // The `onInsertSql` host method, unit-tested directly: its UI
    // trigger (an assistant message's insert action) does not exist
    // until T169, so no runtime test can drive it.
    let mut state = AppState::default();
    let before = state.sql.doc.clone();
    state.insert_ai_sql("SELECT 1");
    assert_eq!(
        state.sql.doc, before,
        "no query tab: the editor is untouched"
    );
    let _ = state.open_query_tab("Query");
    state.insert_ai_sql("SELECT 1");
    assert_eq!(
        state.sql.doc, "SELECT 1",
        "a query tab: the editor SQL is replaced, not appended to"
    );
}

#[test]
fn open_ai_new_query_opens_a_tab_prefilled_with_the_code() {
    // The `onNewQuery` host method, unit-tested directly for the same
    // T169 reason as `insert_ai_sql`.
    let mut state = AppState::default();
    state.open_ai_new_query("SELECT 2");
    assert!(query_selected(&state), "a query tab opened");
    assert_eq!(state.sql.doc, "SELECT 2", "the new tab is prefilled");
}

#[test]
fn open_ai_chat_draft_opens_the_panel_from_closed() {
    // The `if !ai_chat_open` branch of the draft entry: a draft
    // arriving while the panel is closed opens it (React's
    // `openAiChatDraft` sets `aiChatOpen(true)`).
    let mut state = AppState::default();
    assert!(!state.ai_chat_open, "closed to start");
    state.open_ai_chat_draft(AiPendingDraft::new("d-9", "explain this"));
    assert!(state.ai_chat_open, "the draft opens the panel");
    assert_eq!(
        state
            .ai_pending_draft
            .as_ref()
            .map(|d| d.request_id.as_str()),
        Some("d-9"),
        "the host holds the draft for the sync"
    );
}

#[test]
fn a_connection_change_rekeys_the_panel_session() {
    // The session-key effect: a new live connection id re-keys the
    // session (`connectionId::database`), and the host sync reports
    // the change.
    let mut rt = polled();
    open_panel(&mut rt, true);
    assert_eq!(
        rt.state.ai_chat_panel.session_key(),
        Some("default::"),
        "no connection: the 'default' owner"
    );
    rt.state.desktop_connection = Some("conn-1".into());
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.connection_id(),
        Some("conn-1"),
        "the sync writes the live connection id"
    );
    assert_eq!(
        rt.state.ai_chat_panel.session_key(),
        Some("conn-1::"),
        "the session re-keys on the connection"
    );
}
