//! T624 audit for the pure table action decisions: the exact 40-binding set
//! and store delegation, the navigation guard, notice invalidation, count
//! strategy and the load/load-exact call shapes.

use crate::views::use_table_actions as actions;
use crate::views::use_table_actions::{
    exact_reload_options, load_call, may_navigate, notice_invalidating, plan_action, plan_all,
    resolve_count_strategy, resolve_panel_count_strategy, CountMode, CountStrategy, EditorSnapshot,
    TableAction, TableLoadContext,
};

// ── the binding table ─────────────────────────────────────────────────────

#[test]
fn the_action_set_is_exactly_the_react_bindings() {
    assert_eq!(TableAction::ALL.len(), 40);
    let mut distinct = TableAction::ALL.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 40, "every binding must be distinct");
    // In declaration order the delegated store methods must match the hook.
    let names: Vec<&str> = TableAction::ALL.iter().map(|action| action.store_method()).collect();
    assert_eq!(
        names,
        vec![
            "loadTableData", "reloadPanel", "reloadPanel", "setDefaultOrderOverride", "setSort",
            "addSort", "removeSort", "clearSorts", "setColumnOrder", "setFrozenColumnCount",
            "removeFilter", "clearFilters", "addFilter", "setFilters", "updateFilter",
            "setFilterLogic", "applyFilters", "setFilterMode", "setAnyColumnDraft",
            "setRawWhereDraft", "applyAnyColumn", "applyRawWhere", "setFilterPanelOpen",
            "setVisibleColumns", "setPage", "setPageSize", "startEdit", "stageCellChange",
            "stagePasteWrites", "addDraftRowsFromCopy", "cancelEdit", "setRowModeSelection",
            "toggleSelectAll", "setGridSelection", "deleteRows", "unstageRowDelete",
            "previewPendingChanges", "commitPendingChanges", "rollbackPendingChanges",
            "setDetailRow",
        ]
    );
    assert_eq!(TableAction::SelectRow.store_method(), "setRowModeSelection");
    assert_eq!(TableAction::Load.store_method(), "loadTableData");
}

#[test]
fn the_page_moving_and_notice_sets_match_react_exactly() {
    let page_moving: Vec<TableAction> = TableAction::ALL
        .iter()
        .copied()
        .filter(|action| action.is_page_moving())
        .collect();
    let expected = [
        TableAction::Load,
        TableAction::Reload,
        TableAction::CountExact,
        TableAction::SetDefaultOrderOverride,
        TableAction::SetSort,
        TableAction::AddSort,
        TableAction::RemoveSort,
        TableAction::ClearSorts,
        TableAction::ClearFilters,
        TableAction::SetFilters,
        TableAction::ApplyFilters,
        TableAction::ApplyAnyColumn,
        TableAction::ApplyRawWhere,
        TableAction::SetPage,
        TableAction::SetPageSize,
    ];
    assert_eq!(page_moving, expected);

    let mut invalidating = notice_invalidating();
    invalidating.sort_unstable();
    let mut expected_invalidating = expected.to_vec();
    // rollback drops the notice but is NOT page-moving (no navigation guard).
    expected_invalidating.push(TableAction::RollbackPendingChanges);
    expected_invalidating.sort_unstable();
    assert_eq!(invalidating, expected_invalidating);
    assert_eq!(invalidating.len(), 16);
}

// ── navigation guard ──────────────────────────────────────────────────────

#[test]
fn navigation_guard_depends_on_the_panels_open_editor() {
    assert!(may_navigate(None, "p1"), "no editor never blocks");
    let other = EditorSnapshot {
        panel_id: "p2".to_owned(),
        open: true,
        commit_reason: Some("bad draft".to_owned()),
    };
    assert!(may_navigate(Some(&other), "p1"), "another panel's editor is irrelevant");
    let closed = EditorSnapshot { panel_id: "p1".to_owned(), open: false, commit_reason: None };
    assert!(may_navigate(Some(&closed), "p1"));
    let committed = EditorSnapshot { panel_id: "p1".to_owned(), open: true, commit_reason: None };
    assert!(may_navigate(Some(&committed), "p1"));
    let refused = EditorSnapshot {
        panel_id: "p1".to_owned(),
        open: true,
        commit_reason: Some("bad draft".to_owned()),
    };
    assert!(!may_navigate(Some(&refused), "p1"));
}

#[test]
fn a_refused_guard_stops_every_page_moving_binding() {
    for plan in plan_all(false) {
        if plan.action.is_page_moving() {
            assert!(!plan.proceed, "{} must stop", plan.action.store_method());
            assert!(!plan.invalidate_notice, "a refused navigation keeps the notice");
            assert!(!plan.exact_reload);
        } else {
            assert!(plan.proceed, "{} is unguarded", plan.action.store_method());
        }
    }
    let rollback = plan_action(TableAction::RollbackPendingChanges, false);
    assert!(rollback.proceed && rollback.invalidate_notice, "rollback has no guard");
    let render_only = plan_action(TableAction::SetFilterMode, false);
    assert!(render_only.proceed && !render_only.invalidate_notice);
    let unstage = plan_action(TableAction::UnstageRowDelete, false);
    assert!(unstage.proceed && !unstage.invalidate_notice);
}

#[test]
fn an_allowed_guard_invalidates_the_notice_only_for_the_expected_bindings() {
    let plans = plan_all(true);
    assert_eq!(plans.len(), 40);
    for plan in &plans {
        assert!(plan.proceed);
        assert_eq!(plan.invalidate_notice, plan.action.is_page_moving()
            || plan.action == TableAction::RollbackPendingChanges);
    }
    let exact = plan_action(TableAction::CountExact, true);
    assert!(exact.exact_reload && exact.invalidate_notice);
    let reload = plan_action(TableAction::Reload, true);
    assert!(reload.invalidate_notice && !reload.exact_reload);
}

// ── count strategy ────────────────────────────────────────────────────────

#[test]
fn count_strategy_maps_every_mode() {
    assert_eq!(resolve_count_strategy(CountMode::None, 1.0, Some(1.0)), Some(CountStrategy::Unsupported));
    assert_eq!(resolve_count_strategy(CountMode::Exact, 1.0, None), Some(CountStrategy::Exact));
    assert_eq!(resolve_count_strategy(CountMode::Estimated, 1.0, None), Some(CountStrategy::Estimated));
}

#[test]
fn auto_count_strategy_weighs_the_known_prior_and_threshold() {
    assert_eq!(resolve_count_strategy(CountMode::Auto, 200_000.0, None), None);
    assert_eq!(resolve_count_strategy(CountMode::Auto, 200_000.0, Some(f64::NAN)), None);
    assert_eq!(resolve_count_strategy(CountMode::Auto, 200_000.0, Some(f64::INFINITY)), None);
    assert_eq!(resolve_count_strategy(CountMode::Auto, 200_000.0, Some(200_001.0)), Some(CountStrategy::Estimated));
    assert_eq!(resolve_count_strategy(CountMode::Auto, 200_000.0, Some(200_000.0)), Some(CountStrategy::Exact));
    assert_eq!(resolve_count_strategy(CountMode::Auto, 200_000.0, Some(1.0)), Some(CountStrategy::Exact));
    // A dirty threshold never makes the grid lie with an estimate.
    assert_eq!(resolve_count_strategy(CountMode::Auto, 0.0, Some(1.0)), Some(CountStrategy::Exact));
    assert_eq!(resolve_count_strategy(CountMode::Auto, -1.0, Some(1.0)), Some(CountStrategy::Exact));
    assert_eq!(resolve_count_strategy(CountMode::Auto, f64::NAN, Some(1.0)), Some(CountStrategy::Exact));
}

#[test]
fn panel_count_strategy_uses_the_settings_defaults() {
    assert_eq!(resolve_panel_count_strategy(CountMode::Auto, None, None), None);
    assert_eq!(
        resolve_panel_count_strategy(CountMode::Auto, None, Some(200_001.0)),
        Some(CountStrategy::Estimated)
    );
    assert_eq!(
        resolve_panel_count_strategy(CountMode::Auto, None, Some(200_000.0)),
        Some(CountStrategy::Exact)
    );
    assert_eq!(
        resolve_panel_count_strategy(CountMode::None, Some(9.0), None),
        Some(CountStrategy::Unsupported)
    );
}

// ── call shapes ───────────────────────────────────────────────────────────

#[test]
fn load_passes_the_panel_context_and_resolved_strategy() {
    let context = TableLoadContext {
        connection_id: None,
        db_session_id: "c1".to_owned(),
        driver_type: "postgresql".to_owned(),
        database: Some("app".to_owned()),
        schema: None,
        table: "users".to_owned(),
    };
    let call = load_call("panel-users", &context, Some(300_000.0));
    assert_eq!(call.panel_id, "panel-users");
    assert_eq!(call.context, context);
    assert_eq!(call.count_strategy, Some(CountStrategy::Estimated));
    let unknown = load_call("panel-users", &context, None);
    assert_eq!(unknown.count_strategy, None);
}

#[test]
fn count_exact_is_a_full_reread_not_a_paged_one() {
    let options = exact_reload_options();
    assert!(!options.skip_count);
    assert_eq!(options.count_strategy, CountStrategy::Exact);
}
