//! T226: the Data Sync preview step — the executing overlay's fallback
//! word, the session gate, and the embedded preview.

use rune::{col, Cx, El, Env, Runtime};

use crate::views::data_sync::data_sync_compare_step::SyncState;
use crate::views::data_sync::data_sync_preview_step::{
    DataSyncPreviewStep, DataSyncPreviewStepVm,
};
use crate::views::data_sync::sql_preview::SqlPreviewVm;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn vm(
    sync_state: SyncState,
    progress: &str,
    sessions: Option<(&str, &str)>,
    preview: SqlPreviewVm,
) -> DataSyncPreviewStepVm {
    DataSyncPreviewStepVm {
        sync_state,
        execute_progress: progress.to_owned(),
        source_db_session_id: sessions.map(|(s, _)| s.to_owned()),
        target_db_session_id: sessions.map(|(_, t)| t.to_owned()),
        tables: Vec::new(),
        options: Default::default(),
        selected_rows: Vec::new(),
        table_selections: Vec::new(),
        preview,
    }
}

fn view(host: &Host, _cx: &mut Cx<Host>) -> El {
    col().w_full().h_full().child(
        DataSyncPreviewStep::<Host>::new(vm(
            host.sync_state,
            &host.progress,
            host.sessions.as_ref().map(|(s, t)| (s.as_str(), t.as_str())),
            host.preview.clone(),
        ))
        .on_refresh(|s: &mut Host| s.refreshes += 1)
        .on_copy(|s: &mut Host, text| s.copies.push(text))
        .on_filter(|_s, _f| {})
        .build(true),
    )
}

#[derive(Default)]
struct Host {
    sync_state: SyncState,
    progress: String,
    sessions: Option<(String, String)>,
    refreshes: usize,
    copies: Vec<String>,
    preview: SqlPreviewVm,
}

fn mount(host: Host) -> Runtime<Host> {
    let mut rt = Runtime::new(host, view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    rt
}

fn text_of(rt: &Runtime<Host>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn has_key(rt: &Runtime<Host>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<Host>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn the_executing_overlay_mounts_only_while_executing() {
    let host = Host {
        sync_state: SyncState::Executing,
        sessions: Some(("s".to_owned(), "t".to_owned())),
        ..Host::default()
    };
    let rt = mount(host);
    assert!(has_key(&rt, "data-sync-executing-overlay"));
    assert!(has_key(&rt, "data-sync-executing-overlay-spinner"));

    // Every other state hides it.
    for state in [SyncState::Idle, SyncState::Inspecting, SyncState::Comparing] {
        let host = Host {
            sync_state: state,
            sessions: Some(("s".to_owned(), "t".to_owned())),
            ..Host::default()
        };
        let rt = mount(host);
        assert!(!has_key(&rt, "data-sync-executing-overlay"), "{state:?}");
    }
}

#[test]
fn the_overlay_text_prefers_the_progress_string() {
    let host = Host {
        sync_state: SyncState::Executing,
        progress: "12/40".to_owned(),
        sessions: Some(("s".to_owned(), "t".to_owned())),
        ..Host::default()
    };
    let rt = mount(host);
    assert_eq!(text_of(&rt, "data-sync-executing-overlay-text"), "12/40");

    // Empty progress falls back to the pack word.
    let host = Host {
        sync_state: SyncState::Executing,
        sessions: Some(("s".to_owned(), "t".to_owned())),
        ..Host::default()
    };
    let rt = mount(host);
    assert_eq!(
        text_of(&rt, "data-sync-executing-overlay-text"),
        crate::i18n::t("sync.executing").to_owned()
    );
}

#[test]
fn both_sessions_gate_the_preview() {
    // Both present: the preview body mounts.
    let host = Host {
        sessions: Some(("s".to_owned(), "t".to_owned())),
        ..Host::default()
    };
    let rt = mount(host);
    assert!(has_key(&rt, "data-sync-preview"));
    assert!(!has_key(&rt, "data-sync-preview-session-required"));

    // Either absent: the required word.
    for sessions_present in [false, true] {
        let sessions = if sessions_present {
            Some((String::new(), "t".to_owned()))
        } else {
            None
        };
        let host = Host {
            sessions: sessions.clone(),
            ..Host::default()
        };
        let rt = mount(host);
        assert!(has_key(&rt, "data-sync-preview-session-required"), "{sessions:?}");
        assert_eq!(
            text_of(&rt, "data-sync-preview-session-required-text"),
            crate::i18n::t("sync.sessionRequired").to_owned()
        );
        assert!(!has_key(&rt, "data-sync-preview"));
    }
}

#[test]
fn the_overlay_rides_above_the_required_word_too() {
    // Executing without sessions: BOTH the required word and the overlay
    // mount — React renders the overlay regardless of the face below it.
    let host = Host {
        sync_state: SyncState::Executing,
        ..Host::default()
    };
    let rt = mount(host);
    assert!(has_key(&rt, "data-sync-preview-session-required"));
    assert!(has_key(&rt, "data-sync-executing-overlay"));
}

#[test]
fn the_preview_callbacks_pass_through() {
    // With statements present the toolbar's refresh and copy buttons mount;
    // their presses reach the step's wiring.
    use crate::views::data_sync::client_sql_preview::{statements_to_preview_text, DataSyncSqlStatement};
    use crate::views::DataSyncOperation;
    let statements = vec![DataSyncSqlStatement {
        table: "users".to_owned(),
        operation: DataSyncOperation::Insert,
        sql: "INSERT INTO users VALUES (1)".to_owned(),
        preview_sql: "INSERT INTO users VALUES (1)".to_owned(),
        parameters: Vec::new(),
        row_key: Vec::new(),
    }];
    let preview = SqlPreviewVm {
        statements: Some(statements.clone()),
        ..SqlPreviewVm::default()
    };
    let host = Host {
        sessions: Some(("s".to_owned(), "t".to_owned())),
        preview,
        ..Host::default()
    };
    let mut rt = mount(host);
    assert!(has_key(&rt, "data-sync-preview"));
    click(&mut rt, "data-sync-preview-refresh");
    assert_eq!(rt.state.refreshes, 1);
    click(&mut rt, "data-sync-preview-copy");
    assert_eq!(
        rt.state.copies,
        vec![statements_to_preview_text(&statements, Default::default())]
    );
}
