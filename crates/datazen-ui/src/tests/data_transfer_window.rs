//! T147-S3: the Data Transfer window's rendering — real probes against
//! the rendered element tree.
//!
//! Runtime tests render the window in a probe `Runtime` (the Backup
//! window's harness idiom: a `Probe` host state + a `view` closure) and
//! read back the laid-out element tree — never the component's own
//! report of itself. Every assertion is checked against the React
//! render body (`DataTransferWindow.tsx:1078-1899`): the chrome, the
//! step rail, each step's surface, the footer's per-step controls, and
//! the dialog layers.

use rune::{Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::{
    DataTransferWindow, DataTransferWindowEdits, DataTransferWindowVm, DdlKind, MigrationJobsHydration,
    STEPS, SqlFileCompression, SqlFileEncoding, TransferDdlPreviewVm, TransferDestinationMode,
    TransferMode, TransferPreviewVm, TransferWritePlanVm, WizardStep, WriteMode,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

/// The probe host: the window's own state plus the observability the
/// tests need (the copied DDL values and the callback flags).
struct Probe {
    vm: DataTransferWindowVm,
    edits: DataTransferWindowEdits,
    dark: bool,
    hydration: MigrationJobsHydration,
    copied_ddl: Vec<String>,
    pressed: Vec<&'static str>,
}

impl Probe {
    fn new(vm: DataTransferWindowVm) -> Self {
        let edits = DataTransferWindowEdits::new(&vm);
        Self {
            vm,
            edits,
            dark: true,
            hydration: MigrationJobsHydration::default(),
            copied_ddl: Vec::new(),
            pressed: Vec::new(),
        }
    }
}

fn press(p: &mut Probe, name: &'static str) {
    p.pressed.push(name);
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    DataTransferWindow::<Probe>::new(s.vm.clone(), &s.edits)
        .on_profile_save(|p| press(p, "profile-save"))
        .on_profile_load(|p| press(p, "profile-load"))
        .on_destination_database(|p| press(p, "destination-database"))
        .on_choose_sql_file(|p| press(p, "choose-sql-file"))
        .on_next(|p| press(p, "next"))
        .on_back(|p| press(p, "back"))
        .on_execute(|p| press(p, "execute"))
        .on_cancel(|p| press(p, "cancel"))
        .on_re_review(|p| press(p, "re-review"))
        .on_retry_preview(|p| press(p, "retry-preview"))
        .on_reinspect(|p| press(p, "reinspect"))
        .on_copy_ddl(|p, ddl| p.copied_ddl.push(ddl))
        .on_dismiss_limitations(|p| press(p, "dismiss-limitations"))
        .on_dismiss_confirm(|p| press(p, "dismiss-confirm"))
        .on_dismiss_error(|p| press(p, "dismiss-error"))
        .on_profile_name_edit(|p| p.edits.profile_name_edit())
        .on_batch_size_edit(|p| p.edits.batch_size_edit())
        .on_sql_file_database_edit(|p| p.edits.sql_file_database_edit())
        .on_sql_file_schema_edit(|p| p.edits.sql_file_schema_edit())
        .hydration(s.hydration.clone())
        .build(s, s.dark)
}

impl DataTransferWindowEdits {
    fn profile_name_edit(&mut self) -> &mut rune::TextEdit {
        &mut self.profile_name
    }

    fn batch_size_edit(&mut self) -> &mut rune::TextEdit {
        &mut self.batch_size
    }

    fn sql_file_database_edit(&mut self) -> &mut rune::TextEdit {
        &mut self.sql_file_database
    }

    fn sql_file_schema_edit(&mut self) -> &mut rune::TextEdit {
        &mut self.sql_file_schema
    }
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .any(|e| e.key.as_deref() == Some(key))
}

fn endpoints_vm() -> DataTransferWindowVm {
    let mut vm = DataTransferWindowVm::default();
    vm.apply_connections_loaded(vec![
        crate::views::TransferConnectionVm {
            id: "src".to_owned(),
            name: "Source".to_owned(),
            database_type: "mysql".to_owned(),
            database: Some("db1".to_owned()),
            schema: None,
            read_only: false,
        },
        crate::views::TransferConnectionVm {
            id: "tgt".to_owned(),
            name: "Target".to_owned(),
            database_type: "postgresql".to_owned(),
            database: Some("db2".to_owned()),
            schema: None,
            read_only: false,
        },
    ]);
    vm.source_id = "src".to_owned();
    vm.target_id = "tgt".to_owned();
    vm.source_database = "db1".to_owned();
    vm.target_database = "db2".to_owned();
    vm.source_session = Some(crate::views::DedicatedSessionVm {
        connection_id: "src".to_owned(),
        database: "db1".to_owned(),
        db_session_id: "s-src".to_owned(),
    });
    vm.target_session = Some(crate::views::DedicatedSessionVm {
        connection_id: "tgt".to_owned(),
        database: "db2".to_owned(),
        db_session_id: "s-tgt".to_owned(),
    });
    vm
}

fn preview_vm() -> TransferPreviewVm {
    TransferPreviewVm {
        plan_id: "plan-1".to_owned(),
        pairing_path: "ir".to_owned(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![
            TransferDdlPreviewVm {
                source_table: "t1".to_owned(),
                target_table: "t1".to_owned(),
                ddl: "CREATE TABLE t1".to_owned(),
                kind: Some(DdlKind::Table),
                depends_on: None,
            },
            TransferDdlPreviewVm {
                source_table: "t1".to_owned(),
                target_table: "t1".to_owned(),
                ddl: "CREATE INDEX i1".to_owned(),
                kind: Some(DdlKind::Index),
                depends_on: None,
            },
        ],
        write_plans: vec![TransferWritePlanVm {
            source_table: "t1".to_owned(),
            target_table: "t1".to_owned(),
            write_mode: WriteMode::Insert,
            mapped_columns: vec![],
            estimated_rows: Some(10),
            preamble: vec![],
            source_filter_preview: None,
            recordset_preview: None,
        }],
        warnings: vec!["w1".to_owned()],
        can_execute: true,
        block_reason: None,
    }
}

// ---------------------------------------------------------------------------
// The chrome, the rail, and the bounds (React 1079-1140).
// ---------------------------------------------------------------------------

#[test]
fn window_chrome_and_step_rail_lay_out() {
    let rt = runtime(Probe::new(DataTransferWindowVm::default()), DESIGN);
    element(&rt, "data-transfer-window");
    element(&rt, "data-transfer-window-chrome");
    // The title the chrome slot hands over (React 1081).
    element(&rt, "data-transfer-window-title");
    // All six rail cells are laid out with the React step ids.
    for step in STEPS {
        element(&rt, &format!("data-transfer-step-{}", step.as_key()));
    }
    // The current step is the first on a fresh window (React 126).
    element(&rt, "data-transfer-step-disc-endpoints");
}

#[test]
fn window_fits_both_design_sizes() {
    for size in [MIN, DESIGN] {
        let rt = runtime(Probe::new(DataTransferWindowVm::default()), size);
        let root = element(&rt, "data-transfer-window");
        let [x0, y0, x1, y1] = root.rect;
        assert!(
            x0 >= 0.0 && x1 <= size.0 && y1 <= size.1 + 0.5,
            "the window fits {size:?}: {:?}",
            root.rect
        );
    }
}

// ---------------------------------------------------------------------------
// The endpoints step (React 1149-1398).
// ---------------------------------------------------------------------------

#[test]
fn endpoints_step_renders_the_profile_row_and_destination_buttons() {
    let rt = runtime(Probe::new(endpoints_vm()), DESIGN);
    element(&rt, "data-transfer-profile-row");
    element(&rt, "data-transfer-profile-name");
    element(&rt, "data-transfer-profile-save");
    element(&rt, "data-transfer-profile-load");
    element(&rt, "data-transfer-profile-select-trigger");
    // The destination pair (React 1193-1210).
    element(&rt, "data-transfer-destination-database");
    element(&rt, "data-transfer-destination-sql-file");
    // The database destination mounts the endpoints bar with the
    // data-transfer prefix (MigrationEndpointsBar's key derivation).
    element(&rt, "data-transfer-source");
    element(&rt, "data-transfer-target");
}

#[test]
fn sql_file_destination_renders_the_card() {
    let mut vm = endpoints_vm();
    vm.destination_mode = TransferDestinationMode::SqlFile;
    vm.sql_file_target = Some(crate::views::TransferSqlFileTargetVm {
        file_token: "tok".to_owned(),
        ..crate::views::TransferSqlFileTargetVm::default()
    });
    vm.sql_file_encoding = SqlFileEncoding::Utf16Le;
    vm.sql_file_compression = SqlFileCompression::Gzip;
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-sql-file-card");
    element(&rt, "data-transfer-source-trigger");
    element(&rt, "data-transfer-source-database-trigger");
    element(&rt, "data-transfer-sql-file-dialect-trigger");
    element(&rt, "data-transfer-sql-file-encoding-trigger");
    element(&rt, "data-transfer-sql-file-compression-trigger");
    element(&rt, "data-transfer-sql-file-target-database");
    element(&rt, "data-transfer-sql-file-target-schema");
    element(&rt, "data-transfer-sql-file-hint");
    // The database destination's bar is not mounted.
    assert!(!has_key(&rt, "data-transfer-target"));
}

// ---------------------------------------------------------------------------
// The setup step (React 1402-1510).
// ---------------------------------------------------------------------------

#[test]
fn setup_step_renders_the_mode_group_and_the_options() {
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Setup;
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-setup-step");
    element(&rt, "data-transfer-mode");
    // The collation checkbox mounts for the data-only + insert default
    // only when the mode is not data or the write mode is drop-create —
    // the data + insert default hides it (React 1433).
    assert!(!has_key(&rt, "data-transfer-use-target-default-collation"));
    element(&rt, "data-transfer-write-mode-trigger");
    element(&rt, "data-transfer-batch-size");
    // A valid batch size hides the error banner (React 1489-1493).
    assert!(!has_key(&rt, "data-transfer-batch-size-error"));
    element(&rt, "data-transfer-resume-capability-hint");
    element(&rt, "data-transfer-stop-on-error");
    // The destructive confirm is hidden for the insert default (React
    // 1465).
    assert!(!has_key(&rt, "data-transfer-destructive-confirm"));
}

#[test]
fn setup_step_renders_the_destructive_arm() {
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Setup;
    // The collation arm React 1433 guards: mode !== 'data' OR writeMode
    // === 'dropCreateInsert' — the drop-create write mode shows both it
    // and the destructive confirm.
    vm.write_mode = WriteMode::DropCreateInsert;
    vm.confirmed_destructive = true;
    vm.use_target_default_collation = true;
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-use-target-default-collation");
    element(&rt, "data-transfer-destructive-confirm");
    // An invalid batch size mounts the error banner (React 1489-1493).
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Setup;
    vm.batch_size_edit = "501".to_owned();
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-batch-size-error");
}

// ---------------------------------------------------------------------------
// The objects step (React 1512-1555).
// ---------------------------------------------------------------------------

#[test]
fn objects_step_renders_the_empty_state_then_the_rows() {
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Objects;
    let rt = runtime(Probe::new(vm.clone()), DESIGN);
    element(&rt, "data-transfer-objects-empty");
    element(&rt, "data-transfer-reinspect");
    // Rows mount after the inspect, keyed by the source table.
    let row = |source: &str| crate::views::TransferTableRow {
        vm: crate::views::TransferTableVm {
            source_table: source.to_owned(),
            target_table: source.to_owned(),
            ..crate::views::TransferTableVm::default()
        },
        ..crate::views::TransferTableRow::default()
    };
    vm.apply_inspect_rows(vec![row("t1"), row("t2")]);
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-table-row-t1");
    element(&rt, "data-transfer-table-row-t2");
    element(&rt, "data-transfer-table-status-t1");
    assert!(!has_key(&rt, "data-transfer-objects-empty"));
}

// ---------------------------------------------------------------------------
// The mapping step (React 1557-1573).
// ---------------------------------------------------------------------------

#[test]
fn mapping_step_renders_the_seam_and_the_gate_notice() {
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Mapping;
    let rt = runtime(Probe::new(vm.clone()), DESIGN);
    element(&rt, "data-transfer-mapping-seam");
    // No gate error and no block reason: no notice.
    assert!(!has_key(&rt, "data-transfer-mapping-gate-notice"));
    vm.mapping_gate_error = "gate lost".to_owned();
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-mapping-gate-notice");
}

// ---------------------------------------------------------------------------
// The preview step (React 1575-1720).
// ---------------------------------------------------------------------------

#[test]
fn preview_step_renders_the_error_card_without_a_review() {
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Preview;
    vm.preview_error = "backend refused".to_owned();
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-preview-error");
    element(&rt, "data-transfer-preview-error-message");
    element(&rt, "data-transfer-preview-retry");
    element(&rt, "data-transfer-preview-back-mapping");
    assert!(!has_key(&rt, "data-transfer-preview"));
}

#[test]
fn preview_step_renders_the_review_body() {
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Preview;
    vm.preview = Some(preview_vm());
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-preview");
    // The table-kind DDL card (the T219 editor seam) and the index-kind
    // card (the plain pre body) both mount, with their copy buttons.
    element(&rt, "data-transfer-ddl-editor-t1");
    element(&rt, "data-transfer-copy-ddl-t1");
    element(&rt, "data-transfer-copy-ddl-t1-index");
    element(&rt, "data-transfer-ddl-preview-t1-index-1");
    // The write plan and the warning.
    element(&rt, "data-transfer-write-plan-t1");
    element(&rt, "data-transfer-preview-warning-0");
    // No executing overlay while not executing (React 1615).
    assert!(!has_key(&rt, "data-transfer-executing-overlay"));
}

#[test]
fn preview_step_renders_the_executing_overlay_and_the_sql_file_ddl() {
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Preview;
    vm.preview = Some(preview_vm());
    vm.executing = true;
    vm.execute_progress = "5 tables".to_owned();
    // The SQL-file destination renders the plain DDL body (React 1664-1670).
    vm.destination_mode = TransferDestinationMode::SqlFile;
    vm.sql_file_target = Some(crate::views::TransferSqlFileTargetVm {
        file_token: "tok".to_owned(),
        ..crate::views::TransferSqlFileTargetVm::default()
    });
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-executing-overlay");
    element(&rt, "data-transfer-executing-progress");
    element(&rt, "data-transfer-ddl-preview-t1-table-0");
    assert!(!has_key(&rt, "data-transfer-ddl-editor-t1"));
}

#[test]
fn preview_step_renders_the_cancel_disposition() {
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Preview;
    vm.preview = Some(preview_vm());
    vm.job_run.cancel_requested = true;
    vm.job_run.cancel_unknown_job = true;
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-cancel-disposition");
}

// ---------------------------------------------------------------------------
// The result step (React 1722-1728) and the footer (React 1759-1859).
// ---------------------------------------------------------------------------

#[test]
fn result_step_renders_the_seam_and_the_blocked_re_review() {
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Result;
    vm.job_run.apply_view = Some(crate::views::TransferApplyJobViewVm {
        job_id: "a1".to_owned(),
        kind: "dataTransferApply".to_owned(),
        state: crate::views::JobState::Succeeded,
        effect_outcome: Some("unknown".to_owned()),
        progress: crate::views::JobProgress::default(),
        plan_id: "plan-1".to_owned(),
        plan_digest: "digest-1".to_owned(),
        selection_revision: 1,
        commit_boundaries: Vec::new(),
        artifact_ids: Vec::new(),
        cancelled: false,
        partial: false,
        replayed: false,
        error: None,
        recovery_verdict: None,
        recovery_resume_through: None,
        recovery_reason: None,
    });
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-result-seam");
    element(&rt, "data-transfer-rereview");
    // An unknown outcome is uncertain: the re-review is blocked with the
    // notice (React 1838-1846).
    element(&rt, "data-transfer-rereview-blocked");
}

#[test]
fn footer_renders_the_step_appropriate_controls() {
    // The endpoints step: back (disabled) + next.
    let rt = runtime(Probe::new(DataTransferWindowVm::default()), DESIGN);
    element(&rt, "data-transfer-back");
    element(&rt, "data-transfer-next");
    assert!(!has_key(&rt, "data-transfer-execute"));
    assert!(!has_key(&rt, "data-transfer-cancel"));

    // The preview step with a review: the execute button (React 1808-1817).
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Preview;
    vm.preview = Some(preview_vm());
    vm.job_run.finish_prepare(crate::views::TransferPrepareJobViewVm {
        job_id: "p1".to_owned(),
        kind: "dataTransferPrepare".to_owned(),
        state: crate::views::JobState::Succeeded,
        effect_outcome: Some("completed".to_owned()),
        plan_id: "plan-1".to_owned(),
        plan_digest: "digest-1".to_owned(),
        plan_version: 1,
        handler_version: 1,
        checkpoint_version: 1,
        selection_revision: 1,
        expires_at: String::new(),
        can_execute: true,
        block_reason: None,
        review: TransferPreviewVm::default(),
    });
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-execute");
    element(&rt, "data-transfer-cancel");
    assert!(!has_key(&rt, "data-transfer-next"));

    // The result step: the re-review button (React 1829-1837).
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Result;
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-rereview");
    assert!(!has_key(&rt, "data-transfer-execute"));
}

#[test]
fn footer_renders_the_pending_id_notice_during_admission() {
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Preview;
    vm.preview = Some(preview_vm());
    vm.executing = true;
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-cancel-pending-id");
}

// ---------------------------------------------------------------------------
// The hydration hints and the strips (React 1085-1105).
// ---------------------------------------------------------------------------

#[test]
fn hydration_hints_render_when_reported() {
    let vm = DataTransferWindowVm::default();
    let mut probe = Probe::new(vm);
    probe.hydration = MigrationJobsHydration {
        active_jobs: Vec::new(),
        jobs: Vec::new(),
        verification_pending: true,
        hydration_error: Some("hydration failed".to_owned()),
    };
    let rt = runtime(probe, DESIGN);
    element(&rt, "data-transfer-pending-verification-hint");
    element(&rt, "data-transfer-hydration-error");
}

// ---------------------------------------------------------------------------
// The dialogs (React 1863-1897).
// ---------------------------------------------------------------------------

#[test]
fn error_dialog_mounts_in_the_layer_when_open() {
    let mut vm = endpoints_vm();
    vm.step = WizardStep::Preview;
    vm.raise_error("boom");
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-error-dialog-title");
    element(&rt, "data-transfer-error");
    element(&rt, "data-transfer-error-ok");
}

#[test]
fn limitations_and_confirm_seams_mount_while_open() {
    let mut vm = endpoints_vm();
    vm.limitations_open = true;
    vm.execute_confirm_open = true;
    let rt = runtime(Probe::new(vm), DESIGN);
    element(&rt, "data-transfer-limitations");
    element(&rt, "data-transfer-execute-confirm-seam");
}

// ---------------------------------------------------------------------------
// The status bar seam and the i18n keys the window asks for.
// ---------------------------------------------------------------------------

#[test]
fn status_bar_seam_renders() {
    let rt = runtime(Probe::new(DataTransferWindowVm::default()), DESIGN);
    element(&rt, "data-transfer-status-bar-seam");
}

#[test]
fn every_key_the_window_asks_for_exists() {
    // The chrome title and the buttons' labels resolve (React 1081 and
    // the footer's labels).
    for key in [
        "common.dataTransfer",
        "transfer.back",
        "transfer.next",
        "transfer.execute",
        "transfer.cancel",
        "transfer.error",
        "common.ok",
        "migration.verdict.rereview",
        "migration.cancel.unknownJob",
    ] {
        assert!(crate::i18n::has(key), "the window asks for {key}");
    }
    let _ = t("common.dataTransfer");
}
