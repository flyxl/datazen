//! T016: the batch-export dialog port
//! (`components/connection/batch_export_dialog.rs`).
//!
//! Real probes against a mounted modal: open/close (Escape, header, footer),
//! the table list, format selection, mode/output options, the export request,
//! the four body states, the copyable error, unique keys, i18n (no key echo)
//! and window bounds at 960×640 / 1440×816 in both themes. The scene holds the
//! dialog VM in the app state `S`, exactly as a real host would.

use rune::{clipboard, col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Tx};

use crate::components::connection::batch_export_dialog::{
    BatchExportDialog, BatchExportMode, BatchExportStatus, BatchExportVm, DataExportCapability,
    DataFormat, ExportRequest, ExportProgress, OutputMode,
};
use crate::i18n::t;

const KEY: &str = "batch-export-dialog";

#[derive(Clone)]
struct Scene {
    vm: BatchExportVm,
    dark: bool,
    // recorded callbacks
    closes: usize,
    toggled_tables: Vec<String>,
    select_all: usize,
    clear_all: usize,
    modes: Vec<BatchExportMode>,
    formats: Vec<DataFormat>,
    format_toggles: usize,
    format_closes: usize,
    outputs: Vec<OutputMode>,
    exports: Vec<ExportRequest>,
    cancels: usize,
    copies: usize,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            vm: BatchExportVm {
                open: true,
                tables: vec!["users".into(), "orders".into(), "products".into()],
                selected: vec!["users".into()],
                ..BatchExportVm::default()
            },
            dark: true,
            closes: 0,
            toggled_tables: Vec::new(),
            select_all: 0,
            clear_all: 0,
            modes: Vec::new(),
            formats: Vec::new(),
            format_toggles: 0,
            format_closes: 0,
            outputs: Vec::new(),
            exports: Vec::new(),
            cancels: 0,
            copies: 0,
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    BatchExportDialog::<Scene>::new(KEY, &scene.vm)
        .on_close(|s: &mut Scene| {
            s.closes += 1;
            s.vm.open = false;
        })
        .on_toggle_table(|s: &mut Scene, name| s.toggled_tables.push(name))
        .on_select_all(|s: &mut Scene| s.select_all += 1)
        .on_clear_all(|s: &mut Scene| s.clear_all += 1)
        .on_mode(|s: &mut Scene, mode| s.modes.push(mode))
        .on_data_format(|s: &mut Scene, format| s.formats.push(format))
        .on_format_open(|s: &mut Scene| {
            s.format_toggles += 1;
            s.vm.format_open = true;
        })
        .on_format_close(|s: &mut Scene| {
            s.format_closes += 1;
            s.vm.format_open = false;
        })
        .on_output_mode(|s: &mut Scene, output| s.outputs.push(output))
        .on_export(|s: &mut Scene, request| s.exports.push(request))
        .on_cancel_export(|s: &mut Scene| s.cancels += 1)
        .on_copy(|s: &mut Scene, _tx: &mut Tx| s.copies += 1)
        .build(scene.dark, &mut layer);
    col()
        .key("batch-page")
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
    rt.frame(0.0);
}

fn assert_in_window(rt: &Runtime<Scene>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            !e.virtual_extent
                && (r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5)
        })
        .map(|e| format!("{} {:?}", e.key.clone().unwrap_or(e.path.clone()), e.rect))
        .collect();
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

fn assert_unique_keys(rt: &Runtime<Scene>, label: &str) {
    let mut seen = std::collections::HashSet::new();
    let mut dupes = Vec::new();
    for e in rt.elements() {
        if let Some(k) = e.key {
            if k.starts_with(KEY) && !seen.insert(k.clone()) {
                dupes.push(k);
            }
        }
    }
    assert!(dupes.is_empty(), "{label}: duplicate keys {dupes:?}");
}

fn scene_with_status(status: BatchExportStatus) -> Scene {
    let mut scene = Scene::default();
    scene.vm.status = status;
    scene
}

// ---- mount / open / close / i18n ---------------------------------------------

#[test]
fn the_dialog_mounts_its_shell_and_closes_on_escape_header_and_footer() {
    let mut rt = runtime(Scene::default(), (1440.0, 816.0));
    for k in [
        KEY,
        &format!("{KEY}-title"),
        &format!("{KEY}-close"),
        &format!("{KEY}-body"),
        &format!("{KEY}-footer"),
        &format!("{KEY}-scrim"),
        &format!("{KEY}-form"),
        &format!("{KEY}-table-list"),
        &format!("{KEY}-mode-group"),
        &format!("{KEY}-format"),
        &format!("{KEY}-output-group"),
        &format!("{KEY}-footer-export"),
    ] {
        assert!(has(&rt, k), "expected key mounted: {k}");
    }
    assert_eq!(element(&rt, KEY).role, "dialog");

    escape(&mut rt);
    assert_eq!(rt.state.closes, 1, "Escape runs the close handler");
    assert!(!has(&rt, KEY), "the panel unmounts after Escape");
}

#[test]
fn the_header_close_and_footer_cancel_also_close_and_a_false_open_mounts_nothing() {
    let mut rt = runtime(Scene::default(), (1440.0, 816.0));
    press(&mut rt, &format!("{KEY}-close"));
    assert_eq!(rt.state.closes, 1, "the header close runs the handler");

    let mut rt = runtime(Scene::default(), (1440.0, 816.0));
    press(&mut rt, &format!("{KEY}-footer-cancel"));
    assert_eq!(rt.state.closes, 1, "the footer cancel runs the handler");

    let mut closed = Scene::default();
    closed.vm.open = false;
    let rt = runtime(closed, (1440.0, 816.0));
    assert!(!has(&rt, KEY), "a closed VM mounts nothing");
    assert!(!has(&rt, &format!("{KEY}-scrim")), "and no scrim");
}

#[test]
fn the_title_and_labels_are_translated_and_do_not_echo_keys() {
    let rt = runtime(Scene::default(), (1440.0, 816.0));
    assert_eq!(text_of(&rt, &format!("{KEY}-title")), t("batchExport.title"));
    assert_eq!(
        text_of(&rt, &format!("{KEY}-tables-label")),
        t("batchExport.selectTables")
    );
    assert!(
        has_text(&rt, t("batchExport.export")),
        "the primary action shows its translated label"
    );
    // No `t()` key literal is leaked into the rendered tree.
    let echoed = ["batchExport.title", "batchExport.selectTables", "common.selectAll"]
        .iter()
        .filter(|key| has_text(&rt, key))
        .copied()
        .collect::<Vec<_>>();
    assert!(echoed.is_empty(), "i18n keys echoed verbatim: {echoed:?}");
}

// ---- table list --------------------------------------------------------------

#[test]
fn the_table_list_renders_from_the_vm_and_reports_toggles_and_bulk_actions() {
    let mut rt = runtime(Scene::default(), (1440.0, 816.0));
    for name in ["users", "orders", "products"] {
        assert!(
            has(&rt, &format!("{KEY}-table-{name}")),
            "a checkbox is mounted for {name}"
        );
        assert!(
            has_text(&rt, name),
            "the table name {name} is visible"
        );
    }
    assert!(!has(&rt, &format!("{KEY}-table-extra")), "no extra table");

    press(&mut rt, &format!("{KEY}-table-orders"));
    assert_eq!(
        rt.state.toggled_tables,
        vec!["orders".to_string()],
        "toggling a row reports its table name"
    );

    press(&mut rt, &format!("{KEY}-select-all"));
    assert_eq!(rt.state.select_all, 1, "select-all reached the host");
    press(&mut rt, &format!("{KEY}-clear-all"));
    assert_eq!(rt.state.clear_all, 1, "clear-all reached the host");
}

#[test]
fn an_empty_table_list_shows_the_honest_empty_state_and_disables_export() {
    let mut scene = Scene::default();
    scene.vm.tables.clear();
    scene.vm.selected.clear();
    let rt = runtime(scene, (1440.0, 816.0));
    assert!(has(&rt, &format!("{KEY}-tables-empty")));
    assert_eq!(
        text_of(&rt, &format!("{KEY}-tables-empty")),
        t("batchExport.noTablesSelected")
    );
    assert!(
        !element(&rt, &format!("{KEY}-footer-export")).focusable,
        "export is disabled when nothing is selected"
    );
    assert!(
        !element(&rt, &format!("{KEY}-select-all")).focusable,
        "select-all is disabled with no tables"
    );
}

// ---- format selection --------------------------------------------------------

#[test]
fn choosing_a_format_opens_the_select_and_reports_the_stored_value() {
    let mut rt = runtime(Scene::default(), (1440.0, 816.0));
    // The kit trigger derives from the wrapper's field key.
    let trigger = format!("{KEY}-format-field-trigger");
    assert!(has(&rt, &trigger), "the format select trigger is mounted");
    press(&mut rt, &trigger);
    assert_eq!(rt.state.format_toggles, 1, "the trigger reports its toggle");
    assert!(
        has(&rt, &format!("{KEY}-format-field-content")),
        "the listbox is mounted once open"
    );

    // DATA_FORMAT_OPTIONS order: csv(0), tsv(1), json(2), ...
    press(&mut rt, "option1");
    assert_eq!(
        rt.state.formats,
        vec![DataFormat::Tsv],
        "choosing TSV reports the format value, not its label"
    );
    assert_eq!(rt.state.format_closes, 1, "the choice closes the listbox");
}

#[test]
fn web_offers_only_csv_and_structure_only_hides_the_format_block() {
    let mut web = Scene::default();
    web.vm.is_desktop = false;
    let mut rt = runtime(web, (1440.0, 816.0));
    press(&mut rt, &format!("{KEY}-format-field-trigger"));
    // Web has a single option, so index 0 is the only row.
    assert!(has(&rt, "option0"), "CSV is offered on web");
    assert!(!has(&rt, "option1"), "no second format is offered on web");
    press(&mut rt, "option0");
    assert_eq!(
        rt.state.formats,
        vec![DataFormat::Csv],
        "web can still choose CSV"
    );

    let mut structure = Scene::default();
    structure.vm.mode = BatchExportMode::StructureOnly;
    let rt = runtime(structure, (1440.0, 816.0));
    assert!(!has(&rt, &format!("{KEY}-format")), "React hides format in structure_only");
    assert!(!has(&rt, &format!("{KEY}-format-field-trigger")));
}

// ---- mode / output options ---------------------------------------------------

#[test]
fn mode_and_output_radios_report_their_choices_and_reflect_selection() {
    let mut rt = runtime(Scene::default(), (1440.0, 816.0));
    // The default mode is the third option (data_and_structure).
    assert!(
        element(&rt, &format!("{KEY}-mode-group-item-2")).item_selected,
        "the default mode is selected in the radio group"
    );
    press(&mut rt, &format!("{KEY}-mode-group-item-0"));
    assert_eq!(
        rt.state.modes,
        vec![BatchExportMode::StructureOnly],
        "choosing structure-only reports that mode"
    );

    // React defaults desktop output to zip (`IS_DESKTOP_BUILD ? 'zip' : 'single'`).
    assert!(
        element(&rt, &format!("{KEY}-output-group-item-1")).item_selected,
        "zip output is the desktop default"
    );
    press(&mut rt, &format!("{KEY}-output-group-item-0"));
    assert_eq!(
        rt.state.outputs,
        vec![OutputMode::Single],
        "choosing single-file reports that output mode"
    );
}

#[test]
fn zip_output_is_disabled_on_web() {
    let mut web = Scene::default();
    web.vm.is_desktop = false;
    let rt = runtime(web, (1440.0, 816.0));
    assert!(
        !element(&rt, &format!("{KEY}-output-group-item-1")).focusable,
        "zip is not selectable in a web build"
    );
}

// ---- export action -----------------------------------------------------------

#[test]
fn the_export_button_fires_once_with_the_complete_request() {
    let mut rt = runtime(Scene::default(), (1440.0, 816.0));
    press(&mut rt, &format!("{KEY}-footer-export"));
    assert_eq!(rt.state.exports.len(), 1, "the export callback fires exactly once");
    let request = &rt.state.exports[0];
    assert_eq!(request.selected_tables, vec!["users".to_string()]);
    assert_eq!(request.mode, BatchExportMode::DataAndStructure);
    assert_eq!(request.data_format, DataFormat::Csv);
    assert_eq!(request.output_mode, OutputMode::Zip);
}

#[test]
fn export_is_disabled_while_exporting_or_when_the_driver_locks_it() {
    let mut exporting = scene_with_status(BatchExportStatus::Exporting);
    let rt = runtime(exporting, (1440.0, 816.0));
    assert!(
        !element(&rt, &format!("{KEY}-footer-export")).focusable,
        "export is disabled while exporting"
    );

    let mut locked = Scene::default();
    locked.vm.capability = DataExportCapability::LoadedOnly;
    let rt = runtime(locked, (1440.0, 816.0));
    assert!(has(&rt, &format!("{KEY}-locked-warning")), "the lock notice shows");
    assert!(
        has_text(&rt, t("batchExport.disabledByDriver")),
        "the lock notice is translated"
    );
    assert!(
        !element(&rt, &format!("{KEY}-footer-export")).focusable,
        "export is disabled when the driver cannot export whole tables"
    );
}

#[test]
fn escape_does_not_dismiss_while_exporting() {
    let mut rt = runtime(scene_with_status(BatchExportStatus::Exporting), (1440.0, 816.0));
    escape(&mut rt);
    assert_eq!(rt.state.closes, 0, "Escape is ignored during an export");
    assert!(has(&rt, KEY), "the dialog stays mounted");
}

// ---- body states -------------------------------------------------------------

#[test]
fn success_and_both_aborted_states_render_distinct_messages() {
    let success = runtime(scene_with_status(BatchExportStatus::Success), (1440.0, 816.0));
    assert!(has(&success, &format!("{KEY}-success")));
    assert_eq!(
        text_of(&success, &format!("{KEY}-success-text")),
        t("batchExport.success")
    );
    assert!(has(&success, &format!("{KEY}-footer-close")));

    let mut kept = Scene::default();
    kept.vm.status = BatchExportStatus::Aborted;
    let rt = runtime(kept, (1440.0, 816.0));
    assert_eq!(
        text_of(&rt, &format!("{KEY}-aborted-text")),
        t("export.cancelledPartialKept")
    );

    let mut deleted = Scene::default();
    deleted.vm.status = BatchExportStatus::Aborted;
    deleted.vm.partial_deleted = true;
    let rt = runtime(deleted, (1440.0, 816.0));
    assert_eq!(
        text_of(&rt, &format!("{KEY}-aborted-text")),
        t("export.cancelledPartialDeleted")
    );
}

#[test]
fn exporting_renders_a_spinner_and_honest_progress() {
    let mut scene = scene_with_status(BatchExportStatus::Exporting);
    scene.vm.progress = Some(ExportProgress {
        current: 2,
        total: 4,
        table_name: Some("orders".into()),
        rows_written: Some(120),
    });
    let rt = runtime(scene, (1440.0, 816.0));
    assert!(has(&rt, &format!("{KEY}-spinner")));
    let label = text_of(&rt, &format!("{KEY}-exporting-text"));
    assert!(
        label.contains("2/4") && label.contains("orders"),
        "progress reports the current table: {label}"
    );
    assert!(has(&rt, &format!("{KEY}-progress-track")));
    assert!(has(&rt, &format!("{KEY}-progress-fill")));
    assert!(has_text(&rt, t("batchExport.exporting")));

    // No progress yet: no bar, just the exporting label.
    let bare = runtime(scene_with_status(BatchExportStatus::Exporting), (1440.0, 816.0));
    assert_eq!(
        text_of(&bare, &format!("{KEY}-exporting-text")),
        t("batchExport.exporting")
    );
    assert!(!has(&bare, &format!("{KEY}-progress-track")));
}

// ---- copyable error ----------------------------------------------------------

#[test]
fn the_error_branch_is_an_alert_and_copies_the_exact_message() {
    let clipboard = clipboard::memory();
    let mut scene = Scene::default();
    // Keep the form short so the error block is inside the dialog's scroll
    // viewport and the copy control is directly pressable.
    scene.vm.tables.clear();
    scene.vm.selected.clear();
    scene.vm.error = Some("disk full".to_string());
    let mut rt = runtime(scene, (1440.0, 816.0));
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);

    let root = element(&rt, &format!("{KEY}-error"));
    assert_eq!(root.role, "alert", "a non-mono error is announced");
    assert_eq!(text_of(&rt, &format!("{KEY}-error-text")), "disk full");
    press(&mut rt, &format!("{KEY}-error-copy"));
    assert_eq!(rt.state.copies, 1, "on_copy reached the host");
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("disk full"),
        "the exact error is copied"
    );
}

// ---- uniqueness / geometry ---------------------------------------------------

#[test]
fn the_dialog_keeps_unique_keys_and_stays_inside_both_windows_in_both_themes() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            for (label, scene) in [
                ("form", Scene::default()),
                ("exporting", scene_with_status(BatchExportStatus::Exporting)),
                ("success", scene_with_status(BatchExportStatus::Success)),
            ] {
                let mut scene = scene;
                scene.dark = dark;
                let rt = runtime(scene, (w, h));
                let context = format!("{label} dark={dark} {w}x{h}");
                assert_in_window(&rt, w, h, &context);
                assert_unique_keys(&rt, &context);
                let panel = element(&rt, KEY).rect;
                assert!(
                    panel[0] >= -0.5 && panel[0] + panel[2] <= w + 0.5,
                    "panel inside window: {panel:?}"
                );
            }
        }
    }
}

#[test]
fn key_matches_the_react_source_path() {
    assert_eq!(KEY, "batch-export-dialog");
}
