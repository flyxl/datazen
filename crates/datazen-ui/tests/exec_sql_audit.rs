//! T382 independent audit: `ExecuteSqlFileDialog` vs React + T223 slot.
//!
//! Probes branches the coder's suite does not: the "no close while running"
//! guard, status-banner presence with an empty message, progress-log lines,
//! real slot composition, and rect geometry with a long status message.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use datazen_ui::views::execute_sql_file_dialog::{
    execute_sql_file_dialog, ExecuteSqlFileHostPort, ExecuteSqlFileState, ExecuteSqlFileVm,
    SqlFileRequest, SqlFileStatusKind, DIALOG_KEY, STATUS_KEY,
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
}

fn probe() -> Probe {
    Probe {
        vm: ExecuteSqlFileVm {
            open: true,
            connection_name: "Local PG".to_owned(),
            database: Some("postgres".to_owned()),
            db_session_id: "sess-1".to_owned(),
            state: ExecuteSqlFileState::default(),
        },
        dark: true,
        events: Vec::new(),
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let host = ExecuteSqlFileHostPort {
        on_close: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Close))),
        on_execute: Some(Rc::new(|st: &mut Probe, r: SqlFileRequest| st.events.push(Event::Execute(r)))),
        on_executed: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Executed))),
        on_confirm: None,
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

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

/// React's `onClose` is `{ if (!running) onClose(); }` — the header X, Escape
/// and any backdrop dismiss must be inert while an execution is in flight.
#[test]
fn header_close_is_blocked_while_running() {
    let mut p = probe();
    p.vm.state.running = true;
    let mut rt = runtime(p, (960.0, 640.0));
    click(&mut rt, &format!("{DIALOG_KEY}-close"));
    assert!(rt.state.events.is_empty(), "no close request while running");
}

#[test]
fn escape_is_blocked_while_running() {
    let mut p = probe();
    p.vm.state.running = true;
    let mut rt = runtime(p, (960.0, 640.0));
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(1.0);
    rt.frame(1.0);
    assert!(rt.state.events.is_empty(), "Escape must not close while running");
}

/// React renders the status banner only when `statusMessage` is non-empty.
#[test]
fn empty_status_message_renders_no_banner() {
    let rt = runtime(probe(), (1440.0, 816.0));
    assert!(
        !has(&rt, STATUS_KEY),
        "React renders no sql-file-status element for an empty message"
    );
}

/// Progress-log lines are rendered verbatim under the status surface.
#[test]
fn progress_log_lines_render_verbatim() {
    let mut p = probe();
    p.vm.state.status_message = "restoring".to_owned();
    p.vm.state.status_kind = SqlFileStatusKind::Running;
    p.vm.state.progress_log = vec!["step one".to_owned(), "step two".to_owned()];
    let rt = runtime(p, (1440.0, 816.0));
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|t| t == "step one"));
    assert!(rendered.iter().any(|t| t == "step two"));
    assert!(has(&rt, "sql-file-status-log-0"));
    assert!(has(&rt, "sql-file-status-log-1"));
}

/// The T223 slot must compose the real port, not the old structural stand-in.
#[test]
fn slot_composes_the_real_port() {
    let src = include_str!("../src/views/content_view_dialogs.rs");
    assert!(src.contains("execute_sql_file_dialog("), "real port is called");
    assert!(src.contains("ExecuteSqlFileVm"), "real VM is built");
    // The old stand-in built a body string and passed Some("sql-file-status").
    assert!(
        !src.contains("Some(\"sql-file-status\")"),
        "T223 stand-in must be removed"
    );
}

/// A long, wrapping status message stays inside both windows in both palettes.
#[test]
fn long_status_fits_both_windows_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe();
            p.dark = dark;
            p.vm.state.status_message = "a very long restoring status message that must wrap and stay inside the dialog panel".to_owned();
            p.vm.state.status_kind = SqlFileStatusKind::Running;
            let rt = runtime(p, size);
            let mut outside = Vec::new();
            for e in rt.elements() {
                let r = e.rect;
                if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5 {
                    outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
                }
            }
            assert!(outside.is_empty(), "dark={dark} {size:?}: {outside:?}");
        }
    }
}
