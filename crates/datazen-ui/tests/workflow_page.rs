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

use datazen_ui::test_support::runtime;
use datazen_ui::i18n::t;
use datazen_ui::views::workflow_page::{
    WorkflowListItem, WorkflowPanel, WorkflowRunPanel, WorkflowVariableItem,
};
use datazen_ui::views::{StepExecutionResult, WorkflowExecutionResult, WorkflowStepStatus};

/// The section's entry: the rail press + a frame.
fn enter_workflows(rt: &mut rune::Runtime<datazen_ui::AppState>) {
    let (x, y) = rt.find("rail-workflows").expect("workflows navigation");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

fn click(rt: &mut rune::Runtime<datazen_ui::AppState>, key: &str) {
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
    assert!(rt.find("workflow-yaml-editor").is_none());
    click(&mut rt, "workflow-mode-yaml");
    assert!(rt.find("workflow-yaml-editor").is_some(), "the lite document mounts");
    assert!(rt.find("workflow-form-root").is_none(), "the form unmounts");

    // The document is the writer's shape over the loaded draft.
    let doc = rt.state.workflow_yaml.doc().to_owned();
    assert!(doc.starts_with("name: audit\nsteps:\n"), "{doc}");
    assert!(doc.contains("  - compare structures\n"), "{doc}");
    assert!(doc.contains("  - apply plan\n"), "{doc}");

    // Back to visual: the parse reads the document into the draft.
    click(&mut rt, "workflow-mode-visual");
    assert!(rt.find("workflow-form-root").is_some(), "the form is back");
    assert!(rt.find("workflow-yaml-editor").is_none());
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
        datazen_ui::sql_editor::SqlEditor::from_doc("not the lite shape at all");
    rt.frame(0.0);
    click(&mut rt, "workflow-mode-visual");
    assert!(
        rt.find("workflow-yaml-editor").is_some(),
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
    rt.state.workflow_history_loaded(vec![datazen_ui::views::HistoryListItem {
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
        variables: Vec::new(),
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
        rt.state.workflow_variable("city"),
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


// ── the T145 audit round's pins ─────────────────────────────────────────────
//
// The audit round's mutation probes (each a temporary kill of one React
// semantics arm, restored before the commit) showed the trimmed set's blind
// spots; each pin below names the React source location it holds.

/// The tree's element with `key` (the history-section suite's helper shape).
fn element(rt: &rune::Runtime<datazen_ui::AppState>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

/// The toolbar's conditional arms and the ai-create press: React's
/// `workflowsDir &&` shows the open-dir button only while the dir seam holds
/// a value (WorkflowPage.tsx:689-698, the press 467-474), `handleAiCreate`
/// opens the ai-create panel once and reactivates it on the second press
/// (531-540), and the side tabs' presses switch the sidebar's list (731 and
/// 748 — the workflows tab's literal label is React's own, 737).
#[test]
fn the_open_dir_arm_the_ai_create_press_and_the_side_tab_switch() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // React 531-540: the ai-create press opens the panel (once).
    click(&mut rt, "workflow-ai-create-button");
    assert!(rt.find("workflow-tab-ai-1").is_some(), "the ai-create panel's tab");
    assert!(
        rt.find("workflow-ai-create-panel").is_some(),
        "the pane mounts (React's testid)"
    );
    assert!(rt.find("workflow-ai-create-panel-pending").is_some());
    click(&mut rt, "workflow-ai-create-button");
    assert!(
        rt.find("workflow-tab-ai-2").is_none(),
        "the second press reactivates, never a second panel (React 532-536)"
    );

    // React 689-698: the open-dir button rides the `workflowsDir` conditional.
    rt.state.workflow_dir = Some("the flows directory".to_owned());
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        rt.find("workflow-open-dir").is_some(),
        "a dir value mounts the open-dir button"
    );
    click(&mut rt, "workflow-open-dir");
    assert!(
        rt.state.workflow_feedback.contains("needs the desktop host"),
        "the open-dir seam said itself: {}",
        rt.state.workflow_feedback
    );
    rt.state.workflow_dir = None;
    rt.settle_all();
    rt.frame(0.0);
    assert!(rt.find("workflow-open-dir").is_none(), "the conditional hides it again");

    // React 737: the workflows tab keeps its literal label; 731/748: the
    // presses swap the sidebar's list.
    assert_eq!(
        element(&rt, "workflow-side-tab-workflows-text").text.as_deref(),
        Some("Workflows"),
        "React's literal tab label"
    );
    assert_eq!(
        element(&rt, "workflow-side-tab-history-text").text.as_deref(),
        Some(t("workflows.history.title"))
    );
    click(&mut rt, "workflow-side-tab-history");
    assert!(rt.find("workflow-history-list").is_some());
    assert!(rt.find("workflow-list").is_none(), "the flows list unmounted");
    click(&mut rt, "workflow-side-tab-workflows");
    assert!(rt.find("workflow-list").is_some(), "the flows list is back");
    assert!(rt.find("workflow-history-list").is_none());
}

/// The sidebar's small arms — the feedback line's ink follows React's
/// `startsWith('Error')` split (WorkflowPage.tsx:765-774), the loading arm
/// replaces the list (1319-1326), the empty list renders its sentence
/// (1386-1388), and the row's scheduled clock (1347-1351) and description
/// (1353-1355) ride their own conditionals.
#[test]
fn the_sidebar_feedback_ink_the_loading_and_empty_arms_and_the_row_marks() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // React 769: green unless the line starts with `Error`.
    rt.state.workflow_feedback = "Error: the host refused".to_owned();
    rt.settle_all();
    rt.frame(0.0);
    let line = element(&rt, "workflow-feedback");
    assert_eq!(
        line.text.as_deref(),
        Some("Error: the host refused"),
        "the line renders verbatim"
    );
    assert_eq!(
        line.text_color,
        Some(datazen_ui::ui::kit::kit(true).c.danger.fg),
        "an Error-prefixed line reads the danger ink"
    );
    rt.state.workflow_feedback = t("workflows.saved").to_owned();
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "workflow-feedback").text_color,
        Some(datazen_ui::ui::kit::kit(true).c.success.fg),
        "any other line reads the success ink"
    );

    // React 1319-1326: while loading, the spinner arm replaces the list.
    rt.state.workflow_feedback = String::new();
    rt.state.workflow_list_loading = true;
    rt.settle_all();
    rt.frame(0.0);
    assert!(rt.find("workflow-list-loading").is_some(), "the loading arm");
    assert_eq!(
        element(&rt, "workflow-list-loading-text").text.as_deref(),
        Some(t("workflows.loading"))
    );
    assert!(rt.find("workflow-list").is_none(), "the list itself unmounted");

    // React 1386-1388: the empty list renders its sentence.
    rt.state.workflow_list_loading = false;
    rt.state.workflow_list = Vec::new();
    rt.settle_all();
    rt.frame(0.0);
    assert!(rt.find("workflow-list-empty").is_some());
    assert_eq!(
        element(&rt, "workflow-list-empty-text").text.as_deref(),
        Some(t("workflows.noWorkflows"))
    );

    // React 1347-1355: the scheduled clock and the description line.
    let mut scheduled = WorkflowListItem::from_workflow(&datazen_ui_api::Workflow {
        name: "nightly-etl".to_owned(),
        steps: vec!["SELECT 1".to_owned()],
    });
    scheduled.scheduled = true;
    scheduled.description = Some("nightly at 3am".to_owned());
    rt.state.workflow_list = vec![scheduled];
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        rt.find("workflow-nightly-etl-scheduled").is_some(),
        "the scheduled row carries its clock"
    );
    assert_eq!(
        element(&rt, "workflow-nightly-etl-description").text.as_deref(),
        Some("nightly at 3am")
    );
}

/// The panel tab's reads: the status mark's ✓/✗ and ink follow the panel's
/// result (WorkflowPage.tsx:874-883), the executing tab mounts its spinner
/// (884-886) and the Execute button its executing label and disabled state
/// (949-953), the edit tab's label is the draft name (the guards'
/// `panel.draft.name || labels.edit`, workflowPanelGuards.ts:66) and the
/// run tab's label is the name plus the `HH:MM` clock (ts:68-69).
#[test]
fn the_panel_tab_status_mark_the_spinner_and_the_label_reads() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // A failed run's tab: ✗ in the danger ink (React 874-883).
    click(&mut rt, "workflow-audit");
    click(&mut rt, "workflow-execute-button");
    let failed = element(&rt, "workflow-tab-wf-1-status");
    assert_eq!(failed.text.as_deref(), Some("✗"));
    assert_eq!(failed.text_color, Some(datazen_ui::ui::kit::kit(true).c.danger.fg));

    // A successful run's tab: ✓ in the success ink.
    click(&mut rt, "workflow-weekly-report");
    click(&mut rt, "workflow-execute-button");
    let ok = element(&rt, "workflow-tab-wf-2-status");
    assert_eq!(ok.text.as_deref(), Some("✓"));
    assert_eq!(ok.text_color, Some(datazen_ui::ui::kit::kit(true).c.success.fg));

    // The edit tab's label reads the draft's name (the guards' rule); the
    // create flow's tab keeps the create label.
    click(&mut rt, "workflow-audit-edit");
    assert_eq!(
        element(&rt, "workflow-tab-edit-3-label").text.as_deref(),
        Some("audit"),
        "the editing tab labels the draft name (workflowPanelGuards.ts:66)"
    );
    click(&mut rt, "workflow-create");
    assert_eq!(
        element(&rt, "workflow-tab-edit-4-label").text.as_deref(),
        Some(t("workflows.create")),
        "the create flow's tab keeps the create label"
    );

    // The run tab's label is the name plus the HH:MM clock
    // (workflowPanelGuards.ts:68-69), never the raw ISO text.
    let label = element(&rt, "workflow-tab-wf-1-label")
        .text
        .as_deref()
        .expect("the run tab's label")
        .to_owned();
    assert!(
        label.starts_with("audit "),
        "the label reads the workflow name: {label}"
    );
    let clock: String = label.trim_start_matches("audit ").to_owned();
    assert!(
        clock.len() == 5
            && clock.as_bytes()[2] == b':'
            && clock.bytes().all(|b| b.is_ascii_digit() || b == b':'),
        "the tail is React's 2-digit hour:minute clock, not the raw ISO: {label}"
    );

    // The executing panel: the tab's spinner and the button's label + disabled
    // state (React 884-886, 949-953) — seeded the way the job path's window
    // would hold it.
    rt.state.workflow_panels = vec![WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-9".to_owned(),
        workflow_id: "weekly-report".to_owned(),
        workflow_name: "weekly-report".to_owned(),
        started_at: "2026-11-05T09:04:00Z".to_owned(),
        result: None,
        is_executing: true,
        variables: Vec::new(),
    })];
    rt.state.workflow_active_panel = Some("wf-9".to_owned());
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        rt.find("workflow-tab-wf-9-spinner").is_some(),
        "the executing tab mounts its spinner (React 884-886)"
    );
    let execute = element(&rt, "workflow-execute-button");
    assert_eq!(
        execute.label.as_deref(),
        Some(t("workflows.executing")),
        "the button reads the executing label (React 952)"
    );
    assert!(!execute.focusable, "React 949: the executing run disables it");
}

/// The panel-tab press switches the active panel and re-seeds the step view at
/// the result's first shown step (WorkflowPage.tsx:862-870), and the ladder's
/// precedence keeps a result's select-step sentence over the store's error
/// pane (1114-1121 over 1118-1121).
#[test]
fn the_tab_press_switches_and_reseeds_and_the_ladder_keeps_its_order() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // A run with a result, the step view re-seeded at the last step (desc).
    click(&mut rt, "workflow-audit");
    click(&mut rt, "workflow-execute-button");
    assert_eq!(rt.state.workflow_active_step, Some(1));
    click(&mut rt, "workflow-step-compare structures");
    assert_eq!(rt.state.workflow_active_step, Some(0));

    // The edit panel opens over the run (its press makes it the active one;
    // React's openEditPanel leaves the step index alone — the ladder ignores
    // it while an edit pane is active).
    click(&mut rt, "workflow-audit-edit");

    // React 862-870: the run tab's press re-activates the panel and re-seeds
    // the step view at the order's first shown step (desc: the last).
    click(&mut rt, "workflow-tab-wf-1");
    assert_eq!(
        rt.state.workflow_active_panel.as_deref(),
        Some("wf-1"),
        "the tab press made the run panel active"
    );
    assert_eq!(
        rt.state.workflow_active_step,
        Some(1),
        "the press re-seeded the step view (React 865-869)"
    );

    // React 1114-1121: a result with no step picked shows the select-step
    // sentence, and the store's error pane waits its turn behind it.
    rt.state.workflow_active_step = None;
    rt.state.workflow_store_error = Some("the store's error".to_owned());
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        rt.find("workflow-select-step").is_some(),
        "the select-step sentence mounts for a step-less result (React 1114-1117)"
    );
    assert_eq!(
        element(&rt, "workflow-select-step-text").text.as_deref(),
        Some(t("workflows.selectStep"))
    );
    assert!(
        rt.find("workflow-store-error").is_none(),
        "the result beats the store's error in React's ladder (1114 over 1118)"
    );

    // With no result-bearing panel left, the store's error pane takes its own
    // turn (React 1118-1121).
    rt.state.workflow_panels.clear();
    rt.state.workflow_active_panel = None;
    rt.settle_all();
    rt.frame(0.0);
    assert!(rt.find("workflow-store-error").is_some());
    assert_eq!(
        element(&rt, "workflow-store-error-text").text.as_deref(),
        Some("the store's error")
    );
    assert_eq!(
        element(&rt, "workflow-store-error-text").text_color,
        Some(datazen_ui::ui::kit::kit(true).c.danger.fg),
        "React 1120: the error pane reads the red ink"
    );
}

/// `closePanel`'s neighbour rule — the closed tab's slot hands the active id
/// to the neighbour at `Math.min(idx, next.length - 1)` (WorkflowPage.tsx:
/// 324-335): the middle of three closes onto the one that shifted into its
/// slot, the last onto the one before it, and the only one onto none (the
/// empty hint returns, React 1122-1128).
#[test]
fn closing_the_active_panel_follows_the_neighbour() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // Three panels: wf-1 (audit), wf-2 (weekly), edit-3 (the audit edit,
    // active — its press made it so).
    click(&mut rt, "workflow-audit");
    click(&mut rt, "workflow-weekly-report");
    click(&mut rt, "workflow-audit-edit");
    assert_eq!(rt.state.workflow_panels.len(), 3);

    // Closing the active middle slot: the neighbour at the shifted slot is
    // wf-2 (React 331: `next[Math.min(2, 1)]`).
    click(&mut rt, "workflow-editor-cancel");
    assert_eq!(
        rt.state.workflow_active_panel.as_deref(),
        Some("wf-2"),
        "the closed slot hands the active id to wf-2"
    );
    assert!(
        rt.find("workflow-variables").is_some(),
        "the active run panel mounts its variables row"
    );

    // Closing the last active slot: the neighbour is the one before it.
    click(&mut rt, "workflow-tab-wf-2-close");
    assert_eq!(
        rt.state.workflow_active_panel.as_deref(),
        Some("wf-1"),
        "the last slot clamps onto the remaining wf-1"
    );

    // Closing the only panel: no neighbour, the empty hint returns.
    click(&mut rt, "workflow-tab-wf-1-close");
    assert_eq!(rt.state.workflow_active_panel, None);
    assert!(rt.find("workflow-empty-hint").is_some(), "React 1122-1128's hint");
}

/// The reuse rules: a run panel for the same workflow is reactivated, never
/// duplicated (WorkflowPage.tsx:264-286 — and the reactivation clears the step
/// view, 282), and an edit panel for the same editing id is reactivated too
/// (476-482).
#[test]
fn the_run_and_edit_panels_reuse_their_dedupe_slots() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // Two run panels, then the audit row again: the existing panel is
    // reactivated (React 266-269), never a third.
    click(&mut rt, "workflow-audit");
    click(&mut rt, "workflow-weekly-report");
    click(&mut rt, "workflow-audit");
    assert_eq!(rt.state.workflow_panels.len(), 2, "the audit reactivated wf-1");
    assert!(rt.find("workflow-tab-wf-3").is_none());
    assert_eq!(rt.state.workflow_active_panel.as_deref(), Some("wf-1"));

    // React 282: opening a workflow with no run panel yet clears the step
    // view (the run above seeded one) — nightly-backup has no panel yet.
    click(&mut rt, "workflow-execute-button");
    assert!(rt.state.workflow_active_step.is_some());
    click(&mut rt, "workflow-nightly-backup");
    assert_eq!(
        rt.state.workflow_active_step, None,
        "the fresh panel starts step-less (React 282)"
    );
    assert!(
        rt.find("workflow-tab-wf-3").is_some(),
        "nightly-backup's own panel opened"
    );
    click(&mut rt, "workflow-audit");

    // The edit reuse: the same editing id reactivates its panel (React
    // 478-482) — three run panels and two edit ones, never a sixth.
    click(&mut rt, "workflow-audit-edit");
    click(&mut rt, "workflow-create");
    click(&mut rt, "workflow-audit-edit");
    assert_eq!(
        rt.state.workflow_panels.len(),
        5,
        "the audit edit reactivated its panel (3 runs + 2 edits)"
    );
    assert!(rt.find("workflow-tab-edit-6").is_none());
    assert!(
        rt.state
            .workflow_active_panel
            .as_deref()
            .is_some_and(|id| id.starts_with("edit-")),
        "the edit panel is the active one"
    );
}

/// The shared Save: the database need gates the visual mode exactly like
/// React's `stepNeedsDatabase` (WorkflowPage.tsx:1098-1109 over 114-128), the
/// successful save writes the feedback line and refetches (542-555), and the
/// yaml mode's save parses the document — a document the lean parse refuses
/// keeps the panel and names the error (557-576's missing-field arm).
#[test]
fn the_shared_save_gates_on_the_database_and_feeds_back() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // A multi-db connection the rule reads (the memory port's demo engines are
    // not in the registry's multi-db family, so the slot is seeded the way
    // the wired adapter's postgres would land).
    rt.state.workflow_form_connections = vec![datazen_ui::views::WorkflowConnectionOption {
        id: "pg".to_owned(),
        name: "Warehouse".to_owned(),
        database_type: "postgresql".to_owned(),
        database: None,
    }];

    // A valid draft whose only missing piece is the multi-db database: the
    // shared Save stays disabled (React 1103-1108).
    click(&mut rt, "workflow-audit-edit");
    rt.state.workflow_draft.connection = Some("pg".to_owned());
    rt.state.workflow_draft.database = None;
    rt.settle_all();
    rt.frame(0.0);
    let save = element(&rt, "workflow-yaml-save");
    assert!(
        !save.focusable,
        "a query step over a multi-db connection with no database disables the save"
    );

    // The draft's database satisfies the need: the save is back.
    rt.state.workflow_draft.database = Some("warehouse".to_owned());
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        element(&rt, "workflow-yaml-save").focusable,
        "the inherited database re-enables the save"
    );

    // The successful save closes the panel (the workspace-tabs suite pins the
    // host's half), writes the feedback line and refetches the list.
    click(&mut rt, "workflow-yaml-save");
    assert_eq!(
        rt.state.workflow_feedback,
        t("workflows.saved"),
        "React 547: the saved line"
    );
    assert!(
        rt.state.workflow_panels.is_empty(),
        "the saved edit panel closed"
    );
    assert!(
        !rt.state.workflow_list.is_empty(),
        "React 549: the list refetched"
    );

    // The yaml mode's save (React 557-576): the document parses, the panel
    // closes; a document the lean parse refuses keeps the panel and names
    // the error (560-565 over 570-573).
    //
    // Audit-round find (reported, not fixed): the yaml pane
    // (`workflow-edit-yaml`) lacks the definite height cap the visual pane
    // carries (`EDIT_VISUAL_MAX_H`, workflow_page.rs's own answer to the
    // engine's no-shrink rule), so in the default 1440×900 window the shared
    // action bar lands below the window's bottom edge — React's flex foot
    // (WorkflowPage.tsx:1080-1110) never leaves the pane — and the yaml-mode
    // Save press is unreachable at the tree level. The flow is pinned through
    // the press's own callback (the bar's `on_save` seam) meanwhile.
    click(&mut rt, "workflow-audit-edit");
    click(&mut rt, "workflow-mode-yaml");
    rt.state.save_workflow_edit_panel();
    assert_eq!(rt.state.workflow_feedback, t("workflows.saved"));
    assert!(rt.state.workflow_panels.is_empty(), "the yaml save closed it");

    click(&mut rt, "workflow-audit-edit");
    click(&mut rt, "workflow-mode-yaml");
    rt.state.workflow_yaml =
        datazen_ui::sql_editor::SqlEditor::from_doc("not the lite shape at all");
    rt.settle_all();
    rt.frame(0.0);
    rt.state.save_workflow_edit_panel();
    let prefix = t("workflows.editor.parseError").replace("{error}", "");
    assert!(
        rt.state.workflow_feedback.starts_with(prefix.trim_end()),
        "the refused parse named the error: {}",
        rt.state.workflow_feedback
    );
    assert!(
        !rt.state.workflow_panels.is_empty(),
        "the refused save keeps the panel open"
    );

    // The lean parse's own boundary: a document with a name but no steps is
    // the refused shape too (the full grammar is T646's).
    rt.state.workflow_yaml = datazen_ui::sql_editor::SqlEditor::from_doc("name: x\nsteps:\n");
    rt.settle_all();
    rt.frame(0.0);
    rt.state.save_workflow_edit_panel();
    assert!(
        rt.state.workflow_feedback.starts_with(prefix.trim_end()),
        "the steps-less document is refused: {}",
        rt.state.workflow_feedback
    );
}

/// The step detail's header reads and its two conditional blocks: the
/// connection, the time and the rows count (WorkflowPage.tsx:1179-1187), the
/// AI step's result block and its null arm (1237-1243), and the no-result
/// sentence for a data-less query step (1271-1274) — including React's
/// `hasData` conjunction (1173) that keeps a columned, row-less result off
/// the table branch.
#[test]
fn the_step_detail_header_reads_and_the_ai_and_no_result_arms() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // One run panel over three seeded steps: a rich query step, an AI step
    // with a result, and a columned but row-less query step.
    let report = StepExecutionResult {
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
    let ai = StepExecutionResult {
        step_id: "ai".to_owned(),
        step_type: Some("ai".to_owned()),
        status: WorkflowStepStatus::Success,
        result: Some(json!({"result": "the model's answer"})),
        execution_time_ms: 9,
        error: None,
        connection_name: None,
        sql_executed: None,
    };
    let dry = StepExecutionResult {
        step_id: "dry".to_owned(),
        step_type: Some("query".to_owned()),
        status: WorkflowStepStatus::Success,
        result: Some(json!({
            "columns": [{"name": "label", "dataType": "text"}],
            "rows": []
        })),
        execution_time_ms: 2,
        error: None,
        connection_name: None,
        sql_executed: None,
    };
    let result = WorkflowExecutionResult {
        success: true,
        final_output: String::new(),
        steps: vec![report, ai, dry],
        total_time_ms: 15,
        error: None,
    };
    rt.state.workflow_panels = vec![WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-9".to_owned(),
        workflow_id: "weekly-report".to_owned(),
        workflow_name: "weekly-report".to_owned(),
        started_at: "2026-11-05T09:04:00Z".to_owned(),
        result: Some(result),
        is_executing: false,
        variables: Vec::new(),
    })];
    rt.state.workflow_active_panel = Some("wf-9".to_owned());
    rt.state.workflow_active_step = Some(0);
    rt.settle_all();
    rt.frame(0.0);

    // The header's reads (React 1178-1187).
    assert_eq!(
        element(&rt, "workflow-step-detail-connection").text.as_deref(),
        Some("warehouse"),
        "React 1181: the step's connection"
    );
    assert_eq!(
        element(&rt, "workflow-step-detail-time").text.as_deref(),
        Some("4ms"),
        "React 1182: the execution time"
    );
    assert_eq!(
        element(&rt, "workflow-step-detail-rows").text.as_deref(),
        Some(format!("{} {}", 2, t("common.rows")).as_str()),
        "React 1183-1187: the rows count over the result's rows"
    );
    assert_eq!(
        element(&rt, "workflow-step-detail-type").text.as_deref(),
        Some("[query]"),
        "React 1179: the type chip"
    );

    // The AI step's result block (React 1237-1243).
    click(&mut rt, "workflow-step-ai");
    assert!(
        rt.find("workflow-step-ai-result").is_some(),
        "React 1237: an ai step with a result mounts its block"
    );
    assert_eq!(
        element(&rt, "workflow-step-ai-result-text").text.as_deref(),
        Some("the model's answer")
    );

    // The null arm: a null result keeps the block away.
    if let Some(WorkflowPanel::Run(run)) = rt.state.workflow_panels.first_mut() {
        if let Some(step) = run.result.as_mut().and_then(|r| r.steps.get_mut(1)) {
            step.result = Some(json!({"result": null}));
        }
    }
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        rt.find("workflow-step-ai-result").is_none(),
        "React 1237: a null result renders no block"
    );

    // The data-less query step: the table branch stays away (React 1173's
    // conjunction) and the sentence mounts (1271-1274).
    click(&mut rt, "workflow-step-dry");
    assert!(
        rt.find("workflow-step-table").is_none(),
        "React 1173: columns without rows are not data"
    );
    assert!(rt.find("workflow-step-no-result").is_some());
    assert_eq!(
        element(&rt, "workflow-step-no-result-text").text.as_deref(),
        Some(t("workflows.noQueryResult"))
    );
}

/// The chart mode's sampling boundary: the warning rides React's
/// `rows.length > 1000` (WorkflowPage.tsx:1247-1251) — one row past the
/// thousandth mounts it, the thousandth itself does not.
#[test]
fn the_chart_mode_warns_only_past_the_thousandth_row() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // A chartable step result with 1001 rows (one past React's limit).
    let rows: Vec<_> = (0..1001)
        .map(|n| json!({"label": format!("row-{n}"), "n": n}))
        .collect();
    let step = StepExecutionResult {
        step_id: "wide".to_owned(),
        step_type: Some("query".to_owned()),
        status: WorkflowStepStatus::Success,
        result: Some(json!({
            "columns": [
                {"name": "label", "dataType": "text"},
                {"name": "n", "dataType": "int8"}
            ],
            "rows": rows
        })),
        execution_time_ms: 7,
        error: None,
        connection_name: None,
        sql_executed: Some("SELECT label, n FROM facts".to_owned()),
    };
    let result = WorkflowExecutionResult {
        success: true,
        final_output: String::new(),
        steps: vec![step],
        total_time_ms: 7,
        error: None,
    };
    rt.state.workflow_panels = vec![WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-9".to_owned(),
        workflow_id: "weekly-report".to_owned(),
        workflow_name: "weekly-report".to_owned(),
        started_at: "2026-11-05T09:04:00Z".to_owned(),
        result: Some(result),
        is_executing: false,
        variables: Vec::new(),
    })];
    rt.state.workflow_active_panel = Some("wf-9".to_owned());
    rt.state.workflow_active_step = Some(0);
    rt.settle_all();
    rt.frame(0.0);

    click(&mut rt, "workflow-view-chart");
    assert!(
        rt.find("workflow-step-sampled-warning").is_some(),
        "React 1247: 1001 rows mount the sampling warning"
    );
    assert_eq!(
        element(&rt, "workflow-step-sampled-warning-text").text.as_deref(),
        Some(t("workflows.sampledWarning").replace("{limit}", "1000").as_str())
    );

    // The boundary's inside: the thousandth row itself stays quiet.
    click(&mut rt, "workflow-view-table");
    if let Some(WorkflowPanel::Run(run)) = rt.state.workflow_panels.first_mut() {
        if let Some(step) = run.result.as_mut().and_then(|r| r.steps.get_mut(0)) {
            if let Some(result) = step.result.as_mut() {
                result["rows"] = json!([{"label": "row-0", "n": 0}]);
            }
        }
    }
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "workflow-view-chart");
    assert!(
        rt.find("workflow-step-sampled-warning").is_none(),
        "React 1247: the thousand-row boundary itself does not warn"
    );
    assert!(rt.find("chart-view").is_some(), "the chart still mounts");
}

/// The miss feedback arms and the order's re-seed: the edit lookup's miss
/// writes the feedback line (WorkflowPage.tsx:518-525's catch), the history
/// entry's miss the same (317-319), the order toggle's label flips with the
/// setting (989-991) and the asc order re-seeds the step view at the first
/// step (106-108 over 425-427).
#[test]
fn the_miss_feedback_arms_and_the_asc_order_reseeds() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // React 522-524: the edit flow's catch writes the feedback line.
    rt.state.edit_workflow_from_list("nope");
    assert!(
        rt.state.workflow_feedback.contains("workflow not found: nope"),
        "the lookup's miss fed back: {}",
        rt.state.workflow_feedback
    );

    // React 317-319: the history flow's catch writes the feedback line.
    rt.state.open_workflow_history_detail("nope", "ghost");
    assert!(
        rt.state
            .workflow_feedback
            .contains("history entry not found: nope"),
        "the history miss fed back: {}",
        rt.state.workflow_feedback
    );

    // The run's re-seed: desc lands on the last step, the toggled asc order
    // lands on the first (React 425-427 over 106-108).
    rt.state.workflow_feedback = String::new();
    click(&mut rt, "workflow-audit");
    click(&mut rt, "workflow-execute-button");
    assert_eq!(rt.state.workflow_active_step, Some(1), "desc seeds the last");
    click(&mut rt, "workflow-step-order-toggle");
    assert_eq!(
        element(&rt, "workflow-step-order-toggle-label").text.as_deref(),
        Some(t("workflows.stepOrder.asc")),
        "React 989-991: the toggled label reads the asc text"
    );
    click(&mut rt, "workflow-execute-button");
    assert_eq!(
        rt.state.workflow_active_step, Some(0),
        "the asc order re-seeds at the first step"
    );
}

/// The variables row's gating and its seeding: a declared default seeds the
/// input (React's `v.default != null ? String(v.default) : ''`,
/// WorkflowPage.tsx:397-401), the connection variable's choose writes the
/// connection id (924-929), and a run panel whose workflow left the list
/// hides the row (the `currentWorkflow` gate, 910).
#[test]
fn the_variable_default_seeds_and_the_ghost_panel_hides_the_row() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // A workflow whose declared variable carries a default: the select flow
    // seeds the input with it (React 397-401).
    let mut workflow = WorkflowListItem::from_workflow(&datazen_ui_api::Workflow {
        name: "sized-report".to_owned(),
        steps: vec!["SELECT 1".to_owned()],
    });
    workflow.variables = vec![
        WorkflowVariableItem {
            name: "city".to_owned(),
            var_type: "string".to_owned(),
            description: None,
            required: false,
            default: Some("上海".to_owned()),
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
    assert_eq!(
        rt.state.workflow_variable("city"),
        Some("上海"),
        "the declared default seeded the input"
    );

    // The connection variable's Select: the choose writes the connection id
    // (React 924-929 — the options are `value: c.id`; the port's listbox
    // counts an empty first row, so `option1` is the first connection).
    click(&mut rt, "workflow-variable-warehouse-trigger");
    assert!(rt.find("option1").is_some(), "the connection listbox opened");
    click(&mut rt, "option1");
    assert_eq!(
        rt.state.workflow_variable("warehouse"),
        Some("local"),
        "the choose wrote the connection's id, not its label"
    );

    // The ghost panel: a run panel whose workflow left the list mounts no
    // variables row (React 910's `currentWorkflow` gate).
    rt.state.workflow_panels = vec![WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-9".to_owned(),
        workflow_id: "ghost".to_owned(),
        workflow_name: "ghost".to_owned(),
        started_at: "2026-11-05T09:04:00Z".to_owned(),
        result: None,
        is_executing: false,
        variables: Vec::new(),
    })];
    rt.state.workflow_active_panel = Some("wf-9".to_owned());
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        rt.find("workflow-variables").is_none(),
        "React 910: no current workflow, no variables row"
    );
    assert!(
        rt.find("workflow-execute-button").is_none(),
        "and no execute row with it"
    );
    assert!(
        rt.find("workflow-tab-wf-9").is_some(),
        "the panel's own tab stays"
    );
}

// ── the audit round's second half — the independent probes' pins ────────────
//
// Each pin below holds a semantics arm the trimmed set (committed pins + the
// round's first half above) left un-pinned: a per-step rule, an exact literal,
// a reuse arm, a boundary, or an event wiring. Every pin names the React
// source location it holds; the surviving mutants that demanded each one are
// in the audit report.

/// `stepNeedsDatabase`'s per-step arms (WorkflowPage.tsx:114-128) — the rule
/// is per *step*, not per draft: only query/command steps count (121), the
/// step's own connection overrides the draft's (122), the step's own database
/// satisfies the need (125), and an unknown connection id is not multi-db
/// (123-124, `connections.find`'s miss).
#[test]
fn the_save_gate_reads_each_steps_own_needs() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // A multi-db connection (postgresql) and a single-db one (sqlite).
    rt.state.workflow_form_connections = vec![
        datazen_ui::views::WorkflowConnectionOption {
            id: "pg".to_owned(),
            name: "Warehouse".to_owned(),
            database_type: "postgresql".to_owned(),
            database: None,
        },
        datazen_ui::views::WorkflowConnectionOption {
            id: "sqlite".to_owned(),
            name: "Local".to_owned(),
            database_type: "sqlite".to_owned(),
            database: None,
        },
    ];
    click(&mut rt, "workflow-create");

    // A valid draft (id, name, steps) whose only open question is the
    // multi-db database — the per-step arms below flip the Save alone.
    let draft = |steps: Vec<datazen_ui::views::workflow_form::WorkflowStepDraft>| {
        let mut draft = datazen_ui::views::workflow_form::WorkflowDraft::empty();
        draft.id = rune::TextEdit::new("etl".to_owned());
        draft.name = rune::TextEdit::new("etl".to_owned());
        draft.connection = Some("pg".to_owned());
        draft.database = None;
        draft.steps = steps;
        draft
    };
    let save_enabled = |rt: &mut rune::Runtime<datazen_ui::AppState>| {
        rt.settle_all();
        rt.frame(0.0);
        element(rt, "workflow-yaml-save").focusable
    };

    // The baseline: a query step over the draft's multi-db connection with
    // no database anywhere needs one (React 121-126).
    rt.state.workflow_draft = draft(vec![datazen_ui::views::workflow_form::WorkflowStepDraft::query(
        "step1",
    )]);
    assert!(
        !save_enabled(&mut rt),
        "a query step over a multi-db connection still needs its database"
    );

    // React 121: an AI step is not a data step — no database is asked of it.
    let mut ai = datazen_ui::views::workflow_form::WorkflowStepDraft::query("step1");
    ai.kind = datazen_ui::views::workflow_form::WorkflowStepKind::Ai;
    rt.state.workflow_draft = draft(vec![ai]);
    assert!(
        save_enabled(&mut rt),
        "React 121: only query/command steps carry the database need"
    );

    // React 121: a command step counts exactly like a query one.
    rt.state.workflow_draft = draft(vec![
        datazen_ui::views::workflow_form::WorkflowStepDraft::command("step1"),
    ]);
    assert!(
        !save_enabled(&mut rt),
        "a command step over a multi-db connection needs its database too"
    );

    // React 122: the step's own connection overrides the draft's — a step
    // pinned to the single-db sqlite leaves the draft's pg unconsulted.
    let mut own_conn = datazen_ui::views::workflow_form::WorkflowStepDraft::query("step1");
    own_conn.connection = Some("sqlite".to_owned());
    rt.state.workflow_draft = draft(vec![own_conn]);
    assert!(
        save_enabled(&mut rt),
        "React 122: the step's own connection replaces the draft's"
    );

    // React 123-124: a connection the registry never heard of is not
    // multi-db — `connections.find`'s miss reads as no need.
    let mut ghost_conn = datazen_ui::views::workflow_form::WorkflowStepDraft::query("step1");
    ghost_conn.connection = Some("ghost".to_owned());
    rt.state.workflow_draft = draft(vec![ghost_conn]);
    assert!(
        save_enabled(&mut rt),
        "React 123-124: an unknown connection id is not a multi-db one"
    );

    // React 125: the step's own database satisfies the need even while the
    // draft's default is still missing.
    let mut own_db = datazen_ui::views::workflow_form::WorkflowStepDraft::query("step1");
    own_db.database = Some("warehouse".to_owned());
    rt.state.workflow_draft = draft(vec![own_db]);
    assert!(
        save_enabled(&mut rt),
        "React 125: the step's own database satisfies the need"
    );
}

/// The run status line's literal — React's `✓/✗ {totalTimeMs}ms` (the span,
/// WorkflowPage.tsx:968-979) — and the add-to-dashboard arm's success gate
/// (954: `currentResult?.success && activePanel?.type === 'run'`): a failed
/// run keeps the press away, a successful one mounts it.
#[test]
fn the_run_status_reads_the_total_and_gates_the_dashboard_press() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // A failed run over the audit workflow: the status line reads the mark,
    // the total and the unit exactly (React 978), and the failed answer keeps
    // the add-to-dashboard press away (React 954).
    let failed = WorkflowExecutionResult {
        success: false,
        final_output: String::new(),
        steps: Vec::new(),
        total_time_ms: 7,
        error: None,
    };
    rt.state.workflow_panels = vec![WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-9".to_owned(),
        workflow_id: "audit".to_owned(),
        workflow_name: "audit".to_owned(),
        started_at: "2026-11-05T09:04:00Z".to_owned(),
        result: Some(failed),
        is_executing: false,
        variables: Vec::new(),
    })];
    rt.state.workflow_active_panel = Some("wf-9".to_owned());
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "workflow-run-status").text.as_deref(),
        Some("✗ 7ms"),
        "React 978: the mark, the total and the unit, verbatim"
    );
    assert!(
        rt.find("workflow-add-to-dashboard").is_none(),
        "React 954: a failed run mounts no add-to-dashboard press"
    );

    // The successful answer flips both.
    let ok = WorkflowExecutionResult {
        success: true,
        final_output: String::new(),
        steps: Vec::new(),
        total_time_ms: 4,
        error: None,
    };
    if let Some(WorkflowPanel::Run(run)) = rt.state.workflow_panels.first_mut() {
        run.result = Some(ok);
    }
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "workflow-run-status").text.as_deref(),
        Some("✓ 4ms"),
        "the successful line reads the ✓ mark"
    );
    assert!(
        rt.find("workflow-add-to-dashboard").is_some(),
        "React 954: the successful run mounts the press"
    );
}

/// The reuse arms' quiet halves: a second create press reactivates the open
/// create panel — both carry `editingId: null`, so React's find matches the
/// first (WorkflowPage.tsx:476-482) — and a workflow row's reuse press does
/// NOT re-seed the step view (266-269 return before 282's reset; only the tab
/// press re-seeds, 862-870).
#[test]
fn the_create_press_reuses_and_the_row_reuse_keeps_the_step_view() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // React 478-482 with `editingId === null`: the second create press
    // reactivates edit-1, never a second create panel.
    click(&mut rt, "workflow-create");
    assert!(rt.find("workflow-tab-edit-1").is_some(), "the create panel");
    click(&mut rt, "workflow-create");
    assert_eq!(
        rt.state.workflow_panels.len(),
        1,
        "the second create press reactivated edit-1 (React 478-482)"
    );
    assert!(rt.find("workflow-tab-edit-2").is_none());
    assert_eq!(
        rt.state.workflow_active_panel.as_deref(),
        Some("edit-1"),
        "the create panel is the active one"
    );
    click(&mut rt, "workflow-editor-cancel");

    // The row reuse: run audit (wf-2), execute (the step view seeds at the
    // order's first step), open nightly-backup (a fresh panel resets the step
    // view, React 282), then press the audit row again — the reuse path
    // returns before the reset, so the step view STAYS cleared (266-269).
    click(&mut rt, "workflow-audit");
    click(&mut rt, "workflow-execute-button");
    assert_eq!(rt.state.workflow_active_step, Some(1), "the run seeded a step");
    click(&mut rt, "workflow-nightly-backup");
    assert_eq!(rt.state.workflow_active_step, None, "the fresh panel reset it");
    click(&mut rt, "workflow-audit");
    assert_eq!(
        rt.state.workflow_active_panel.as_deref(),
        Some("wf-2"),
        "the audit row reactivated its own panel"
    );
    assert_eq!(
        rt.state.workflow_active_step, None,
        "React 266-269: the reuse press does not re-seed the step view"
    );

    // The tab press on the same panel DOES re-seed (React 862-870) — the
    // contrast that pins the reuse arm's own quietness.
    click(&mut rt, "workflow-tab-wf-2");
    assert_eq!(
        rt.state.workflow_active_step,
        Some(1),
        "React 862-870: the tab press re-seeded the step view"
    );
}

/// The variable label's description arm (WorkflowPage.tsx:919-921) — React
/// renders `({v.description})` beside the name — and the feedback line's own
/// mount gate (765: `feedback &&`): an empty line mounts nothing.
#[test]
fn the_variable_label_reads_its_description_and_the_empty_feedback_hides() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // React 765: an empty feedback line is no line at all.
    assert!(
        rt.find("workflow-feedback").is_none(),
        "React 765: the empty feedback mounts nothing"
    );

    // A workflow whose variable declares a description: the label row reads
    // the name and the parenthesised description beside it (React 916-921).
    let mut workflow = WorkflowListItem::from_workflow(&datazen_ui_api::Workflow {
        name: "sized-report".to_owned(),
        steps: vec!["SELECT 1".to_owned()],
    });
    workflow.variables = vec![WorkflowVariableItem {
        name: "city".to_owned(),
        var_type: "string".to_owned(),
        description: Some("the city filter".to_owned()),
        required: false,
        default: None,
    }];
    rt.state.workflow_list = vec![workflow.clone()];
    rt.state.select_workflow_list_item(&workflow);
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "workflow-variable-city-label").text.as_deref(),
        Some("city"),
        "React 917: the label leads with the name"
    );
    let described = rt
        .elements()
        .into_iter()
        .any(|e| e.text.as_deref() == Some("(the city filter)"));
    assert!(
        described,
        "React 919-921: the description renders parenthesised beside the name"
    );
}

/// The step detail's gate conjunctions and literals: the view toggle rides
/// `hasData && chartable` (WorkflowPage.tsx:1189) — a columned, row-less step
/// mounts none; the no-result sentence rides `stepType === 'query' &&
/// !step.error` (1271-1274) — a failed query step and a data-less AI step
/// mount none; and the SQL block renders its text verbatim (1223-1229).
#[test]
fn the_step_detail_gates_the_toggle_the_sentence_and_the_sql() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // One run panel over three steps: a rich query step, a failed query step
    // with an error, and a data-less AI step.
    let rich = StepExecutionResult {
        step_id: "rich".to_owned(),
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
    let failed = StepExecutionResult {
        step_id: "failed".to_owned(),
        step_type: Some("query".to_owned()),
        status: WorkflowStepStatus::Failed,
        result: None,
        execution_time_ms: 3,
        error: Some("the step's own error".to_owned()),
        connection_name: None,
        sql_executed: None,
    };
    let ai = StepExecutionResult {
        step_id: "ai".to_owned(),
        step_type: Some("ai".to_owned()),
        status: WorkflowStepStatus::Success,
        result: None,
        execution_time_ms: 9,
        error: None,
        connection_name: None,
        sql_executed: None,
    };
    // A data-bearing but not-chartable step (text columns only): React's
    // `hasData && chartable` conjunction keeps its toggle away too.
    let words = StepExecutionResult {
        step_id: "words".to_owned(),
        step_type: Some("query".to_owned()),
        status: WorkflowStepStatus::Success,
        result: Some(json!({
            "columns": [{"name": "label", "dataType": "text"}],
            "rows": [{"label": "one"}]
        })),
        execution_time_ms: 2,
        error: None,
        connection_name: None,
        sql_executed: None,
    };
    let result = WorkflowExecutionResult {
        success: false,
        final_output: String::new(),
        steps: vec![rich, failed, ai, words],
        total_time_ms: 16,
        error: None,
    };
    rt.state.workflow_panels = vec![WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-9".to_owned(),
        workflow_id: "audit".to_owned(),
        workflow_name: "audit".to_owned(),
        started_at: "2026-11-05T09:04:00Z".to_owned(),
        result: Some(result),
        is_executing: false,
        variables: Vec::new(),
    })];
    rt.state.workflow_active_panel = Some("wf-9".to_owned());
    rt.state.workflow_active_step = Some(0);
    rt.settle_all();
    rt.frame(0.0);

    // The rich step: the SQL block's text is verbatim (React 1225-1227) and
    // its data mounts the view toggle (1189's conjunction holds).
    assert_eq!(
        element(&rt, "workflow-step-sql-text").text.as_deref(),
        Some("SELECT label, n FROM facts"),
        "React 1226: the executed SQL renders verbatim"
    );
    assert!(rt.find("workflow-step-view-toggle").is_some());

    // The failed query step: its own error block shows and the no-result
    // sentence stays away (React 1271's `!step.error` arm).
    click(&mut rt, "workflow-step-failed");
    assert!(
        element(&rt, "workflow-step-error-text").text.is_some(),
        "the failed step's error renders"
    );
    assert!(
        rt.find("workflow-step-no-result").is_none(),
        "React 1271: a failed query step carries an error, not the sentence"
    );

    // The data-less AI step: no table branch, no view toggle (React 1189's
    // hasData) and no no-result sentence (1271's query-type gate).
    click(&mut rt, "workflow-step-ai");
    assert!(
        rt.find("workflow-step-view-toggle").is_none(),
        "React 1189: no data, no toggle"
    );
    assert!(
        rt.find("workflow-step-no-result").is_none(),
        "React 1271: the sentence is the query step's alone"
    );

    // The data-bearing but not-chartable step: the conjunction's other arm —
    // data alone is not chartable, so no toggle (React 1189).
    click(&mut rt, "workflow-step-words");
    assert!(
        rt.find("workflow-step-table").is_some(),
        "the text-only table branch mounts"
    );
    assert!(
        rt.find("workflow-step-view-toggle").is_none(),
        "React 1189: data without a chartable column mounts no toggle"
    );
}

/// The rows count's preference and gate (WorkflowPage.tsx:1167-1187): the
/// declared `rows_count` wins over the table's own length (`typeof
/// step.result?.rows_count === 'number'`), and a declared zero keeps the
/// count line away (1183's `rowsCount > 0`).
#[test]
fn the_rows_count_prefers_the_declared_count_and_gates_on_zero() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // A step whose two rows carry a declared count of 500: the header reads
    // the declaration, not the table (React 1167-1168).
    let declared = StepExecutionResult {
        step_id: "counted".to_owned(),
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
            ],
            "rows_count": 500
        })),
        execution_time_ms: 4,
        error: None,
        connection_name: None,
        sql_executed: None,
    };
    let result = WorkflowExecutionResult {
        success: true,
        final_output: String::new(),
        steps: vec![declared],
        total_time_ms: 4,
        error: None,
    };
    rt.state.workflow_panels = vec![WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-9".to_owned(),
        workflow_id: "audit".to_owned(),
        workflow_name: "audit".to_owned(),
        started_at: "2026-11-05T09:04:00Z".to_owned(),
        result: Some(result),
        is_executing: false,
        variables: Vec::new(),
    })];
    rt.state.workflow_active_panel = Some("wf-9".to_owned());
    rt.state.workflow_active_step = Some(0);
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "workflow-step-detail-rows").text.as_deref(),
        Some(format!("{} {}", 500, t("common.rows")).as_str()),
        "React 1168: the declared rows_count wins over the table's length"
    );

    // A declared zero keeps the count line away even while rows exist
    // (React 1183's `rowsCount > 0` gate).
    if let Some(WorkflowPanel::Run(run)) = rt.state.workflow_panels.first_mut() {
        if let Some(step) = run.result.as_mut().and_then(|r| r.steps.get_mut(0)) {
            step.result = Some(json!({
                "columns": [
                    {"name": "label", "dataType": "text"},
                    {"name": "n", "dataType": "int8"}
                ],
                "rows": [
                    {"label": "one", "n": 3}
                ],
                "rows_count": 0
            }));
        }
    }
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        rt.find("workflow-step-detail-rows").is_none(),
        "React 1183: a declared zero hides the count line"
    );
    // The data itself is unaffected — the table branch still mounts.
    assert!(rt.find("workflow-step-table").is_some());
}

/// The status ladder's icon arms in both copies (the steps bar's tab,
/// WorkflowPage.tsx:1014, and the detail header's mark, 1180): success and
/// failure render their text glyphs, while the skipped and timed-out steps
/// render icon marks (the panel's lucide fallback ruling) — a text-armed
/// mutant would give them glyphs. The bar's `[{stepType}]` chip renders its
/// literal (1016).
#[test]
fn the_status_ladder_keeps_the_icon_arms_and_the_type_chip() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    let step = |id: &str, status: WorkflowStepStatus| StepExecutionResult {
        step_id: id.to_owned(),
        step_type: Some("query".to_owned()),
        status,
        result: None,
        execution_time_ms: 1,
        error: None,
        connection_name: None,
        sql_executed: None,
    };
    let result = WorkflowExecutionResult {
        success: false,
        final_output: String::new(),
        steps: vec![
            step("ok", WorkflowStepStatus::Success),
            step("bad", WorkflowStepStatus::Failed),
            step("skip", WorkflowStepStatus::Skipped),
            step("slow", WorkflowStepStatus::TimedOut),
        ],
        total_time_ms: 4,
        error: None,
    };
    rt.state.workflow_panels = vec![WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-9".to_owned(),
        workflow_id: "audit".to_owned(),
        workflow_name: "audit".to_owned(),
        started_at: "2026-11-05T09:04:00Z".to_owned(),
        result: Some(result),
        is_executing: false,
        variables: Vec::new(),
    })];
    rt.state.workflow_active_panel = Some("wf-9".to_owned());
    rt.state.workflow_active_step = Some(0);
    rt.settle_all();
    rt.frame(0.0);

    // The steps bar's marks (React 1014): the text glyphs for the first two
    // arms, the icon arms (no text) for the skipped and timed-out ones.
    assert_eq!(
        element(&rt, "workflow-step-ok-status").text.as_deref(),
        Some("✓"),
        "the success arm's glyph"
    );
    assert_eq!(
        element(&rt, "workflow-step-bad-status").text.as_deref(),
        Some("✗"),
        "the failure arm's glyph"
    );
    assert!(
        element(&rt, "workflow-step-skip-status").text.is_none(),
        "the skipped arm renders an icon, not a text glyph"
    );
    assert!(
        element(&rt, "workflow-step-slow-status").text.is_none(),
        "the timed-out arm renders an icon, not a text glyph"
    );
    // React 1016: the type chip's literal.
    assert_eq!(
        element(&rt, "workflow-step-ok-type").text.as_deref(),
        Some("[query]"),
        "React 1016: the bracketed type chip"
    );

    // The detail header's own ladder (React 1180): the same four arms.
    assert_eq!(
        element(&rt, "workflow-step-detail-status").text.as_deref(),
        Some("✓"),
        "the header's success mark"
    );
    assert_eq!(
        element(&rt, "workflow-step-detail-status").text_color,
        Some(datazen_ui::ui::kit::kit(true).c.success.fg),
        "the success mark's ink"
    );
    click(&mut rt, "workflow-step-bad");
    assert_eq!(
        element(&rt, "workflow-step-detail-status").text.as_deref(),
        Some("✗"),
        "the header's failure mark"
    );
    assert_eq!(
        element(&rt, "workflow-step-detail-status").text_color,
        Some(datazen_ui::ui::kit::kit(true).c.danger.fg),
        "the failure mark's ink"
    );
    click(&mut rt, "workflow-step-skip");
    assert!(
        element(&rt, "workflow-step-detail-status").text.is_none(),
        "the header's skipped mark is an icon"
    );
    click(&mut rt, "workflow-step-slow");
    assert!(
        element(&rt, "workflow-step-detail-status").text.is_none(),
        "the header's timed-out mark is an icon"
    );
}

/// The history detail panel reads the entry's own fields (WorkflowPage.tsx:
/// 302-309): the lean list item the wiring's `workflowHistoryGet` would answer
/// carries its own success flag and total time, and the panel's status line
/// reads them — not the last run's.
#[test]
fn the_history_panel_reads_the_entrys_own_fields() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);
    click(&mut rt, "workflow-side-tab-history");

    rt.state.workflow_history_loaded(vec![datazen_ui::views::HistoryListItem {
        id: "run-7".to_owned(),
        workflow_id: "audit".to_owned(),
        workflow_name: "audit".to_owned(),
        success: true,
        total_time_ms: 9,
        created_at: "2026-11-05T09:04:00+08:00".to_owned(),
    }]);
    rt.settle_all();
    rt.frame(0.0);
    rt.frame(0.0);
    click(&mut rt, "workflow-history-list-item-run-7");
    assert!(rt.find("workflow-tab-hist-1").is_some(), "the panel opened");

    // The panel's status line reads the entry's own success and total
    // (React 307-309: the panel carries `entry.result`), verbatim.
    assert_eq!(
        element(&rt, "workflow-run-status").text.as_deref(),
        Some("✓ 9ms"),
        "React 302-309: the entry's own success and total time"
    );
}

/// The create flow's stub document (WorkflowPage.tsx:488-494): React's create
/// stub serializes one `SELECT 1` query step — the lite writer's projection of
/// it is the `SELECT 1` step under the draft's name.
#[test]
fn the_create_stub_carries_the_select_one_step() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    click(&mut rt, "workflow-create");
    let doc = rt.state.workflow_yaml.doc().to_owned();
    assert!(
        doc.starts_with("name:"),
        "the create stub writes the writer's shape: {doc}"
    );
    assert!(
        doc.contains("  - SELECT 1"),
        "React 493: the create stub's one step is the SELECT 1 literal: {doc}"
    );
}

/// The sidebar's resizable drag (WorkflowPage.tsx:210-216): `useResizable`
/// holds the sidebar between 180 and 420 around its 256 default, and the
/// drag previews live — the sidebar's own width follows the gesture and clamps
/// inside the bounds.
///
/// Audit-round find (reported, not fixed): the commit half is broken — the
/// port passes React's own storage key `workflow.sidebar`
/// (`SIDEBAR_WIDTH_KEY`) to the settings layer, whose `Setting::Layout`
/// grammar accepts only lowercase letters, digits and hyphens (every other
/// splitter ships a hyphenated key, e.g. `query-sidebar-width`), so the write
/// is rejected and `finish_drag`'s `let _ =` swallows the error: nothing
/// persists and the released drag snaps the sidebar back to the 256 default.
/// The pins hold the live halves and the current snap-back; the fix (a
/// hyphenated key) flips the release half.
#[test]
fn the_sidebar_drag_previews_live_and_the_release_snaps_back() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // The default: never resized, no persisted size, the sidebar at 256.
    let sidebar = |rt: &rune::Runtime<datazen_ui::AppState>| {
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("workflow-sidebar") && !e.exiting)
            .map(|e| e.rect[2])
            .unwrap_or_default()
    };
    assert_eq!(
        rt.state.layout_size("workflow.sidebar"),
        None,
        "the un-resized sidebar holds no persisted size"
    );
    assert_eq!(sidebar(&rt), 256.0, "React 212: the default width");

    // The drag's live preview: 256 + 100 travels to 356 (React 210-216's
    // real drag, inside the bounds) and the sidebar's own width follows.
    let (x, y) = rt.find("splitter:workflow.sidebar").expect("the sidebar's splitter");
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.splitter_preview,
        Some(("workflow.sidebar".to_owned(), 256)),
        "the press seeds the preview at the start size"
    );
    rt.pointer_move(x + 100.0, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.splitter_preview,
        Some(("workflow.sidebar".to_owned(), 356)),
        "the move previews 256 + 100"
    );
    assert_eq!(sidebar(&rt), 356.0, "the sidebar's width follows the preview");

    // The min bound clamps the preview live (React 213's 180).
    rt.pointer_move(x - 600.0, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.splitter_preview,
        Some(("workflow.sidebar".to_owned(), 180)),
        "React 213: the min bound clamps the live preview"
    );
    assert_eq!(sidebar(&rt), 180.0);

    // The max bound clamps the preview live (React 214's 420).
    rt.pointer_move(x + 2000.0, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.splitter_preview,
        Some(("workflow.sidebar".to_owned(), 420)),
        "React 214: the max bound clamps the live preview"
    );
    assert_eq!(sidebar(&rt), 420.0);

    // The release: the settings layer rejects the dotted key (the audit
    // find above), the commit is dropped, and the sidebar snaps back to the
    // default — nothing persists.
    rt.pointer_up(x + 2000.0, y);
    rt.frame(1.0 / 60.0);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.splitter_preview, None, "the release clears the preview");
    assert_eq!(
        rt.state.layout_size("workflow.sidebar"),
        None,
        "the dotted key's commit was rejected — nothing persisted"
    );
    assert_eq!(
        sidebar(&rt),
        256.0,
        "the released drag snaps back to the default (the un-persisted commit)"
    );
}

/// The confirm dialog's Escape path — the Dialog base's key route rides the
/// same `on_close` the header close takes (React's ConfirmDialog resolves
/// the ask false on Escape): the ask settles without running the delete.
/// The counter-evidence: unwiring `on_close` in `views/modals.rs` (the
/// dialog would ignore Escape and the header X alike) leaves every other
/// confirm test green — only this pin dies.
#[test]
fn the_confirm_dialogs_escape_resolves_the_ask_cancel() {
    let mut rt = runtime();
    rt.frame(0.0);
    enter_workflows(&mut rt);

    // The delete ask mounts the dialog.
    rt.state.ask_workflow_delete("audit");
    rt.settle_all();
    rt.frame(0.0);
    assert!(rt.find("workflow-confirm-dialog").is_some());

    // Escape settles the ask false — the workflow stays, nothing ran.
    rt.key(rune::KeyEvent {
        key: rune::Key::Escape,
        mods: rune::Mods::default(),
    });
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        rt.find("workflow-confirm-dialog").is_none(),
        "Escape closed the dialog"
    );
    assert!(
        rt.state.workflow_confirm_request.is_none(),
        "Escape answered the ask"
    );
    assert!(
        rt.state.workflow_operation_error.is_none(),
        "Escape ran no delete"
    );
    assert!(
        rt.find("workflow-audit").is_some(),
        "the workflow stays"
    );
}
