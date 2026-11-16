use super::*;
use crate::constants::{QUERY_EDITOR_HEIGHT_DEFAULT, QUERY_EDITOR_HEIGHT_KEY};

#[test]
fn a3_query_editor_height_split_persists_via_layout_setting() {
    let mut state = AppState::default();
    assert_eq!(state.layout_size(QUERY_EDITOR_HEIGHT_KEY), None);
    state
        .set_layout_size(QUERY_EDITOR_HEIGHT_KEY, 360)
        .expect("layout setting");
    assert_eq!(state.layout_size(QUERY_EDITOR_HEIGHT_KEY), Some(360));

    let settings = state.ui_settings();
    let hit = settings.iter().any(|s| {
        matches!(
            s,
            Setting::Layout { key, value } if key == QUERY_EDITOR_HEIGHT_KEY && *value == 360
        )
    });
    assert!(hit, "query editor height is stored as Setting::Layout");
    assert_eq!(QUERY_EDITOR_HEIGHT_DEFAULT, 320);
}

#[test]
fn a3_multi_result_tabs_switch_and_pin_across_runs() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_query_tab("Query").unwrap();
    runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");

    runtime.state.run_query();
    assert_eq!(runtime.state.query_result_tabs.len(), 1);
    let first = runtime.state.query_result_tabs[0].id;
    assert_eq!(runtime.state.selected_query_result, Some(first));
    assert!(runtime.state.result_grid.is_some(), "first run produced a result");
    assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 0), "1");

    // Second run creates another result tab and selects it.
    runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
    runtime.state.run_query();
    assert_eq!(runtime.state.query_result_tabs.len(), 2);
    let second = runtime.state.query_result_tabs[1].id;
    assert_ne!(first, second);
    assert_eq!(runtime.state.selected_query_result, Some(second));

    // Switch back to the first result tab and keep its grid content.
    assert!(runtime.state.select_query_result_tab(first));
    assert_eq!(runtime.state.selected_query_result, Some(first));
    assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 0), "1");

    // Pin the first tab; it remains present after a new run.
    runtime.state.toggle_pin_query_result_tab(first);
    assert!(runtime
        .state
        .query_result_tabs
        .iter()
        .any(|t| t.id == first && t.pinned));

    runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
    runtime.state.run_query();
    assert!(runtime
        .state
        .query_result_tabs
        .iter()
        .any(|t| t.id == first && t.pinned), "pinned tab persists across runs");
    assert!(runtime.state.query_result_tabs.len() >= 3);
}

#[test]
fn a3_query_workspace_renders_toolbar_sidebar_and_result_tabs() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_query_tab("Query").unwrap();
    runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
    runtime.state.run_query();
    runtime.settle_all();
    runtime.frame(0.0);

    assert!(runtime.find("query-toolbar").is_some(), "toolbar rendered");
    assert!(runtime.find("sql-run").is_some());
    assert!(runtime.find("sql-format").is_some());
    assert!(runtime.find("sql-explain").is_some());
    assert!(runtime.find("query-sidebar").is_some(), "favorites/history/snippets sidebar rendered");
    assert!(runtime.find("query-result-tabs").is_some(), "result tabs bar rendered");
    assert!(runtime.find("query-result-tab-1").is_some(), "first result tab rendered");
}