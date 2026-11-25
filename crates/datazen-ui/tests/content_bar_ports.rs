//! T173: ContentToolbar / ContentStatusBar controlled ports.
//!
//! Real probes against the rendered element tree plus pure matrices for the
//! row-count text and the expanded-width arithmetic. Every assertion observes
//! runtime rects/keys or the VM's own pure helpers.

use rune::{col, tok, Cx, El, Env, Runtime};

use datazen_ui::views::content_status_bar::{
    build as build_status_bar, format_row_count, CountStrategy, StatusBarVm,
};
use datazen_ui::views::content_toolbar::{
    content_toolbar_expanded_min_width, estimate_expanded_toolbar_width, ContentToolbar,
    ContentToolbarVm, KvPanelState, TOOLBAR_HEIGHT,
};
use datazen_ui::i18n::t;

const TOOLBAR: &str = "content-toolbar";
const STATUS: &str = "content-status-bar";

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct Probe {
    toolbar: ContentToolbarVm,
    status: StatusBarVm,
    dark: bool,
    actions: Vec<&'static str>,
}

impl Default for Probe {
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
            actions: Vec::new(),
        }
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut toolbar = ContentToolbar::<Probe>::new(&s.toolbar)
        .on_new_query(|st: &mut Probe| st.actions.push("new-query"))
        .on_create_table(|st: &mut Probe| st.actions.push("new-table"))
        .on_er_diagram(|st: &mut Probe| st.actions.push("er"))
        .on_objects(|st: &mut Probe| st.actions.push("objects"))
        .on_privileges(|st: &mut Probe| st.actions.push("privileges"))
        .on_batch_export(|st: &mut Probe| st.actions.push("export"))
        .on_docs(|st: &mut Probe| st.actions.push("docs"))
        .on_ai_chat(|st: &mut Probe| st.actions.push("ai"))
        .on_detail(|st: &mut Probe| st.actions.push("detail"));
    if !s.toolbar.show_new_query {
        toolbar = ContentToolbar::<Probe>::new(&s.toolbar);
    }
    col()
        .w_full()
        .h_full()
        .child(toolbar.build(tok::theme(s.dark)))
        .child(build_status_bar(&s.status, tok::theme(s.dark)))
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(probe, view, size, env());
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
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

fn all_actions() -> ContentToolbarVm {
    ContentToolbarVm {
        show_new_query: true,
        show_new_table: true,
        show_er_diagram: true,
        show_objects: true,
        show_batch_export: true,
        ai_chat_open: true,
        detail_panel_applicable: true,
        detail_open: true,
        show_docs: true,
        show_ai: true,
        has_context_bar: true,
        kv_panel: Some(KvPanelState {
            selected_key: Some("user:42".to_owned()),
        }),
        compact: true,
        title: "Workspace".to_owned(),
        new_query_label: t("common.newQuery").to_owned(),
        new_query_aria: t("common.newQuery").to_owned(),
    }
}

#[test]
fn toolbar_mounts_every_react_testid_key_when_all_flags_are_on() {
    let mut probe = Probe::default();
    probe.toolbar = all_actions();
    let rt = runtime(probe, (1440.0, 816.0));

    for key in [
        TOOLBAR,
        "content-toolbar-title",
        "conn-toolbar-new-query",
        "toolbar-new-query",
        "content-toolbar-new-table",
        "content-toolbar-er-diagram",
        "content-toolbar-objects",
        "content-toolbar-privileges",
        "conn-toolbar-export",
        "conn-toolbar-ai",
        "conn-toolbar-kv-context-bar",
        "content-toolbar-docs",
        "content-toolbar-detail-toggle",
    ] {
        assert!(has_key(&rt, key), "React toolbar key mounted: {key}");
    }
    assert!((element(&rt, TOOLBAR).rect[3] - TOOLBAR_HEIGHT).abs() < 1.0, "48pt band");
    assert_eq!(element(&rt, TOOLBAR).role, "toolbar");
}

#[test]
fn toolbar_ai_button_follows_the_kv_panel_state_machine() {
    // Relational (no relay): button present, no KV tooltip claim.
    let mut probe = Probe::default();
    probe.toolbar.show_ai = true;
    probe.toolbar.kv_panel = None;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(has_key(&rt, "conn-toolbar-ai"), "relational keeps the AI button");

    // KV panel without a selected key: no affordance at all.
    let mut probe = Probe::default();
    probe.toolbar.show_ai = true;
    probe.toolbar.kv_panel = Some(KvPanelState::default());
    let rt = runtime(probe, (960.0, 640.0));
    assert!(!has_key(&rt, "conn-toolbar-ai"), "no key in scope hides AI");

    // Blank key counts as no key.
    let mut probe = Probe::default();
    probe.toolbar.show_ai = true;
    probe.toolbar.kv_panel = Some(KvPanelState {
        selected_key: Some("   ".to_owned()),
    });
    let rt = runtime(probe, (960.0, 640.0));
    assert!(!has_key(&rt, "conn-toolbar-ai"), "blank key hides AI");

    // A real key brings it back, with an active state when aiChatOpen.
    let mut probe = Probe::default();
    probe.toolbar.show_ai = true;
    probe.toolbar.kv_panel = Some(KvPanelState {
        selected_key: Some("user:42".to_owned()),
    });
    probe.toolbar.ai_chat_open = true;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(has_key(&rt, "conn-toolbar-ai"), "keyed KV panel shows AI");
    let button = element(&rt, "conn-toolbar-ai");
    assert!(button.focusable, "AI button is enabled");
    assert!(!has_key(&rt, "conn-toolbar-kv-context-bar"), "no KV slot here");
}

#[test]
fn toolbar_conditional_actions_and_compact_labels_follow_react() {
    // Export needs both showBatchExport and showNewQuery.
    let mut probe = Probe::default();
    probe.toolbar.show_new_query = false;
    probe.toolbar.show_batch_export = true;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(!has_key(&rt, "conn-toolbar-export"), "batch export alone is not enough");

    let mut probe = Probe::default();
    probe.toolbar.show_new_query = false;
    probe.toolbar.show_batch_export = true;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(!has_key(&rt, "conn-toolbar-export"), "no new query, no export");

    // showObjects mounts both objects and privileges.
    let mut probe = Probe::default();
    probe.toolbar.show_objects = true;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(has_key(&rt, "content-toolbar-objects"));
    assert!(has_key(&rt, "content-toolbar-privileges"));

    // Compact hides the visible label; expanded renders it.
    let mut probe = Probe::default();
    probe.toolbar.compact = true;
    let rt = runtime(probe, (960.0, 640.0));
    let labels: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    assert!(
        !labels.iter().any(|label| label == t("common.newQuery")),
        "compact toolbar has no expanded labels: {labels:?}"
    );
    let mut probe = Probe::default();
    probe.toolbar.compact = false;
    let rt = runtime(probe, (960.0, 640.0));
    let labels: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    assert!(labels.iter().any(|label| label == t("common.newQuery")));
}

#[test]
fn toolbar_presses_reach_every_host_callback() {
    let mut probe = Probe::default();
    probe.toolbar = all_actions();
    probe.toolbar.compact = false;
    let mut rt = runtime(probe, (1440.0, 816.0));
    for (key, action) in [
        ("conn-toolbar-new-query", "new-query"),
        ("content-toolbar-new-table", "new-table"),
        ("content-toolbar-er-diagram", "er"),
        ("content-toolbar-objects", "objects"),
        ("content-toolbar-privileges", "privileges"),
        ("conn-toolbar-export", "export"),
        ("content-toolbar-docs", "docs"),
        ("conn-toolbar-ai", "ai"),
        ("content-toolbar-detail-toggle", "detail"),
    ] {
        click(&mut rt, key);
        assert_eq!(rt.state.actions.last().copied(), Some(action), "{key} reached its callback");
    }
    assert_eq!(rt.state.actions.len(), 9, "each action fired exactly once");
}

#[test]
fn expanded_min_width_follows_the_react_button_arithmetic() {
    assert_eq!(estimate_expanded_toolbar_width(0, 0.0), 32.0);
    assert_eq!(estimate_expanded_toolbar_width(1, 0.0), 32.0 + 96.0);
    assert_eq!(estimate_expanded_toolbar_width(2, 0.0), 32.0 + 192.0 + 8.0);

    let base = ContentToolbarVm::default();
    let none = content_toolbar_expanded_min_width(&base);
    let mut one = base.clone();
    one.show_new_query = true;
    let with_query = content_toolbar_expanded_min_width(&one);
    assert_eq!(with_query - none, 96.0, "one left button adds one expanded width");

    let mut objects = base.clone();
    objects.show_objects = true;
    let with_objects = content_toolbar_expanded_min_width(&objects);
    assert_eq!(with_objects - none, 2.0 * 96.0 + 8.0, "objects counts as two buttons");

    let mut detail = base.clone();
    detail.detail_panel_applicable = true;
    assert_eq!(
        content_toolbar_expanded_min_width(&detail) - none,
        36.0,
        "detail toggle adds its fixed width"
    );

    let mut export_only = base;
    export_only.show_batch_export = true;
    assert_eq!(
        content_toolbar_expanded_min_width(&export_only),
        none,
        "batch export without new query adds nothing"
    );
}

#[test]
fn row_count_text_never_turns_unknown_into_zero() {
    assert_eq!(format_row_count(0), "0");
    assert_eq!(format_row_count(1_234), "1,234");
    assert_eq!(format_row_count(999_999), "999,999");
    assert_eq!(format_row_count(1_000_000), "1.0M");
    assert_eq!(format_row_count(1_200_000), "1.2M");
    assert_eq!(format_row_count(1_000_000_000), "1.0B");

    let vm = |kind, rows| StatusBarVm {
        total_rows_kind: kind,
        total_rows: rows,
        ..Default::default()
    };
    assert_eq!(vm(CountStrategy::Unsupported, 99).row_count_text(), Some(t("pagination.rowsUnknown").to_owned()));
    assert_eq!(
        vm(CountStrategy::Estimated, 1_200_000).row_count_text(),
        Some(format!("{} {}", t("pagination.rowCountEstimated").replace("{count}", "1.2M"), t("connWin.rowCount")))
    );
    assert_eq!(
        vm(CountStrategy::Exact, 1_234).row_count_text(),
        Some(format!("1,234 {}", t("connWin.rowCount")))
    );
    assert_eq!(vm(CountStrategy::Exact, 0).row_count_text(), None, "zero rows shows nothing");
}

#[test]
fn status_bar_context_cluster_joins_only_present_facts() {
    let vm = StatusBarVm {
        database_label: Some("SQLite".to_owned()),
        connection_name: Some("Local".to_owned()),
        current_database: Some("main".to_owned()),
        table_name: Some("users".to_owned()),
        column_count: 3,
        total_rows: 1_234,
        total_rows_kind: CountStrategy::Exact,
        ..Default::default()
    };
    let expected = format!(
        "SQLite · Local · main · users · 3 {} · 1,234 {}",
        t("connWin.fields"),
        t("connWin.rowCount")
    );
    assert_eq!(vm.context_text(), expected);

    let sparse = StatusBarVm {
        connection_name: Some(String::new()),
        table_name: Some("orders".to_owned()),
        total_rows: 0,
        ..Default::default()
    };
    assert_eq!(sparse.context_text(), "orders", "empty parts are filtered out");
}

#[test]
fn status_bar_shortcuts_follow_the_platform_modifier() {
    let mac = StatusBarVm {
        mod_key: "⌘".to_owned(),
        key_sep: String::new(),
        ..Default::default()
    };
    assert_eq!(
        mac.shortcuts_text(),
        format!(
            "⌘N {} · ⌘W {} · Space {}",
            t("common.newQuery"),
            t("common.close"),
            t("detail.title")
        )
    );
    let other = StatusBarVm {
        mod_key: "Ctrl".to_owned(),
        key_sep: "+".to_owned(),
        ..Default::default()
    };
    assert!(other.shortcuts_text().starts_with("Ctrl+N "), "non-mac uses a separator");
    assert!(other.shortcuts_text().contains("Ctrl+W "));
}

#[test]
fn status_bar_keys_and_geometry_hold_at_both_sizes_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = Probe::default();
            probe.dark = dark;
            probe.toolbar = all_actions();
            probe.status = StatusBarVm {
                connected: true,
                connected_label: t("conn.connected").to_owned(),
                table_name: Some("users".to_owned()),
                column_count: 3,
                total_rows: 1_234,
                total_rows_kind: CountStrategy::Exact,
                mod_key: "⌘".to_owned(),
                key_sep: String::new(),
                ..Default::default()
            };
            let rt = runtime(probe, (w, h));
            assert_in_window(&rt, w, h, &format!("bars {w}x{h} dark={dark}"));

            for key in [
                TOOLBAR,
                "conn-toolbar-new-query",
                "content-toolbar-new-table",
                "conn-toolbar-ai",
                STATUS,
                "status-connection-dot",
                "status-connection-label",
                "status-context",
                "status-shortcuts",
            ] {
                assert!(has_key(&rt, key), "key at {w}x{h} dark={dark}: {key}");
            }
            assert_eq!(element(&rt, STATUS).role, "status");
            assert!((element(&rt, STATUS).rect[3] - 40.0).abs() < 1.0, "40pt status bar");
            assert_eq!(
                element(&rt, "status-connection-label").text.as_deref(),
                Some(t("conn.connected"))
            );
        }
    }
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Returning `96.0` for the `showObjects` branch in
//    `content_toolbar_expanded_min_width` made
//    `expanded_min_width_follows_the_react_button_arithmetic` fail with:
//    `left: 96.0 right: 200.0` (objects counts as two buttons).
// 2. Mapping `CountStrategy::Unsupported` to `None` in `row_count_text` made
//    `row_count_text_never_turns_unknown_into_zero` fail with:
//    `left: None right: Some("行数不可用")`.
