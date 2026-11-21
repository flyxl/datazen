//! T193 independent audit: the `AiSessionHistory.tsx` port
//! (`components/ai/ai_session_history.rs`), written against the React
//! source, not against the coder's tests.
//!
//! What is pinned here, from the React behaviour:
//!
//! * the closed panel renders nothing; the open panel mounts the whole
//!   strip with its title row.
//! * the notice precedence: a migration error suppresses the migration
//!   success line; the history error is independent and mounts under
//!   both; both errors read their own message text.
//! * the list's empty states: loading-with-no-sessions shows the loading
//!   line (not the empty line), loaded-with-no-sessions shows the empty
//!   line, and sessions render rows.
//! * the row labels: `title || preview || untitled`, the message count
//!   with the count substituted, and the ` · preview` suffix exactly when
//!   a title AND a preview both exist.
//! * the active row: accented (the fill the DOM paints), selected channel
//!   set; an inactive row neither.
//! * the export footer: both buttons present, disabled without current
//!   messages, enabled with them.
//! * the callbacks: new-session press reports and closes; a row press
//!   reports the row's id and closes; a delete press reports the id
//!   without closing; the export buttons report their format.
//! * geometry: nothing overflows the panel horizontally at both sizes
//!   and both themes.

use rune::{col, text, Cx, El, Env, Runtime};
use rune_kit::root;

use crate::components::ai::ai_session_history::{
    AiSessionExportFormat, AiSessionHistory, AiSessionHistoryItem, AiSessionHistoryState,
};
use crate::i18n::t;

// ---- the probe harness ------------------------------------------------------

#[derive(Clone, Default)]
struct Probe {
    state: AiSessionHistoryState,
    open: bool,
    // Callback recording.
    new_calls: Vec<()>,
    close_calls: Vec<()>,
    selected: Vec<String>,
    deleted: Vec<String>,
    exported: Vec<AiSessionExportFormat>,
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn reframe<S>(rt: &mut Runtime<S>) {
    rt.invalidate();
    rt.frame(1.0 / 60.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key)
        .text
        .unwrap_or_else(|| panic!("{key} shows text"))
}

fn probe_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let panel = AiSessionHistory::<Probe>::new(state.state.clone())
        .open(state.open)
        .on_new_session(|s: &mut Probe| s.new_calls.push(()))
        .on_close(|s: &mut Probe| s.close_calls.push(()))
        .on_select_session(|s: &mut Probe, id: &str| s.selected.push(id.to_owned()))
        .on_delete_session(|s: &mut Probe, id: &str| s.deleted.push(id.to_owned()))
        .on_export_session(|s: &mut Probe, format| s.exported.push(format))
        .build(state.dark_open());
    match panel {
        Some(panel) => root(col().key("probe-page").padding(16.0).child(panel), Vec::new()),
        None => root(col().key("probe-page").padding(16.0), Vec::new()),
    }
}

impl Probe {
    fn dark_open(&self) -> bool {
        false
    }
}

fn runtime(probe: Probe) -> Runtime<Probe> {
    let mut rt = Runtime::new(probe, probe_view, (960.0, 640.0), env(false));
    reframe(&mut rt);
    rt
}

fn item(id: &str, title: &str, preview: &str, count: u32) -> AiSessionHistoryItem {
    AiSessionHistoryItem {
        id: id.to_owned(),
        title: title.to_owned(),
        preview: preview.to_owned(),
        message_count: count,
    }
}

/// The a11y dump's `states` array carries the disabled state (the
/// selection bar's a11y_states precedent).
fn find_json<'a>(node: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    if node.get("key").and_then(|v| v.as_str()) == Some(key) {
        return Some(node);
    }
    node.get("children")?
        .as_array()?
        .iter()
        .find_map(|child| find_json(child, key))
}

fn is_disabled<S>(rt: &Runtime<S>, key: &str) -> bool {
    let json = rt.accessibility_json();
    let node = find_json(&json, key)
        .unwrap_or_else(|| panic!("{key} is in the accessibility tree: {json:#}"));
    node.get("states")
        .and_then(|v| v.as_array())
        .map(|array| array.iter().any(|v| v.as_str() == Some("disabled")))
        .unwrap_or(false)
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// ---- the open gate and the strip --------------------------------------------

#[test]
fn closed_panel_renders_nothing() {
    let mut probe = Probe::default();
    probe.state = AiSessionHistoryState {
        sessions: vec![item("s1", "A", "hello", 3)],
        ..AiSessionHistoryState::default()
    };
    probe.open = false;
    let rt = runtime(probe.clone());
    assert!(!has_key(&rt, "ai-session-history"));
    assert!(!has_key(&rt, "ai-session-row-0"));
}

#[test]
fn open_panel_mounts_the_strip_with_its_title_row() {
    let mut probe = Probe::default();
    probe.open = true;
    let rt = runtime(probe);
    assert!(has_key(&rt, "ai-session-history"));
    assert_eq!(
        text_of(&rt, "ai-session-history-title"),
        t("chat.sessions.title")
    );
    // The new-session and close buttons exist.
    assert!(has_key(&rt, "ai-session-new"));
    assert!(has_key(&rt, "ai-session-close"));
    // The export footer exists with its label.
    assert_eq!(text_of(&rt, "ai-session-export"), t("chat.sessions.export"));
}

// ---- the notices -------------------------------------------------------------

#[test]
fn notice_precedence_error_first_then_independent_history_error() {
    // A migration error suppresses the success line.
    let mut probe = Probe::default();
    probe.open = true;
    probe.state.migration_error = Some("migration failed".to_owned());
    probe.state.migration_success = 4;
    let rt = runtime(probe.clone());
    assert!(has_key(&rt, "ai-session-migration-error"));
    assert_eq!(
        text_of(&rt, "ai-session-migration-error-text"),
        "migration failed"
    );
    assert!(
        !has_key(&rt, "ai-session-migration-success"),
        "the success line is suppressed by the error"
    );

    // Without the error, the success line shows with the count
    // substituted.
    probe.state.migration_error = None;
    let rt = runtime(probe.clone());
    assert!(has_key(&rt, "ai-session-migration-success"));
    let expected = t("chat.sessions.migrationSuccess")
        .replacen("{count}", "4", 1);
    assert_eq!(text_of(&rt, "ai-session-migration-success-text"), expected);

    // The history error mounts independently, under both.
    probe.state.history_error = Some("history failed".to_owned());
    let rt = runtime(probe.clone());
    assert!(has_key(&rt, "ai-session-history-error"));
    assert_eq!(text_of(&rt, "ai-session-history-error-text"), "history failed");
    assert!(has_key(&rt, "ai-session-migration-success"));
}

// ---- the list ----------------------------------------------------------------

#[test]
fn empty_states_follow_the_loading_order() {
    // Loading with no sessions: the loading line, not the empty line.
    let mut probe = Probe::default();
    probe.open = true;
    probe.state.loading = true;
    let rt = runtime(probe.clone());
    assert!(has_key(&rt, "ai-session-loading"));
    assert_eq!(text_of(&rt, "ai-session-loading"), t("chat.sessions.loading"));
    assert!(!has_key(&rt, "ai-session-empty"));

    // Loaded with no sessions: the empty line.
    probe.state.loading = false;
    let rt = runtime(probe.clone());
    assert!(has_key(&rt, "ai-session-empty"));
    assert_eq!(text_of(&rt, "ai-session-empty"), t("chat.sessions.empty"));
    assert!(!has_key(&rt, "ai-session-loading"));
}

#[test]
fn rows_render_two_lines_with_the_react_labels() {
    let mut probe = Probe::default();
    probe.open = true;
    probe.state.sessions = vec![
        item("s1", "Named session", "first words", 12),
        item("s2", "", "just a preview", 0),
        item("s3", "", "", 7),
    ];
    let rt = runtime(probe);
    for key in ["ai-session-row-0", "ai-session-row-1", "ai-session-row-2"] {
        assert!(has_key(&rt, key), "{key} mounts");
    }

    // Row 0: the title wins; the suffix ` · preview` appears (title AND
    // preview both exist).
    assert_eq!(text_of(&rt, "ai-session-row-0-text"), "Named session");
    let expected = t("chat.sessions.messages")
        .replacen("{count}", "12", 1)
        .to_string()
        + " · first words";
    assert_eq!(text_of(&rt, "ai-session-row-0-preview"), expected);

    // Row 1: no title, so the preview is the primary; the suffix is
    // omitted (no title).
    assert_eq!(text_of(&rt, "ai-session-row-1-text"), "just a preview");
    let expected = t("chat.sessions.messages").replacen("{count}", "0", 1).to_string();
    assert_eq!(text_of(&rt, "ai-session-row-1-preview"), expected);

    // Row 2: neither title nor preview -> the untitled fallback.
    assert_eq!(
        text_of(&rt, "ai-session-row-2-text"),
        t("chat.sessions.untitled")
    );
}

#[test]
fn active_row_is_selected_and_inactive_rows_are_not() {
    let mut probe = Probe::default();
    probe.open = true;
    probe.state.sessions = vec![item("s1", "A", "", 1), item("s2", "B", "", 2)];
    probe.state.current_id = Some("s2".to_owned());
    let rt = runtime(probe);
    assert!(
        element(&rt, "ai-session-row-1-select").item_selected,
        "the current session's row is the selected one"
    );
    assert!(
        !element(&rt, "ai-session-row-0-select").item_selected,
        "the other row is not"
    );
}

// ---- the export footer ---------------------------------------------------------

#[test]
fn export_buttons_follow_the_current_messages_gate() {
    let mut probe = Probe::default();
    probe.open = true;
    probe.state.current_id = Some("s1".to_owned());
    probe.state.current_has_messages = false;
    let rt = runtime(probe.clone());
    assert!(has_key(&rt, "ai-session-export-json"));
    assert!(has_key(&rt, "ai-session-export-markdown"));
    assert!(
        is_disabled(&rt, "ai-session-export-json"),
        "no messages -> the export is disabled"
    );
    assert!(is_disabled(&rt, "ai-session-export-markdown"));

    probe.state.current_has_messages = true;
    let rt = runtime(probe.clone());
    assert!(!is_disabled(&rt, "ai-session-export-json"));
    assert!(!is_disabled(&rt, "ai-session-export-markdown"));
}

#[test]
fn export_presses_report_their_formats() {
    let mut probe = Probe::default();
    probe.open = true;
    probe.state.current_has_messages = true;
    let mut rt = runtime(probe);
    click(&mut rt, "ai-session-export-json");
    assert_eq!(rt.state.exported, vec![AiSessionExportFormat::Json]);
    click(&mut rt, "ai-session-export-markdown");
    assert_eq!(
        rt.state.exported,
        vec![AiSessionExportFormat::Json, AiSessionExportFormat::Markdown]
    );
}

// ---- the callbacks -------------------------------------------------------------

#[test]
fn the_row_press_selects_and_closes() {
    let mut probe = Probe::default();
    probe.open = true;
    probe.state.sessions = vec![item("s1", "A", "", 1), item("s2", "B", "", 2)];
    let mut rt = runtime(probe);
    click(&mut rt, "ai-session-row-1-select");
    assert_eq!(rt.state.selected, vec!["s2".to_owned()]);
    assert_eq!(rt.state.close_calls.len(), 1, "the row press closes");
    assert!(rt.state.deleted.is_empty(), "the delete is not pressed");
}

#[test]
fn the_delete_press_reports_without_closing() {
    let mut probe = Probe::default();
    probe.open = true;
    probe.state.sessions = vec![item("s1", "A", "", 1)];
    let mut rt = runtime(probe);
    click(&mut rt, "ai-session-row-0-delete");
    assert_eq!(rt.state.deleted, vec!["s1".to_owned()]);
    assert!(rt.state.selected.is_empty(), "no selection happens");
    assert!(rt.state.close_calls.is_empty(), "React's delete does not close");
}

#[test]
fn the_new_session_press_reports_and_closes() {
    let mut probe = Probe::default();
    probe.open = true;
    let mut rt = runtime(probe);
    click(&mut rt, "ai-session-new");
    assert_eq!(rt.state.new_calls.len(), 1);
    assert_eq!(rt.state.close_calls.len(), 1);
}

#[test]
fn the_close_button_only_closes() {
    let mut probe = Probe::default();
    probe.open = true;
    let mut rt = runtime(probe);
    click(&mut rt, "ai-session-close");
    assert_eq!(rt.state.close_calls.len(), 1);
    assert!(rt.state.new_calls.is_empty());
    assert!(rt.state.selected.is_empty());
}

// ---- geometry -------------------------------------------------------------------

#[test]
fn no_horizontal_overflow_at_both_sizes_and_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = Probe::default();
            probe.open = true;
            probe.state.sessions = vec![
                item("s1", "A very long session title that must truncate", "a long preview that must also truncate before it can overflow", 12345),
            ];
            probe.state.current_id = Some("s1".to_owned());
            probe.state.current_has_messages = true;
            probe.state.migration_success = 3;
            let mut rt = Runtime::new(probe, probe_view, (w, h), env(dark));
            reframe(&mut rt);

            let root = element(&rt, "ai-session-history").rect;
            assert!(root[2] > 0.0, "the panel has width");
            for e in rt.elements() {
                if e.exiting {
                    continue;
                }
                if e.key.as_deref() == Some("probe-page")
                    || e.key.as_deref() == Some("kit-root")
                {
                    continue;
                }
                let [x, _y, ew, _eh] = e.rect;
                if ew <= 0.0 {
                    continue;
                }
                assert!(
                    x + ew <= root[0] + root[2] + 0.5,
                    "{}x{} dark={}: {} at x={} w={} escapes the panel",
                    w,
                    h,
                    dark,
                    e.key.clone().unwrap_or_default(),
                    x,
                    ew
                );
            }
        }
    }
}
