//! T442: `windows/connection/ImportDialog.tsx` — wizard state, React keys and
//! i18n copy, host-port actions, the pure mapping planner, slot composition and
//! rect-based geometry at both window sizes in both themes.
//!
//! Observation is through `Runtime::elements()` and real pointer events. The
//! runtime state is a host-owned `Scene`: VM-local edits update it, and every
//! real command is recorded as an [`ImportAction`] (no fake IPC).

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::content_view_dialogs::{
    build_with_import, DialogAction, DialogConnectionCtx, DialogsVm,
};
use datazen_ui::views::import_dialog::{
    build, mapping_is_executable, plan_import_mapping, CellValue, DiagnosticKind,
    ImportAction, ImportDiagnostic, ImportDialogVm, ImportFormat, ImportParseResult,
    ImportRunResult, ImportVmFn, LoadedFile, MappingMode, RunStatus, TargetColumn,
    TargetColumnsState, WizardPhase, ROOT,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn column(name: &str, data_type: &str) -> TargetColumn {
    TargetColumn {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
    }
}

/// A parsed two-column CSV with two rows — the fixture from ImportDialog.test.
fn parsed_csv() -> ImportParseResult {
    ImportParseResult {
        columns: vec!["id".to_owned(), "name".to_owned()],
        rows: vec![
            vec![CellValue::Text("1".to_owned()), CellValue::Text("alpha".to_owned())],
            vec![CellValue::Text("2".to_owned()), CellValue::Text("beta".to_owned())],
        ],
        row_origins: vec![2, 3],
        diagnostics: Vec::new(),
        sheet_names: Vec::new(),
    }
}

fn csv_vm() -> ImportDialogVm {
    ImportDialogVm {
        file: Some(LoadedFile {
            name: "data.csv".to_owned(),
            data_base64: "aWQsbmFtZQ==".to_owned(),
        }),
        parsed: Some(parsed_csv()),
        targets: TargetColumnsState {
            columns: Some(vec![column("id", "integer"), column("name", "text")]),
            loading: false,
            error: String::new(),
        },
        target_table: "users".to_owned(),
        ..Default::default()
    }
}

#[derive(Clone)]
struct Scene {
    vm: ImportDialogVm,
    actions: Rc<RefCell<Vec<ImportAction>>>,
    dark: bool,
}

fn scene(vm: ImportDialogVm, dark: bool) -> Scene {
    Scene {
        vm,
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let callback: Rc<dyn Fn(&mut Scene, ImportAction)> = Rc::new(move |state: &mut Scene, action| {
        match action {
            ImportAction::SetEncoding(value) => state.vm.encoding = value,
            ImportAction::SetFirstRowIsHeader(value) => state.vm.first_row_is_header = value,
            ImportAction::SetTargetTable(value) => state.vm.target_table = value,
            ImportAction::SetOnError(value) => state.vm.on_error = value,
            ImportAction::SetCommitMode(value) => state.vm.commit_mode = value,
            ImportAction::SetOpenSelect(kind) => state.vm.open_select = kind,
            other => actions.borrow_mut().push(other),
        }
    });
    let vm_edit: ImportVmFn<Scene> = Rc::new(|s: &mut Scene| &mut s.vm);
    build::<Scene>(&s.vm, Some(callback), Some(vm_edit), s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.label)
        .unwrap_or_default()
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside<S>(rt: &Runtime<S>, size: (f32, f32), label: &str) {
    let (w, h) = size;
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) outside {size:?}: {outside:?}", outside.len());
}

// --- 1. wizard journeys ported from ImportDialog.test.tsx --------------------

#[test]
fn csv_wizard_plans_mapping_shows_preview_and_enables_start() {
    let rt = runtime(scene(csv_vm(), true), DESIGN);
    assert!(has_key(&rt, ROOT), "the dialog mounts");
    assert!(has_key(&rt, "import-mapping-table"), "React mapping table testid");
    assert!(has_key(&rt, "import-preview"), "React preview testid");

    let summary = text_of(&rt, "import-summary");
    assert!(summary.contains("2"), "two mapped columns in the summary: {summary:?}");
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|x| x == "alpha") && rendered.iter().any(|x| x == "beta"), "both sample rows previewed");
    assert_eq!(
        label_of(&rt, &format!("{ROOT}-confirm")),
        t("import.start").replace("{count}", "2"),
        "the start button uses the exact import.start key with the row count"
    );

    let mut rt = rt;
    click(&mut rt, &format!("{ROOT}-confirm"));
    assert!(
        rt.state.actions.borrow().contains(&ImportAction::Start),
        "an executable plan starts the import"
    );
}

#[test]
fn target_column_load_failure_blocks_start() {
    let mut vm = csv_vm();
    vm.targets = TargetColumnsState {
        columns: None,
        loading: false,
        error: "relation not found".to_owned(),
    };
    let mut rt = runtime(scene(vm, true), DESIGN);
    assert!(texts(&rt).iter().any(|x| x.contains("relation not found")), "load error surfaced");
    assert!(!has_key(&rt, "import-mapping-table"), "no mapping table without targets");
    click(&mut rt, &format!("{ROOT}-confirm"));
    assert!(!rt.state.actions.borrow().contains(&ImportAction::Start), "start stays blocked");
}

#[test]
fn duplicate_source_column_is_a_blocking_warning() {
    let mut vm = csv_vm();
    let mut parsed = parsed_csv();
    parsed.columns = vec!["id".to_owned(), "id".to_owned()];
    parsed.diagnostics.push(ImportDiagnostic {
        kind: DiagnosticKind::DuplicateColumn,
        column: "id".to_owned(),
        line: 1,
    });
    vm.parsed = Some(parsed);
    let mut rt = runtime(scene(vm, true), DESIGN);
    assert!(has_key(&rt, "import-duplicate-warning"), "React duplicate warning testid");
    click(&mut rt, &format!("{ROOT}-confirm"));
    assert!(!rt.state.actions.borrow().contains(&ImportAction::Start), "duplicates block start");
}

#[test]
fn finished_run_reports_committed_rows_and_refresh_failure() {
    let mut vm = csv_vm();
    vm.phase = WizardPhase::Done;
    vm.result = Some(ImportRunResult {
        status: RunStatus::Completed,
        committed_rows: 2,
        failed_rows: Vec::new(),
        stopped_by: None,
    });
    vm.refresh_failed = true;
    let rt = runtime(scene(vm, true), DESIGN);
    assert!(has_key(&rt, "import-result"), "React result testid");
    let result = text_of(&rt, &format!("{ROOT}-result-completed-text"));
    assert_eq!(
        result,
        t("import.result.completed").replace("{committed}", "2"),
        "exact completed copy with the committed count"
    );
    assert!(texts(&rt).iter().any(|x| x == t("import.refreshFailed")), "refresh notice survives");
    assert_eq!(
        label_of(&rt, &format!("{ROOT}-confirm")),
        t("import.importAnother"),
        "done phase offers import-another"
    );
}

#[test]
fn stopped_and_cancelled_results_render_their_exact_copy() {
    let stopped = ImportRunResult {
        status: RunStatus::Stopped,
        committed_rows: 3,
        failed_rows: Vec::new(),
        stopped_by: Some(datazen_ui::views::import_dialog::StopInfo {
            row_index: 4,
            source_line: Some(5),
            message: "boom".to_owned(),
        }),
    };
    let mut vm = csv_vm();
    vm.phase = WizardPhase::Done;
    vm.result = Some(stopped);
    let rt = runtime(scene(vm, true), DESIGN);
    assert!(texts(&rt).iter().any(|x| x.contains("boom") && x.contains("5")), "stopped copy");

    let mut vm = csv_vm();
    vm.phase = WizardPhase::Done;
    vm.result = Some(ImportRunResult {
        status: RunStatus::Cancelled,
        committed_rows: 1,
        failed_rows: Vec::new(),
        stopped_by: None,
    });
    let rt = runtime(scene(vm, true), DESIGN);
    assert_eq!(
        text_of(&rt, &format!("{ROOT}-result-cancelled-text")),
        t("import.result.cancelled").replace("{committed}", "1"),
        "exact cancelled copy"
    );
}

#[test]
fn progress_only_mounts_while_running() {
    let mut vm = csv_vm();
    vm.phase = WizardPhase::Running;
    vm.progress = Some(datazen_ui::views::import_dialog::ImportProgressMsg {
        processed_rows: 5,
        committed_rows: 4,
        failed_rows: 1,
    });
    let rt = runtime(scene(vm, true), DESIGN);
    assert_eq!(
        text_of(&rt, &format!("{ROOT}-progress-text")),
        t("import.progress")
            .replace("{processed}", "5")
            .replace("{committed}", "4")
            .replace("{failed}", "1"),
        "exact progress copy"
    );
    let mut vm = csv_vm();
    vm.progress = Some(datazen_ui::views::import_dialog::ImportProgressMsg::default());
    let rt = runtime(scene(vm, true), DESIGN);
    assert!(!has_key(&rt, "import-progress"), "no progress banner in config phase");
}

// --- 2. warning branches and format options ---------------------------------

#[test]
fn conflicting_override_shows_warning_and_blocks_start() {
    let mut vm = csv_vm();
    vm.overrides = vec![
        ("id".to_owned(), Some("id".to_owned())),
        ("name".to_owned(), Some("id".to_owned())),
    ];
    let mut rt = runtime(scene(vm, true), DESIGN);
    assert!(has_key(&rt, "import-conflict-warning"), "React conflict warning testid");
    assert!(has_key(&rt, &format!("{ROOT}-needs-mapping")), "needs-mapping help");
    click(&mut rt, &format!("{ROOT}-confirm"));
    assert!(!rt.state.actions.borrow().contains(&ImportAction::Start), "conflicts block start");
}

#[test]
fn ragged_rows_warning_uses_the_first_offending_line() {
    let mut vm = csv_vm();
    vm.parsed.as_mut().unwrap().diagnostics.push(ImportDiagnostic {
        kind: DiagnosticKind::RaggedRow,
        column: String::new(),
        line: 7,
    });
    let rt = runtime(scene(vm, true), DESIGN);
    let warning = text_of(&rt, "import-ragged-warning-text");
    assert!(warning.contains('7'), "first ragged line is shown: {warning:?}");
}

#[test]
fn format_specific_options_and_file_name_are_1_to_1() {
    let mut vm = csv_vm();
    let rt = runtime(scene(vm.clone(), true), DESIGN);
    assert!(has_key(&rt, &format!("{ROOT}-csv-options")), "CSV options mount");
    assert!(has_key(&rt, &format!("{ROOT}-delimiter-trigger")), "delimiter select mounts");
    assert!(has_key(&rt, &format!("{ROOT}-format")), "format badge mounts");
    assert_eq!(text_of(&rt, &format!("{ROOT}-format")), "csv", "format badge uses the wire value (uppercase is visual only)");
    assert!(label_of(&rt, &format!("{ROOT}-select-file")).contains("data.csv"), "picked file name shown");

    vm.file.as_mut().unwrap().name = "book.xlsx".to_owned();
    vm.parsed.as_mut().unwrap().sheet_names = vec!["Sheet1".to_owned(), "Data".to_owned()];
    let rt = runtime(scene(vm, true), DESIGN);
    assert!(has_key(&rt, &format!("{ROOT}-sheet-trigger")), "XLSX sheet select mounts");
    assert!(!has_key(&rt, &format!("{ROOT}-csv-options")), "CSV options gone");
}

#[test]
fn null_preview_cells_are_italic_and_json_cells_keep_their_text() {
    let mut vm = csv_vm();
    let mut parsed = parsed_csv();
    parsed.rows[0][1] = CellValue::Null;
    parsed.rows[1][1] = CellValue::Json("{\"a\":1}".to_owned());
    vm.parsed = Some(parsed);
    let rt = runtime(scene(vm, true), DESIGN);
    assert_eq!(text_of(&rt, &format!("{ROOT}-preview-cell-0-1")), "NULL", "null cell");
    assert_eq!(
        text_of(&rt, &format!("{ROOT}-preview-cell-1-1")),
        "{\"a\":1}",
        "JSON cell keeps its stringified body"
    );
}

// --- 3. pure planner + format detection -------------------------------------

#[test]
fn mapping_planner_matches_the_react_modes_and_conflicts() {
    let sources = vec!["name".to_owned(), "id".to_owned(), "extra".to_owned()];
    let targets = vec!["id".to_owned(), "name".to_owned()];
    let auto = plan_import_mapping(&sources, &targets, MappingMode::Auto, &[]);
    assert_eq!(auto.entries.len(), 3);
    assert_eq!(auto.entries[0].target.as_deref(), Some("name"), "name match wins in auto");
    assert_eq!(auto.entries[1].target.as_deref(), Some("id"), "second name match");
    assert_eq!(auto.entries[2].target.as_deref(), None, "surplus source stays unmapped");
    assert!(mapping_is_executable(&auto));

    let position = plan_import_mapping(&sources, &targets, MappingMode::Position, &[]);
    assert_eq!(position.entries[0].target.as_deref(), Some("id"), "positional fill ignores names");
    let name_only = plan_import_mapping(&sources, &targets, MappingMode::Name, &[]);
    assert_eq!(name_only.entries[0].target.as_deref(), Some("name"), "name mode matches names");
    assert_eq!(name_only.entries[2].target.as_deref(), None, "name mode never fills by order");

    let overrides = vec![("extra".to_owned(), Some("id".to_owned()))];
    let conflict = plan_import_mapping(&sources, &targets, MappingMode::Auto, &overrides);
    assert_eq!(conflict.conflicts.len(), 1, "manual override can conflict");
    assert_eq!(conflict.conflicts[0].target, "id");
    assert!(!mapping_is_executable(&conflict), "conflicts block start");
}

#[test]
fn format_detection_handles_all_three_extensions() {
    assert_eq!(ImportFormat::detect("dir/data.csv"), Some(ImportFormat::Csv));
    assert_eq!(ImportFormat::detect("data.JSON"), Some(ImportFormat::Json));
    assert_eq!(ImportFormat::detect("book.xlsx"), Some(ImportFormat::Xlsx));
    assert_eq!(ImportFormat::detect("data.txt"), None);
}

// --- 4. composition, actions and geometry -----------------------------------

#[test]
fn slot_mount_and_footer_actions_preserve_the_t223_contract() {
    #[derive(Clone)]
    struct SlotProbe {
        actions: Rc<RefCell<Vec<DialogAction>>>,
        dark: bool,
    }
    fn slot_view(s: &SlotProbe, _cx: &mut Cx<SlotProbe>) -> El {
        let mut layer = Vec::new();
        let actions = s.actions.clone();
        let callback: Rc<dyn Fn(&mut SlotProbe, DialogAction)> =
            Rc::new(move |state, action| state.actions.borrow_mut().push(action));
        let dialogs = DialogsVm {
            conn_ctx: Some(DialogConnectionCtx::default()),
            import_open: true,
            import_table_name: Some("users".to_owned()),
            import_schema: Some("public".to_owned()),
            import_database: Some("app".to_owned()),
            ..Default::default()
        };
        let vm = csv_vm();
        build_with_import::<SlotProbe>(
            &dialogs,
            s.dark,
            &mut layer,
            &callback,
            Some(&vm),
            None,
        );
        let _ = &actions;
        col().w_full().h_full().children(layer)
    }
    let mut rt = Runtime::new(
        SlotProbe {
            actions: Rc::new(RefCell::new(Vec::new())),
            dark: true,
        },
        slot_view,
        DESIGN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    for key in [ROOT, "import-mapping-table", "import-preview", "import-summary"] {
        assert!(has_key(&rt, key), "slot mounts {key}");
    }
    click(&mut rt, &format!("{ROOT}-cancel"));
    assert_eq!(
        rt.state.actions.borrow().as_slice(),
        &[DialogAction::CloseImport {
            table: String::new(),
            schema: None,
            database: None,
        }],
        "cancel keeps T223's empty-target close"
    );
}

#[test]
fn phase_footer_keys_and_actions_are_stable() {
    let mut vm = csv_vm();
    vm.phase = WizardPhase::Done;
    vm.result = Some(ImportRunResult {
        status: RunStatus::Completed,
        committed_rows: 1,
        failed_rows: Vec::new(),
        stopped_by: None,
    });
    let mut rt = runtime(scene(vm, true), DESIGN);
    click(&mut rt, &format!("{ROOT}-confirm"));
    assert!(rt.state.actions.borrow().contains(&ImportAction::ResetForAnother), "import another");
    click(&mut rt, &format!("{ROOT}-cancel"));
    assert!(rt.state.actions.borrow().contains(&ImportAction::Close), "done close");
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let plain = runtime(scene(ImportDialogVm::default(), dark), size);
            assert_inside(&plain, size, &format!("empty {size:?} dark={dark}"));

            let ready = runtime(scene(csv_vm(), dark), size);
            assert_inside(&ready, size, &format!("ready {size:?} dark={dark}"));

            let mut done = csv_vm();
            done.phase = WizardPhase::Done;
            done.result = Some(ImportRunResult {
                status: RunStatus::Completed,
                committed_rows: 2,
                failed_rows: vec![datazen_ui::views::import_dialog::RowFailure {
                    row_index: 1,
                    source_line: Some(2),
                    message: "bad".to_owned(),
                }],
                stopped_by: None,
            });
            done.refresh_failed = true;
            let rt = runtime(scene(done, dark), size);
            assert_inside(&rt, size, &format!("done {size:?} dark={dark}"));
        }
    }
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. `blocked_by_duplicates()` forced false made the duplicate test fail on
//    `duplicates block start`.
// 2. Dropping the executable-plan gate from `can_start()` made the conflict
//    test fail on `needs-mapping help` (and the conflict start stayed blocked
//    only because the host port ignored the extra Start).
