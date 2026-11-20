//! T132 independent audit: the grid data-export dialog port
//! (`components/data_table/data_export_dialog.rs` +
//! `components/data_table/data_export_dialog_options.rs`).
//!
//! Fresh vectors against a mounted modal and its VM: the open-reset
//! contract, the scope/format gating and fallbacks, the export-button gate,
//! the request the host receives, the locked warning, the disabled-format
//! reason lines, the two settled states, the summary's live counts, key
//! uniqueness, no i18n key echo, and window bounds at 960×640 / 1440×816 in
//! both themes. The scene holds the dialog VM in the app state `S`, exactly
//! as a real host would.

use rune::{col, Cx, El, Env, KeyEvent, Key, Mods, Runtime};

use crate::components::connection::batch_export_dialog::{
    DataExportCapability, DataFormat,
};
use crate::components::data_table::data_export_dialog::{
    DataExportColumn, DataExportDialog, DataExportRequest, DataExportStatus, DataExportVm,
};
use crate::components::data_table::data_export_dialog_options::{
    build_format_options, disabled_reason_keys, ExportScope,
};
use crate::i18n::t;

const KEY: &str = "data-export-dialog";

#[derive(Clone)]
struct Scene {
    vm: DataExportVm,
    dark: bool,
    // recorded callbacks
    closes: usize,
    scopes: Vec<ExportScope>,
    formats: Vec<DataFormat>,
    exports: Vec<DataExportRequest>,
    cancels: usize,
    toggles: Vec<String>,
    select_all: usize,
    clear_all: usize,
    resets: usize,
}

impl Default for Scene {
    fn default() -> Self {
        let mut vm = DataExportVm {
            open: true,
            table_name: "users".to_owned(),
            db_session_id: Some("sess-1".to_owned()),
            database_type: Some("postgresql".to_owned()),
            total_rows: Some(1_000_000),
            rows_len: 40,
            selected_rows_len: 5,
            primary_key_columns: vec!["id".to_owned()],
            columns: vec![
                DataExportColumn::new("id").with_type("integer"),
                DataExportColumn::new("name").with_type("text"),
                DataExportColumn::new("email").with_type("text"),
            ],
            ..DataExportVm::default()
        };
        // The host's open ritual: take every column, start on the page scope.
        vm.reset_on_open();
        Self {
            vm,
            dark: true,
            closes: 0,
            scopes: Vec::new(),
            formats: Vec::new(),
            exports: Vec::new(),
            cancels: 0,
            toggles: Vec::new(),
            select_all: 0,
            clear_all: 0,
            resets: 0,
        }
    }
}

fn scene_is_selected(scene: &Scene, name: &str) -> bool {
    scene.vm.selected_cols.iter().any(|c| c == name)
}

fn scene_toggle(scene: &mut Scene, name: String) {
    scene.toggles.push(name.clone());
    scene.vm.selected_cols.retain(|c| c != &name);
}

fn scene_select_all(scene: &mut Scene) {
    scene.select_all += 1;
    scene.vm.selected_cols = scene.vm.columns.iter().map(|c| c.name.clone()).collect();
}

fn scene_clear_all(scene: &mut Scene) {
    scene.clear_all += 1;
    scene.vm.selected_cols.clear();
}

fn scene_reset_default(scene: &mut Scene) {
    scene.resets += 1;
    scene.vm.selected_cols = scene.vm.columns.iter().map(|c| c.name.clone()).collect();
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    DataExportDialog::<Scene>::new(KEY, &scene.vm)
        .on_close(|s: &mut Scene| {
            s.closes += 1;
            s.vm.open = false;
        })
        .on_scope_change(|s: &mut Scene, scope| s.scopes.push(scope))
        .on_scope_open(|s: &mut Scene| s.vm.scope_open = true)
        .on_scope_close(|s: &mut Scene| s.vm.scope_open = false)
        .on_format_change(|s: &mut Scene, format| s.formats.push(format))
        .on_format_open(|s: &mut Scene| s.vm.format_open = true)
        .on_format_close(|s: &mut Scene| s.vm.format_open = false)
        .on_export(|s: &mut Scene, request| s.exports.push(request))
        .on_cancel_export(|s: &mut Scene| s.cancels += 1)
        .picker_is_selected(scene_is_selected)
        .picker_on_toggle(scene_toggle)
        .picker_on_select_all(scene_select_all)
        .picker_on_clear_all(scene_clear_all)
        .picker_on_reset_default(scene_reset_default)
        .build(scene, scene.dark, &mut layer);
    col()
        .key("grid-page")
        .w_full()
        .h_full()
        .children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        size,
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    maybe(rt, key).unwrap_or_else(|| panic!("{key} is mounted"))
}

fn has_text<S>(rt: &Runtime<S>, expected: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(expected))
}

fn press(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

fn escape(rt: &mut Runtime<Scene>) {
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
}

fn scene_with_status(status: DataExportStatus) -> Scene {
    let mut scene = Scene::default();
    scene.vm.status = status;
    scene
}

// ---- VM: the open-reset contract --------------------------------------------

#[test]
fn reset_on_open_restores_the_callers_scope_and_every_column_but_keeps_the_format() {
    let mut scene = Scene::default();
    // Dirty the dialog the way a finished run leaves it.
    scene.vm.format = DataFormat::Json;
    scene.vm.scope = ExportScope::EntireTable;
    scene.vm.initial_scope = Some(ExportScope::FilteredTable);
    scene.vm.has_filtered_builder = true;
    scene.vm.status = DataExportStatus::Aborted;
    scene.vm.cancelling = true;
    scene.vm.partial_deleted = true;
    scene.vm.error = Some("boom".to_owned());
    scene.vm.selected_cols = vec!["id".to_owned()];

    scene.vm.reset_on_open();

    // Everything the React effect resets, resets.
    assert_eq!(scene.vm.status, DataExportStatus::Form);
    assert_eq!(scene.vm.error, None);
    assert!(!scene.vm.cancelling);
    assert!(!scene.vm.partial_deleted);
    assert_eq!(scene.vm.scope, ExportScope::FilteredTable);
    assert_eq!(scene.vm.selected_cols.len(), 3);
    // The format deliberately survives, as in React.
    assert_eq!(scene.vm.format, DataFormat::Json);
}

#[test]
fn a_closed_dialog_mounts_nothing() {
    let mut scene = Scene::default();
    scene.vm.open = false;
    let rt = runtime(scene, (1440.0, 816.0));
    assert!(maybe(&rt, KEY).is_none(), "the panel is not mounted");
}

// ---- VM: the scope gates ------------------------------------------------------

#[test]
fn the_entire_scope_needs_a_full_table_driver_a_session_and_a_real_table() {
    let mut scene = Scene::default();
    assert!(scene.vm.allow_entire(), "full driver + session + real table");

    scene.vm.db_session_id = None;
    assert!(!scene.vm.allow_entire(), "no session to stream through");
    scene.vm.db_session_id = Some("sess-1".to_owned());

    scene.vm.capability = DataExportCapability::LoadedOnly;
    assert!(!scene.vm.allow_entire(), "a loaded-only driver cannot pull tables");
    scene.vm.capability = DataExportCapability::FullTable;

    scene.vm.table_name = "data".to_owned();
    assert!(!scene.vm.allow_entire(), "the 'data' placeholder is no table");
    scene.vm.table_name = "users".to_owned();

    scene.vm.capability = DataExportCapability::None;
    assert!(!scene.vm.allow_entire(), "a locked driver exports nothing");
}

#[test]
fn the_filtered_scope_exists_exactly_when_its_builder_does() {
    let mut scene = Scene::default();
    assert!(!scene.vm.allow_filtered(), "no builder, no scope");

    scene.vm.has_filtered_builder = true;
    assert!(scene.vm.allow_filtered());

    scene.vm.db_session_id = None;
    assert!(!scene.vm.allow_filtered(), "the statement runs on a session");
    scene.vm.db_session_id = Some("sess-1".to_owned());

    scene.vm.capability = DataExportCapability::None;
    assert!(!scene.vm.allow_filtered(), "a locked driver offers no scope");
}

#[test]
fn an_impossible_scope_settles_back_on_the_page_scope() {
    // The entire scope the driver does not serve.
    let mut scene = Scene::default();
    scene.vm.scope = ExportScope::EntireTable;
    scene.vm.capability = DataExportCapability::LoadedOnly;
    assert_eq!(scene.vm.settled_scope(), ExportScope::CurrentPage);

    // The filtered scope whose builder disappeared.
    let mut scene = Scene::default();
    scene.vm.scope = ExportScope::FilteredTable;
    scene.vm.has_filtered_builder = false;
    assert_eq!(scene.vm.settled_scope(), ExportScope::CurrentPage);

    // A servable scope stays.
    let mut scene = Scene::default();
    scene.vm.scope = ExportScope::EntireTable;
    scene.vm.has_filtered_builder = true;
    assert_eq!(scene.vm.settled_scope(), ExportScope::EntireTable);
}

// ---- VM: the format fallback ---------------------------------------------------

#[test]
fn an_unavailable_format_settles_on_csv_but_a_served_one_stays() {
    // Parquet has no writer on the provided scopes.
    let mut scene = Scene::default();
    scene.vm.format = DataFormat::Parquet;
    assert_eq!(scene.vm.settled_format(), DataFormat::Csv);

    // SQL UPDATE needs a key column for its WHERE.
    scene.vm.format = DataFormat::SqlUpdate;
    scene.vm.primary_key_columns.clear();
    assert_eq!(scene.vm.settled_format(), DataFormat::Csv);

    // A served pick stays put.
    scene.vm.format = DataFormat::Json;
    assert_eq!(scene.vm.settled_format(), DataFormat::Json);

    // The key column restores SQL UPDATE on a provided scope.
    scene.vm.format = DataFormat::SqlUpdate;
    scene.vm.primary_key_columns = vec!["id".to_owned()];
    assert_eq!(scene.vm.settled_format(), DataFormat::SqlUpdate);
}

// ---- VM: the export-button gate -------------------------------------------------

#[test]
fn the_export_button_gates_on_runs_locks_empty_rows_and_empty_columns() {
    let mut scene = Scene::default();
    assert!(!scene.vm.export_disabled(), "a healthy form exports");

    scene.vm.status = DataExportStatus::Exporting;
    assert!(scene.vm.export_disabled(), "no double runs");
    scene.vm.status = DataExportStatus::Form;

    scene.vm.capability = DataExportCapability::None;
    assert!(scene.vm.export_disabled(), "a locked driver exports nothing");
    scene.vm.capability = DataExportCapability::FullTable;

    // An empty page blocks the page scope; the streaming scopes do not
    // check the loaded rows (the server has them).
    scene.vm.rows_len = 0;
    assert!(scene.vm.export_disabled());
    scene.vm.scope = ExportScope::EntireTable;
    assert!(!scene.vm.export_disabled(), "the server has the rows");
    scene.vm.scope = ExportScope::Selected;
    scene.vm.selected_rows_len = 0;
    assert!(scene.vm.export_disabled(), "an empty selection exports nothing");
    scene.vm.selected_rows_len = 5;

    // An empty column selection exports nothing (DBR-06).
    scene.vm.selected_cols.clear();
    assert!(scene.vm.export_disabled());
    // A selection scope answers the loaded rows, not the page rows.
    scene.vm.selected_cols = vec!["id".to_owned()];
    scene.vm.rows_len = 40;
    assert!(!scene.vm.export_disabled());
}

#[test]
fn the_row_count_follows_the_scope_it_answers() {
    let mut scene = Scene::default();
    // The page scope reports the loaded rows.
    assert_eq!(scene.vm.row_count(), 40);
    // The selected scope reports the selection.
    scene.vm.scope = ExportScope::Selected;
    assert_eq!(scene.vm.row_count(), 5);
    // The streaming scopes report the server's total, or the loaded rows
    // until the server answers.
    scene.vm.scope = ExportScope::EntireTable;
    assert_eq!(scene.vm.row_count(), 1_000_000);
    scene.vm.total_rows = None;
    assert_eq!(scene.vm.row_count(), 40);
}

#[test]
fn the_picked_columns_keep_the_display_order() {
    // A scrambled selection still answers in the grid's column order —
    // the order the export writes (DBR-06).
    let mut scene = Scene::default();
    scene.vm.selected_cols = vec!["email".to_owned(), "id".to_owned()];
    assert_eq!(scene.vm.selected_column_names(), ["id", "email"]);
}

// ---- options: fresh vectors ------------------------------------------------------

#[test]
fn three_disabled_reasons_dedupe_in_first_appearance_order() {
    // Web, a streaming scope, and no key column: every rule fires. The list
    // order is csv / tsv / json / markdown / xlsx / parquet / sql_insert /
    // sql_update, so the web reason appears first (tsv), parquet's
    // rust-only rule second, and the no-key rule last (sql_update).
    let options = build_format_options(ExportScope::EntireTable, false, false);
    assert_eq!(
        disabled_reason_keys(&options),
        [
            "export.formatDisabledWebSqlScope",
            "export.formatDisabledParquetRustOnly",
            "export.formatDisabledNoKey"
        ]
    );
}

// ---- mounted dialog -------------------------------------------------------------

#[test]
fn the_dialog_mounts_its_parts_and_the_summary_answers_the_live_counts() {
    let rt = runtime(Scene::default(), (1440.0, 816.0));

    for key in [
        KEY,
        &format!("{KEY}-title"),
        &format!("{KEY}-format-field-trigger"),
        &format!("{KEY}-range-field-trigger"),
        &format!("{KEY}-columns-list"),
        &format!("{KEY}-summary"),
        &format!("{KEY}-footer-cancel"),
        &format!("{KEY}-footer-export"),
    ] {
        assert!(maybe(&rt, key).is_some(), "{key} is mounted");
    }
    // The picker rows sit under the re-keyed prefix, PK chip on the key
    // column, and the type label next to the name.
    assert!(has(&rt, &format!("{KEY}-columns-column-id-pk")));
    assert_eq!(text_of(&rt, &format!("{KEY}-columns-column-id-type")), "integer");

    // The summary: 40 rows on the page scope, 3 picked columns, CSV.
    let summary = text_of(&rt, &format!("{KEY}-summary-line"));
    let expected_line = t("export.willExport")
        .replace("{rows}", "40")
        .replace("{cols}", "3");
    assert_eq!(summary, format!("{expected_line}, {}", t("export.formatAs")));
    assert_eq!(
        text_of(&rt, &format!("{KEY}-summary-format")),
        "CSV",
        "the fallback format labels the summary"
    );

    // No error is mounted on a clean form, and no locked warning either.
    assert!(!has(&rt, &format!("{KEY}-error")));
    assert!(!has(&rt, &format!("{KEY}-locked-warning")));
    // The description carries the real table's name (React shows it only for
    // a real table; the header's description text has no key of its own).
    assert!(has_text(
        &rt,
        &t("export.description").replace("{table}", "users")
    ));
}

#[test]
fn an_export_press_hands_the_host_the_complete_request() {
    let mut scene = Scene::default();
    scene.vm.selected_cols = vec!["name".to_owned(), "id".to_owned()];
    let mut rt = runtime(scene, (1440.0, 816.0));

    press(&mut rt, &format!("{KEY}-footer-export"));
    let requests = &rt.state.exports;
    assert_eq!(requests.len(), 1, "exactly one request");
    let request = &requests[0];
    assert_eq!(request.scope, ExportScope::CurrentPage);
    assert_eq!(request.format, DataFormat::Csv);
    // Display order, not selection order.
    assert_eq!(
        request.columns,
        vec!["id".to_owned(), "name".to_owned()]
    );
    assert_eq!(request.primary_key.as_deref(), Some("id"));
    assert_eq!(request.table_name, "users");
    assert_eq!(request.db_session_id.as_deref(), Some("sess-1"));
}

#[test]
fn a_locked_driver_mounts_the_warning_and_a_disabled_export_button() {
    let mut scene = Scene::default();
    scene.vm.capability = DataExportCapability::None;
    scene.vm.primary_key_columns.clear();
    scene.vm.is_desktop = false;
    scene.vm.scope = ExportScope::EntireTable;
    let rt = runtime(scene, (1440.0, 816.0));

    assert_eq!(
        text_of(&rt, &format!("{KEY}-locked-warning-text")),
        t("export.disabledByDriver")
    );
    // The locked export button is disabled, and the raw keys never echo.
    assert!(!element(&rt, &format!("{KEY}-footer-export")).focusable);
    assert!(!has_text(&rt, "export.disabledByDriver"));
}

#[test]
fn web_streaming_without_a_key_column_mounts_three_reason_lines() {
    // A servable web streaming scope: full driver, live session, real table.
    let mut scene = Scene::default();
    scene.vm.capability = DataExportCapability::FullTable;
    scene.vm.primary_key_columns.clear();
    scene.vm.is_desktop = false;
    scene.vm.scope = ExportScope::EntireTable;
    let rt = runtime(scene, (1440.0, 816.0));

    // Web + streaming + no key: three reason lines, in the option list's
    // first-appearance order (web first at tsv, parquet's rust-only rule
    // second, the no-key rule last at sql_update).
    assert_eq!(
        text_of(&rt, &format!("{KEY}-format-reason-0")),
        t("export.formatDisabledWebSqlScope")
    );
    assert_eq!(
        text_of(&rt, &format!("{KEY}-format-reason-1")),
        t("export.formatDisabledParquetRustOnly")
    );
    assert_eq!(
        text_of(&rt, &format!("{KEY}-format-reason-2")),
        t("export.formatDisabledNoKey")
    );
    assert!(!has(&rt, &format!("{KEY}-format-reason-3")));
    // The raw i18n keys never echo on screen.
    assert!(!has_text(&rt, "export.formatDisabledWebSqlScope"));
    assert!(!has_text(&rt, "export.formatDisabledNoKey"));
}

#[test]
fn the_aborted_state_reports_the_partial_files_fate_both_ways() {
    for deleted in [true, false] {
        let mut scene = scene_with_status(DataExportStatus::Aborted);
        scene.vm.partial_deleted = deleted;
        let rt = runtime(scene, (1440.0, 816.0));

        let expected = if deleted {
            t("export.cancelledPartialDeleted")
        } else {
            t("export.cancelledPartialKept")
        };
        // The two messages are the two fates; check the exact one.
        let actual = text_of(&rt, &format!("{KEY}-aborted-text"));
        assert_eq!(actual, expected, "partial_deleted = {deleted}");
        assert!(has(&rt, &format!("{KEY}-footer-close")));
        assert!(!has(&rt, &format!("{KEY}-footer-export")));
    }
}

#[test]
fn the_success_state_mounts_the_panel_and_closes_from_the_footer() {
    let mut scene = scene_with_status(DataExportStatus::Success);
    let mut rt = runtime(scene, (1440.0, 816.0));

    assert_eq!(text_of(&rt, &format!("{KEY}-success-message")), t("export.success"));
    assert!(has(&rt, &format!("{KEY}-success-icon")));
    assert!(!has(&rt, &format!("{KEY}-format-field-trigger")), "the form is gone");

    press(&mut rt, &format!("{KEY}-footer-close"));
    assert_eq!(rt.state.closes, 1, "the settled footer closes");
    assert!(!rt.state.vm.open);
}

#[test]
fn escape_closes_the_dialog_even_mid_run_like_the_react_onclose() {
    let mut scene = scene_with_status(DataExportStatus::Exporting);
    let mut rt = runtime(scene, (1440.0, 816.0));
    escape(&mut rt);
    assert_eq!(rt.state.closes, 1);
    assert!(!rt.state.vm.open);
}

#[test]
fn the_dialog_keeps_unique_keys_and_stays_inside_both_windows_in_both_themes() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            for (label, scene) in [
                ("form", Scene::default()),
                ("exporting", scene_with_status(DataExportStatus::Exporting)),
                ("success", scene_with_status(DataExportStatus::Success)),
                ("aborted", scene_with_status(DataExportStatus::Aborted)),
            ] {
                let mut scene = scene;
                scene.dark = dark;
                let rt = runtime(scene, (w, h));

                let mut seen = std::collections::HashSet::new();
                let mut dupes = Vec::new();
                for e in rt.elements() {
                    // Kit-internal auto keys ("control", "label", …) live
                    // outside the dialog's prefix; only the dialog's own keys
                    // must be unique.
                    if let Some(k) = e.key.clone() {
                        if k.starts_with(KEY) && !seen.insert(k.clone()) {
                            dupes.push(k);
                        }
                    }
                }
                assert!(
                    dupes.is_empty(),
                    "{label} dark={dark} {w}x{h}: duplicate keys {dupes:?}"
                );

                let outside: Vec<String> = rt
                    .elements()
                    .iter()
                    .filter(|e| {
                        let r = e.rect;
                        !e.virtual_extent
                            && (r[0] < -0.5
                                || r[1] < -0.5
                                || r[0] + r[2] > w + 0.5
                                || r[1] + r[3] > h + 0.5)
                    })
                    .map(|e| format!("{:?}", e.key.clone().unwrap_or(e.path.clone())))
                    .collect();
                assert!(
                    outside.is_empty(),
                    "{label} dark={dark} {w}x{h}: out of window: {outside:?}"
                );
            }
        }
    }
}
