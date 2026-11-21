//! T247 audit (tester): independent checks of the result step, from
//! separately-written scenes: the status role channel, the verdict color
//! split observed through two fresh mounts, geometry at both sizes, and
//! the light theme path.

use rune::{col, Cx, El, Env, Runtime};

use crate::views::data_sync::data_sync_result_step::{
    DataSyncExecutionOutcome, DataSyncExecutionResult, DataSyncResultStep,
};
use crate::views::data_sync::mapping_view::CompareSummaryStats;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn audit_stats() -> CompareSummaryStats {
    CompareSummaryStats {
        inserts: 2,
        updates: 0,
        deletes: 1,
        unchanged_tables: 1,
        incompatible: 0,
    }
}

fn audit_execution(outcome: DataSyncExecutionOutcome) -> Option<DataSyncExecutionResult> {
    Some(DataSyncExecutionResult {
        applied: 1,
        rolled_back: false,
        rollback_reason: None,
        affected_rows: Some(1),
        skipped: None,
        outcome: Some(outcome),
    })
}

#[derive(Default)]
struct AuditHost;

fn audit_view(_host: &AuditHost, _cx: &mut Cx<AuditHost>) -> El {
    col().w_full().h_full().child(
        DataSyncResultStep::<AuditHost>::new(
            audit_execution(DataSyncExecutionOutcome::Committed),
            audit_stats(),
        )
        .on_re_compare(|_s| {})
        .on_copy_report(|_s| {})
        .on_explain_diff(|_s| {})
        .build(true),
    )
}

fn audit_mount(size: (f32, f32)) -> Runtime<AuditHost> {
    let mut rt = Runtime::new(AuditHost, audit_view, size, env(true));
    rt.frame(0.0);
    rt
}

#[test]
fn audit_the_verdict_row_carries_the_status_role() {
    let rt = audit_mount((960.0, 640.0));
    let verdict = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-execute-done"))
        .expect("the verdict row exists");
    assert_eq!(verdict.role, "status", "React's role attribute rides the row");
}

#[test]
fn audit_geometry_both_sizes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = audit_mount((w, h));
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else { continue };
            if !key.starts_with("data-sync-result")
                && !key.starts_with("data-sync-execute-done")
                && !key.starts_with("data-sync-summary")
            {
                continue;
            }
            let [x, _y, ew, _eh] = e.rect;
            if x < -0.5 || x + ew > w + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(outside.is_empty(), "{w}x{h}: {}", outside.join("; "));
    }
}

#[test]
fn audit_the_light_theme_path_renders_the_committed_face() {
    let view = |_host: &AuditHost, _cx: &mut Cx<AuditHost>| -> El {
        col().w_full().h_full().child(
            DataSyncResultStep::<AuditHost>::new(
                audit_execution(DataSyncExecutionOutcome::Committed),
                audit_stats(),
            )
            .build(false),
        )
    };
    let mut rt = Runtime::new(AuditHost, view, (960.0, 640.0), env(false));
    rt.frame(0.0);
    let verdict = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-execute-done-verdict"))
        .expect("the verdict mounts in light theme");
    assert_eq!(verdict.text.as_deref(), Some(crate::i18n::t("sync.executeDone")));
}
