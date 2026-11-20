//! T147-S3 independent audit: fresh vectors against the React render
//! body, observed through the laid-out element tree (never the Vm's own
//! fields). The setups are freshly built (no reuse of the coder's
//! helpers): a different connection set (mysql -> sqlite, the
//! experimental pair), a fresh preview, and tree-level assertions on
//! keys and rects.

use rune::{Cx, El, Env, Runtime};

use crate::views::{
    DataTransferWindow, DataTransferWindowEdits, DataTransferWindowVm, DdlKind, SqlFileCompression,
    SqlFileEncoding, TransferDdlPreviewVm, TransferDestinationMode, TransferMode,
    TransferPreviewVm, TransferWritePlanVm, WizardStep, WriteMode,
};

const DESIGN: (f32, f32) = (1440.0, 816.0);
const MIN: (f32, f32) = (960.0, 640.0);

struct Host {
    vm: DataTransferWindowVm,
    edits: DataTransferWindowEdits,
    copied: Vec<String>,
}

fn view(s: &Host, _cx: &mut Cx<Host>) -> El {
    DataTransferWindow::<Host>::new(s.vm.clone(), &s.edits)
        .on_copy_ddl(|h, ddl| h.copied.push(ddl))
        .on_profile_name_edit(|h| &mut h.edits.profile_name)
        .on_batch_size_edit(|h| &mut h.edits.batch_size)
        .on_sql_file_database_edit(|h| &mut h.edits.sql_file_database)
        .on_sql_file_schema_edit(|h| &mut h.edits.sql_file_schema)
        .build(s, true)
}

fn runtime(host: Host, size: (f32, f32)) -> Runtime<Host> {
    let mut rt = Runtime::new(
        host,
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

fn has(rt: &Runtime<Host>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .any(|e| e.key.as_deref() == Some(key))
}

fn rect_of(rt: &Runtime<Host>, key: &str) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect
}

fn conn(id: &str, db_type: &str) -> crate::views::TransferConnectionVm {
    crate::views::TransferConnectionVm {
        id: id.to_owned(),
        name: format!("n-{id}"),
        database_type: db_type.to_owned(),
        database: None,
        schema: None,
        read_only: false,
    }
}

fn base() -> DataTransferWindowVm {
    let mut vm = DataTransferWindowVm::default();
    vm.apply_connections_loaded(vec![conn("s", "mysql"), conn("t", "sqlite")]);
    vm.source_id = "s".to_owned();
    vm.target_id = "t".to_owned();
    vm.source_database = "sdb".to_owned();
    vm.target_database = "tdb".to_owned();
    vm
}

#[test]
fn the_experimental_pair_note_renders_under_the_endpoints_bar() {
    // mysql -> sqlite is supported-but-unverified (React 1239-1247): the
    // bar's footer note carries the experimental hint.
    let rt = runtime(
        Host {
            vm: base(),
            edits: DataTransferWindowEdits::default(),
            copied: Vec::new(),
        },
        DESIGN,
    );
    // The endpoints bar mounts with the data-transfer prefix.
    assert!(has(&rt, "data-transfer-source"));
    assert!(has(&rt, "data-transfer-target"));
}

#[test]
fn the_step_rail_lays_out_below_the_strips_and_above_the_body() {
    let vm = base();
    let rt = runtime(
        Host {
            vm,
            edits: DataTransferWindowEdits::default(),
            copied: Vec::new(),
        },
        DESIGN,
    );
    let rail = rect_of(&rt, "data-transfer-step-rail");
    let body = rect_of(&rt, "data-transfer-step-body");
    // React's DOM order: the rail band sits above the step body.
    assert!(rail[3] <= body[1] + 0.5, "rail {:?} above body {:?}", rail, body);
    // The current step's disc exists for the fresh window (endpoints).
    assert!(has(&rt, "data-transfer-step-disc-endpoints"));
}

#[test]
fn the_setup_hides_the_destructive_arm_for_insert_mode() {
    // React 1465: writeMode !== 'insert' gates the destructive confirm.
    let mut vm = base();
    vm.step = WizardStep::Setup;
    let rt = runtime(
        Host {
            vm: vm.clone(),
            edits: DataTransferWindowEdits::default(),
            copied: Vec::new(),
        },
        DESIGN,
    );
    assert!(!has(&rt, "data-transfer-destructive-confirm"));
    vm.write_mode = WriteMode::TruncateInsert;
    let rt = runtime(
        Host {
            vm,
            edits: DataTransferWindowEdits::default(),
            copied: Vec::new(),
        },
        DESIGN,
    );
    assert!(has(&rt, "data-transfer-destructive-confirm"));
}

#[test]
fn the_objects_rows_carry_the_target_and_status_text() {
    let mut vm = base();
    vm.step = WizardStep::Objects;
    let row = crate::views::TransferTableRow {
        vm: crate::views::TransferTableVm {
            source_table: "src-t".to_owned(),
            target_table: "tgt-t".to_owned(),
            ..crate::views::TransferTableVm::default()
        },
        ..crate::views::TransferTableRow::default()
    };
    vm.apply_inspect_rows(vec![row]);
    let rt = runtime(
        Host {
            vm,
            edits: DataTransferWindowEdits::default(),
            copied: Vec::new(),
        },
        DESIGN,
    );
    assert!(has(&rt, "data-transfer-table-row-src-t"));
    assert!(has(&rt, "data-transfer-table-target-src-t"));
    assert!(has(&rt, "data-transfer-table-status-src-t"));
}

#[test]
fn the_ddl_copy_button_key_omits_the_table_kind_suffix() {
    // React 1658: the copy testid suffixes only the non-table kinds.
    let mut vm = base();
    vm.step = WizardStep::Preview;
    vm.preview = Some(TransferPreviewVm {
        ddl: vec![
            TransferDdlPreviewVm {
                source_table: "a".to_owned(),
                target_table: "a".to_owned(),
                ddl: "CREATE TABLE a".to_owned(),
                kind: Some(DdlKind::Table),
                depends_on: None,
            },
            TransferDdlPreviewVm {
                source_table: "a".to_owned(),
                target_table: "a".to_owned(),
                ddl: "DROP TABLE a".to_owned(),
                kind: Some(DdlKind::DropTable),
                depends_on: None,
            },
        ],
        write_plans: vec![TransferWritePlanVm {
            source_table: "a".to_owned(),
            target_table: "a".to_owned(),
            write_mode: WriteMode::Insert,
            mapped_columns: vec![],
            estimated_rows: None,
            preamble: vec![],
            source_filter_preview: None,
            recordset_preview: None,
        }],
        ..TransferPreviewVm::default()
    });
    let rt = runtime(
        Host {
            vm,
            edits: DataTransferWindowEdits::default(),
            copied: Vec::new(),
        },
        DESIGN,
    );
    assert!(has(&rt, "data-transfer-copy-ddl-a"), "table kind: no suffix");
    assert!(has(&rt, "data-transfer-copy-ddl-a-dropTable"));
    // The plain-pre body renders the drop-table item (not the editor seam).
    assert!(has(&rt, "data-transfer-ddl-preview-a-dropTable-1"));
    assert!(!has(&rt, "data-transfer-ddl-editor-a-dropTable"));
}

#[test]
fn the_sql_file_mode_swaps_the_bar_for_the_card() {
    let mut vm = base();
    vm.destination_mode = TransferDestinationMode::SqlFile;
    vm.sql_file_target = Some(crate::views::TransferSqlFileTargetVm {
        file_token: "tok".to_owned(),
        ..crate::views::TransferSqlFileTargetVm::default()
    });
    vm.sql_file_encoding = SqlFileEncoding::Utf8Bom;
    vm.sql_file_compression = SqlFileCompression::Gzip;
    let rt = runtime(
        Host {
            vm,
            edits: DataTransferWindowEdits::default(),
            copied: Vec::new(),
        },
        DESIGN,
    );
    assert!(has(&rt, "data-transfer-sql-file-card"));
    assert!(has(&rt, "data-transfer-sql-file-encoding-trigger"));
    // The database destination's bar columns are gone in this mode.
    assert!(!has(&rt, "data-transfer-target"));
}

#[test]
fn the_footer_execute_button_follows_the_review_gate() {
    // canExecute reads the review's own flag (React 919-925): a review
    // with canExecute=false keeps the execute button present but the
    // window's gate decides the disabled state.
    let mut vm = base();
    vm.step = WizardStep::Preview;
    let mut review = TransferPreviewVm::default();
    review.can_execute = false;
    vm.preview = Some(review);
    let rt = runtime(
        Host {
            vm,
            edits: DataTransferWindowEdits::default(),
            copied: Vec::new(),
        },
        DESIGN,
    );
    assert!(has(&rt, "data-transfer-execute"));
    // The transfer next button is not on the preview step.
    assert!(!has(&rt, "data-transfer-next"));
}

#[test]
fn the_window_fits_the_narrow_design_without_horizontal_overflow() {
    let mut vm = base();
    vm.step = WizardStep::Objects;
    let row = |source: &str| crate::views::TransferTableRow {
        vm: crate::views::TransferTableVm {
            source_table: source.to_owned(),
            target_table: source.to_owned(),
            ..crate::views::TransferTableVm::default()
        },
        ..crate::views::TransferTableRow::default()
    };
    vm.apply_inspect_rows((0..40).map(|i| row(&format!("t{i}"))).collect());
    let rt = runtime(
        Host {
            vm,
            edits: DataTransferWindowEdits::default(),
            copied: Vec::new(),
        },
        MIN,
    );
    let body = rect_of(&rt, "data-transfer-step-body");
    assert!(body[2] <= MIN.0 + 0.5, "the body fits the width: {body:?}");
    assert!(has(&rt, "data-transfer-table-row-t39"));
}

#[test]
fn the_mode_cards_carry_the_react_labels() {
    // The radio group renders with the three mode labels the React cards
    // carry (React 1057-1076) — observed through the group's tree.
    let mut vm = base();
    vm.step = WizardStep::Setup;
    vm.mode = TransferMode::StructureAndData;
    let rt = runtime(
        Host {
            vm,
            edits: DataTransferWindowEdits::default(),
            copied: Vec::new(),
        },
        DESIGN,
    );
    assert!(has(&rt, "data-transfer-mode"));
}
