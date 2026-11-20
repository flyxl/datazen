//! Independent tester audit for T223 (`ContentViewDialogs` port).
//!
//! Precise probes for branches the port's own tests leave open:
//! 1. import cancel / header-close routing (only confirm + SQL-file close are
//!    covered there);
//! 2. the `current_database -> initial_database` fallback in dialog context;
//! 3. the dialogs actually mount through T199's `content-view-dialogs` slot,
//!    with rect-based geometry at both sizes in both themes;
//! 4. a source guard for the 800-line / no-bare-panic contract.

use std::rc::Rc;

use rune::{col, tok, Cx, El, Env, Runtime};

use crate::state_def::AppState;
use crate::views::content_view_dialogs::{
    build, DialogAction, DialogConnectionCtx, DialogsVm, ExportCapability,
};

fn env() -> Env {
    Env { system_dark: true, scale: 1.0 }
}

#[derive(Clone)]
struct AuditProbe {
    dialogs: DialogsVm,
    dark: bool,
    actions: Vec<DialogAction>,
}

impl Default for AuditProbe {
    fn default() -> Self {
        Self { dialogs: DialogsVm::default(), dark: true, actions: Vec::new() }
    }
}

fn ctx() -> DialogConnectionCtx {
    DialogConnectionCtx {
        db_session_id: "sess-1".to_owned(),
        connection_id: "cfg-1".to_owned(),
        connection_name: "Local Postgres".to_owned(),
        database_type: "postgres".to_owned(),
    }
}

fn only(set: fn(&mut DialogsVm)) -> DialogsVm {
    let mut vm = DialogsVm {
        conn_ctx: Some(ctx()),
        export_capability: ExportCapability::FullTable,
        ..Default::default()
    };
    set(&mut vm);
    vm
}

fn view(s: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    let mut layer = Vec::new();
    let callback: Rc<dyn Fn(&mut AuditProbe, DialogAction)> =
        Rc::new(|state: &mut AuditProbe, action: DialogAction| state.actions.push(action));
    build::<AuditProbe>(&s.dialogs, s.dark, &mut layer, &callback);
    col().w_full().h_full().children(layer)
}

fn runtime(probe: AuditProbe, size: (f32, f32)) -> Runtime<AuditProbe> {
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

fn outside_nodes<S>(rt: &Runtime<S>, w: f32, h: f32) -> Vec<String> {
    rt.elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect()
}

#[test]
fn import_cancel_and_other_header_closes_route_to_the_host() {
    // React's `onCloseImport` carries no target; the port's empty-target
    // variant is how cancel is distinguished from the confirm's real target.
    let probe = AuditProbe {
        dialogs: only(|vm| {
            vm.import_open = true;
            vm.import_table_name = Some("users".to_owned());
            vm.import_schema = Some("public".to_owned());
            vm.import_database = Some("postgres".to_owned());
        }),
        ..Default::default()
    };
    let mut rt = runtime(probe, (1440.0, 816.0));
    click(&mut rt, "content-view-import-dialog-cancel");
    assert_eq!(
        rt.state.actions,
        vec![DialogAction::CloseImport {
            table: String::new(),
            schema: None,
            database: None,
        }],
        "import cancel carries the empty target"
    );

    for (set, close_key, expected) in [
        (only(|vm| vm.export_open = true) , "content-view-export-dialog-close", DialogAction::CloseExport),
        (only(|vm| vm.create_db_open = true), "content-view-create-database-dialog-close", DialogAction::CloseCreateDb),
        (only(|vm| vm.create_user_open = true), "content-view-create-user-dialog-close", DialogAction::CloseCreateUser),
    ] {
        // Export needs a table name to mount.
        let mut vm = set;
        if vm.export_open {
            vm.export_table_name = Some("users".to_owned());
        }
        let mut rt = runtime(AuditProbe { dialogs: vm, ..Default::default() }, (1440.0, 816.0));
        click(&mut rt, close_key);
        assert_eq!(rt.state.actions, vec![expected], "{close_key} header close");
    }
}

#[test]
fn dialog_context_uses_current_database_then_initial_database() {
    let probe = AuditProbe {
        dialogs: only(|vm| {
            vm.create_schema_open = true;
            vm.current_database = None;
            vm.initial_database = Some("seed_db".to_owned());
        }),
        ..Default::default()
    };
    let rt = runtime(probe, (960.0, 640.0));
    assert_eq!(
        element(&rt, "content-view-create-schema-dialog-context").text.as_deref(),
        Some("seed_db"),
        "initial database is the fallback when current is absent"
    );

    let probe = AuditProbe {
        dialogs: only(|vm| {
            vm.create_schema_open = true;
            vm.current_database = Some("live_db".to_owned());
            vm.initial_database = Some("seed_db".to_owned());
        }),
        ..Default::default()
    };
    let rt = runtime(probe, (960.0, 640.0));
    assert_eq!(
        element(&rt, "content-view-create-schema-dialog-context").text.as_deref(),
        Some("live_db"),
        "current database wins"
    );
}

// --- integration through T199's keyed slot --------------------------------

thread_local! {
    static SLOT_FIXTURE: std::cell::RefCell<(crate::views::ContentViewVm, bool)> =
        std::cell::RefCell::new((crate::views::ContentViewVm::default(), true));
}

fn all_open() -> DialogsVm {
    DialogsVm {
        conn_ctx: Some(ctx()),
        export_capability: ExportCapability::FullTable,
        export_open: true,
        export_table_name: Some("users".to_owned()),
        total_rows: 42,
        selected_rows: 3,
        table_columns: vec!["id".to_owned()],
        batch_export_open: true,
        exportable_table_names: vec!["users".to_owned()],
        import_open: true,
        import_table_name: Some("users".to_owned()),
        sql_file_open: true,
        create_db_open: true,
        create_schema_open: true,
        create_user_open: true,
        ..Default::default()
    }
}

fn slot_view(state: &AppState, _cx: &mut Cx<AppState>) -> El {
    let (vm, dark) = SLOT_FIXTURE.with(|f| f.borrow().clone());
    col().w_full().h_full().child(crate::views::content_view::build(state, tok::theme(dark), dark, &vm, &mut Vec::new()))
}

#[test]
fn dialogs_mount_through_the_content_view_slot_at_both_sizes_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let vm = crate::views::ContentViewVm {
                dialogs_vm: Some(all_open()),
                ..Default::default()
            };
            SLOT_FIXTURE.with(|f| *f.borrow_mut() = (vm, dark));
            let mut rt = Runtime::new(AppState::default(), slot_view, (w, h), env());
            rt.frame(0.0);
            for key in [
                "content-view-dialogs",
                "content-view-export-dialog",
                "content-view-batch-export-dialog",
                "content-view-import-dialog",
                "content-view-sql-file-dialog",
                "content-view-create-database-dialog",
                "content-view-create-schema-dialog",
                "content-view-create-user-dialog",
            ] {
                assert!(has_key(&rt, key), "slot {w}x{h} dark={dark}: {key}");
            }
            let outside = outside_nodes(&rt, w, h);
            assert!(outside.is_empty(), "slot {w}x{h} dark={dark}: outside {outside:?}");
        }
    }
}

#[test]
fn new_dialog_source_respects_the_800_line_cap_and_has_no_bare_panics_or_oklch() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/views/content_view_dialogs.rs");
    let src = std::fs::read_to_string(&path).expect("content_view_dialogs.rs is readable");
    let lines = src.lines().count();
    assert!(lines <= 800, "content_view_dialogs.rs has {lines} lines (> 800)");
    for forbidden in ["oklch", ".unwrap()", ".expect(", "todo!()", "unimplemented!()", "panic!("] {
        assert!(!src.contains(forbidden), "production dialog source contains {forbidden}");
    }
}
