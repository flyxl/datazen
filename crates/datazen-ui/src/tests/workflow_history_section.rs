//! T113 (segment-2's workflow history section port): the two run-history
//! list views of `WorkflowHistorySection.tsx`.
//!
//! The sidebar list's rows (clock, name, formatted `createdAt`, ✓/✗), its
//! clear bar and empty sentence, and its `onView(id, name)` hand-back; the
//! inline tab's rows (plus the `totalTimeMs` meta), its red clear link and
//! empty branch, its detail branch (the back link over the embedded result
//! panel, with the panel's per-step state riding the tab's slots), and the
//! `toLocaleString` formatter's clock math and refusal branches.

use std::collections::BTreeSet;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::{
    locale_string, locale_string_with_offset, HistoryListItem, HistoryListSlots,
    HistoryTabSlots, StepExecutionResult, StepUiSlots, WorkflowExecutionResult,
    WorkflowHistoryList, WorkflowHistoryTab, WorkflowStepStatus,
};

/// The probe's own state: the runs, the detail the tab hosts, the embedded
/// panel's interaction sets, and the log the slots write (the host half of
/// React's prop callbacks).
#[derive(Default)]
struct HistoryState {
    dark: bool,
    items: Vec<HistoryListItem>,
    detail: Option<WorkflowExecutionResult>,
    viewed: Vec<(String, String)>,
    viewed_tab: Vec<String>,
    cleared: u32,
    closed_detail: u32,
    expanded: BTreeSet<String>,
    chart: BTreeSet<String>,
}

fn list_view(state: &HistoryState, _cx: &mut Cx<HistoryState>) -> El {
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .child(WorkflowHistoryList::new(&state.items, list_slots()).build(state.dark))
}

fn tab_view(state: &HistoryState, _cx: &mut Cx<HistoryState>) -> El {
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .child(
            WorkflowHistoryTab::new(&state.items, state.detail.as_ref(), tab_slots())
                .build(state, state.dark),
        )
}

/// The list at the sidebar width the workflows page gives it.
fn list_runtime(items: Vec<HistoryListItem>, dark: bool) -> Runtime<HistoryState> {
    let mut rt = Runtime::new(
        HistoryState {
            items,
            dark,
            ..HistoryState::default()
        },
        list_view,
        (280.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

/// The tab at the AI panel's width, optionally hosting a detail result.
fn tab_runtime(
    items: Vec<HistoryListItem>,
    detail: Option<WorkflowExecutionResult>,
    dark: bool,
) -> Runtime<HistoryState> {
    let mut rt = Runtime::new(
        HistoryState {
            items,
            detail,
            dark,
            ..HistoryState::default()
        },
        tab_view,
        (480.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<HistoryState>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<HistoryState>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<HistoryState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

/// One recorded run (the contract's `WorkflowRunListItem` fields).
fn run(
    id: &str,
    name: &str,
    success: bool,
    total_time_ms: u64,
    created_at: &str,
) -> HistoryListItem {
    HistoryListItem {
        id: id.to_owned(),
        workflow_id: format!("wf-{id}"),
        workflow_name: name.to_owned(),
        success,
        total_time_ms,
        created_at: created_at.to_owned(),
    }
}

/// The detail the tab hosts: one successful step with SQL — enough to pin
/// the embedded panel's mount and its state slots.
fn detail_result() -> WorkflowExecutionResult {
    WorkflowExecutionResult {
        success: true,
        final_output: String::new(),
        steps: vec![StepExecutionResult {
            step_id: "s1".to_owned(),
            step_type: None,
            status: WorkflowStepStatus::Success,
            result: None,
            execution_time_ms: 5,
            error: None,
            connection_name: None,
            sql_executed: Some("SELECT 1".to_owned()),
        }],
        total_time_ms: 42,
        error: None,
    }
}

fn list_slots() -> HistoryListSlots<HistoryState> {
    HistoryListSlots {
        view: |s, id, name| s.viewed.push((id, name)),
        clear: |s| s.cleared += 1,
    }
}

fn tab_slots() -> HistoryTabSlots<HistoryState> {
    HistoryTabSlots {
        view: |s, id| s.viewed_tab.push(id),
        clear: |s| s.cleared += 1,
        close_detail: |s| {
            s.closed_detail += 1;
            s.detail = None;
        },
        panel: StepUiSlots {
            expanded: |s| &s.expanded,
            toggle: |s, id| {
                if !s.expanded.remove(&id) {
                    s.expanded.insert(id);
                }
            },
            chart: |s| &s.chart,
            set_chart: |s, id, chart| {
                if chart {
                    s.chart.insert(id);
                } else {
                    s.chart.remove(&id);
                }
            },
        },
    }
}

// ---- the sidebar list -------------------------------------------------------

#[test]
fn the_list_renders_each_runs_clock_name_formatted_time_and_mark() {
    let items = vec![
        run("r1", "Nightly ETL", true, 900, "2024-06-01T12:00:00Z"),
        run("r2", "Nightly ETL", false, 0, "2024-06-02T03:04:05+08:00"),
    ];
    let rt = list_runtime(items, true);
    for id in ["r1", "r2"] {
        assert!(
            has_key(&rt, &format!("workflow-history-list-item-{id}")),
            "run {id} renders its row"
        );
        assert!(has_key(&rt, &format!("workflow-history-list-item-{id}-clock")));
        assert_eq!(
            element(&rt, &format!("workflow-history-list-item-{id}-name"))
                .text
                .as_deref(),
            Some("Nightly ETL")
        );
    }
    // The created line is the formatted createdAt, not the raw ISO text the
    // contract carries.
    assert_eq!(
        element(&rt, "workflow-history-list-item-r1-created")
            .text
            .as_deref(),
        Some(locale_string("2024-06-01T12:00:00Z").as_str())
    );
    assert_ne!(
        element(&rt, "workflow-history-list-item-r1-created")
            .text
            .as_deref(),
        Some("2024-06-01T12:00:00Z")
    );

    // ✓ green on the successful run, ✗ red on the failed one; the mark
    // sits right of the flex-1 body block (React's row edge).
    let ok = element(&rt, "workflow-history-list-item-r1-status");
    let fail = element(&rt, "workflow-history-list-item-r2-status");
    assert_eq!(ok.text.as_deref(), Some("✓"));
    assert_eq!(fail.text.as_deref(), Some("✗"));
    assert_ne!(
        ok.text_color, fail.text_color,
        "the mark's ink follows the outcome"
    );
    let body = element(&rt, "workflow-history-list-item-r1-body");
    assert!(
        ok.rect[0] > body.rect[0] + body.rect[2],
        "the mark is right of the body block: mark {:?}, body {:?}",
        ok.rect,
        body.rect
    );

    // Runs exist: the clear bar mounts, the empty sentence does not.
    assert!(has_key(&rt, "workflow-history-list-clear-bar"));
    assert_eq!(
        element(&rt, "workflow-history-list-clear-text")
            .text
            .as_deref(),
        Some(t("workflows.history.clear"))
    );
    assert!(!has_key(&rt, "workflow-history-list-empty"));
}

#[test]
fn the_list_hands_the_host_the_run_id_and_name_and_clears() {
    let mut rt = list_runtime(
        vec![run("r1", "Nightly ETL", true, 900, "2024-06-01T12:00:00Z")],
        true,
    );
    click(&mut rt, "workflow-history-list-item-r1");
    assert_eq!(
        rt.state.viewed,
        vec![("r1".to_owned(), "Nightly ETL".to_owned())],
        "onView hands the host the run id and the workflow name"
    );
    click(&mut rt, "workflow-history-list-clear");
    assert_eq!(rt.state.cleared, 1, "the clear link pressed the host's slot");
    assert!(
        rt.state.viewed.len() == 1,
        "clear did not open a run (React's two controls are separate)"
    );
}

#[test]
fn an_empty_list_reads_the_sentence_and_mounts_neither_clear_nor_rows() {
    let rt = list_runtime(Vec::new(), true);
    assert_eq!(
        element(&rt, "workflow-history-list-empty-text")
            .text
            .as_deref(),
        Some(t("workflows.history.empty"))
    );
    assert!(!has_key(&rt, "workflow-history-list-clear-bar"));
    assert!(!has_key(&rt, "workflow-history-list-clear"));
    // The empty sentence's own root mounts; no run's row key does.
    assert!(has_key(&rt, "workflow-history-list-empty"));
    assert!(!has_key(&rt, "workflow-history-list-item-r1"));
}

// ---- the inline tab ---------------------------------------------------------

#[test]
fn the_tab_lists_names_meta_and_marks_and_hands_the_host_the_id() {
    let mut rt = tab_runtime(
        vec![
            run("r1", "Nightly ETL", true, 900, "2024-06-01T12:00:00Z"),
            run("r2", "Backfill", false, 0, "2024-06-02T03:04:05+08:00"),
        ],
        None,
        true,
    );
    assert_eq!(
        element(&rt, "workflow-history-tab-item-r1-name")
            .text
            .as_deref(),
        Some("Nightly ETL")
    );
    // The meta line: clock, the formatted createdAt, the total ms.
    assert!(has_key(&rt, "workflow-history-tab-item-r1-clock"));
    assert_eq!(
        element(&rt, "workflow-history-tab-item-r1-created")
            .text
            .as_deref(),
        Some(locale_string("2024-06-01T12:00:00Z").as_str())
    );
    assert_eq!(
        element(&rt, "workflow-history-tab-item-r1-time")
            .text
            .as_deref(),
        Some("900ms")
    );
    // A zero total still renders its number, like React's template literal.
    assert_eq!(
        element(&rt, "workflow-history-tab-item-r2-time")
            .text
            .as_deref(),
        Some("0ms")
    );

    let ok = element(&rt, "workflow-history-tab-item-r1-status");
    let fail = element(&rt, "workflow-history-tab-item-r2-status");
    assert_eq!(ok.text.as_deref(), Some("✓"));
    assert_eq!(fail.text.as_deref(), Some("✗"));
    assert_ne!(ok.text_color, fail.text_color);
    // The mark sits right of the flex-1 body (React's justify-between).
    let body = element(&rt, "workflow-history-tab-item-r1-body");
    assert!(
        ok.rect[0] > body.rect[0] + body.rect[2],
        "the mark is right of the body block: mark {:?}, body {:?}",
        ok.rect,
        body.rect
    );

    // Pressing a row hands the host the run id alone (React's onView(id)).
    click(&mut rt, "workflow-history-tab-item-r1");
    assert_eq!(rt.state.viewed_tab, vec!["r1".to_owned()]);
}

#[test]
fn the_tab_clear_link_reads_danger_ink_and_clears_only_with_runs() {
    let mut rt = tab_runtime(
        vec![run("r1", "Nightly ETL", true, 900, "2024-06-01T12:00:00Z")],
        None,
        true,
    );
    let clear = element(&rt, "workflow-history-tab-clear-text");
    assert_eq!(
        clear.text.as_deref(),
        Some(t("workflows.history.clear"))
    );
    assert_eq!(
        clear.text_color,
        Some(crate::ui::kit::kit(true).c.danger.fg),
        "the tab's clear link reads the danger ink (React text-red-400)"
    );
    click(&mut rt, "workflow-history-tab-clear");
    assert_eq!(rt.state.cleared, 1);

    // No runs: no clear link, the sentence instead.
    let empty = tab_runtime(Vec::new(), None, true);
    assert!(!has_key(&empty, "workflow-history-tab-clear"));
    assert_eq!(
        element(&empty, "workflow-history-tab-empty-text")
            .text
            .as_deref(),
        Some(t("workflows.history.empty"))
    );
}

#[test]
fn a_detail_mounts_the_back_link_over_the_embedded_panel_not_the_rows() {
    let mut rt = tab_runtime(
        vec![run("r1", "Nightly ETL", true, 900, "2024-06-01T12:00:00Z")],
        Some(detail_result()),
        true,
    );
    // The back link: the X icon and the sentence in the accent ink.
    assert!(has_key(&rt, "workflow-history-tab-back-icon"));
    let back = element(&rt, "workflow-history-tab-back-text");
    assert_eq!(back.text.as_deref(), Some(t("workflows.history.back")));
    assert_eq!(
        back.text_color,
        Some(crate::ui::kit::kit(true).c.text_accent),
        "the back link reads the accent ink (React text-accent)"
    );

    // The embedded result panel mounts under its own prefix; its head reads
    // the detail's outcome and timing.
    assert!(has_key(&rt, "workflow-execution-result"));
    assert_eq!(
        element(&rt, "workflow-execution-result-status")
            .text
            .as_deref(),
        Some(format!("✓ {}", t("workflows.result")).as_str())
    );
    assert_eq!(
        element(&rt, "workflow-execution-result-total-time")
            .text
            .as_deref(),
        Some("42ms")
    );
    assert!(has_key(&rt, "workflow-execution-result-step-0-row"));

    // React's detail branch renders neither the rows nor the clear link.
    assert!(!has_key(&rt, "workflow-history-tab-item-r1"));
    assert!(!has_key(&rt, "workflow-history-tab-clear"));

    // Back hands the host the close; with the detail gone the rows return.
    click(&mut rt, "workflow-history-tab-back");
    assert_eq!(rt.state.closed_detail, 1);
    assert!(rt.state.detail.is_none());
    assert!(has_key(&rt, "workflow-history-tab-item-r1"));
    assert!(!has_key(&rt, "workflow-history-tab-back"));
}

#[test]
fn the_detail_branch_keeps_the_panels_step_state_in_the_host() {
    let mut rt = tab_runtime(
        vec![run("r1", "Nightly ETL", true, 900, "2024-06-01T12:00:00Z")],
        Some(detail_result()),
        true,
    );
    // The embedded step row toggles through the tab's panel slots: the
    // expansion lands in the host's set and the body shows the SQL.
    assert!(!has_key(&rt, "workflow-execution-result-step-0-body"));
    click(&mut rt, "workflow-execution-result-step-0-row");
    assert!(
        rt.state.expanded.contains("s1"),
        "the panel's toggle slot wrote the expansion into the host's state"
    );
    assert!(has_key(&rt, "workflow-execution-result-step-0-body"));
    assert_eq!(
        element(&rt, "workflow-execution-result-step-0-sql-text")
            .text
            .as_deref(),
        Some("SELECT 1")
    );
    // The detail itself is untouched by the step interaction: the back link
    // stays mounted and the close slot never ran.
    assert!(has_key(&rt, "workflow-history-tab-back"));
    assert_eq!(rt.state.closed_detail, 0);
}

// ---- the createdAt formatter ------------------------------------------------

#[test]
fn locale_string_formats_the_zoned_instant_in_the_supplied_offset() {
    assert_eq!(
        locale_string_with_offset("2024-06-01T12:00:00Z", 8 * 3_600),
        "2024-06-01 20:00:00"
    );
    assert_eq!(
        locale_string_with_offset("2024-06-01T12:00:00Z", -5 * 3_600),
        "2024-06-01 07:00:00"
    );
    // A zoned input normalises through its own offset first.
    assert_eq!(
        locale_string_with_offset("2024-06-01T00:00:00+08:00", 0),
        "2024-05-31 16:00:00"
    );
}

#[test]
fn locale_string_keeps_zone_less_or_unparseable_text_verbatim() {
    // The contract's createdAt always carries a zone; anything else renders
    // as-is rather than guessing one (see the module docs).
    assert_eq!(
        locale_string_with_offset("2024-06-01 12:00:00", 0),
        "2024-06-01 12:00:00"
    );
    assert_eq!(locale_string_with_offset("not-a-date", 0), "not-a-date");
    assert_eq!(locale_string("not-a-date"), "not-a-date");
}

// ---- geometry ----------------------------------------------------------------

#[test]
fn both_views_fit_their_windows_in_either_theme() {
    let items = vec![
        run("r1", "Nightly ETL", true, 900, "2024-06-01T12:00:00Z"),
        run("r2", "Backfill", false, 0, "2024-06-02T03:04:05+08:00"),
    ];
    for dark in [true, false] {
        let list = list_runtime(items.clone(), dark);
        assert_fits(&list, 280.0, 640.0, &["workflow-history-list"]);
        // The detail branch is the tab's widest shape: the back link over
        // the embedded panel's own keys.
        let tab = tab_runtime(items.clone(), Some(detail_result()), dark);
        assert_fits(
            &tab,
            480.0,
            640.0,
            &["workflow-history-tab", "workflow-execution-result"],
        );
    }
}

/// Every keyed element under `prefixes` stays inside the window.
fn assert_fits(rt: &Runtime<HistoryState>, w: f32, h: f32, prefixes: &[&str]) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let Some(key) = e.key.as_deref() else {
            continue;
        };
        if !prefixes.iter().any(|prefix| key.starts_with(prefix)) {
            continue;
        }
        let [x, y, ew, eh] = e.rect;
        if x < -0.5 || y < -0.5 || x + ew > w + 0.5 || y + eh > h + 0.5 {
            outside.push(format!("{key} {:?}", e.rect));
        }
    }
    assert!(
        outside.is_empty(),
        "{w}x{h}: {} element(s) overflow the window: {outside:?}",
        outside.len()
    );
}
