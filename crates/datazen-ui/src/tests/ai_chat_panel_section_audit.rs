//! T009-S7 independent audit: fresh vectors against the React source
//! (`ContentViewDrawers.tsx`, `ContentToolbar.tsx`, `AiChatPanel.tsx`,
//! `ContentView.tsx`, `types/ai.ts`) and the zh-CN packs.
//!
//! Deliberately different angles from the coder's
//! `ai_chat_panel_section.rs`: the draft issue-key components the
//! coder did not isolate (reopen reset, configured-flag flip), the
//! snapshot caps (20 columns / 5 rows / 500 chars), the table-tab and
//! blank-editor snapshot branches, the never-mounted and closed-panel
//! sync branches, the handle clamps, the aside/content geometry at
//! 1440x816 (not the coder's 1440x900), and the 960x640 squeeze traced
//! to its pre-existing source. Geometry is asserted on `rt.elements()`
//! rects from the layout engine, never on the code's own report.

use super::*;
use crate::app_poll;
use crate::components::ai::{AiPendingDraft};
use crate::data_grid::DataGrid;
use crate::sql_editor::{Sel, Selection, SqlEditor};
use rune::{Env, Runtime};

fn audit_click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn audit_drag<S>(rt: &mut Runtime<S>, key: &str, dx: f32) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_move(x + dx, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x + dx, y);
    rt.frame(1.0 / 60.0);
}

fn audit_el<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn audit_has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

/// Elements leaving the window horizontally, keyed by key (or path for
/// anonymous nodes). The SQL editor is a horizontal scroller by
/// design, so its scroll *content* is exempt — exactly the coder's
/// invariant, re-derived here: opening the aside must add no *new*
/// overflow.
fn audit_overflow<S>(rt: &Runtime<S>, w: f32) -> std::collections::BTreeSet<String> {
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
            if e.path.starts_with(&format!("{prefix}/"))
                && e.key.as_deref() != Some("sql-editor-scroll")
            {
                continue;
            }
        }
        let r = e.rect;
        if r[0] < -0.5 || r[0] + r[2] > w + 0.5 {
            // Opening the panel wraps the content in
            // `workspace-with-ai-chat/workspace-content/`; strip that
            // segment so the same anonymous node compares equal open
            // vs closed.
            let id = e.key.clone().unwrap_or_else(|| {
                e.path
                    .replace("workspace-with-ai-chat/workspace-content/", "")
            });
            outside.insert(id);
        }
    }
    outside
}

fn audit_polled() -> Runtime<AppState> {
    let mut rt = runtime();
    rt.poll = Some(app_poll);
    rt
}

fn audit_open(rt: &mut Runtime<AppState>, configured: bool) {
    rt.state.active = Section::Workspace;
    rt.state.ai_chat_panel.set_is_configured(configured);
    rt.state.ai_chat_open = true;
    rt.frame(0.0);
}

/// A grid with `cols` columns (`c00`…) and `rows` rows of `r{row}c{col}`
/// cells; `long` optionally replaces one cell with `len` `x` characters.
fn audit_grid(cols: usize, rows: usize, long: Option<(usize, usize, usize)>) -> DataGrid {
    let mut grid = DataGrid::new();
    grid.load_page(&datazen_ui_api::TablePage {
        table: "t".into(),
        columns: (0..cols).map(|c| format!("c{c:02}")).collect(),
        rows: (0..rows)
            .map(|r| {
                (0..cols)
                    .map(|c| {
                        let is_long = long.map_or(false, |(lr, lc, _)| lr == r && lc == c);
                        if is_long {
                            "x".repeat(long.unwrap().2)
                        } else {
                            format!("r{r}c{c}")
                        }
                    })
                    .collect()
            })
            .collect(),
        page: 0,
        page_size: 25,
        total_rows: rows as u32,
        sort: None,
        filter: None,
        primary_key: vec!["c00".into()],
        read_only: None,
    });
    grid
}

#[test]
fn audit_reopening_after_dismiss_reissues_the_parked_draft() {
    // The toggle's open branch resets the draft issue key (React's
    // remount re-runs the draft effect from a fresh ref). The coder
    // proved dismiss-sticks while open; this proves reopen re-issues.
    let mut rt = audit_polled();
    audit_open(&mut rt, true);
    rt.state
        .ai_chat_panel
        .area_mut()
        .set_input("user is typing");
    rt.state
        .open_ai_chat_draft(AiPendingDraft::new("draft-1", "explain this error"));
    rt.frame(0.0);
    assert!(
        rt.state
            .ai_chat_panel
            .area()
            .pending_conflict()
            .is_some(),
        "non-empty input parks the draft as a conflict"
    );
    rt.state.ai_chat_panel.dismiss_conflict();
    rt.frame(0.0);
    assert!(
        rt.state.ai_chat_panel.area().pending_conflict().is_none(),
        "dismissed while open"
    );

    audit_click(&mut rt, "conn-toolbar-ai");
    assert!(!rt.state.ai_chat_open, "closed");
    audit_click(&mut rt, "conn-toolbar-ai");
    assert!(rt.state.ai_chat_open, "reopened");
    // The click's own frame polled before the toggle landed, so the
    // re-issue needs one more poll (same one-frame lag the coder's
    // cancel test documents).
    rt.frame(0.0);
    assert_eq!(
        rt.state
            .ai_chat_panel
            .area()
            .pending_conflict()
            .map(|draft| draft.request_id.as_str()),
        Some("draft-1"),
        "reopen re-issues the still-pending draft without any input change"
    );

    // And the re-issued draft still follows the clear-input path.
    rt.state.ai_chat_panel.area_mut().set_input("");
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.area().input_text(),
        "explain this error",
        "cleared input: the draft prefills"
    );
    assert!(
        rt.state.ai_pending_draft.is_none(),
        "acked: the host clears its hold"
    );
}

#[test]
fn audit_draft_held_while_unconfigured_prefills_once_configured() {
    // `handle_draft_request` is a no-op while unconfigured (VM guard),
    // but the host must keep holding the draft: the issue key's
    // configured-flag component re-issues it when the flag flips.
    let mut rt = audit_polled();
    audit_open(&mut rt, false);
    rt.state
        .open_ai_chat_draft(AiPendingDraft::new("draft-2", "why is this slow?"));
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.area().input_text(),
        "",
        "unconfigured: no prefill"
    );
    assert!(
        rt.state.ai_chat_panel.consumed_draft_ids().is_empty(),
        "unconfigured: no ack"
    );
    assert!(
        rt.state.ai_pending_draft.is_some(),
        "unconfigured: the host still holds the draft"
    );

    rt.state.ai_chat_panel.set_is_configured(true);
    rt.frame(0.0);
    assert!(
        audit_has(&rt, "ai-chat-panel"),
        "configured: the full panel mounts"
    );
    assert_eq!(
        rt.state.ai_chat_panel.area().input_text(),
        "why is this slow?",
        "flipping configured re-issues the held draft"
    );
    assert_eq!(
        rt.state.ai_chat_panel.consumed_draft_ids(),
        ["draft-2"],
        "prefill acks the draft"
    );
    assert!(
        rt.state.ai_pending_draft.is_none(),
        "acked: the host clears its hold"
    );
}

#[test]
fn audit_draft_prefills_immediately_on_an_empty_input() {
    // The direct path (coder only drove the re-issue path): empty
    // input, draft arrives, prefill + ack + focus in one poll pass.
    let mut rt = audit_polled();
    audit_open(&mut rt, true);
    rt.state
        .open_ai_chat_draft(AiPendingDraft::new("draft-3", "summarize this table"));
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.area().input_text(),
        "summarize this table",
        "empty input: the draft prefills at once"
    );
    assert_eq!(
        rt.state.ai_chat_panel.consumed_draft_ids(),
        ["draft-3"],
        "prefill acks the draft"
    );
    assert!(
        rt.state.ai_pending_draft.is_none(),
        "acked: the host clears its hold"
    );
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
fn audit_snapshot_truncates_to_react_caps() {
    // React `ContentViewDrawers.tsx:186-224`: 20 columns, 5 rows, 20
    // cells per row, 500 chars per cell.
    let mut rt = audit_polled();
    audit_open(&mut rt, true);
    assert!(rt.state.open_query_tab("Query").is_some());
    rt.state.sql = SqlEditor::from_doc("SELECT * FROM wide");
    rt.state.result_grid = Some(audit_grid(25, 8, Some((2, 19, 600))));
    rt.frame(0.0);
    let snapshot = rt
        .state
        .ai_chat_panel
        .area()
        .snapshot()
        .expect("the live query panel seeds a snapshot");
    let preview = snapshot
        .result_preview
        .as_ref()
        .expect("the result grid seeds a preview");
    assert_eq!(preview.columns.len(), 20, "columns cap at 20");
    assert_eq!(preview.columns[19], "c19", "the 20th column kept");
    assert_eq!(preview.rows.len(), 5, "rows cap at 5");
    assert_eq!(preview.rows[0].len(), 20, "cells per row cap at 20");
    assert_eq!(
        preview.rows[2][19].as_str().unwrap().len(),
        500,
        "the 600-char cell truncates to 500 chars"
    );
    assert!(
        preview.rows[2][19].as_str().unwrap().chars().all(|c| c == 'x'),
        "truncation keeps the head of the cell"
    );
    assert_eq!(preview.rows[4][0], serde_json::Value::String("r4c0".into()));
}

#[test]
fn audit_snapshot_survives_a_blank_editor_and_dies_without_content() {
    let mut rt = audit_polled();
    audit_open(&mut rt, true);
    assert!(rt.state.open_query_tab("Query").is_some());
    // Whitespace-only editor + a loaded grid: the snapshot still
    // exists (React: `editorSql || … || resultPreview`), with no
    // editor_sql of its own.
    rt.state.sql = SqlEditor::from_doc("   \n  ");
    rt.state.result_grid = Some(audit_grid(2, 1, None));
    rt.frame(0.0);
    let snapshot = rt
        .state
        .ai_chat_panel
        .area()
        .snapshot()
        .expect("grid content alone seeds a snapshot");
    assert_eq!(snapshot.editor_sql, None, "blank editor contributes no SQL");
    assert!(snapshot.result_preview.is_some(), "the preview carries it");

    // Nothing at all: no snapshot (React passes `undefined`).
    rt.state.sql = SqlEditor::from_doc("");
    rt.state.result_grid = None;
    rt.frame(0.0);
    assert!(
        rt.state.ai_chat_panel.area().snapshot().is_none(),
        "no content anywhere: the snapshot clears"
    );
}

#[test]
fn audit_snapshot_uses_the_editor_selection() {
    let mut rt = audit_polled();
    audit_open(&mut rt, true);
    assert!(rt.state.open_query_tab("Query").is_some());
    rt.state.sql = SqlEditor::from_doc("SELECT id, name FROM t");
    rt.state.sql.selection = Selection {
        ranges: vec![Sel { anchor: 0, head: 9 }],
        main: 0,
    };
    rt.frame(0.0);
    let snapshot = rt
        .state
        .ai_chat_panel
        .area()
        .snapshot()
        .expect("editor content seeds a snapshot");
    assert_eq!(
        snapshot.selection.as_deref(),
        Some("SELECT id"),
        "the trimmed selection is the snapshot's selection"
    );
    assert_eq!(
        snapshot.editor_sql.as_deref(),
        Some("SELECT id, name FROM t"),
        "the full editor SQL rides alongside"
    );
}

#[test]
fn audit_snapshot_clears_on_a_table_tab_and_returns_on_the_query_tab() {
    let mut rt = audit_polled();
    audit_open(&mut rt, true);
    let query_id = rt.state.open_query_tab("Query").expect("query tab opens");
    rt.state.sql = SqlEditor::from_doc("SELECT 1");
    rt.state.result_grid = Some(audit_grid(1, 1, None));
    rt.frame(0.0);
    assert!(
        rt.state.ai_chat_panel.area().snapshot().is_some(),
        "query tab: snapshot present"
    );
    // A table tab with the same grid loaded: React's memo requires a
    // query panel, so the snapshot must clear even though the grid is
    // still there.
    let table_id = rt
        .state
        .tabs
        .open(datazen_ui_api::TabKind::Table, "customers")
        .expect("table tab opens");
    assert_ne!(table_id, query_id);
    rt.frame(0.0);
    assert!(!query_selected(&rt.state), "a table tab is selected now");
    assert!(
        rt.state.ai_chat_panel.area().snapshot().is_none(),
        "table tab: the snapshot clears despite the loaded grid"
    );
    assert!(rt.state.select_tab(query_id), "back to the query tab");
    rt.frame(0.0);
    assert!(
        rt.state.ai_chat_panel.area().snapshot().is_some(),
        "query tab again: the snapshot returns"
    );
}

#[test]
fn audit_never_mounted_panel_records_no_cancel_and_remounts() {
    // The sync's mount tracking must not fire the unmount cancel for
    // a panel that was never mounted — and `ai_chat_open` survives a
    // section switch (unlike React, where leaving the connection view
    // drops it), so the panel re-mounts on return.
    let mut rt = audit_polled();
    rt.state.ai_chat_panel.set_is_configured(true);
    rt.state.ai_chat_open = true;
    rt.state.active = Section::Connections;
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(
        rt.state.ai_chat_panel.cancelled_request_ids().is_empty(),
        "never mounted: no cancel"
    );
    assert!(!rt.state.ai_chat_mounted, "never mounted: flag stays false");

    rt.state.active = Section::Workspace;
    rt.frame(0.0);
    assert!(audit_has(&rt, "ai-chat-panel"), "workspace: panel mounts");
    // Seed the in-flight run only now: the mount's ensure_session
    // clears a request id set before it (fresh session).
    rt.state
        .ai_chat_panel
        .set_request_id(Some("req-9".to_string()));
    rt.frame(0.0);
    assert!(
        rt.state.ai_chat_panel.cancelled_request_ids().is_empty(),
        "mounted: still nothing cancelled"
    );

    rt.state.active = Section::Settings;
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.cancelled_request_ids(),
        ["req-9"],
        "leaving for settings cancels the in-flight run once"
    );
    assert!(rt.state.ai_chat_open, "the open flag survives the switch");
    rt.state.active = Section::Workspace;
    // A direct poke bypasses the event handlers, so the runtime only
    // re-renders when the poll reports a change — and this return
    // changes nothing the poll tracks. `settle_all` forces the
    // commit (the S2 audit's pinned rule); without it the tree stays
    // on the stale settings page and the panel looks unmounted.
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        audit_has(&rt, "ai-chat-panel"),
        "back in the workspace: the panel re-mounts"
    );
}

#[test]
fn audit_closed_panel_ignores_connection_changes() {
    // The prop writes run only while mounted: a connection arriving
    // while the panel is closed must not reach the panel state.
    let mut rt = audit_polled();
    rt.state.active = Section::Workspace;
    rt.state.desktop_connection = Some("conn-9".into());
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.connection_id(),
        None,
        "closed: the live connection id is not written in"
    );
    assert_eq!(
        rt.state.ai_chat_panel.session_key(),
        None,
        "closed: no session is ensured either"
    );
    assert!(!rt.state.ai_chat_mounted, "closed: never mounted");
}

#[test]
fn audit_insert_replaces_and_new_query_adds_a_second_tab() {
    let mut state = AppState::default();
    let first = state.open_query_tab("Query").expect("first query tab");
    state.sql = SqlEditor::from_doc("SELECT a");
    state.insert_ai_sql("SELECT b");
    assert_eq!(state.sql.doc, "SELECT b", "insert replaces, never appends");
    state.open_ai_new_query("SELECT c");
    assert_eq!(state.tabs.tabs().len(), 2, "new query opens a second tab");
    assert_ne!(
        state.tabs.selected().map(|tab| tab.id),
        Some(first),
        "the new tab is selected"
    );
    assert!(query_selected(&state), "the new tab is a query tab");
    assert_eq!(state.sql.doc, "SELECT c", "the new tab is prefilled");

    // On a table tab the insert is a no-op (React only calls it for a
    // `query` panel) and the document is untouched.
    let table_id = state
        .tabs
        .open(datazen_ui_api::TabKind::Table, "customers")
        .expect("table tab opens");
    assert!(state.select_tab(table_id));
    state.insert_ai_sql("SELECT x");
    assert_eq!(
        state.sql.doc, "SELECT c",
        "table tab: the editor is untouched"
    );
}

#[test]
fn audit_handle_drag_clamps_at_both_ends() {
    let mut rt = audit_polled();
    audit_open(&mut rt, true);
    audit_drag(&mut rt, "splitter:ai-sidebar-width", -100.0);
    let grown = audit_el(&rt, "ai-chat-aside-body");
    assert!(
        (grown.rect[2] - 420.0).abs() < 1.5,
        "100pt left drag widens 320 to 420, got {:?}",
        grown.rect
    );
    audit_drag(&mut rt, "splitter:ai-sidebar-width", -280.0);
    let maxed = audit_el(&rt, "ai-chat-aside-body");
    assert!(
        (maxed.rect[2] - 600.0).abs() < 1.5,
        "drag past the max clamps at React's 600, got {:?}",
        maxed.rect
    );
    audit_drag(&mut rt, "splitter:ai-sidebar-width", 360.0);
    let mined = audit_el(&rt, "ai-chat-aside-body");
    assert!(
        (mined.rect[2] - 240.0).abs() < 1.5,
        "drag past the min clamps at React's 240, got {:?}",
        mined.rect
    );
}

#[test]
fn audit_aside_order_and_content_shrink_at_both_sizes_both_themes() {
    // 1440x816 (the task's size, not the coder's 1440x900) and
    // 960x640, dark and light: the handle sits left of the body, the
    // content column ends where the handle starts, the body is 320pt
    // ending at the window's right edge, and the panel stays inside
    // the body. The root order is read off rects (layout output), not
    // the builder's child order.
    for (width, height) in [(1440.0, 816.0), (960.0, 640.0)] {
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
                // Closed control: the query tab's own rect. Opening the
                // aside must change neither the row's width nor its
                // height — at 816 the 728-tall row overflows the window
                // vertically, but the query tab is 728 tall closed too,
                // so that overflow is the query workspace's own.
                let closed_tab = audit_el(&rt, "query-tab");
                rt.state.ai_chat_open = true;
                rt.frame(0.0);
                let handle = audit_el(&rt, "splitter:ai-sidebar-width");
                let body = audit_el(&rt, "ai-chat-aside-body");
                let content = audit_el(&rt, "workspace-content");
                let row = audit_el(&rt, "workspace-with-ai-chat");
                assert!(
                    handle.rect[0] + handle.rect[2] <= body.rect[0] + 0.5,
                    "{label}: handle {:?} sits left of the body {:?}",
                    handle.rect,
                    body.rect
                );
                assert!(
                    content.rect[0] + content.rect[2] <= handle.rect[0] + 0.5,
                    "{label}: content {:?} ends where the handle {:?} starts",
                    content.rect,
                    handle.rect
                );
                assert!(
                    (body.rect[2] - 320.0).abs() < 0.5,
                    "{label}: aside body is 320pt, got {:?}",
                    body.rect
                );
                assert!(
                    (body.rect[0] + body.rect[2] - (row.rect[0] + row.rect[2])).abs()
                        < 0.5,
                    "{label}: aside body {:?} ends at the row's right edge {:?}",
                    body.rect,
                    row.rect
                );
                assert!(
                    (body.rect[3] - row.rect[3]).abs() < 0.5,
                    "{label}: aside body {:?} inherits the row height {:?}",
                    body.rect,
                    row.rect
                );
                assert!(
                    (row.rect[2] - closed_tab.rect[2]).abs() < 0.5
                        && (row.rect[3] - closed_tab.rect[3]).abs() < 0.5,
                    "{label}: opening keeps the row {:?} the closed tab's size {:?}",
                    row.rect,
                    closed_tab.rect
                );
                assert!(
                    content.rect[2] > 0.0,
                    "{label}: the content column keeps positive width, got {:?}",
                    content.rect
                );
                if configured {
                    let panel = audit_el(&rt, "ai-chat-panel");
                    assert!(
                        panel.rect[0] >= body.rect[0] - 0.5
                            && panel.rect[0] + panel.rect[2]
                                <= body.rect[0] + body.rect[2] + 0.5,
                        "{label}: panel {:?} stays inside the body {:?}",
                        panel.rect,
                        body.rect
                    );
                }
            }
        }
    }
}

#[test]
fn audit_960_squeeze_is_preexisting_and_the_toggle_stays_inside() {
    // The coder noted the 960x640 query toolbar / query area squeeze.
    // This pins down whose it is: with the panel *closed* the query
    // workspace already overflows at 960 (the toolbar row does not
    // wrap — pre-existing layout, untouched by S7 except for one more
    // child), the S7-added toggle itself stays inside the window in
    // both states, and opening the aside adds no new overflow.
    let mut rt = Runtime::new(
        AppState::default(),
        view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.poll = Some(app_poll);
    rt.state.active = Section::Workspace;
    let _ = rt.state.open_query_tab("Query");
    rt.state.ai_chat_panel.set_is_configured(true);
    rt.frame(0.0);
    let closed = audit_overflow(&rt, 960.0);
    eprintln!("audit 960x640 closed-panel overflow: {closed:?}");
    assert!(
        !closed.is_empty(),
        "the squeeze exists with the panel closed: pre-existing, not S7-introduced"
    );
    assert!(
        !closed.contains("conn-toolbar-ai"),
        "the S7-added toggle does not itself overflow while closed: {closed:?}"
    );
    let toggle_closed = audit_el(&rt, "conn-toolbar-ai");
    assert!(
        toggle_closed.rect[0] + toggle_closed.rect[2] <= 960.5,
        "toggle {:?} stays inside the 960 window while closed",
        toggle_closed.rect
    );

    rt.state.ai_chat_open = true;
    rt.frame(0.0);
    let open = audit_overflow(&rt, 960.0);
    eprintln!("audit 960x640 open-panel overflow: {open:?}");
    assert!(
        open.is_subset(&closed),
        "opening the aside adds overflow: {:?} (closed: {:?})",
        open.difference(&closed).collect::<Vec<_>>(),
        closed
    );
    assert!(
        !open.contains("conn-toolbar-ai"),
        "the toggle does not overflow with the panel open either: {open:?}"
    );
}

#[test]
fn audit_reopen_close_repeats_the_cancel_receipt_per_hide() {
    // Each hide records its own receipt for the same run — the S6
    // `stop_and_unmount_cancel_record_the_in_flight_request` pin,
    // lifted to the Section level. The store (T088) dedupes by
    // request id; the outbox itself is append-only by design.
    let mut rt = audit_polled();
    audit_open(&mut rt, true);
    rt.state
        .ai_chat_panel
        .set_request_id(Some("req-1".to_string()));
    rt.frame(0.0);
    audit_click(&mut rt, "conn-toolbar-ai");
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.cancelled_request_ids(),
        ["req-1"],
        "first close cancels once"
    );
    audit_click(&mut rt, "conn-toolbar-ai");
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.cancelled_request_ids(),
        ["req-1"],
        "reopen records nothing new"
    );
    audit_click(&mut rt, "conn-toolbar-ai");
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(
        rt.state.ai_chat_panel.cancelled_request_ids(),
        ["req-1", "req-1"],
        "second close records a second receipt for the same run"
    );
}

#[test]
fn audit_empty_grid_contributes_no_preview() {
    // React's memo requires columns or rows for a preview; a 0x0 grid
    // has neither, so the snapshot keeps the editor SQL but no preview
    // (and the host's `grid.cols == 0 && grid.rows == 0` branch runs).
    let mut rt = audit_polled();
    audit_open(&mut rt, true);
    assert!(rt.state.open_query_tab("Query").is_some());
    rt.state.sql = SqlEditor::from_doc("SELECT 1");
    // `DataGrid::new()` is a demo-sized grid, not an empty one: load a
    // genuinely empty page (0 columns, 0 rows) for the 0x0 branch.
    let mut grid = DataGrid::new();
    grid.load_page(&datazen_ui_api::TablePage {
        table: "t".into(),
        columns: vec![],
        rows: vec![],
        page: 0,
        page_size: 25,
        total_rows: 0,
        sort: None,
        filter: None,
        primary_key: vec![],
        read_only: None,
    });
    assert_eq!(grid.cols, 0, "the page really is 0 columns");
    assert_eq!(grid.rows, 0, "the page really is 0 rows");
    rt.state.result_grid = Some(grid);
    rt.frame(0.0);
    let snapshot = rt
        .state
        .ai_chat_panel
        .area()
        .snapshot()
        .expect("the editor SQL alone seeds a snapshot");
    assert_eq!(
        snapshot.editor_sql.as_deref(),
        Some("SELECT 1"),
        "the editor SQL carries the snapshot"
    );
    assert!(
        snapshot.result_preview.is_none(),
        "a 0x0 grid contributes no preview"
    );
}

#[test]
fn audit_i18n_keys_used_by_the_s7_surface_exist() {
    // S7 adds no new `t()` call (the AI label is React's literal
    // `label="AI"`), but its surface depends on these keys existing:
    // the toolbar's pre-existing `common.newQuery`, the settings
    // placeholder's `common.aiAssistant` behind the `ai` deep-link,
    // and `docs.openAiHelp` for the help seam S7 leaves unwired.
    assert!(crate::i18n::has("common.newQuery"), "common.newQuery exists");
    assert_eq!(
        crate::i18n::t("common.newQuery"),
        "新建查询",
        "common.newQuery resolves to real text, not the key echo"
    );
    assert!(
        crate::i18n::has("common.aiAssistant"),
        "common.aiAssistant exists for the ai settings section"
    );
    assert!(
        crate::i18n::has("docs.openAiHelp"),
        "docs.openAiHelp exists for the unwired help button"
    );
}

#[test]
fn audit_settings_deep_link_targets_a_real_section() {
    // The unconfigured view's button deep-links `ai`; `ai` must be a
    // registered settings section id, not a typo that silently falls
    // back to `general`.
    let mut state = AppState::default();
    state.set_settings_section("ai");
    assert_eq!(
        state.settings_section(),
        "ai",
        "ai is a real settings section"
    );
    state.set_settings_section("no-such-section");
    assert_eq!(
        state.settings_section(),
        "general",
        "unknown ids fall back to general"
    );
}




