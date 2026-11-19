//! T145: the `WorkflowPage` port — the page-level assembly (the toolbar, the
//! sidebar with its two side tabs, the panel tab bar, the variables +
//! execute row, the step detail view and the dialogs' ask state), mounted
//! through the real app (the section arm in `views/workspace.rs`): the main
//! structure, the lite-editor swap's list pin, the edit pane's form-vs-yaml
//! seam, the family mounts (the form, the history list, the shared action
//! bar), the run panel over the real engine paths, the step detail's
//! table/chart branches with the `ChartView` wiring over the page's lean
//! pipeline, the confirm dialog's pending/resolve transitions and the
//! context-menu seam's trigger.

use serde_json::json;

use super::runtime;
use crate::i18n::t;
use crate::views::workflow_page::{
    WorkflowListItem, WorkflowPanel, WorkflowRunPanel, WorkflowVariableItem,
};
use crate::views::{StepExecutionResult, WorkflowExecutionResult, WorkflowStepStatus};

/// The section's entry: the rail press + a frame.
fn enter_workflows(rt: &mut rune::Runtime<crate::AppState>) {
    let (x, y) = rt.find("rail-workflows").expect("workflows navigation");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

fn click(rt: &mut rune::Runtime<crate::AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} not found"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

/// The page's main structure, tree-observed after the lite editor swap: the
/// toolbar (the open-dir seam's conditional hides the button — the dir slot
/// stays empty), the sidebar with its two side tabs, the workflow list (the
/// lite page's rows, still keyed `workflow-{name}`), and the empty hint while
/// no panel is open.
#[test]
fn the_page_mounts_its_sections_and_the_list_survives_the_swap() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // The page root (React's embedded testid) and the toolbar's icon buttons.
    assert!(rt.find("workflow-workspace").is_some(), "the page root");
    assert!(rt.find("workflow-toolbar").is_some());
    assert!(rt.find("workflow-reload").is_some());
    assert!(rt.find("workflow-create").is_some());
    assert!(rt.find("workflow-ai-create-button").is_some());
    assert!(
        rt.find("workflow-open-docs").is_some(),
        "the docs button mounts (its press is the window-manager seam)"
    );
    assert!(
        rt.find("workflow-open-dir").is_none(),
        "the dir seam keeps the open-dir button hidden (React's own `workflowsDir &&`)"
    );

    // The sidebar and its two side tabs.
    assert!(rt.find("workflow-sidebar").is_some());
    assert!(rt.find("workflow-side-tab-workflows").is_some());
    assert!(rt.find("workflow-side-tab-history").is_some());

    // The lite swap pin: the list still renders the memory port's three
    // workflows, keyed `workflow-{name}` (the id the projection reads).
    assert!(rt.find("workflow-list").is_some());
    assert!(rt.find("workflow-audit").is_some());
    assert!(rt.find("workflow-nightly-backup").is_some());
    assert!(rt.find("workflow-weekly-report").is_some());
    assert!(rt.find("workflow-list-empty").is_none(), "the list is loaded");

    // The row's affordances (React's hover reveal, kept visible — the
    // history-section ruling) and the main column's empty state.
    assert!(rt.find("workflow-audit-edit").is_some());
    assert!(rt.find("workflow-audit-delete").is_some());
    assert!(rt.find("workflow-empty-hint").is_some());
    assert!(
        rt.find("workflow-panel-tabs").is_none(),
        "no panel is open, no tab bar"
    );
    assert!(
        rt.find("workflow-execute-button").is_none(),
        "no run panel, no execute row"
    );
}

/// The edit pane — the family mounts the page was waiting for: the visual
/// form (T070's, `showActionBar={false}`), the mode tabs, and the shared
/// bottom action bar (T019's, carrying React's page `saveTestId` key).
#[test]
fn the_edit_pane_mounts_the_form_the_mode_tabs_and_the_shared_bar() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);
    click(&mut rt, "workflow-audit-edit");

    assert!(rt.find("workflow-edit-panel").is_some(), "the edit pane");
    assert!(rt.find("workflow-editor-mode-tabs").is_some());
    assert!(rt.find("workflow-mode-visual").is_some());
    assert!(rt.find("workflow-mode-yaml").is_some());
    assert!(
        rt.find("workflow-form-root").is_some(),
        "T070's visual form mounts (its first production consumer)"
    );
    assert!(
        rt.find("workflow-form-step-0").is_some(),
        "the loaded draft's steps render"
    );
    assert!(
        rt.find("workflow-form-save").is_none(),
        "the form's own footer is off (React's showActionBar=false)"
    );
    assert!(
        rt.find("workflow-editor-action-bar").is_some(),
        "the shared action bar mounts"
    );
    assert!(rt.find("workflow-yaml-save").is_some(), "the page's saveTestId");
    assert!(rt.find("workflow-editor-cancel").is_some());

    // The panel's tab joins the tab bar; the tab's glyph and label read.
    assert!(rt.find("workflow-panel-tabs").is_some());
    assert!(rt.find("workflow-tab-edit-1").is_some());

    // The create flow's empty draft gates the shared Save (React's disabled
    // rule: id/name/steps).
    click(&mut rt, "workflow-create");
    assert!(rt.find("workflow-tab-edit-2").is_some(), "a second edit panel");
    let save = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("workflow-yaml-save"))
        .expect("the shared Save");
    assert!(!save.focusable, "an empty draft disables the shared Save");

    // Cancel closes the active edit panel (React's closePanel).
    click(&mut rt, "workflow-editor-cancel");
    assert!(
        rt.find("workflow-tab-edit-2").is_none(),
        "the create panel closed"
    );
    assert!(
        rt.find("workflow-tab-edit-1").is_some(),
        "the audit edit panel stayed"
    );
}

/// The edit mode's form-vs-yaml seam: the mode switch is real in both
/// directions — yaml writes the lite document over the draft's projection,
/// visual parses it back — and a document the lean parse refuses is React's
/// parse-error branch (the operation error names it, the mode stays).
#[test]
fn the_yaml_mode_swaps_the_pane_and_round_trips_the_document() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);
    click(&mut rt, "workflow-audit-edit");

    // Visual first: no yaml surface.
    assert!(rt.find("yaml-editor").is_none());
    click(&mut rt, "workflow-mode-yaml");
    assert!(rt.find("yaml-editor").is_some(), "the lite document mounts");
    assert!(rt.find("workflow-form-root").is_none(), "the form unmounts");

    // The document is the writer's shape over the loaded draft.
    let doc = rt.state.workflow_yaml.doc().to_owned();
    assert!(doc.starts_with("name: audit\nsteps:\n"), "{doc}");
    assert!(doc.contains("  - compare structures\n"), "{doc}");
    assert!(doc.contains("  - apply plan\n"), "{doc}");

    // Back to visual: the parse reads the document into the draft.
    click(&mut rt, "workflow-mode-visual");
    assert!(rt.find("workflow-form-root").is_some(), "the form is back");
    assert!(rt.find("yaml-editor").is_none());
    assert_eq!(
        rt.state.workflow_draft.name.text, "audit",
        "the parse read the name"
    );
    assert_eq!(
        rt.state.workflow_draft.steps.len(),
        2,
        "the parse read the steps"
    );
    assert_eq!(rt.state.workflow_draft.steps[0].sql, "compare structures");

    // The parse-error branch: a document the lean parse refuses keeps the
    // mode and names the error (React's `editorMode` stays `p`).
    click(&mut rt, "workflow-mode-yaml");
    rt.state.workflow_yaml =
        crate::sql_editor::SqlEditor::from_doc("not the lite shape at all");
    rt.frame(0.0);
    click(&mut rt, "workflow-mode-visual");
    assert!(
        rt.find("yaml-editor").is_some(),
        "the mode stayed yaml (the refused parse)"
    );
    assert!(
        rt.state.workflow_operation_error.is_some(),
        "the parse seam named the error"
    );
}

/// The history side tab — the history list's first production mount (T113's
/// wait): the empty seam first, then the wiring's `workflow_history_loaded`
/// transition fills the list, and the row's press opens the history detail
/// panel over the lean entry.
#[test]
fn the_history_side_tab_mounts_the_family_list() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);
    click(&mut rt, "workflow-side-tab-history");

    assert!(rt.find("workflow-history-list").is_some(), "the list mounts");
    // React's own `items.length > 0` gate hides the clear affordance while
    // the list is empty — it mounts once the wiring's transition fills it.
    assert!(
        rt.find("workflow-history-list-clear").is_none(),
        "the empty list hides the clear affordance (React's own gate)"
    );
    assert!(
        rt.find("workflow-workspace").is_some(),
        "the page stays (the side tab is a sidebar branch)"
    );

    // The wiring transition (the list use case is the seam; tests pin it —
    // the wapps `wapps_loaded` pattern).
    rt.settle_all();
    rt.state.workflow_history_loaded(vec![crate::views::HistoryListItem {
        id: "run-7".to_owned(),
        workflow_id: "audit".to_owned(),
        workflow_name: "audit".to_owned(),
        success: false,
        total_time_ms: 9,
        created_at: "2026-11-05T09:04:00+08:00".to_owned(),
    }]);
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(
        rt.find("workflow-history-list-item-run-7").is_some(),
        "the recorded run renders"
    );
    assert!(rt.find("workflow-history-list-item-run-7-name").is_some());
    assert!(
        rt.find("workflow-history-list-clear").is_some(),
        "the filled list mounts the clear affordance"
    );

    // The row's press opens the history detail panel.
    click(&mut rt, "workflow-history-list-item-run-7");
    assert!(rt.find("workflow-tab-hist-1").is_some(), "the history panel");
    // The lean entry carries no steps, so the run status reads the entry's
    // own failure and the steps bar stays empty.
    assert!(rt.find("workflow-run-status").is_some());
    let status = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("workflow-run-status"))
        .expect("the run status line");
    assert!(status.text.as_deref().unwrap_or_default().starts_with('✗'));
    // React's `{currentResult && (<div>…)}` mounts the bar with zero steps
    // (an empty row) — the lean entry keeps the bar, without tabs.
    assert!(rt.find("workflow-steps").is_some(), "the bar mounts empty");

    // The clear answer's transition (the wiring's call) empties the list.
    rt.state.workflow_history_cleared();
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        rt.find("workflow-history-list-item-run-7").is_none(),
        "the cleared list renders no rows"
    );
}

/// The run panel over the real engine path: the row's press opens the panel,
/// the variables row's Execute runs it (the sync memory path), the run
/// status + steps bars read the answer, and the step detail opens on the
/// order's first step (desc: the last). A successful run mounts the
/// add-to-dashboard press (React's `currentResult?.success` rule).
#[test]
fn the_run_panel_runs_and_mounts_the_result_panel() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    click(&mut rt, "workflow-audit");
    assert!(rt.find("workflow-tab-wf-1").is_some(), "the run panel's tab");
    assert!(rt.find("workflow-variables").is_some(), "the execute row");
    assert!(rt.find("workflow-execute-button").is_some());
    assert!(
        rt.find("workflow-add-to-dashboard").is_none(),
        "no result yet, no add-to-dashboard"
    );
    // The lite projection carries no variables: the row is the empty-variables
    // case (React's own branch).
    assert!(rt.find("workflow-variables-fields").is_none());

    click(&mut rt, "workflow-execute-button");
    assert!(rt.state.workflow_execution.is_some(), "the run filled the VM");
    // The panel's own copy of the answer + the executing mark cleared.
    let panel = rt.state.workflow_panels.first().cloned();
    let (is_executing, has_result) = match panel {
        Some(WorkflowPanel::Run(run)) => (run.is_executing, run.result.is_some()),
        _ => (true, false),
    };
    assert!(!is_executing && has_result, "the panel took the answer");
    // The step view re-seeded at the order's first step (desc: the last one).
    assert_eq!(rt.state.workflow_active_step, Some(1));

    // The run status row and the steps bar (desc order: apply plan first).
    assert!(rt.find("workflow-run-status").is_some());
    let status = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("workflow-run-status"))
        .expect("the run status line");
    assert!(status.text.as_deref().unwrap_or_default().starts_with('✗'));
    assert!(rt.find("workflow-steps").is_some());
    assert!(rt.find("workflow-step-apply plan").is_some());
    assert!(rt.find("workflow-step-compare structures").is_some());

    // The step detail opened on the active step (apply plan): the header, the
    // failed step's error block, the connectionless lean projection's rows
    // count. The execution-result panel stays mounted below (the documented
    // interim keeping T019's panel a production consumer).
    assert!(rt.find("workflow-step-detail").is_some());
    assert!(rt.find("workflow-step-error").is_some());
    assert!(rt.find("workflow-execution-result").is_some());

    // A step tab press switches the detail (React's setActiveStepIndex).
    click(&mut rt, "workflow-step-compare structures");
    assert_eq!(rt.state.workflow_active_step, Some(0));
    assert!(rt.find("workflow-step-error").is_none(), "the ok step has no error block");

    // The step-result order toggle flips the bars' order (the settings
    // persistence is the seam).
    click(&mut rt, "workflow-step-order-toggle");
    let first_step_tab = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("workflow-step-compare structures"))
        .expect("the asc order puts the first step first");
    let second_step_tab = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("workflow-step-apply plan"))
        .expect("the applying step");
    assert!(
        first_step_tab.rect[0] < second_step_tab.rect[0],
        "asc order: {:?} before {:?}",
        first_step_tab.rect,
        second_step_tab.rect
    );

    // A successful run (weekly-report's steps carry no `apply`) mounts the
    // add-to-dashboard press.
    click(&mut rt, "workflow-weekly-report");
    assert!(rt.find("workflow-tab-wf-2").is_some(), "the second run panel");
    click(&mut rt, "workflow-execute-button");
    assert!(rt.find("workflow-add-to-dashboard").is_some());
}

/// The step detail's table and chart branches — the `ChartView` wiring over
/// the page's lean pipeline: a chartable step result renders the view
/// toggle, the chart mode mounts the T102 container (its canvas keeps the
/// honest renderer placeholder), and the data-point click seam flips back
/// to the table. The table branch renders the lean table's cells.
#[test]
fn the_step_detail_table_and_chart_view_wiring() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // A run panel carrying a rich step result (the typed contract's shape —
    // the lite report the memory port answers has no payloads, so the panel
    // is seeded the way the wiring's typed answer would fill it).
    let step = StepExecutionResult {
        step_id: "report".to_owned(),
        step_type: Some("query".to_owned()),
        status: WorkflowStepStatus::Success,
        result: Some(json!({
            "columns": [
                {"name": "label", "dataType": "text"},
                {"name": "n", "dataType": "int8"}
            ],
            "rows": [
                {"label": "one", "n": 3},
                {"label": "two", "n": 5}
            ]
        })),
        execution_time_ms: 4,
        error: None,
        connection_name: Some("warehouse".to_owned()),
        sql_executed: Some("SELECT label, n FROM facts".to_owned()),
    };
    let result = WorkflowExecutionResult {
        success: true,
        final_output: String::new(),
        steps: vec![step],
        total_time_ms: 4,
        error: None,
    };
    rt.state.workflow_panels = vec![WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-9".to_owned(),
        workflow_id: "weekly-report".to_owned(),
        workflow_name: "weekly-report".to_owned(),
        started_at: "2026-11-05T09:04:00+08:00".to_owned(),
        result: Some(result),
        is_executing: false,
    })];
    rt.state.workflow_active_panel = Some("wf-9".to_owned());
    // React's execute seeds `activeStepIndex` at the order's first step; the
    // direct seeding does the same (desc order, one step → index 0).
    rt.state.workflow_active_step = Some(0);
    rt.settle_all();
    rt.frame(0.0);

    // The step detail over the panel's answer: the header reads the
    // connection and the SQL block, and the lean table renders the cells.
    assert!(rt.find("workflow-step-detail").is_some());
    assert!(rt.find("workflow-step-sql").is_some());
    assert!(rt.find("workflow-step-table").is_some());
    assert!(rt.find("workflow-step-table-col-0").is_some());
    assert!(rt.find("workflow-step-table-cell-0-0").is_some());
    // The chartable result renders the view toggle.
    assert!(rt.find("workflow-step-view-toggle").is_some());
    assert!(rt.find("workflow-view-table").is_some());
    assert!(rt.find("workflow-view-chart").is_some());

    // The chart mode mounts the ChartView container over the lean pipeline.
    click(&mut rt, "workflow-view-chart");
    assert!(rt.find("chart-view").is_some(), "the T102 container mounts");
    assert!(rt.find("chart-view-body").is_some());
    assert!(
        rt.find("workflow-step-table").is_none(),
        "the table branch unmounted (React's conditional render)"
    );
    // The renderer factory stays unwired: the canvas keeps its honest
    // placeholder note.
    assert!(
        rt.find("chart-renderer-note-bar").is_some()
            || rt.find("chart-renderer-note-line").is_some(),
        "the unported renderer says itself"
    );

    // The table toggle flips back.
    click(&mut rt, "workflow-view-table");
    assert!(rt.find("workflow-step-table").is_some());
    assert!(rt.find("chart-view").is_none());
}

/// The variables row's real state — a workflow whose declared variables the
/// typed list would carry renders its inputs (the projection carries none,
/// so the list is seeded the way the wiring's typed answer would fill it):
/// the text variable's field and the connection variable's select over the
/// saved connections.
#[test]
fn the_variables_row_renders_the_declared_inputs() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    let mut workflow = WorkflowListItem::from_workflow(
        &datazen_ui_api::Workflow {
            name: "sized-report".to_owned(),
            steps: vec!["SELECT 1".to_owned()],
        },
    );
    workflow.variables = vec![
        WorkflowVariableItem {
            name: "city".to_owned(),
            var_type: "string".to_owned(),
            description: Some("the city filter".to_owned()),
            required: true,
            default: None,
        },
        WorkflowVariableItem {
            name: "warehouse".to_owned(),
            var_type: "connection".to_owned(),
            description: None,
            required: false,
            default: None,
        },
    ];
    rt.state.workflow_list = vec![workflow.clone()];
    rt.state.select_workflow_list_item(&workflow);
    rt.settle_all();
    rt.frame(0.0);

    assert!(rt.find("workflow-variables-fields").is_some());
    assert!(rt.find("workflow-variable-city").is_some());
    // The required mark reads the declaration (the label row's red `*`
    // text node — the optional variable renders none).
    assert!(
        rt.find("workflow-variable-city-required").is_some(),
        "the required mark"
    );
    assert!(
        rt.find("workflow-variable-warehouse-required").is_none(),
        "the optional variable carries no mark"
    );
    assert!(rt.find("workflow-variable-city-label").is_some());
    // The connection variable renders the select over the saved connections.
    assert!(rt.find("workflow-variable-warehouse-trigger").is_some());

    // The variable's commit writes the run's map (React's `setVariables`).
    rt.state.set_workflow_variable("city", "上海".to_owned());
    assert_eq!(
        rt.state.workflow_variables.get("city").map(|edit| edit.text.as_str()),
        Some("上海")
    );
}

/// The confirm dialog's pending/resolve transitions — the delete ask and the
/// history-clear ask over the modal layer: the dialog mounts while the ask is
/// pending, cancel resolves without running the command, confirm runs the
/// command's branch (the delete use case's honest failure), and a new ask is
/// a fresh mount (the epoch). The add-to-dashboard pending dialog rides the
/// same layer.
#[test]
fn the_confirm_dialog_pending_and_resolve_transitions() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // The delete ask mounts the dialog over the modal layer.
    rt.state.ask_workflow_delete("audit");
    // Direct state mutation does not mark the view dirty; settle_all does
    // (the runtime's documented rebuild entry).
    rt.settle_all();
    rt.frame(0.0);
    assert!(rt.find("workflow-confirm-dialog").is_some());
    assert!(rt.find("workflow-confirm-dialog-message").is_some());
    assert!(rt.find("workflow-confirm-dialog-warning").is_some());
    let message = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("workflow-confirm-dialog-message"))
        .expect("the ask's message");
    assert_eq!(
        message.text.as_deref(),
        Some(t("workflows.deleteConfirm")),
        "the ask resolved the translated message"
    );
    let epoch = rt.state.workflow_confirm_epoch;
    assert_eq!(epoch, 1, "the ask bumped the epoch");

    // Cancel resolves the ask without running the delete.
    click(&mut rt, "workflow-confirm-dialog-cancel");
    assert!(rt.find("workflow-confirm-dialog").is_none(), "the dialog unmounted");
    assert!(rt.state.workflow_confirm_request.is_none());
    assert!(
        rt.state.workflow_operation_error.is_none(),
        "cancel ran no command"
    );

    // Confirm runs the delete's honest failure branch (the use case is not
    // wired) into the page's operation error slot.
    rt.state.ask_workflow_delete("audit");
    // Direct state mutation does not mark the view dirty; settle_all does
    // (the runtime's documented rebuild entry).
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(rt.state.workflow_confirm_epoch, epoch + 1, "a fresh mount");
    click(&mut rt, "workflow-confirm-dialog-confirm");
    assert!(
        rt.state
            .workflow_operation_error
            .as_deref()
            .is_some_and(|error| error.contains("workflow delete needs the live host")),
        "the delete seam's failure landed in the operation error"
    );
    // The strip banner renders the error with its dismiss control.
    assert!(rt.find("workflow-operation-error").is_some());
    click(&mut rt, "workflow-operation-error-dismiss");
    assert!(rt.state.workflow_operation_error.is_none(), "the dismiss cleared it");

    // The history-clear ask resolves the same way into its own seam sentence.
    rt.state.ask_workflow_history_clear();
    rt.settle_all();
    rt.frame(0.0);
    let message = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("workflow-confirm-dialog-message"))
        .expect("the clear ask's message");
    assert_eq!(message.text.as_deref(), Some(t("workflows.history.clearConfirm")));
    click(&mut rt, "workflow-confirm-dialog-confirm");
    assert!(
        rt.state
            .workflow_operation_error
            .as_deref()
            .is_some_and(|error| error.contains("workflow history clear needs the live host")),
        "the clear seam's failure landed"
    );

    // The delete answer's success transition (the wiring's call) drops the
    // workflow's run panels and reloads the list — the wapps remove pattern.
    rt.state.select_workflow_list_item(&rt.state.workflow_list[0].clone());
    rt.settle_all();
    rt.frame(0.0);
    assert!(rt.find("workflow-tab-wf-1").is_some());
    rt.state.workflow_deleted("audit");
    rt.settle_all();
    rt.frame(0.0);
    assert!(rt.find("workflow-tab-wf-1").is_none(), "the run panel closed");
    assert!(
        rt.find("workflow-audit").is_some(),
        "the list refetched (the memory port still lists it)"
    );

    // The add-to-dashboard pending dialog rides the same layer.
    rt.state.open_add_to_dashboard();
    rt.settle_all();
    rt.frame(0.0);
    assert!(rt.find("workflow-add-dashboard-dialog").is_some());
    assert!(rt.find("workflow-add-dashboard-dialog-pending").is_some());
    click(&mut rt, "workflow-add-dashboard-dialog-close");
    assert!(rt.find("workflow-add-dashboard-dialog").is_none());
}

/// The context-menu seam's trigger — the row's secondary press records the
/// workflow and the window point (the entries builder and the native host
/// are T646's); the row's real edit/delete affordances stay the mean-while.
#[test]
fn the_context_menu_seam_records_the_row_press() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    let (x, y) = rt.find("workflow-audit").expect("the row");
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.0);
    let request = rt
        .state
        .workflow_context_menu
        .clone()
        .expect("the seam's trigger record");
    assert_eq!(request.workflow_id, "audit");
    assert_eq!(request.point, (x, y), "the window point the press happened at");

    // The row's own affordances still exist as the mean-while actions.
    assert!(rt.find("workflow-audit-edit").is_some());
    assert!(rt.find("workflow-audit-delete").is_some());
}

/// The mount effect's halves and the command seams: the section entry
/// refills the list and the connections (the form's slot), the reload press
/// runs the refetch half it owns, and the docs press's feedback line says
/// the window-manager seam (the workflow list's own keys are the wapps
/// pattern's `wapps_loaded` pin — here the real fill already ran).
#[test]
fn the_mount_effect_fills_the_list_and_the_connections() {
    let mut rt = runtime();
    rt.frame(0.0);
    // The section entry (React's mount effect) already filled the list.
    enter_workflows(&mut rt);
    assert_eq!(rt.state.workflow_list.len(), 3, "the list filled");
    assert!(!rt.state.workflow_list_loading, "the fill settled");
    // The connections half filled the form's slot (the memory port's demo
    // connections — the unwired adapter's fallback).
    assert!(
        !rt.state.workflow_form_connections.is_empty(),
        "the connections slot filled"
    );

    // The reload press runs the refetch half (the dir rescan command is the
    // seam — the module docs' ruling).
    click(&mut rt, "workflow-reload");
    assert_eq!(rt.state.workflow_list.len(), 3, "the refetch ran");

    // The docs press's feedback names the window-manager seam.
    click(&mut rt, "workflow-open-docs");
    assert!(
        rt.state
            .workflow_feedback
            .contains("needs the window manager"),
        "the docs seam said itself: {}",
        rt.state.workflow_feedback
    );
    assert!(rt.find("workflow-feedback").is_some(), "the feedback line renders");
}

