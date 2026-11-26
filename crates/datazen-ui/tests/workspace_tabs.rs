mod common;
use common::*;
use datazen_ui::test_support::*;
#[allow(unused_imports)]
use datazen_ui::{
    query_selected, sql_editor, type_, view, AppState, Section, Setting, APP_FONTS,
    APP_FONT_FAMILIES, NAVIGATOR_WIDTH_DEFAULT, NAVIGATOR_WIDTH_KEY, NAVIGATOR_WIDTH_MAX,
    NAVIGATOR_WIDTH_MIN, RAIL_WIDTH_COLLAPSED, RAIL_WIDTH_EXPANDED, SIDEBAR_RAIL_WIDTH_KEY,
    TABLE_PAGE_SIZE,
};
#[allow(unused_imports)]
use rune::{Env, Runtime};

#[test]
fn the_widget_and_workflow_forms_save_through_the_host() {
    let mut adapter = AdapterServingWorkflows::default();
    let saved_widget = std::rc::Rc::clone(&adapter.saved_widget);
    let saved_workflow = std::rc::Rc::clone(&adapter.saved_workflow);
    let ran = std::rc::Rc::clone(&adapter.ran);
    let mut runtime = runtime();
    runtime.state.desktop = Box::new(adapter);
    runtime.frame(0.0);

    // The widget form's save lands in the host's dashboard use case and
    // the form clears.
    let (x, y) = runtime
        .find("rail-dashboards")
        .expect("dashboards navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    runtime.state.widget_title = rune::TextEdit::new("Weekly");
    runtime.state.widget_query = rune::TextEdit::new("select label, n from facts");
    let (x, y) = runtime.find("widget-save").expect("the save button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert_eq!(
        *saved_widget.borrow(),
        Some(("Weekly".to_owned(), "select label, n from facts".to_owned())),
        "the host's dashboard received the widget"
    );
    assert!(
        runtime.state.widget_title.text.is_empty(),
        "the form cleared after the host's save"
    );

    // The workflow page's edit pane saves through the host's registry, and
    // the run panel runs through the host's engine (T145's page port — the
    // lite inline editor's save/run pair it replaces).
    let (x, y) = runtime
        .find("rail-workflows")
        .expect("workflows navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let (x, y) = runtime
        .find("workflow-host-etl-edit")
        .expect("the row's edit affordance");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(
        runtime.find("workflow-form-root").is_some(),
        "the visual form mounts in the edit pane"
    );
    let (x, y) = runtime
        .find("workflow-yaml-save")
        .expect("the shared action bar's save button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert_eq!(
        *saved_workflow.borrow(),
        Some((
            "host-etl".to_owned(),
            vec!["SELECT count(*) FROM users".to_owned()]
        )),
        "the host's registry received the workflow"
    );
    assert!(
        runtime.state.workflow_panels.is_empty(),
        "the edit panel closed after the save"
    );

    let (x, y) = runtime
        .find("workflow-host-etl")
        .expect("the row is still there");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let (x, y) = runtime
        .find("workflow-execute-button")
        .expect("the run panel's execute button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert_eq!(
        *ran.borrow(),
        Some("host-etl".to_owned()),
        "the host ran the workflow"
    );
    assert_eq!(
        runtime.state.workflow_results,
        vec![datazen_ui_api::WorkflowStepResult {
            step: "SELECT count(*) FROM users".to_owned(),
            ok: true,
            summary: "ok: 1 rows".to_owned(),
        }],
        "the results are the host's"
    );
}

#[test]
fn the_workflow_list_is_sorted_by_name() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime
        .find("rail-workflows")
        .expect("workflows navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    // T145: the page port's root (the lite page's `workflows-page` went with
    // it); the list still keys the rows `workflow-{name}`.
    assert!(runtime.find("workflow-workspace").is_some());
    assert!(runtime.find("workflow-list").is_some());
    let audit = runtime
        .presented("workflow-audit")
        .expect("audit is listed")
        .0;
    let nightly = runtime
        .presented("workflow-nightly-backup")
        .expect("nightly-backup is listed")
        .0;
    let weekly = runtime
        .presented("workflow-weekly-report")
        .expect("weekly-report is listed")
        .0;
    assert!(
        audit[1] < nightly[1] && nightly[1] < weekly[1],
        "the list reads alphabetically: audit {audit:?} nightly {nightly:?} weekly {weekly:?}"
    );
}

#[test]
fn the_workflow_yaml_edits_and_undoes_through_the_document_model() {
    use datazen_ui::app_key;
    use rune::{Key, KeyEvent, Mods};

    let mut runtime = runtime();
    runtime.on_key = Some(app_key);
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-workflows").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);

    // T145: the edit pane's yaml mode carries the document (the lite page's
    // inline yaml surface it replaces) — the row's edit affordance opens the
    // pane, the mode tab writes the document over the loaded draft.
    let (x, y) = runtime.find("workflow-audit-edit").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(
        runtime.find("workflow-edit-panel").is_some(),
        "the edit pane"
    );
    assert!(
        runtime.find("workflow-yaml-editor").is_none(),
        "the visual mode mounts no yaml surface yet"
    );
    let (x, y) = runtime.find("workflow-mode-yaml").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);

    assert!(
        runtime.find("workflow-yaml-editor").is_some(),
        "the YAML surface is the editor model"
    );
    assert!(
        runtime.find("yaml-editor").is_none(),
        "the lite editor's old key is replaced by the real editor surface"
    );
    let doc = runtime.state.workflow_yaml.doc().to_owned();
    assert!(doc.starts_with("name: audit\nsteps:\n"), "{doc}");
    assert!(doc.contains("  - compare structures\n"));
    // The plain panel shares the editor's line renderer (sql-line-* keys),
    // just without the SQL spans.
    assert!(runtime.find("sql-line-2").is_some(), "plain lines render");

    runtime.state.workflow_yaml.focus();
    runtime.key(KeyEvent {
        key: Key::Text("X".into()),
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert!(
        runtime.state.workflow_yaml.doc().starts_with('X'),
        "typing goes in"
    );
    assert_ne!(runtime.state.workflow_yaml.doc(), doc);

    runtime.key(KeyEvent {
        key: Key::Command('z'),
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert_eq!(
        runtime.state.workflow_yaml.doc(),
        doc,
        "Command+Z still undoes on the YAML document"
    );
}

#[test]
fn workflow_run_results_separate_failures_from_successes() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-workflows").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    // T145: the row's press opens the run panel (React's openRunPanel), and
    // the variables row's Execute runs it.
    let (x, y) = runtime.find("workflow-audit").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(
        runtime.find("workflow-tab-wf-1").is_some(),
        "the run panel tab"
    );

    let (x, y) = runtime.find("workflow-execute-button").expect("run button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    // The run status row: the failed run's ✗ mark (0ms — the lite report
    // carries no timing).
    assert!(runtime.find("workflow-run-status").is_some());
    let status = runtime
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("workflow-run-status"))
        .expect("the run status line");
    assert!(status.text.as_deref().unwrap_or_default().starts_with('✗'));

    // The steps bar (desc order — the settings default): the applying step
    // is first; the panel's own rows carry the ok/fail split below.
    assert!(runtime.find("workflow-step-apply plan").is_some());
    assert!(runtime.find("workflow-step-compare structures").is_some());
    assert!(runtime.find("workflow-execution-result").is_some());
    assert!(runtime
        .find("workflow-execution-result-step-0-row")
        .is_some());
    assert!(runtime
        .find("workflow-execution-result-step-1-row")
        .is_some());
    let results = &runtime.state.workflow_results;
    assert_eq!(results[0].summary, "ok: 12 rows");
    assert_eq!(results[1].summary, "the replica refused the plan");
}

#[test]
fn tab_strip_selects_and_closes_table_tabs() {
    let mut runtime = runtime();
    let first = runtime.state.open_table_tab("customers").unwrap();
    let second = runtime.state.open_table_tab("orders").unwrap();
    runtime.frame(0.0);

    let (x, y) = runtime.find(&format!("tab-{first}")).expect("first tab");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert_eq!(runtime.state.tabs.selected().unwrap().id, first);

    let (x, y) = runtime
        .find(&format!("tab-close-{second}"))
        .expect("close second");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(runtime.state.tabs.tabs().iter().all(|tab| tab.id != second));
    assert_eq!(runtime.state.tabs.selected().unwrap().id, first);
}

#[test]
fn clicking_a_session_table_opens_a_table_tab() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    assert!(
        runtime.find("object-tree").is_none(),
        "object tree is shown only when a session/catalog is open"
    );

    let (x, y) = runtime.find("connection-local").expect("connection");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(
        runtime.find("object-tree").is_some(),
        "object tree appears after connecting"
    );

    let (x, y) = runtime
        .find("tree-table-customers")
        .expect("table in the tree");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert_eq!(runtime.state.tabs.tabs().len(), 1);
    assert_eq!(runtime.state.tabs.selected().unwrap().title, "customers");
    assert_eq!(
        runtime.state.tabs.selected().unwrap().kind,
        datazen_ui_api::TabKind::Table
    );
}

#[test]
fn the_open_table_tab_is_the_table_in_the_grid() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("connection-local").expect("connection");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);

    let (x, y) = runtime.find("tree-table-orders").expect("orders");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    let orders = runtime.state.tabs.selected().unwrap().id;
    assert_eq!(runtime.state.grid.cell(0, 1), "order-0");

    let (x, y) = runtime.find("tree-table-customers").expect("customers");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    let customers = runtime.state.tabs.selected().unwrap().id;
    assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");

    runtime.frame(1.0);
    let (x, y) = runtime.find(&format!("tab-{orders}")).expect("orders tab");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert_eq!(runtime.state.tabs.selected().unwrap().id, orders);
    assert_eq!(runtime.state.grid.cell(0, 1), "order-0");
    assert_ne!(customers, orders);
}

#[test]
fn closing_the_selected_table_tab_follows_the_remaining_table() {
    use datazen_ui_api::WorkspaceApi;
    let mut runtime = runtime();
    runtime.state.workspace.connect("local").unwrap();
    let orders = runtime.state.open_table_tab("orders").unwrap();
    let customers = runtime.state.open_table_tab("customers").unwrap();
    assert!(runtime.state.close_tab(customers));
    assert_eq!(runtime.state.tabs.selected().unwrap().id, orders);
    assert_eq!(runtime.state.grid.cell(0, 1), "order-0");
}

#[test]
fn a_padded_table_title_loads_the_trimmed_table() {
    use datazen_ui_api::WorkspaceApi;
    let mut runtime = runtime();
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_table_tab("customers").unwrap();
    runtime.state.open_table_tab(" orders ").unwrap();
    assert_eq!(runtime.state.tabs.selected().unwrap().title, "orders");
    assert_eq!(runtime.state.grid.cell(0, 1), "order-0");
}

#[test]
fn a_query_tab_keeps_the_editor_above_an_empty_result() {
    use datazen_ui::app_key;
    use rune::{Key, KeyEvent, Mods};

    let mut runtime = runtime();
    runtime.on_key = Some(app_key);
    runtime.frame(0.0);
    let (x, y) = runtime.find("open-query").expect("new query");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);

    assert!(runtime.find("query-tab").is_some());
    assert!(runtime.find("sql-editor-field").is_some());
    assert!(runtime.find("query-results").is_some());
    assert_eq!(
        runtime.state.tabs.selected().unwrap().kind,
        datazen_ui_api::TabKind::Query
    );

    runtime.state.sql.focus();
    runtime.key(KeyEvent {
        key: Key::Text("select 1".into()),
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert!(
        runtime.state.sql.doc().starts_with("select 1"),
        "{}",
        runtime.state.sql.doc()
    );
    assert!(runtime.find("query-results").is_some());
}

#[test]
fn a_query_tab_stays_in_the_workspace_and_off_the_connections_page() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("open-query").expect("new query");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(runtime.find("sql-editor-field").is_some());
    assert!(runtime.find("tab-strip").is_some());

    let (x, y) = runtime
        .find("rail-connections")
        .expect("connections navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    assert_eq!(runtime.state.active, Section::Connections);
    assert!(runtime.find("connection-local").is_some());
    assert!(
        runtime.find("sql-editor-field").is_none(),
        "the editor belongs to the workspace"
    );
    assert!(
        runtime.find("query-results").is_none(),
        "the results area belongs to the workspace"
    );
    assert!(
        runtime.find("tab-strip").is_none(),
        "tabs are a workspace control"
    );
    assert_eq!(
        runtime.state.tabs.tabs().len(),
        1,
        "the query tab itself is kept"
    );
}

#[test]
fn the_unwired_desktop_adapter_shows_its_fixed_error_and_keeps_the_demo_list() {
    let mut runtime = runtime();
    runtime.frame(0.0);

    let (x, y) = runtime
        .find("rail-connections")
        .expect("connections navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    // The adapter is not wired yet: its one fixed sentence is on the page.
    let message = runtime
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some("desktop-adapter-message"))
        .and_then(|el| el.text)
        .expect("the adapter's message is displayed");
    assert_eq!(message, datazen_ui_api::DESKTOP_NOT_IMPLEMENTED);

    // The memory port's demo list stays behind it.
    assert!(
        runtime.find("connection-local").is_some(),
        "the port's list is the fallback"
    );
    assert!(
        runtime.state.desktop_connections.is_none(),
        "no adapter list replaced the port's"
    );
    assert_eq!(
        runtime.state.desktop_error.as_deref(),
        Some(datazen_ui_api::DESKTOP_NOT_IMPLEMENTED)
    );
}

#[test]
fn the_desktop_adapter_message_leaves_the_connections_page_with_the_section() {
    let mut runtime = runtime();
    runtime.frame(0.0);

    let (x, y) = runtime
        .find("rail-connections")
        .expect("connections navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(runtime.find("desktop-adapter-message").is_some());

    let (x, y) = runtime.find("rail-settings").expect("settings navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("desktop-adapter-message").is_none(),
        "the message belongs to the connections page"
    );
}

#[test]
fn a_wired_adapter_replaces_the_demo_list_with_the_use_case_answer() {
    use datazen_ui_api::{ConnectionProfile, DemoAdapter};

    let mut runtime = runtime();
    // H3: the desktop process lists connections through the adapter; the
    // port's demo list is only the fallback.
    runtime.state.desktop = Box::new(DemoAdapter::with_connections(vec![ConnectionProfile {
        id: "warehouse".into(),
        name: "Warehouse".into(),
        engine: "postgres".into(),
        host: None,
    }]));
    runtime.frame(0.0);

    let (x, y) = runtime
        .find("rail-connections")
        .expect("connections navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    assert!(
        runtime.find("connection-warehouse").is_some(),
        "the use case's connection is on the page"
    );
    assert!(
        runtime.find("connection-local").is_none(),
        "the adapter's list replaced the port's demo list"
    );
    assert!(
        runtime.find("desktop-adapter-message").is_none(),
        "no fallback message: the adapter answered"
    );
    assert!(runtime.state.desktop_connections.is_some());
}

/// An adapter whose read use case serves one distinctive row, to see it
/// win over the port's page in the wired path.

#[test]
fn the_wired_adapter_serves_the_table_page_before_the_port() {
    let mut runtime = runtime();
    runtime.state.desktop = Box::new(AdapterServingOneRow);
    runtime.frame(0.0);

    let (x, y) = runtime.find("connection-local").expect("demo connection");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    assert!(runtime.state.grid.has_page(), "a page loaded");
    assert_eq!(
        runtime.state.grid.total_rows(),
        1,
        "the adapter's one-row page won, not the port's forty"
    );
    assert_eq!(runtime.state.active, Section::Workspace);
}

#[test]
fn a_read_that_fails_everywhere_keeps_the_previous_grid_page() {
    let mut runtime = runtime();
    runtime.frame(0.0);

    let (x, y) = runtime.find("connection-local").expect("demo connection");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(runtime.state.grid.has_page());
    let (page, total) = (runtime.state.grid.page(), runtime.state.grid.total_rows());
    assert_eq!(total, 40, "the port answered the first read");

    // A table no use case knows: the adapter fails (it is unwired in the
    // tests), the port fails, and the grid keeps the page it had.
    assert!(
        !runtime.state.show_table("ghost"),
        "no read use case knows this table"
    );
    runtime.frame(0.0);
    assert!(
        runtime.state.grid.has_page(),
        "the failed read did not clear the grid"
    );
    assert_eq!(runtime.state.grid.page(), page, "the page number stays");
    assert_eq!(
        runtime.state.grid.total_rows(),
        total,
        "the previous page stays behind the failure"
    );
}

/// An adapter that serves a writable page but rejects every commit with a
/// real error, to see the dirty marks survive the failure.

#[test]
fn a_failed_commit_keeps_the_dirty_marks() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.state.desktop = Box::new(AdapterFailingCommits);
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_table_tab("customers").unwrap();
    runtime.frame(0.0);

    // One edited cell: the adapter's page is writable, the edit stages.
    runtime.state.grid.select(0, 1);
    runtime.state.grid.begin_edit();
    runtime.state.grid.editor = rune::TextEdit::new(String::new());
    runtime.state.grid.editor.insert("renamed");
    runtime.state.commit_grid_edit();
    runtime.frame(0.0);
    assert_eq!(runtime.state.grid.cell(0, 1), "renamed");
    assert_eq!(runtime.state.grid.dirty_cells(), 1);

    // The commit fails with a real error: the dirty mark and the edit
    // stay exactly as they were.
    let (x, y) = runtime.find("commit-staged").expect("commit button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert_eq!(
        runtime.state.grid.dirty_cells(),
        1,
        "the failure keeps the dirty mark"
    );
    assert_eq!(
        runtime.state.grid.cell(0, 1),
        "renamed",
        "the edit is still staged in the grid"
    );
}

#[test]
fn a_wired_commit_lands_in_the_adapters_use_case_world() {
    use datazen_ui_api::{DemoAdapter, WorkspaceApi};

    let mut runtime = runtime();
    runtime.state.desktop = Box::new(DemoAdapter::default());
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_table_tab("customers").unwrap();
    runtime.frame(0.0);
    assert_eq!(
        runtime.state.grid.total_rows(),
        40,
        "the adapter's world serves the page"
    );

    runtime.state.grid.select(0, 1);
    runtime.state.grid.begin_edit();
    runtime.state.grid.editor = rune::TextEdit::new(String::new());
    runtime.state.grid.editor.insert("renamed");
    runtime.state.commit_grid_edit();
    runtime.frame(0.0);
    assert_eq!(runtime.state.grid.dirty_cells(), 1);

    let (x, y) = runtime.find("commit-staged").expect("commit button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert_eq!(
        runtime.state.grid.dirty_cells(),
        0,
        "the reload after the commit clears the marks"
    );
    assert_eq!(
        runtime.state.grid.cell(0, 1),
        "renamed",
        "the committed value reads back through the adapter's world"
    );
}

#[test]
fn the_settings_page_is_its_own_page_not_the_old_preview_card() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").expect("settings navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    assert_eq!(runtime.state.active, Section::Settings);
    assert!(
        runtime.find("settings-page").is_some(),
        "the settings page is real since P2"
    );
    assert!(
        runtime.find("datazen-preview-card").is_none(),
        "the placeholder card is gone"
    );
    assert!(
        runtime.find("connection-local").is_none(),
        "the connection list is its own page"
    );
    assert!(
        runtime.find("workspace-empty").is_none(),
        "the workspace empty state is not here"
    );
}

#[test]
fn settings_nav_groups_render_all_twelve_sections() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").expect("settings navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    assert!(
        runtime.find("settings-nav-group-app").is_some(),
        "app group rendered"
    );
    assert!(
        runtime.find("settings-nav-group-integration").is_some(),
        "integration group rendered"
    );
    for section in datazen_ui::settings_state::SETTINGS_SECTIONS {
        assert!(
            runtime
                .find(&format!("settings-nav-{}", section.id))
                .is_some(),
            "missing nav item for {}",
            section.id
        );
    }
}

#[test]
fn settings_navigation_switches_rendered_section() {
    for (id, marker) in [
        ("appearance", "appearance-color-scheme-select-trigger"),
        ("dataBrowsing", "settings-page-size-set"),
        ("editor", "settings-snippets"),
        ("mcpServer", "settings-section-mcpServer"),
    ] {
        let mut runtime = runtime();
        runtime.state.active = Section::Settings;
        runtime.frame(0.0);

        assert_eq!(runtime.state.settings_section(), "general");
        assert!(runtime.find("settings-section-general").is_some());
        assert!(
            runtime.find(&format!("settings-section-{id}")).is_none(),
            "section {id} hidden before switch"
        );

        runtime.state.set_settings_section(id);
        runtime.settle_all();
        runtime.frame(0.0);
        assert_eq!(runtime.state.settings_section(), id, "active section {id}");
        assert!(
            runtime.find(&format!("settings-section-{id}")).is_some(),
            "section {id} rendered"
        );
        assert!(runtime.find(marker).is_some(), "marker {marker} for {id}");
        assert!(runtime.find("settings-section-general").is_none());
    }
}

#[test]
fn settings_section_is_deep_link_addressable_by_key() {
    let ids = [
        "general",
        "appearance",
        "dataBrowsing",
        "editor",
        "behavior",
        "logging",
        "ai",
        "prompts",
        "mcpServer",
        "mcpClient",
        "tunnels",
        "extensions",
    ];
    for id in ids {
        let mut runtime = runtime();
        runtime.state.active = Section::Settings;
        runtime.state.set_settings_section(id);
        runtime.frame(0.0);
        assert_eq!(runtime.state.settings_section(), id, "deep-link key {id}");
        assert!(
            runtime.find(&format!("settings-section-{id}")).is_some(),
            "section {id} should render"
        );
    }

    let mut runtime = runtime();
    runtime.state.active = Section::Settings;
    runtime.state.set_settings_section("does-not-exist");
    runtime.frame(0.0);
    assert_eq!(runtime.state.settings_section(), "general");
    assert!(runtime.find("settings-section-general").is_some());
}
