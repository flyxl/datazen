//! T382: `ExecuteSqlFileDialog` port — keys, validation, host callbacks, the
//! status state machine and full-window overlay geometry at 960×640 / 1440×816
//! in both palettes. Observations come from the runtime element tree.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::views::execute_sql_file_dialog::{
    execute_sql_file_dialog, ExecuteSqlFileHostPort, ExecuteSqlFileState, ExecuteSqlFileVm,
    SqlFileRequest, SqlFileStatusKind, DIALOG_KEY, EXECUTE_KEY, STATUS_KEY,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Close,
    Execute(SqlFileRequest),
    Executed,
}

#[derive(Clone)]
struct Probe {
    vm: ExecuteSqlFileVm,
    dark: bool,
    events: Vec<Event>,
    confirm: bool,
}

fn probe(database: Option<&str>) -> Probe {
    Probe {
        vm: ExecuteSqlFileVm {
            open: true,
            connection_name: "Local PG".to_owned(),
            database: database.map(str::to_owned),
            db_session_id: "sess-1".to_owned(),
            state: ExecuteSqlFileState::default(),
        },
        dark: true,
        events: Vec::new(),
        confirm: true,
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let host = ExecuteSqlFileHostPort {
        on_close: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Close))),
        on_execute: Some(Rc::new(|st: &mut Probe, request: SqlFileRequest| {
            st.events.push(Event::Execute(request))
        })),
        on_executed: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Executed))),
        on_confirm: Some(Rc::new(|st: &mut Probe| st.confirm)),
    };
    execute_sql_file_dialog(&s.vm, s.dark, &mut layer, host);
    rune_kit::root(col().w_full().h_full(), layer)
}

fn runtime(p: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

#[test]
fn dialog_mounts_the_react_and_slot_keys() {
    let rt = runtime(probe(Some("postgres")), (1440.0, 816.0));
    for key in [
        DIALOG_KEY,
        &format!("{DIALOG_KEY}-title"),
        &format!("{DIALOG_KEY}-body"),
        &format!("{DIALOG_KEY}-cancel"),
        &format!("{DIALOG_KEY}-confirm"),
        EXECUTE_KEY,
        STATUS_KEY,
        &format!("{DIALOG_KEY}-scrim"),
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    for key in ["common.executeSqlFile", "common.close", "sqlFile.selectAndRun"] {
        assert!(rendered.iter().any(|text| text == crate::i18n::t(key)), "{key}: exact translated text");
    }
    assert_eq!(element(&rt, &format!("{DIALOG_KEY}-title")).text.as_deref(), Some(crate::i18n::t("common.executeSqlFile")));
    assert_eq!(element(&rt, STATUS_KEY).label.as_deref(), Some("idle"), "status kind is observable");
    assert_eq!(element(&rt, DIALOG_KEY).rect[2], 672.0, "React max-w-2xl panel width");
}

#[test]
fn closed_dialog_mounts_nothing() {
    let mut p = probe(Some("postgres"));
    p.vm.open = false;
    let rt = runtime(p, (960.0, 640.0));
    assert!(!has(&rt, DIALOG_KEY), "closed dialog is absent");
    assert!(!has(&rt, EXECUTE_KEY), "execute button is absent");
}

#[test]
fn validation_disables_execute_without_a_database() {
    let rt = runtime(probe(None), (1440.0, 816.0));
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|text| text.contains(crate::i18n::t("sqlFile.noDatabase"))), "no-database hint");
    assert!(!rendered.iter().any(|text| text.contains("postgres")), "no target database leaked");

    let mut rt = runtime(probe(None), (1440.0, 816.0));
    click(&mut rt, EXECUTE_KEY);
    assert!(rt.state.events.is_empty(), "a disabled execute sends no host request");

    let mut rt = runtime(probe(Some("")), (1440.0, 816.0));
    click(&mut rt, EXECUTE_KEY);
    assert!(rt.state.events.is_empty(), "an empty database sends no host request");
}

#[test]
fn execute_and_close_reach_the_host_port() {
    let mut rt = runtime(probe(Some("postgres")), (1440.0, 816.0));
    let rendered = texts(&rt);
    let target = crate::i18n::t("sqlFile.targetDatabase").replace("{database}", "postgres");
    assert!(rendered.iter().any(|text| text.contains(&target)), "target database is disclosed");

    click(&mut rt, EXECUTE_KEY);
    assert_eq!(
        rt.state.events,
        [
            Event::Execute(SqlFileRequest {
                db_session_id: "sess-1".to_owned(),
                database: "postgres".to_owned(),
            }),
            Event::Executed,
        ],
        "the host gets the session/database request then the success hook"
    );

    click(&mut rt, &format!("{DIALOG_KEY}-cancel"));
    assert_eq!(rt.state.events.last(), Some(&Event::Close));
}

#[test]
fn confirmation_gate_can_cancel_execution() {
    let mut p = probe(Some("postgres"));
    p.confirm = false;
    let mut rt = runtime(p, (1440.0, 816.0));
    click(&mut rt, EXECUTE_KEY);
    assert!(rt.state.events.is_empty(), "a declined confirmation sends no request");
}

#[test]
fn status_state_machine_follows_react() {
    let mut state = ExecuteSqlFileState::default();
    assert_eq!(state.status_kind, SqlFileStatusKind::Idle);
    assert!(state.begin(Some("postgres")), "a database starts execution");
    assert!(state.running);
    assert_eq!(state.status_kind, SqlFileStatusKind::Running);
    assert_eq!(state.status_message, crate::i18n::t("backup.restoring"));
    assert_eq!(state.progress_log, vec![crate::i18n::t("backup.restoring")]);
    assert!(!state.begin(None), "no database never starts execution");

    state.apply_progress("step 1".to_owned(), false);
    assert_eq!(state.status_kind, SqlFileStatusKind::Running);
    state.apply_progress("done".to_owned(), true);
    assert_eq!(state.status_kind, SqlFileStatusKind::Success);
    state.settle();
    assert!(!state.running);

    state.apply_error("boom".to_owned());
    assert_eq!(state.status_kind, SqlFileStatusKind::Error);
    state.reset();
    assert_eq!(state, ExecuteSqlFileState::default(), "closing resets every field");
}

#[test]
fn overlay_geometry_covers_both_windows_in_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe(Some("postgres"));
            p.dark = dark;
            p.vm.state.status_message = "restoring".to_owned();
            p.vm.state.status_kind = SqlFileStatusKind::Running;
            let rt = runtime(p, size);
            let scrim = element(&rt, &format!("{DIALOG_KEY}-scrim")).rect;
            assert!(
                scrim[0] <= 0.5 && scrim[1] <= 0.5 && scrim[0] + scrim[2] >= size.0 - 0.5
                    && scrim[1] + scrim[3] >= size.1 - 0.5,
                "dark={dark} {size:?}: scrim {scrim:?} does not cover the window"
            );
            let panel = element(&rt, DIALOG_KEY).rect;
            assert!(
                panel[0] >= -0.5 && panel[1] >= -0.5 && panel[0] + panel[2] <= size.0 + 0.5
                    && panel[1] + panel[3] <= size.1 + 0.5,
                "dark={dark} {size:?}: panel {panel:?} outside the window"
            );
        }
    }
}
