use super::*;
#[allow(unused_imports)]
use crate::{
    query_selected, sql_editor, type_, view, AppState, Section, Setting, APP_FONTS,
    APP_FONT_FAMILIES, NAVIGATOR_WIDTH_DEFAULT, NAVIGATOR_WIDTH_KEY, NAVIGATOR_WIDTH_MAX,
    NAVIGATOR_WIDTH_MIN, RAIL_WIDTH_COLLAPSED, RAIL_WIDTH_EXPANDED, SIDEBAR_RAIL_WIDTH_KEY,
    TABLE_PAGE_SIZE,
};
#[allow(unused_imports)]
use rune::{Env, Runtime};

    #[test]
    fn a_workflow_run_lands_through_the_job_channel() {
        use crate::app_poll;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-workflows").expect("workflows navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        // A workflow to run: the workflow form saves through the memory port
        // (the JobAdapter answers no sync workflows).
        runtime.state.workflow_editing = Some(("engine run".into(), vec!["SELECT 1".into()]));
        runtime.settle_all();
        runtime.frame(0.0);

        let (x, y) = runtime.find("workflow-run").expect("the run button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime.state.workflow_loading.is_some(),
            "the engine job is in flight"
        );
        assert!(runtime.state.workflow_results.is_empty(), "no results yet");

        runtime.settle_all();
        runtime.frame(0.0);
        runtime.frame(0.0);
        assert!(runtime.state.workflow_loading.is_none(), "the delivery landed");
        assert_eq!(runtime.state.workflow_results.len(), 1, "the step report is up");
        assert_eq!(runtime.state.workflow_results[0].step, "SELECT 1");
        assert!(runtime.state.workflow_results[0].ok);
    }

    #[test]
    fn the_sync_wizard_refuses_the_same_endpoint_twice() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-tools").expect("tools navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("tools-page").is_some());
        assert!(runtime.find("sync-tool").is_some());
        assert!(runtime.find("tool-compare").is_some());
        assert!(runtime.find("tool-transfer").is_some());

        // Cycle the target onto the source: both ends local.
        let (x, y) = runtime.find("sync-target").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_target, "local", "one connection away");

        let (x, y) = runtime.find("sync-endpoints-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_step, 0, "the same end twice does not advance");
        assert!(runtime.find("sync-error").is_some(), "the refusal is visible");

        // Back to distinct endpoints: the step advances.
        let (x, y) = runtime.find("sync-target").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_target, "replica");
        let (x, y) = runtime.find("sync-endpoints-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_step, 1);
        assert!(runtime.find("sync-error").is_none());
    }

    #[test]
    fn the_sync_table_step_selects_all_and_clears() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        runtime.state.sync_next_endpoints();
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("sync-step-2").is_some());
        assert!(runtime.state.sync_tables.is_empty());

        let (x, y) = runtime.find("sync-select-all").expect("select all");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_tables.len(), 3, "the local source sees three tables");

        let (x, y) = runtime.find("sync-clear").expect("clear");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.sync_tables.is_empty(), "clear empties the selection");

        // A single chip toggles one table; Next needs at least one.
        let (x, y) = runtime.find("sync-table-customers").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.sync_tables.contains("customers"));
        let (x, y) = runtime.find("sync-tables-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_step, 2);

        runtime.state.sync_clear_tables();
        runtime.state.sync_step = 1;
        runtime.state.sync_next_tables();
        assert_eq!(runtime.state.sync_step, 1, "no selection, no next step");
        assert!(runtime.state.sync_error.is_some());
    }

    #[test]
    fn the_sync_options_default_and_round_trip() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.sync_next_endpoints();
        runtime.state.sync_select_all();
        runtime.state.sync_next_tables();
        runtime.settle_all();
        runtime.frame(0.0);

        assert!(runtime.find("sync-step-3").is_some());
        assert_eq!(
            runtime.state.sync_options,
            [false, false, true],
            "defaults: no create, no drop, copy data on"
        );

        let (x, y) = runtime.find("sync-option-0").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_options, [true, false, true]);

        let (x, y) = runtime.find("sync-option-2").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_options, [true, false, false]);

        // Toggling back restores the defaults.
        let (x, y) = runtime.find("sync-option-0").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let (x, y) = runtime.find("sync-option-2").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_options, [false, false, true], "the round trip restores the defaults");
    }

    #[test]
    fn the_sync_preview_is_a_read_only_grid() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.sync_next_endpoints();
        runtime.state.sync_tables.insert("customers".into());
        runtime.state.sync_next_tables();
        runtime.settle_all();
        runtime.frame(1.0);

        let (x, y) = runtime.find("sync-preview-next").expect("preview button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.settle_all();
        runtime.frame(0.0);

        let grid = runtime.state.sync_preview_grid.as_mut().expect("the preview grid");
        assert_eq!(grid.cell(0, 1), "customer-0");
        assert!(!grid.is_editable(), "the preview page cannot be edited");
        grid.begin_edit();
        assert!(!grid.is_editing(), "the editor refuses the preview");
        assert!(runtime.find("result-grid").is_some(), "drawn through the B8 sheet");
        assert!(runtime.find("result-cell-0-0").is_some());
    }

    #[test]
    fn the_sync_run_needs_tables_and_finishes_through_snapshots() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.sync_next_endpoints();
        runtime.state.sync_tables.insert("customers".into());
        runtime.state.sync_next_tables();
        runtime.state.sync_show_preview();
        runtime.settle_all();
        runtime.frame(1.0);

        // No selection, no run: clear the tables behind the wizard's back.
        runtime.state.sync_tables.clear();
        let (x, y) = runtime.find("sync-run").expect("run button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.sync_job.is_none(), "no tables selected, no job");
        assert!(runtime.find("sync-error").is_some());

        runtime.state.sync_tables.insert("customers".into());
        let (x, y) = runtime.find("sync-run").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.state.sync_job.is_some());
        assert_eq!(runtime.state.sync_snapshot.as_ref().unwrap().status, "running");

        let (x, y) = runtime.find("sync-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let (x, y) = runtime.find("sync-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.sync_snapshot.as_ref().unwrap().status,
            "completed",
            "the sync job finishes through snapshots"
        );
        assert!(runtime.find("sync-progress").is_some());
    }

    #[test]
    fn the_compare_step_marks_same_name_objects() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("tool-compare").expect("compare tab");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("compare-tool").is_some());
        assert!(runtime.find("compare-object-customers").is_some());
        assert!(runtime.find("compare-match-customers").is_some(), "same name on both ends");
        assert!(runtime.find("compare-match-orders").is_some());
        assert!(runtime.find("compare-missing-catalog").is_some(), "the replica lacks analytics");
    }

    #[test]
    fn an_empty_compare_plan_has_its_own_empty_state() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("tool-compare").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        assert!(runtime.find("compare-plan").is_some(), "local vs replica has a plan");
        assert!(runtime.find("compare-plan-empty").is_none());

        // The same endpoint twice: the plan is empty.
        let (x, y) = runtime.find("compare-target").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.compare_target, "local");
        assert!(runtime.find("compare-plan").is_none(), "an empty plan draws no lines");
        assert!(runtime.find("compare-plan-empty").is_some(), "the empty state shows");
    }

    #[test]
    fn a_failed_plan_apply_keeps_the_plan_visible() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("tool-compare").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("compare-plan").is_some());

        let (x, y) = runtime.find("compare-apply").expect("apply button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.state.compare_job.is_some());
        assert_eq!(runtime.state.compare_snapshot.as_ref().unwrap().status, "running");

        let (x, y) = runtime.find("compare-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        runtime.frame(1.0);
        let (x, y) = runtime.find("compare-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        let snapshot = runtime.state.compare_snapshot.clone().unwrap();
        assert_eq!(snapshot.status, "failed");
        assert_eq!(
            snapshot.log.last().map(String::as_str),
            Some("the replica cannot create analytics tables")
        );
        assert!(
            runtime.find("compare-plan").is_some(),
            "the failed apply did not clear the plan"
        );
        assert!(runtime.find("compare-progress").is_some());

        // An empty plan (same endpoints) refuses with an error and no job.
        let (x, y) = runtime.find("compare-target").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let (x, y) = runtime.find("compare-apply").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.compare_job.is_none());
        assert!(runtime.find("compare-error").is_some(), "nothing to apply is visible");
    }

    #[test]
    fn a_column_already_in_a_pair_cannot_pair_again() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("tool-transfer").expect("transfer tab");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("transfer-tool").is_some());

        // Pair id -> id.
        let (x, y) = runtime.find("transfer-pair").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("transfer-pair-row-0").is_some());
        assert!(!runtime.state.transfer_unmapped(true).contains(&"id".to_owned()));

        // A duplicate name cannot pair again: id is already used on both ends.
        let (x, y) = runtime.find("transfer-pair").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("transfer-pair-row-1").is_none(), "the duplicate pairing was refused");
        assert!(runtime.find("transfer-error").is_some(), "the refusal is visible");

        // A fresh pair still works and both unmapped lists shrink.
        runtime.state.transfer_pair_left = "name".into();
        runtime.state.transfer_pair_right = "name".into();
        let (x, y) = runtime.find("transfer-pair").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("transfer-pair-row-1").is_some());
        assert!(!runtime.state.transfer_unmapped(true).contains(&"name".to_owned()));
        assert!(runtime.find("transfer-unmapped-left").is_some());
        assert!(runtime.find("transfer-unmapped-right").is_some());
    }

    #[test]
    fn cancelling_the_transfer_confirmation_leaves_no_job() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("tool-transfer").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        // No pairs: the confirmation refuses with no job.
        let (x, y) = runtime.find("transfer-start").expect("transfer button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("transfer-confirm").is_some());
        let (x, y) = runtime.find("transfer-yes").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.transfer_job.is_none(), "no pairs, no job");
        assert!(runtime.find("transfer-error").is_some());

        // With a pair: cancelling the confirmation leaves no job.
        runtime.frame(1.0);
        runtime.state.pair_transfer_columns();
        let (x, y) = runtime.find("transfer-start").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("transfer-no").expect("cancel button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.transfer_job.is_none(), "a cancelled confirmation gets no job");
        assert!(runtime.find("transfer-job").is_none());

        // Confirming hands out a job and the progress follows snapshots.
        runtime.frame(1.0);
        let (x, y) = runtime.find("transfer-start").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("transfer-yes").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.transfer_job.is_some(), "the port handed out a job id");
        assert_eq!(runtime.state.transfer_snapshot.as_ref().unwrap().status, "running");
        runtime.frame(1.0);
        let (x, y) = runtime.find("transfer-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let (x, y) = runtime.find("transfer-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.transfer_snapshot.as_ref().unwrap().status, "completed");
        assert!(runtime.find("transfer-progress").is_some());
    }

    #[test]
    fn an_empty_dashboard_has_its_own_empty_state() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-dashboards").expect("dashboards navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("dashboards-page").is_some());
        assert!(runtime.find("dashboard-empty").is_some(), "the empty list has its own state");
        assert!(runtime.find("widget-1").is_none());
    }

    #[test]
    fn a_bar_chart_shows_the_zero_and_the_maximum() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.frame(0.0);
        runtime
            .state
            .workspace
            .save_dashboard_widget("Weekly", "select 1")
            .unwrap();
        let (x, y) = runtime.find("rail-dashboards").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.settle_all();
        runtime.frame(1.0);

        assert!(runtime.find("widget-1").is_some());
        assert!(runtime.find("widget-chart-1").is_some());
        let zero = runtime.presented("widget-block-1-mon").expect("the zero bar is drawn");
        let max = runtime.presented("widget-block-1-thu").expect("the maximum bar is drawn");
        assert!(zero.0[3] >= 1.0, "the zero bar keeps a sliver: {:?}", zero);
        assert!(max.0[3] >= 80.0, "the maximum bar is tall: {:?}", max);
        assert!(max.0[3] > zero.0[3] + 60.0, "the chart is proportional");
    }

    #[test]
    fn a_widget_without_a_title_cannot_be_saved() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("rail-dashboards").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("widget-form").is_some());

        let (x, y) = runtime.find("widget-save").expect("save button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("dashboard-empty").is_some(), "nothing was saved");
        assert!(runtime.find("widget-form-error").is_some(), "the refusal is visible");

        runtime.state.widget_title = rune::TextEdit::new("Weekly");
        runtime.state.widget_query = rune::TextEdit::new("select 1");
        let (x, y) = runtime.find("widget-save").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("widget-1").is_some(), "the widget joins the dashboard");
        assert!(runtime.find("widget-chart-1").is_some(), "its chart renders");
        assert!(runtime.find("widget-form-error").is_none());
    }

    /// N13: an adapter whose registry and dashboard answer, recording what
    /// the forms save. Its lists win over the memory port's demo lists.


    #[test]
    fn a_host_registry_and_dashboard_replace_the_demo_lists() {
        let mut runtime = runtime();
        runtime.state.desktop = Box::new(AdapterServingWorkflows::default());
        runtime.frame(0.0);

        // The workflows page lists the host's workflow, not the port's demo.
        let (x, y) = runtime.find("rail-workflows").expect("workflows navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(
            runtime.find("workflow-host-etl").is_some(),
            "the host's workflow is listed"
        );
        assert!(
            runtime.find("workflow-audit").is_none(),
            "the memory port's demo list stepped back"
        );

        // The editor opens the host's steps.
        let (x, y) = runtime.find("workflow-host-etl").expect("the host's row");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("workflow-form").is_some(), "the editor opened");
        assert_eq!(
            runtime.state.workflow_editing.as_ref().unwrap().1,
            vec!["SELECT count(*) FROM users".to_owned()],
            "the steps are the host's"
        );

        // The dashboards page shows the host's widget with its own bars.
        let (x, y) = runtime.find("rail-dashboards").expect("dashboards navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("widget-7").is_some(), "the host's widget is on the page");
        assert!(runtime.find("widget-bar-7-one").is_some(), "its chart reads the host's bars");
        assert!(runtime.find("dashboard-empty").is_none(), "the empty state stepped back");
    }
