//! T100 independent audit: fresh vectors for the query error panel and
//! the query context selectors.
//!
//! Deliberately different from the coder's `query_context_and_error.rs`:
//! a multi-line error message, the minimum window in light theme with
//! only `onDiagnose` armed (the stethoscope branch), a retry+ask-in-chat
//! combination with no explain, a single-label path hierarchy with no
//! selects, a fresh flat database choose, and a fresh level-1 choose in
//! the path form. One of the coder's numbers (the 1500ms copied
//! deadline) is reproduced from the audit's own clock reading.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};
use rune_kit::root;

use crate::components::query::{
    level_label_key, path_hierarchy_placeholder_segments, PathHierarchySegment,
    QueryContextSelectors, QueryContextSelectorsState, QueryErrorPanel, COPIED_FEEDBACK_MS,
};
use crate::i18n::t;
use crate::ui::kit::kit;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

// ---- the error panel, fresh vectors ------------------------------------------

#[derive(Default)]
struct Probe {
    copied_until: f64,
    copied_calls: Vec<f64>,
    diagnose: u32,
    retry: u32,
    ask_in_chat: u32,
}

const AUDIT_MESSAGE: &str = "ERROR: relation \"t_user\" does not exist\nLINE 1: SELECT * FROM t_user;\n                              ^";

fn panel_view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    let panel = QueryErrorPanel::<Probe>::new(AUDIT_MESSAGE)
        .copied(state.copied_until > 0.0)
        .on_diagnose(|s: &mut Probe| s.diagnose += 1)
        .on_retry(|s: &mut Probe| s.retry += 1)
        .on_ask_in_chat(|s: &mut Probe| s.ask_in_chat += 1)
        .on_copy_tx(|s: &mut Probe, tx: &mut rune::Tx| {
            s.copied_calls.push(tx.clock());
            s.copied_until = tx.clock() + COPIED_FEEDBACK_MS as f64 / 1000.0;
        });
    col().key("audit-page").w_full().h_full().padding(16.0).child(panel.build(dark))
}

fn element<S: 'static>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

fn has<S: 'static>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click<S: 'static>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key}: is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

#[test]
fn audit_the_diagnose_branch_and_multi_line_message_at_min_light() {
    let mut rt = Runtime::new(Probe::default(), panel_view, MIN, Env { system_dark: false, scale: 1.0 });
    rt.frame(0.0);
    // Only onDiagnose is armed (no onExplain): the stethoscope branch.
    assert_eq!(
        element(&rt, "query-explain-error").label.as_deref(),
        Some(t("diagnosis.diagnose")),
        "the fallback label is the diagnose one"
    );
    // The multi-line message renders whole: both lines are in the tree.
    let message = element(&rt, "query-error-message");
    assert_eq!(
        message.text.as_deref(),
        Some(AUDIT_MESSAGE),
        "the message keeps its newlines"
    );
    // The wash and radius the coder asserted in dark, here in light.
    let k = kit(false);
    assert_eq!(element(&rt, "query-error-panel").fill, k.alpha(k.c.danger.fg, 0.1));
    assert_eq!(element(&rt, "query-error-panel").radius, k.radius.sm);
    // The actions that exist here: explain (diagnose), retry, ask-in-chat.
    for key in ["query-explain-error", "query-retry", "query-ask-in-chat"] {
        assert!(has(&rt, key), "{key} mounts");
    }
    // fix SQL was never armed: absent.
    assert!(!has(&rt, "query-fix-sql"), "query-fix-sql does not mount without onFixSql");
    // Each press fires its own callback and nothing else.
    click(&mut rt, "query-retry");
    assert_eq!(rt.state.retry, 1);
    click(&mut rt, "query-ask-in-chat");
    assert_eq!(rt.state.ask_in_chat, 1);
    click(&mut rt, "query-explain-error");
    assert_eq!(rt.state.diagnose, 1, "the fallback fired on the explain row");
    assert_eq!(rt.state.copied_calls.len(), 0);
}

#[test]
fn audit_copy_reproduces_the_1500ms_deadline_from_its_own_clock() {
    let mut rt = Runtime::new(Probe::default(), panel_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    click(&mut rt, "query-copy-error");
    assert_eq!(rt.state.copied_calls.len(), 1, "one copy press");
    // The coder's number, from the audit's own clock: deadline = clock + 1.5.
    assert_eq!(
        rt.state.copied_until,
        rt.state.copied_calls[0] + 1.5,
        "copied_until is the copy clock plus 1500ms"
    );
    // Before the deadline the label is `common.copied`; the probe's
    // copied flag is `copied_until > 0`, so it is already flipped here.
    assert_eq!(
        element(&rt, "query-copy-error").label.as_deref(),
        Some(t("common.copied")),
        "the control shows the copied label while copied_until is set"
    );
}

// ---- the selectors, fresh vectors ---------------------------------------------

#[derive(Default)]
struct SelectorsProbe {
    selectors: QueryContextSelectorsState,
    selected: Vec<(usize, String)>,
    segments: Vec<PathHierarchySegment>,
    databases: Vec<String>,
    current_database: Option<String>,
    context_schema: Option<String>,
}

fn flat_view(state: &SelectorsProbe, cx: &mut Cx<SelectorsProbe>) -> El {
    let dark = cx.system_dark();
    let mut layer = Vec::new();
    let selectors = QueryContextSelectors::<SelectorsProbe>::new(
        true,
        &state.databases,
        state.current_database.as_deref(),
        state.context_schema.as_deref(),
    )
    .flat_state(&state.selectors)
    .host_mut(|s: &mut SelectorsProbe| &mut s.selectors)
    .on_select_level(|s: &mut SelectorsProbe, index, value| {
        s.selected.push((index, value.to_string()));
    })
    .build(dark, &mut layer)
    .expect("flat mounts");
    root(col().key("audit-page").w_full().h_full().padding(16.0).child(selectors), layer)
}

fn path_view(state: &SelectorsProbe, cx: &mut Cx<SelectorsProbe>) -> El {
    let dark = cx.system_dark();
    let mut layer = Vec::new();
    let selectors = QueryContextSelectors::<SelectorsProbe>::path_hierarchy(
        &state.segments,
        &state.selectors,
    )
    .host_mut(|s: &mut SelectorsProbe| &mut s.selectors)
    .on_select_level(|s: &mut SelectorsProbe, index, value| {
        s.selected.push((index, value.to_string()));
    })
    .build(dark, &mut layer)
    .expect("path mounts");
    root(col().key("audit-page").w_full().h_full().padding(16.0).child(selectors), layer)
}

#[test]
fn audit_a_label_only_hierarchy_mounts_no_selects() {
    // Every level single-option: labels only, no compact selects, and no
    // separators can be selects-only — they still join the labels.
    let probe = SelectorsProbe {
        segments: vec![
            PathHierarchySegment::Label { name: "edge".to_string() },
            PathHierarchySegment::Label { name: "analytics".to_string() },
        ],
        ..Default::default()
    };
    let mut rt = Runtime::new(probe, path_view, MIN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    assert!(has(&rt, "query-context-selectors"));
    assert_eq!(
        element(&rt, "query-context-path-label-0").text.as_deref(),
        Some("edge")
    );
    assert_eq!(
        element(&rt, "query-context-path-label-1").text.as_deref(),
        Some("analytics")
    );
    assert_eq!(
        element(&rt, "query-context-separator-1").text.as_deref(),
        Some("/"),
        "the separator joins the two labels"
    );
    assert!(!has(&rt, "query-context-level-0-trigger"), "no select mounts for a label-only path");
}

#[test]
fn audit_a_fresh_flat_database_choose_delivers_level_zero() {
    // Three databases, none current (the placeholder shows), choose the
    // third: onSelectLevel(0, 'warehouse').
    let probe = SelectorsProbe {
        databases: vec![
            "oltp".to_string(),
            "olap".to_string(),
            "warehouse".to_string(),
        ],
        ..Default::default()
    };
    let mut rt = Runtime::new(probe, flat_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    let trigger = element(&rt, "query-context-database-trigger");
    assert_eq!(trigger.role, "combobox");
    // No current database: the placeholder label shows (React's
    // `currentDatabase ?? ''`).
    assert_eq!(trigger.label.as_deref(), Some(t("query.database")), "the placeholder labels the select");
    // Open and choose `warehouse` through a press on the trigger then the
    // option row.
    click(&mut rt, "query-context-database-trigger");
    assert_eq!(rt.state.selectors.open_level, Some(0), "the trigger press opened level 0");
    let option = rt
        .elements()
        .into_iter()
        .find(|e| {
            e.role == "option"
                && rt
                    .elements()
                    .iter()
                    .any(|t| t.path == format!("{}/label", e.path) && t.text.as_deref() == Some("warehouse"))
        })
        .expect("the warehouse option is laid out");
    let (x, y) = (option.rect[0] + option.rect[2] / 2.0, option.rect[1] + option.rect[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(rt.state.selected, vec![(0, "warehouse".to_string())]);
    assert_eq!(rt.state.selectors.open_level, None, "the choose closed the listbox");
    // The flat form records no focus advance.
    assert_eq!(rt.state.selectors.focus_level, None, "the flat form advances no focus");
}

#[test]
fn audit_a_fresh_level_two_choose_and_the_placeholder_keys() {
    // The placeholder segments from the library constant: two disabled
    // selects at levels 0 and 1 with the catalog/schema placeholders.
    let placeholder = path_hierarchy_placeholder_segments();
    assert_eq!(placeholder.len(), 2);
    assert_eq!(level_label_key(0, true), "query.catalog", "level 0 in hierarchy mode is the catalog");
    assert_eq!(level_label_key(1, true), "query.schema", "the rest are schema");
    assert_eq!(level_label_key(0, false), "query.database", "flat level 0 is the database");
    assert_eq!(level_label_key(2, false), "query.schema", "deep flat levels are schema");
    assert_eq!(level_label_key(4, false), "query.schema", "past the catalog: still schema");

    // A deep hierarchy: level 2 is the last select; choosing from it
    // records focus level 3.
    let probe = SelectorsProbe {
        segments: vec![
            PathHierarchySegment::Label { name: "558".to_string() },
            PathHierarchySegment::Select {
                level_index: 0,
                options: vec!["hive".to_string()],
                value: "hive".to_string(),
            },
            PathHierarchySegment::Select {
                level_index: 2,
                options: vec!["gold".to_string(), "logs".to_string()],
                value: String::new(),
            },
        ],
        ..Default::default()
    };
    let mut rt = Runtime::new(probe, path_view, DESIGN, Env { system_dark: false, scale: 1.0 });
    rt.frame(0.0);
    // Level indices need not be contiguous: level 2 mounts at its own key.
    assert!(has(&rt, "query-context-level-2-trigger"));
    rt.state.selectors.open_level = Some(2);
    rt.settle_all();
    rt.frame(0.0);
    let option = rt
        .elements()
        .into_iter()
        .find(|e| {
            e.role == "option"
                && rt
                    .elements()
                    .iter()
                    .any(|t| t.path == format!("{}/label", e.path) && t.text.as_deref() == Some("logs"))
        })
        .expect("the logs option is laid out");
    let (x, y) = (option.rect[0] + option.rect[2] / 2.0, option.rect[1] + option.rect[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(rt.state.selected, vec![(2, "logs".to_string())], "onSelectLevel(2, 'logs')");
    assert_eq!(rt.state.selectors.focus_level, Some(3), "the path form advances focus past the level");
}
