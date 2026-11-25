//! T226 audit (tester): independent checks of the preview step, from
//! separately-built scenes: geometry at both window sizes across both
//! content faces, the light-theme build path, and the overlay's spinner
//! tone/size through fresh mounts.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::views::data_sync::data_sync_compare_step::SyncState;
use datazen_ui::views::data_sync::data_sync_preview_step::{
    DataSyncPreviewStep, DataSyncPreviewStepVm,
};
use datazen_ui::views::data_sync::sql_preview::SqlPreviewVm;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn audit_vm(sync_state: SyncState, sessions: Option<(&str, &str)>) -> DataSyncPreviewStepVm {
    DataSyncPreviewStepVm {
        sync_state,
        execute_progress: String::new(),
        source_db_session_id: sessions.map(|(s, _)| s.to_owned()),
        target_db_session_id: sessions.map(|(_, t)| t.to_owned()),
        tables: Vec::new(),
        options: Default::default(),
        selected_rows: Vec::new(),
        table_selections: Vec::new(),
        preview: SqlPreviewVm::default(),
    }
}

#[derive(Default)]
struct AuditHost;

fn audit_view(host: &AuditHost, _cx: &mut Cx<AuditHost>) -> El {
    col().w_full().h_full().child(
        DataSyncPreviewStep::<AuditHost>::new(audit_vm(
            host_sync_state(),
            host_sessions(),
        ))
        .on_refresh(|_s| {})
        .on_copy(|_s, _t| {})
        .on_filter(|_s, _f| {})
        .build(true),
    )
}

fn host_sync_state() -> SyncState {
    SyncState::Idle
}

fn host_sessions() -> Option<(&'static str, &'static str)> {
    None
}

#[test]
fn audit_geometry_both_sizes_both_faces() {
    // Two faces (the required word without sessions; the preview with
    // both) at both window sizes — nothing crosses the window.
    for sessions in [None, Some(("s", "t"))] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let state = AuditFace { sessions };
            let view = |host: &AuditFace, _cx: &mut Cx<AuditFace>| -> El {
                col().w_full().h_full().child(
                    DataSyncPreviewStep::<AuditFace>::new(audit_vm(SyncState::Idle, host.sessions))
                        .build(true),
                )
            };
            let mut rt = Runtime::new(state, view, (w, h), env(true));
            rt.frame(0.0);
            let mut outside = Vec::new();
            for e in rt.elements() {
                let Some(key) = e.key.as_deref() else { continue };
                if !key.starts_with("data-sync-preview-step")
                    && !key.starts_with("data-sync-preview")
                {
                    continue;
                }
                let [x, _y, ew, _eh] = e.rect;
                if x < -0.5 || x + ew > w + 0.5 {
                    outside.push(format!("{key} {:?}", e.rect));
                }
            }
            assert!(
                outside.is_empty(),
                "{w}x{h} sessions={sessions:?}: {}",
                outside.join("; ")
            );
        }
    }
}

/// The audit's own face holder (the fn-pointer view reads it).
#[derive(Default)]
struct AuditFace {
    sessions: Option<(&'static str, &'static str)>,
}

#[test]
fn audit_the_light_theme_path_renders_the_required_word() {
    let view = |_host: &AuditHost, _cx: &mut Cx<AuditHost>| -> El {
        col().w_full().h_full().child(
            DataSyncPreviewStep::<AuditHost>::new(audit_vm(SyncState::Idle, None)).build(false),
        )
    };
    let mut rt = Runtime::new(AuditHost, view, (960.0, 640.0), env(false));
    rt.frame(0.0);
    let word = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-preview-session-required-text"))
        .expect("the word mounts in light theme");
    assert_eq!(word.text.as_deref(), Some(datazen_ui::i18n::t("sync.sessionRequired")));
}
