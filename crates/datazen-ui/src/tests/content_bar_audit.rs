//! Independent tester audit for T173 (ContentToolbar / ContentStatusBar ports).
//!
//! The port's own tests check the obvious flags but miss three observable
//! contracts that React and the WDIO specs pin:
//!
//! 1. `content-toolbar-er-diagram-button` (React's inner button testid, used by
//!    `e2e/helpers.ts:2599`) must survive the port;
//! 2. `DetailPanelToggle` uses `detail.hide` / `detail.show` as its title, not
//!    the generic `detail.title` (`e2e/specs/detail-panel.ts:76,97,116`);
//! 3. a keyed KV panel's AI button carries `redis.ai.context.tooltip` as its
//!    title (`ContentToolbar.test.tsx:216`).
//!
//! Plus rect/key probes for the two slot branches the port leaves untested.

use rune::{col, tok, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::content_status_bar::{build as build_status, StatusBarVm};
use crate::views::content_toolbar::{ContentToolbar, ContentToolbarVm, KvPanelState};

const TOOLBAR: &str = "content-toolbar";

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct AuditProbe {
    toolbar: ContentToolbarVm,
    status: StatusBarVm,
    dark: bool,
}

impl Default for AuditProbe {
    fn default() -> Self {
        Self {
            toolbar: ContentToolbarVm {
                title: "Workspace".to_owned(),
                new_query_label: t("common.newQuery").to_owned(),
                new_query_aria: t("common.newQuery").to_owned(),
                show_new_query: true,
                ..Default::default()
            },
            status: StatusBarVm {
                connected: true,
                connected_label: t("conn.connected").to_owned(),
                mod_key: "⌘".to_owned(),
                key_sep: String::new(),
                ..Default::default()
            },
            dark: true,
        }
    }
}

fn view(s: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    col()
        .w_full()
        .h_full()
        .child(ContentToolbar::<AuditProbe>::new(&s.toolbar).build(tok::theme(s.dark)))
        .child(build_status(&s.status, tok::theme(s.dark)))
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

/// REPRO D1 — React wraps the ER button in a testid'd span and also tags the
/// button itself; `e2e/helpers.ts:2599` queries the inner key.
#[test]
fn er_diagram_inner_button_testid_is_preserved() {
    let mut probe = AuditProbe::default();
    probe.toolbar.show_er_diagram = true;
    let rt = runtime(probe, (1440.0, 816.0));
    assert!(has_key(&rt, "content-toolbar-er-diagram"), "outer wrapper key");
    assert!(
        has_key(&rt, "content-toolbar-er-diagram-button"),
        "React's inner ER button testid (e2e/helpers.ts:2599)"
    );
}

/// REPRO D2 — React `DetailPanelToggle` swaps its title with the open state.
#[test]
fn detail_toggle_title_tracks_open_state_with_the_react_keys() {
    let mut probe = AuditProbe::default();
    probe.toolbar.detail_panel_applicable = true;
    probe.toolbar.detail_open = false;
    let rt = runtime(probe, (960.0, 640.0));
    assert_eq!(
        element(&rt, "content-toolbar-detail-toggle").label.as_deref(),
        Some(t("detail.show")),
        "closed detail panel offers detail.show"
    );

    let mut probe = AuditProbe::default();
    probe.toolbar.detail_panel_applicable = true;
    probe.toolbar.detail_open = true;
    let rt = runtime(probe, (960.0, 640.0));
    assert_eq!(
        element(&rt, "content-toolbar-detail-toggle").label.as_deref(),
        Some(t("detail.hide")),
        "open detail panel offers detail.hide"
    );
}

/// REPRO D3 — React sets `title={kvPanelState ? t('redis.ai.context.tooltip') : undefined}`.
#[test]
fn keyed_kv_ai_button_carries_the_context_tooltip_title() {
    let mut probe = AuditProbe::default();
    probe.toolbar.show_ai = true;
    probe.toolbar.kv_panel = Some(KvPanelState {
        selected_key: Some("user:42".to_owned()),
    });
    let rt = runtime(probe, (960.0, 640.0));
    assert_eq!(
        element(&rt, "conn-toolbar-ai").label.as_deref(),
        Some(t("redis.ai.context.tooltip")),
        "KV AI button title is the exact React key"
    );
}

#[test]
fn kv_slot_branches_mount_the_container_and_suppress_the_plain_cluster() {
    let mut probe = AuditProbe::default();
    probe.toolbar.has_context_bar = true;
    probe.status.has_kv_slot = true;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(has_key(&rt, "conn-toolbar-kv-context-bar"));
    assert!(has_key(&rt, "conn-status-kv-bar"));
    assert!(!has_key(&rt, "status-context"), "KV slot replaces the centre text");

    let rt = runtime(AuditProbe::default(), (960.0, 640.0));
    assert!(!has_key(&rt, "conn-toolbar-kv-context-bar"));
    assert!(!has_key(&rt, "conn-status-kv-bar"));
    assert!(has_key(&rt, "status-context"), "plain footer keeps its context text");
}

#[test]
fn adversarial_bar_geometry_holds_at_both_sizes_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = AuditProbe::default();
            probe.dark = dark;
            probe.toolbar = ContentToolbarVm {
                title: "Connections".to_owned(),
                new_query_label: "New query".to_owned(),
                new_query_aria: "New query".to_owned(),
                show_new_query: true,
                show_new_table: true,
                show_er_diagram: true,
                show_objects: true,
                show_batch_export: true,
                show_docs: true,
                show_ai: true,
                detail_panel_applicable: true,
                compact: false,
                ..Default::default()
            };
            probe.status = StatusBarVm {
                connected: true,
                connected_label: t("conn.connected").to_owned(),
                mod_key: "⌘".to_owned(),
                table_name: Some("a_very_long_table_name_for_the_footer".to_owned()),
                current_database: Some("analytics_production".to_owned()),
                column_count: 42,
                total_rows: 12_345_678,
                ..Default::default()
            };
            let rt = runtime(probe, (w, h));
            let outside: Vec<String> = rt
                .elements()
                .iter()
                .filter(|e| {
                    let r = e.rect;
                    r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
                })
                .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                .collect();
            assert!(outside.is_empty(), "dark={dark} {w}x{h}: outside {outside:?}");
            assert!((element(&rt, TOOLBAR).rect[3] - 48.0).abs() < 1.0);
            assert!((element(&rt, "content-status-bar").rect[3] - 40.0).abs() < 1.0);
        }
    }
}
