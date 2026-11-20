//! T223: ContentViewDialogs — real probes against mounted dialog layers:
//! React keys, open gates, callback routing and two-size/two-theme geometry.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::views::content_view_dialogs::{
    build, DialogAction, DialogConnectionCtx, DialogsVm, ExportCapability,
};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct Probe {
    dialogs: DialogsVm,
    dark: bool,
    actions: Vec<DialogAction>,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            dialogs: DialogsVm::default(),
            dark: true,
            actions: Vec::new(),
        }
    }
}

fn all_open() -> DialogsVm {
    DialogsVm {
        conn_ctx: Some(DialogConnectionCtx {
            db_session_id: "sess-1".to_owned(),
            connection_id: "cfg-1".to_owned(),
            connection_name: "Local Postgres".to_owned(),
            database_type: "postgres".to_owned(),
        }),
        current_database: Some("public".to_owned()),
        initial_database: Some("postgres".to_owned()),
        export_capability: ExportCapability::FullTable,
        export_open: true,
        export_table_name: Some("users".to_owned()),
        table_columns: vec!["id".to_owned(), "email".to_owned()],
        table_rows: 7,
        selected_rows: 2,
        total_rows: 100,
        batch_export_open: true,
        batch_export_initial_selected: vec!["users".to_owned()],
        exportable_table_names: vec!["users".to_owned(), "orders".to_owned()],
        import_open: true,
        import_table_name: Some("users".to_owned()),
        import_schema: Some("public".to_owned()),
        import_database: Some("postgres".to_owned()),
        sql_file_open: true,
        create_db_open: true,
        create_schema_open: true,
        create_user_open: true,
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let callback: Rc<dyn Fn(&mut Probe, DialogAction)> =
        Rc::new(|state: &mut Probe, action: DialogAction| state.actions.push(action));
    build::<Probe>(
        &s.dialogs,
        s.dark,
        &mut layer,
        &callback,
    );
    col().w_full().h_full().children(layer)
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(probe, view, size, env());
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) outside {w}x{h}: {outside:?}",
        outside.len()
    );
}

const DIALOGS: [&str; 7] = [
    "content-view-export-dialog",
    "content-view-batch-export-dialog",
    "content-view-import-dialog",
    "content-view-sql-file-dialog",
    "content-view-create-database-dialog",
    "content-view-create-schema-dialog",
    "content-view-create-user-dialog",
];

#[test]
fn no_connection_context_mounts_no_dialogs() {
    let mut probe = Probe::default();
    probe.dialogs = all_open();
    probe.dialogs.conn_ctx = None;
    let rt = runtime(probe, (960.0, 640.0));
    for key in DIALOGS {
        assert!(!has_key(&rt, key), "no dialog without connCtx: {key}");
    }
}

#[test]
fn each_open_dialog_mounts_its_react_keys() {
    let mut probe = Probe::default();
    probe.dialogs = all_open();
    let rt = runtime(probe, (1440.0, 816.0));
    for key in DIALOGS {
        assert!(has_key(&rt, key), "dialog key: {key}");
        assert!(has_key(&rt, &format!("{key}-title")), "title: {key}");
        assert!(has_key(&rt, &format!("{key}-body")), "body: {key}");
        assert!(has_key(&rt, &format!("{key}-cancel")), "cancel: {key}");
        assert!(has_key(&rt, &format!("{key}-confirm")), "confirm: {key}");
    }
    assert!(has_key(&rt, "sql-file-execute"), "React SQL file testid");
    // The real T382 dialog follows React: the status banner is conditional on
    // a non-empty message, so a freshly opened dialog has no status element.
    assert!(!has_key(&rt, "sql-file-status"), "no status banner for an empty message");
    assert!(has_key(&rt, "import-summary"), "React import summary testid");
    assert_eq!(
        element(&rt, "content-view-sql-file-dialog-title").text.as_deref(),
        Some(crate::i18n::t("common.executeSqlFile"))
    );
}

#[test]
fn closed_or_incomplete_dialogs_mount_nothing() {
    // Open flags false: no dialogs at all.
    let mut probe = Probe::default();
    probe.dialogs.conn_ctx = all_open().conn_ctx;
    let rt = runtime(probe, (960.0, 640.0));
    for key in DIALOGS {
        assert!(!has_key(&rt, key), "closed dialog absent: {key}");
    }

    // Export stays gated on exportTableName even when open is true.
    let mut probe = Probe::default();
    probe.dialogs = all_open();
    probe.dialogs.export_table_name = None;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(!has_key(&rt, "content-view-export-dialog"), "no table name, no export dialog");
    assert!(has_key(&rt, "content-view-batch-export-dialog"), "other dialogs unaffected");
}

#[test]
fn footer_and_dismiss_callbacks_reach_the_host() {
    // One dialog per runtime: all seven open at once stack their scrims, so a
    // centre-point click would land on the topmost one instead of the target.
    let only = |set: fn(&mut DialogsVm)| {
        let mut vm = all_open();
        vm.export_open = false;
        vm.batch_export_open = false;
        vm.import_open = false;
        vm.sql_file_open = false;
        vm.create_db_open = false;
        vm.create_schema_open = false;
        vm.create_user_open = false;
        set(&mut vm);
        vm
    };

    let cases: [(DialogsVm, &str, DialogAction); 8] = [
        (only(|v| v.export_open = true), "content-view-export-dialog-confirm", DialogAction::CloseExport),
        (only(|v| v.export_open = true), "content-view-export-dialog-cancel", DialogAction::CloseExport),
        (only(|v| v.batch_export_open = true), "content-view-batch-export-dialog-confirm", DialogAction::CloseBatchExport),
        (only(|v| v.import_open = true), "content-view-import-dialog-confirm", DialogAction::CloseImport {
            table: "users".to_owned(), schema: Some("public".to_owned()), database: Some("postgres".to_owned()) }),
        (only(|v| v.sql_file_open = true), "sql-file-execute", DialogAction::Executed),
        (only(|v| v.create_db_open = true), "content-view-create-database-dialog-confirm", DialogAction::DbCreated),
        (only(|v| v.create_schema_open = true), "content-view-create-schema-dialog-confirm", DialogAction::SchemaCreated),
        (only(|v| v.create_user_open = true), "content-view-create-user-dialog-confirm", DialogAction::UserCreated),
    ];
    for (vm, key, expected) in cases {
        let mut probe = Probe::default();
        probe.dialogs = vm;
        let mut rt = runtime(probe, (1440.0, 816.0));
        click(&mut rt, key);
        assert_eq!(rt.state.actions, vec![expected], "{key} reached its callback");
    }

    let mut probe = Probe::default();
    let mut vm = all_open();
    vm.sql_file_open = true;
    vm.export_open = false;
    vm.batch_export_open = false;
    vm.import_open = false;
    vm.create_db_open = false;
    vm.create_schema_open = false;
    vm.create_user_open = false;
    probe.dialogs = vm;
    let mut rt = runtime(probe, (1440.0, 816.0));
    click(&mut rt, "content-view-sql-file-dialog-close");
    assert_eq!(rt.state.actions, vec![DialogAction::CloseSqlFile], "header close");
}

#[test]
fn geometry_holds_at_both_sizes_in_both_themes_with_every_dialog_open() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = Probe::default();
            probe.dialogs = all_open();
            probe.dark = dark;
            let rt = runtime(probe, (w, h));
            assert_in_window(&rt, w, h, &format!("dialogs {w}x{h} dark={dark}"));
            for key in DIALOGS {
                assert!(has_key(&rt, key), "{key} at {w}x{h} dark={dark}");
            }
        }
    }
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Dropping the `export_table_name` guard from the export mount made
//    `closed_or_incomplete_dialogs_mount_nothing` fail with:
//    `no table name, no export dialog`.
// 2. Wiring the SQL-file confirm button to `CloseSqlFile` instead of `Executed`
//    made `footer_and_dismiss_callbacks_reach_the_host` fail with:
//    `left: Some("CloseSqlFile") right: Some("Executed")`.
