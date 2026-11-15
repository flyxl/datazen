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
fn the_foreign_keys_view_keeps_compound_keys_together() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_table_tab("orders").unwrap();
    runtime.frame(0.0);

    let (x, y) = runtime.find("view-fks").expect("foreign keys button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    assert!(runtime.find("fks-list").is_some());
    assert!(runtime.find("fks-header").is_some());
    assert!(
        runtime.find("fk-row-0").is_some(),
        "orders has one foreign key"
    );
    assert!(runtime.find("fk-row-1").is_none());
    let keys = runtime
        .state
        .workspace
        .table_foreign_keys("orders")
        .unwrap();
    assert_eq!(keys[0].columns, vec!["id", "name"]);
    assert_eq!(keys[0].target_columns, vec!["id", "name"]);
}

#[test]
fn the_er_page_draws_the_current_schema_and_collapses_wide_tables() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("connection-local").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    runtime.state.open_table_tab("customers").unwrap();
    runtime.frame(0.0);

    let (x, y) = runtime.find("er-button").expect("er button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    assert!(runtime.find("er-page").is_some());
    assert!(runtime.find("er-canvas").is_some());
    assert!(
        runtime.find("er-box-customers").is_some(),
        "the current schema's tables"
    );
    assert!(runtime.find("er-box-orders").is_some());
    assert!(
        runtime.find("er-col-customers-0").is_some(),
        "narrow tables list their columns"
    );
    assert!(runtime.find("er-col-customers-7").is_some());
    assert!(
        runtime.find("er-box-catalog").is_none(),
        "analytics is another schema"
    );

    // A wide table in its own schema collapses to the count.
    runtime.state.open_table_tab("catalog").unwrap();
    runtime.settle_all();
    runtime.frame(0.0);
    let (x, y) = runtime.find("er-button").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(runtime.find("er-box-catalog").is_some());
    assert!(
        runtime.find("er-collapsed-catalog").is_some(),
        "30 columns collapse to the count"
    );
    assert!(
        runtime.find("er-col-catalog-12").is_none(),
        "not every column is drawn"
    );
}

#[test]
fn er_boxes_drag_and_the_position_survives_rerenders() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("connection-local").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    runtime.state.open_table_tab("customers").unwrap();
    runtime.frame(0.0);
    let (x, y) = runtime.find("er-button").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);

    let before = runtime
        .presented("er-box-customers")
        .expect("the box is drawn")
        .0;
    let (x, y) = runtime.find("er-box-customers").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_move(x + 40.0, y + 20.0);
    runtime.pointer_move(x + 80.0, y + 50.0);
    runtime.pointer_up(x + 80.0, y + 50.0);
    runtime.frame(0.0);

    assert_eq!(
        runtime.state.er_positions.get("customers"),
        Some(&(80.0, 50.0)),
        "the drag moved the box by its travel"
    );
    let after = runtime
        .presented("er-box-customers")
        .expect("still drawn")
        .0;
    assert!(
        after[0] >= before[0] + 70.0 && after[1] >= before[1] + 40.0,
        "before {before:?} after {after:?}"
    );

    // A plain rerender keeps the position.
    runtime.frame(1.0);
    let kept = runtime.presented("er-box-customers").unwrap().0;
    assert!(
        (kept[0] - after[0]).abs() < 1.0 && (kept[1] - after[1]).abs() < 1.0,
        "kept {kept:?}"
    );
    assert!(runtime.state.er_drag.is_none(), "the gesture ended");
}

#[test]
fn column_drafts_stay_separate_from_the_official_columns() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_table_tab("customers").unwrap();
    runtime.frame(0.0);
    let (x, y) = runtime.find("view-columns").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(runtime.find("column-drafts").is_some());
    assert!(
        runtime.find("column-row-8").is_none(),
        "8 official columns only"
    );

    runtime.state.draft_name = rune::TextEdit::new("score");
    runtime.state.draft_type = rune::TextEdit::new("int");
    let (x, y) = runtime.find("draft-add").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    assert!(runtime.find("draft-row-0").is_some(), "the draft is queued");
    assert!(
        runtime.find("column-row-8").is_none(),
        "the official list is untouched"
    );
    assert_eq!(
        runtime
            .state
            .workspace
            .table_columns("customers")
            .unwrap()
            .len(),
        8,
        "the port's columns are unchanged"
    );

    runtime.state.draft_name = rune::TextEdit::new("name");
    runtime.state.draft_type = rune::TextEdit::new("varchar(80)");
    let (x, y) = runtime.find("draft-retype").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("draft-row-1").is_some(),
        "the retype draft is queued too"
    );
}

#[test]
fn the_drafts_preview_as_sql_without_executing() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_table_tab("customers").unwrap();
    runtime.frame(0.0);
    let (x, y) = runtime.find("view-columns").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(runtime.find("draft-sql").is_none(), "no drafts, no preview");

    runtime.state.draft_name = rune::TextEdit::new("name");
    runtime.state.draft_type = rune::TextEdit::new("varchar(80)");
    let (x, y) = runtime.find("draft-retype").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    assert!(runtime.find("draft-sql").is_some(), "the preview appears");
    let drafts = runtime.state.column_drafts.get("customers").unwrap();
    assert_eq!(
        drafts[0].sql("customers"),
        "ALTER TABLE customers ALTER COLUMN name TYPE varchar(80);",
        "a retype draft shows its SQL"
    );
    assert_eq!(
        runtime
            .state
            .workspace
            .table_columns("customers")
            .unwrap()
            .len(),
        8,
        "the preview executed nothing"
    );
}

#[test]
fn applying_the_drafts_lands_and_a_failure_keeps_them() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_table_tab("customers").unwrap();
    runtime.frame(0.0);
    let (x, y) = runtime.find("view-columns").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);

    runtime.state.draft_name = rune::TextEdit::new("score");
    runtime.state.draft_type = rune::TextEdit::new("int");
    let (x, y) = runtime.find("draft-add").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);

    let (x, y) = runtime.find("draft-apply").expect("apply button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(runtime.find("draft-row-0").is_none(), "the drafts are gone");
    assert!(runtime.find("draft-sql").is_none());
    assert!(
        runtime.find("column-row-8").is_some(),
        "the official list grew"
    );
    assert_eq!(
        runtime.state.workspace.table_columns("customers").unwrap()[8].name,
        "score"
    );
    assert_eq!(
        runtime.state.workspace.applied_ddl().len(),
        1,
        "the port recorded the apply"
    );

    // A read-only table refuses: the drafts stay, the official list is untouched.
    runtime.state.open_table_tab("orders").unwrap();
    runtime.settle_all();
    runtime.frame(1.0);
    runtime.state.draft_name = rune::TextEdit::new("x");
    runtime.state.draft_type = rune::TextEdit::new("int");
    let (x, y) = runtime.find("draft-add").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let (x, y) = runtime.find("draft-apply").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("draft-row-0").is_some(),
        "the failed apply keeps the drafts"
    );
    assert!(
        runtime.find("column-row-2").is_none(),
        "the official columns are unchanged"
    );
    assert_eq!(
        runtime
            .state
            .workspace
            .table_columns("orders")
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn the_theme_toggles_through_the_port_and_survives_reopening() {
    use datazen_ui_api::{Setting, SettingsApi};

    let mut runtime = runtime();
    runtime.frame(0.0);

    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(runtime.find("settings-page").is_some());
        runtime.state.set_settings_section("appearance");
        runtime.settle_all();
        runtime.frame(0.0);
    assert!(runtime.find("settings-theme").is_some());

    runtime
        .state
        .workspace
        .write_setting(Setting::Theme { dark: true })
        .unwrap();
    runtime.frame(0.0);

    let (x, y) = runtime.find("settings-theme").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime
            .state
            .workspace
            .read_settings()
            .contains(&Setting::Theme { dark: false }),
        "the toggle wrote the port"
    );

    // Leave and come back: the setting is where the port keeps it.
    let (x, y) = runtime.find("rail-connections").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
        runtime.state.set_settings_section("appearance");
        runtime.settle_all();
        runtime.frame(0.0);
    assert!(
        runtime
            .state
            .workspace
            .read_settings()
            .contains(&Setting::Theme { dark: false }),
        "reopening the page keeps the theme"
    );

    let (x, y) = runtime.find("settings-theme").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(runtime
        .state
        .workspace
        .read_settings()
        .contains(&Setting::Theme { dark: true }));
}

#[test]
fn the_page_size_setting_drives_the_grid_paging() {
    use datazen_ui_api::{Setting, SettingsApi, WorkspaceApi};

    let mut runtime = runtime();
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_table_tab("customers").unwrap();
    runtime.frame(0.0);
    assert_eq!(runtime.state.grid.page_size(), 25, "the default");

    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
        runtime.state.set_settings_section("dataBrowsing");
        runtime.settle_all();
        runtime.frame(0.0);
    runtime.state.page_size_input = rune::TextEdit::new("10");
    let (x, y) = runtime.find("settings-page-size-set").expect("set button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime
            .state
            .workspace
            .read_settings()
            .contains(&Setting::TablePageSize { rows: 10 }),
        "the write went through the port"
    );

    // The next page now turns at the new size.
    runtime.state.active = Section::Workspace;
    runtime.state.next_table_page();
    runtime.frame(0.0);
    assert_eq!(runtime.state.grid.page(), 1);
    assert_eq!(runtime.state.grid.rows, 10, "the second page has 10 rows");
    assert_eq!(runtime.state.grid.cell(0, 1), "customer-10");
    assert_eq!(runtime.state.grid.cell(9, 1), "customer-19");

    // An invalid value keeps the old setting and shows the error.
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
        runtime.state.set_settings_section("dataBrowsing");
        runtime.settle_all();
        runtime.frame(0.0);
    runtime.state.page_size_input = rune::TextEdit::new("0");
    let (x, y) = runtime.find("settings-page-size-set").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime
            .state
            .workspace
            .read_settings()
            .contains(&Setting::TablePageSize { rows: 10 }),
        "the rejected write changed nothing"
    );
    assert!(
        runtime.find("settings-error").is_some(),
        "the rejection is visible"
    );
}

#[test]
fn a_connection_form_save_needs_an_address() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    assert!(runtime.find("connection-local").is_some());
    assert!(runtime.find("connection-new").is_some(), "new connection action is present");
    assert!(runtime.find("connection-form").is_none(), "form is dialogized, not inline");

    let (x, y) = runtime.find("connection-new").expect("new connection button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(runtime.find("connection-dialog").is_some(), "dialog opens");
    assert!(runtime.find("connection-form").is_some());

    runtime.state.save_connection_form();
    runtime.frame(0.0);
    assert!(runtime.find("connection-remote-postgres").is_none(), "nothing was saved");
    assert!(runtime.find("connection-form-error").is_some(), "the refusal is visible");
    assert!(runtime.find("connection-dialog").is_some(), "dialog stays open on refusal");

    runtime.state.connection_name = rune::TextEdit::new("Remote Postgres");
    runtime.state.connection_engine = rune::TextEdit::new("postgres");
    runtime.state.connection_address = rune::TextEdit::new("db.example.com:5432");
    runtime.state.save_connection_form();
    runtime.frame(0.0);
    assert!(
        runtime
            .state
            .workspace
            .connections()
            .iter()
            .any(|c| c.id == "remote-postgres"),
        "the saved connection joins the list"
    );
    assert!(runtime.state.connection_error.is_none(), "the error clears");
    assert!(!runtime.state.connection_dialog_open, "dialog closes on successful save");
}

#[test]
fn connection_dialog_renders_grouped_sections_and_ssh_ui() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("connection-new").expect("new connection button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    assert!(runtime.find("connection-dialog").is_some(), "dialog is open");
    assert!(runtime.find("connection-section-basic").is_some(), "basic group rendered");
    assert!(runtime.find("connection-section-connection").is_some(), "connection group rendered");
    assert!(runtime.find("connection-section-ssh").is_some(), "ssh tunnel group rendered");
    assert!(runtime.find("connection-name-input").is_some());
    assert!(runtime.find("connection-engine-input").is_some());
    assert!(runtime.find("connection-address-input").is_some());
    assert!(runtime.find("connection-database-input").is_some());
    assert!(runtime.find("connection-username-input").is_some());
    assert!(runtime.find("connection-password-input").is_some());
    assert!(runtime.find("connection-ssh-toggle").is_some(), "ssh toggle present (UI-only)");
    assert!(runtime.find("connection-ssh-host-input").is_some());

    runtime.state.close_connection_dialog();
    runtime.frame(0.0);
    assert!(!runtime.state.connection_dialog_open, "dialog closes");
    // form lives inside the dialog; closed means dialog not open and no inline form.
    assert!(runtime.find("connection-new").is_some(), "new connection action remains available");
}

#[test]
fn snippets_add_update_delete_and_duplicate_names_are_refused() {
    use datazen_ui_api::{SettingsApi, WorkspaceApi};

    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
        runtime.state.set_settings_section("editor");
        runtime.settle_all();
        runtime.frame(0.0);
    assert!(runtime.find("settings-snippets").is_some());

    runtime.state.snippet_name = rune::TextEdit::new("top");
    runtime.state.snippet_sql = rune::TextEdit::new("SELECT * FROM customers LIMIT 10");
    let (x, y) = runtime.find("snippet-save").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("snippet-row-0").is_some(),
        "the snippet is listed"
    );

    runtime.state.snippet_name = rune::TextEdit::new("top");
    runtime.state.snippet_sql = rune::TextEdit::new("SELECT 1");
    let (x, y) = runtime.find("snippet-save").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("snippet-error").is_some(),
        "the duplicate name is refused"
    );
    assert!(runtime.find("snippet-row-1").is_none(), "still one snippet");

    runtime.state.snippet_name = rune::TextEdit::new("top");
    runtime.state.snippet_sql = rune::TextEdit::new("SELECT 1");
    let (x, y) = runtime.find("snippet-update").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert_eq!(
        runtime.state.workspace.snippets()[0].sql,
        "SELECT 1",
        "update changed the sql"
    );

    runtime.frame(1.0);
    let (x, y) = runtime.find("snippet-delete-0").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("snippet-row-0").is_none(),
        "the snippet is gone"
    );
    assert!(runtime.state.workspace.snippets().is_empty());
}

/// A host adapter that owns snippet storage (T3): the panel reads it
/// host-first and the writes land in it, not the memory port.

#[test]
fn the_snippets_panel_reads_and_writes_the_host_store_first() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.state.desktop = Box::new(SnippetHostAdapter {
        snippets: vec![datazen_ui_api::Snippet {
            name: "host snippet".into(),
            sql: "SELECT 1".into(),
        }],
    });
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
        runtime.state.set_settings_section("editor");
        runtime.settle_all();
        runtime.frame(0.0);
    assert!(runtime.find("settings-snippets").is_some());

    // The host's snippet is what the panel lists — the memory port stays
    // untouched.
    assert!(
        runtime.find("snippet-row-0").is_some(),
        "the host snippet is listed"
    );
    assert!(
        runtime.state.workspace.snippets().is_empty(),
        "the memory port stays empty"
    );

    // A save reaches the host, not the port.
    runtime.state.snippet_name = rune::TextEdit::new("from the form");
    runtime.state.snippet_sql = rune::TextEdit::new("SELECT 2");
    let (x, y) = runtime.find("snippet-save").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("snippet-row-1").is_some(),
        "the new snippet is listed"
    );
    assert!(
        runtime.state.workspace.snippets().is_empty(),
        "the write went to the host"
    );
}

#[test]
fn an_export_without_a_range_gets_no_job() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
        runtime.state.set_settings_section("dataBrowsing");
        runtime.settle_all();
        runtime.frame(0.0);
    assert!(runtime.find("export-section").is_some());

    let (x, y) = runtime.find("export-start").expect("export button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    answer_save_dialog(&mut runtime, Some("/picked/exports/albums.csv"));
    assert!(runtime.state.export_job.is_none(), "no range, no job");
    assert!(
        runtime.find("export-error").is_some(),
        "the refusal is visible"
    );
    assert!(runtime.find("export-job").is_none());

    runtime.state.export_range = rune::TextEdit::new("customers");
    let (x, y) = runtime.find("export-start").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    answer_save_dialog(&mut runtime, Some("/picked/exports/customers.csv"));
    assert_eq!(
        runtime.state.export_job,
        Some(1),
        "the port handed out a job id"
    );
    assert!(runtime.find("export-job").is_some());
    assert!(runtime.find("export-error").is_none());

    // The format chip cycles.
    let (x, y) = runtime.find("export-format").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert_eq!(runtime.state.export_format(), "json");
}

/// A host adapter that records export and backup requests (T2) and owns
/// their job snapshots.

#[test]
fn an_export_uses_the_picked_save_path() {
    let (adapter, exports, _backups) = PathAskingAdapter::new();
    let mut runtime = runtime();
    runtime.state.desktop = Box::new(adapter);
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
        runtime.state.set_settings_section("dataBrowsing");
        runtime.settle_all();
        runtime.frame(0.0);
    runtime.state.export_range = rune::TextEdit::new("customers");

    let (x, y) = runtime.find("export-start").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    answer_save_dialog(&mut runtime, Some("/picked/exports/customers.csv"));

    assert_eq!(
        exports.borrow().as_slice(),
        [(
            "result".to_owned(),
            "csv".to_owned(),
            "customers".to_owned(),
            Some("/picked/exports/customers.csv".to_owned())
        )]
        .as_slice(),
        "the picked path reaches the host with the request"
    );
    assert_eq!(runtime.state.export_job, Some(7));
    assert!(runtime.state.export_error.is_none());
}

#[test]
fn a_cancelled_save_dialog_starts_no_export() {
    let (adapter, exports, _backups) = PathAskingAdapter::new();
    let mut runtime = runtime();
    runtime.state.desktop = Box::new(adapter);
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
        runtime.state.set_settings_section("dataBrowsing");
        runtime.settle_all();
        runtime.frame(0.0);
    runtime.state.export_range = rune::TextEdit::new("customers");

    let (x, y) = runtime.find("export-start").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    answer_save_dialog(&mut runtime, None);

    assert!(exports.borrow().is_empty(), "a cancel reaches no adapter");
    assert_eq!(runtime.state.export_job, None);
    assert!(
        runtime
            .state
            .export_error
            .as_deref()
            .unwrap()
            .contains("cancelled"),
        "the cancellation is honest: {:?}",
        runtime.state.export_error
    );
}
