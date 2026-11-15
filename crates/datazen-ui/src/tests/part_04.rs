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
fn a_save_dialog_that_cannot_show_falls_back_to_the_default_directory() {
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

    // The first press opens the dialog; a second press while it is open
    // is refused, and the refusal's answer falls back to the default
    // directory with an honest notice.
    let (x, y) = runtime.find("export-start").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.file_dialog().is_some(),
        "the first press opened a dialog"
    );
    let (x, y) = runtime.find("export-start").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    runtime.frame(0.0);
    assert_eq!(
        exports.borrow().as_slice(),
        [(
            "result".to_owned(),
            "csv".to_owned(),
            "customers".to_owned(),
            None
        )]
        .as_slice(),
        "the refused dialog fell back to the default directory"
    );
    assert!(
        runtime
            .state
            .export_error
            .as_deref()
            .unwrap()
            .contains("default directory"),
        "the fallback says so: {:?}",
        runtime.state.export_error
    );

    // The still-open first dialog answers later with its picked path.
    runtime
        .resolve_file_dialog(Some("/picked/exports/late.csv".into()))
        .unwrap();
    runtime.frame(0.0);
    assert_eq!(
        exports.borrow()[1].3.as_deref(),
        Some("/picked/exports/late.csv")
    );
}

#[test]
fn browsing_picks_the_import_source() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
        runtime.state.set_settings_section("dataBrowsing");
        runtime.settle_all();
        runtime.frame(0.0);
    assert!(
        runtime.find("import-browse").is_some(),
        "the browse button is on step 1"
    );

    let (x, y) = runtime.find("import-browse").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    let (kind, _) = runtime
        .file_dialog()
        .expect("the browse asked for an open dialog");
    assert_eq!(kind, rune::FileDialogKind::Open);
    runtime
        .resolve_file_dialog(Some("/imports/dump.csv".into()))
        .unwrap();
    runtime.frame(0.0);
    assert_eq!(
        runtime.state.import_source.text, "/imports/dump.csv",
        "the picked path fills the source input"
    );
}

#[test]
fn a_failed_export_shows_its_error_line_from_the_snapshot() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
        runtime.state.set_settings_section("dataBrowsing");
        runtime.settle_all();
        runtime.frame(0.0);

    // A range naming no table fails on the third snapshot.
    runtime.state.export_range = rune::TextEdit::new("no-such-table");
    let (x, y) = runtime.find("export-start").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    answer_save_dialog(&mut runtime, Some("/picked/exports/none.csv"));
    runtime.frame(1.0);
    let snapshot = runtime.state.export_snapshot.clone().unwrap();
    assert_eq!(snapshot.status, "running");
    assert_eq!(snapshot.percent, 0);
    assert!(runtime.find("export-progress").is_some());
    assert!(runtime.find("export-log-0").is_some());

    let (x, y) = runtime.find("export-refresh").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert_eq!(runtime.state.export_snapshot.as_ref().unwrap().percent, 50);

    runtime.frame(1.0);
    let (x, y) = runtime.find("export-refresh").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    let snapshot = runtime.state.export_snapshot.clone().unwrap();
    assert_eq!(snapshot.status, "failed");
    assert_eq!(
        snapshot.log.last().map(String::as_str),
        Some("no such table no-such-table"),
        "the failure's error sentence is in the snapshot"
    );
    assert!(
        runtime
            .find(&format!("export-log-{}", snapshot.log.len() - 1))
            .is_some(),
        "the error line is drawn"
    );
    assert!(runtime.find("export-status").is_some());
}

#[test]
fn the_import_wizard_needs_a_target_to_move_on() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
        runtime.state.set_settings_section("dataBrowsing");
        runtime.settle_all();
        runtime.frame(0.0);
    assert!(runtime.find("import-step-1").is_some());
    assert!(runtime.find("import-step-2").is_none());

    // Step 1 needs a source.
    let (x, y) = runtime.find("import-next").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(runtime.find("import-step-1").is_some(), "still on step 1");
    assert!(runtime.find("import-error").is_some());

    runtime.state.import_source = rune::TextEdit::new("dump.csv");
    let (x, y) = runtime.find("import-next").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(runtime.find("import-step-2").is_some(), "step 2 arrived");
    assert!(runtime.find("import-step-1").is_none());

    // Step 2 needs a target table: no target, no next step.
    let (x, y) = runtime.find("import-finish").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(runtime.find("import-step-2").is_some(), "still on step 2");
    assert!(runtime.find("import-ready").is_none());
    assert!(
        runtime.find("import-error").is_some(),
        "the refusal is visible"
    );

    runtime.state.import_target = rune::TextEdit::new("customers");
    let (x, y) = runtime.find("import-finish").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(runtime.find("import-ready").is_some(), "both steps done");
}

#[test]
fn a_backup_page_stops_and_shows_stopped() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("connection-local").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
        runtime.state.set_settings_section("dataBrowsing");
        runtime.settle_all();
        runtime.frame(0.0);
    assert!(runtime.find("backup-section").is_some());

    let (x, y) = runtime.find("backup-start").expect("start button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    answer_save_dialog(&mut runtime, Some("/picked/backups/backup.sql"));
    runtime.frame(1.0);
    let snapshot = runtime.state.backup_snapshot.clone().unwrap();
    assert_eq!(snapshot.status, "running");
    assert!(runtime.find("backup-progress").is_some());
    assert!(runtime.find("backup-log-1").is_some());

    let (x, y) = runtime.find("backup-stop").expect("stop button");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    let snapshot = runtime.state.backup_snapshot.clone().unwrap();
    assert_eq!(
        snapshot.status, "stopped",
        "after stopping the status is stopped"
    );
    assert!(snapshot.log.last() == Some(&"stopped by the user".to_owned()));
    assert!(
        runtime
            .find(&format!("backup-log-{}", snapshot.log.len() - 1))
            .is_some(),
        "the stop line is drawn"
    );

    // A second stop is refused and visible.
    let (x, y) = runtime.find("backup-stop").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(runtime.find("backup-error").is_some());
}

#[test]
fn a_value_search_lists_hits_and_a_hit_locates_the_table_with_a_filter() {
    use crate::app_poll;

    let mut runtime = runtime();
    runtime.poll = Some(app_poll);
    runtime.state.desktop = Box::new(JobAdapter::new());
    runtime.frame(0.0);

    // A live session: the adapter's catalog delivery is what the search
    // gates on (the JobAdapter lists no connections to press).
    let generation = runtime.state.next_read_generation();
    runtime
        .state
        .deliver_host_job(AdapterDelivery::ConnectionOpened {
            generation,
            id: "live".into(),
            catalog: Ok(datazen_ui_api::Catalog {
                databases: vec![datazen_ui_api::DatabaseNode {
                    name: "main".into(),
                    schemas: vec![datazen_ui_api::SchemaNode {
                        name: "main".into(),
                        tables: vec!["events".into(), "users".into()],
                    }],
                }],
            }),
        });

    // Tools page, the search tab.
    let (x, y) = runtime.find("rail-tools").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let (x, y) = runtime.find("tool-search").expect("the search tab");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(runtime.find("value-search-tool").is_some());

    // No value: the honest refusal.
    let (x, y) = runtime.find("value-search-start").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("value-search-error").is_some(),
        "a value is required"
    );

    runtime.state.value_search_value = rune::TextEdit::new("hello");
    let (x, y) = runtime.find("value-search-start").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.state.value_search_loading.is_some(),
        "the job is in flight"
    );
    runtime.frame(1.0);
    runtime.frame(1.0);
    assert!(
        runtime.state.value_search_loading.is_none(),
        "the delivery landed"
    );
    assert!(
        runtime.find("value-hit-0").is_some(),
        "the first hit is listed"
    );
    assert!(
        runtime.find("value-hit-1").is_some(),
        "the second hit is listed"
    );

    // A hit locates: the workspace opens the table with the filter.
    let (x, y) = runtime.find("value-hit-1").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert_eq!(
        runtime.state.active,
        Section::Workspace,
        "locate leaves the tools page"
    );
    let filter = runtime
        .state
        .filter
        .clone()
        .expect("the locate set a filter");
    assert_eq!(filter.column, "name");
    assert_eq!(
        filter.contains, "hello",
        "the filter carries the searched value"
    );

    // Cancel while in flight: the delivery lands nowhere.
    runtime.state.value_search_value = rune::TextEdit::new("again");
    let (x, y) = runtime.find("rail-tools").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let (x, y) = runtime.find("value-search-start").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.state.value_search_loading.is_some(),
        "the second search runs"
    );
    let (x, y) = runtime.find("value-search-cancel").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.state.value_search_loading.is_none(),
        "the cancel cleared the marker"
    );
    assert!(
        runtime.find("value-search-error").is_some(),
        "the cancel says so"
    );
    runtime.frame(1.0);
    runtime.frame(1.0);
    assert!(
        runtime.find("value-search-hits").is_none(),
        "the cancelled scan's delivery landed nowhere"
    );
}

#[test]
fn a_foreign_key_cell_navigates_to_the_referenced_row() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("connection-local").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    runtime.state.open_table_tab("orders").unwrap();
    runtime.frame(1.0);
    assert!(runtime.state.grid.has_page(), "the orders page loaded");

    // The focused cell's column (id) carries the demo's outbound key.
    runtime.state.grid.select(0, 0);
    runtime.frame(0.0);
    assert!(
        runtime.find("fk-go-to").is_some(),
        "the go-to chip is on the bar"
    );

    let (x, y) = runtime.find("fk-go-to").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let selected = runtime.state.tabs.selected().map(|tab| tab.title.clone());
    assert_eq!(
        selected,
        Some("customers".into()),
        "the referenced table opened"
    );
    let filter = runtime
        .state
        .filter
        .clone()
        .expect("the navigation set a filter");
    assert_eq!(filter.column, "id");
    assert_eq!(
        filter.contains, "1",
        "the focused cell's value is the filter"
    );

    // A table without outbound keys shows no chip.
    runtime.state.open_table_tab("catalog").unwrap();
    runtime.frame(1.0);
    runtime.state.grid.select(0, 0);
    runtime.frame(0.0);
    assert!(
        runtime.find("fk-go-to").is_none(),
        "the catalog table carries no outbound key"
    );
}

/// A host adapter that answers the inbound references (T5).

#[test]
fn the_referenced_by_list_opens_the_referencing_table_filtered() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("connection-local").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    runtime.state.open_table_tab("customers").unwrap();
    runtime.frame(1.0);
    runtime.state.grid.select(0, 0);
    runtime.frame(0.0);

    // The unwired adapter refuses honestly: the demo world has no scan.
    let (x, y) = runtime.find("fk-referenced-by").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("fk-error").is_some(),
        "the demo world refuses honestly"
    );
    runtime.state.close_inbound_refs();
    runtime.settle_all();
    runtime.frame(0.0);

    // With the host answering: the list shows, and a row opens the
    // referencing table filtered on the focused row's value. A direct
    // state call needs the settle to land in the view.
    runtime.state.desktop = Box::new(InboundAdapter);
    runtime.state.load_inbound_refs();
    runtime.settle_all();
    runtime.frame(0.0);

    assert!(
        runtime.find("fk-inbound-0").is_some(),
        "the inbound list is up"
    );
    let (x, y) = runtime.find("fk-inbound-0").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let selected = runtime.state.tabs.selected().map(|tab| tab.title.clone());
    assert_eq!(
        selected,
        Some("orders".into()),
        "the referencing table opened"
    );
    let filter = runtime
        .state
        .filter
        .clone()
        .expect("the reference set a filter");
    assert_eq!(filter.column, "id");
    assert_eq!(
        filter.contains, "0",
        "the focused row's id value is the filter"
    );
}

/// A host adapter that owns the favorites store (T6).

#[test]
fn the_favorites_panel_saves_opens_renames_and_deletes() {
    use datazen_ui_api::WorkspaceApi;

    // The demo process refuses honestly: no favorites without the host.
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(runtime.find("settings-favorites").is_some());
    assert!(runtime.find("favorite-empty").is_some(), "no favorites yet");
    runtime.state.favorite_title = rune::TextEdit::new("top");
    let (x, y) = runtime.find("favorite-save").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("favorite-error").is_some(),
        "the demo world refuses honestly"
    );

    // The host's store answers: the current query saves under the title.
    runtime.state.desktop = Box::new(FavoritesHostAdapter {
        favorites: Vec::new(),
    });
    runtime.state.sql = sql_editor::SqlEditor::from_doc("SELECT * FROM users");
    runtime.settle_all();
    runtime.frame(0.0);
    runtime.state.favorite_title = rune::TextEdit::new("top spenders");
    let (x, y) = runtime.find("favorite-save").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("favorite-row-0").is_some(),
        "the saved favorite is listed"
    );
    assert!(runtime.find("favorite-error").is_none());
    runtime.frame(1.0);

    // Open: a query tab with the title carries the statement.
    let (x, y) = runtime.find("favorite-open-0").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert_eq!(
        runtime.state.active,
        Section::Workspace,
        "open leaves the settings page"
    );
    let selected = runtime.state.tabs.selected().map(|tab| tab.title.clone());
    assert_eq!(selected, Some("top spenders".into()));
    assert_eq!(runtime.state.sql.doc(), "SELECT * FROM users");

    // Rename: the form's title moves the row's title.
    let (x, y) = runtime.find("rail-settings").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    runtime.state.favorite_title = rune::TextEdit::new("renamed");
    let (x, y) = runtime.find("favorite-rename-0").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    let listed = runtime.state.host_favorites();
    assert_eq!(listed[0].title, "renamed");

    // Delete: the row leaves.
    let (x, y) = runtime.find("favorite-delete-0").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.state.host_favorites().is_empty(),
        "the favorite is gone"
    );
}

/// A host adapter that parses and confirms imports (T7).

#[test]
fn the_connections_page_imports_a_previewed_file() {
    let (adapter, parsed_paths, confirmed) = ImportHostAdapter::new();
    let mut runtime = runtime();
    runtime.state.desktop = Box::new(adapter);
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-connections").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(
        runtime.find("import-connections").is_some(),
        "the import button is on the page"
    );

    // The button opens an Open dialog; the picked path parses. The
    // password field serves the encrypted formats.
    runtime.state.import_password = rune::TextEdit::new("import-password");
    runtime.settle_all();
    runtime.frame(0.0);
    let (x, y) = runtime.find("import-connections").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    let (kind, _) = runtime.file_dialog().expect("the import asked for a file");
    assert_eq!(kind, rune::FileDialogKind::Open);
    runtime
        .resolve_file_dialog(Some("/exports/connections.tableplusconnection".into()))
        .unwrap();
    runtime.frame(0.0);
    runtime.frame(0.0);
    assert!(
        runtime.find("import-preview").is_some(),
        "the preview is up"
    );
    assert!(
        runtime.find("imported-connection-0").is_some(),
        "the first connection is listed"
    );
    assert!(
        runtime.find("imported-connection-1").is_some(),
        "the second connection is listed"
    );

    // Confirm: the host confirms by the preview's handle, the notice
    // counts what landed, the preview leaves.
    runtime.frame(1.0);
    let (x, y) = runtime.find("import-confirm").unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        runtime.find("import-notice").is_some(),
        "the outcome is visible"
    );
    assert!(runtime.state.import_preview.is_none(), "the preview left");
    assert_eq!(
        parsed_paths.borrow().as_slice(),
        ["/exports/connections.tableplusconnection"],
        "the host parsed the picked path"
    );
    assert_eq!(
        confirmed.borrow().as_slice(),
        [42],
        "the confirm carries the preview's handle"
    );
    assert!(
        runtime
            .state
            .import_notice
            .as_deref()
            .unwrap()
            .contains("2 connections imported"),
        "the notice counts: {:?}",
        runtime.state.import_notice
    );
}
