//! T009-S6: AiChatPanel shell + session wiring (`AiChatPanel.tsx`
//! 93–404, 385–949).
//!
//! Pure-logic tests call the state machine directly; runtime tests
//! render the assembled panel in a probe `Runtime` and read back the
//! laid-out element tree — never the component's own report of itself.
//! Geometry is asserted on `rt.elements()` rects, which come from the
//! layout engine, not from the code under test; the unconfigured text
//! is pinned against the literal zh-CN text from
//! `packages/frontend/src/locales/zh-CN/ai.ts` / `core.ts` /
//! `settings.ts`.
//!
//! The panel is a view-model placeholder for the `aiStore` slice
//! (T088): the tests seed the state through its `set_*` methods and
//! assert the outboxes (sent messages, consumed draft ids, cancelled
//! request ids, memory-create requests) record the calls, because no
//! store exists to read back yet.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::ai::{
    AiAgentMode, AiChatContextSnapshot, AiChatMessage, AiChatPanel, AiChatPanelState,
    AiChatResultPreview, AiChatRole, AiChatTab, AiMemoryMode, AiMemoryReference, AiMemoryScope,
    AiPendingDraft, AiProfile, AiPromptTemplate, AiQuestion, AiSafetyGate, ConflictResolution,
};
use crate::i18n::t;

// Literal zh-CN text from packages/frontend/src/locales/zh-CN/ai.ts,
// core.ts and settings.ts — pinned here, not through `t()`.
const NOT_CONFIGURED: &str = "请先在设置中配置 AI 服务";
const GO_TO_CONFIGURE: &str = "前往配置";
const MEMORY_DISABLED: &str = "记忆已关闭，请先在设置 → AI → 记忆 中启用。";
const REMEMBERED: &str = "已加入记忆候选队列。";
const REMEMBER_FAILED: &str = "保存记忆候选失败。";
const APPEND_SEPARATOR: &str = "\n\n---\n\n";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn focused_key<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

/// Tab through the focus order until `key` is focused (at most 60 stops),
/// so keyboard tests do not depend on the exact tab order.
fn focus_by_tab<S>(rt: &mut Runtime<S>, key: &str) {
    for _ in 0..60 {
        if focused_key(rt).as_deref() == Some(key) {
            return;
        }
        press(rt, Key::Tab);
    }
    panic!("{key} never took focus after 60 Tabs");
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn element_text<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn any_text<S>(rt: &Runtime<S>, text: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(text))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

/// Every laid-out element stays inside the window horizontally.
fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let r = e.rect;
        if r[0] < -0.5 || r[0] + r[2] > w + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) horizontally out of window {w}: {outside:?}",
        outside.len()
    );
}

// ---- fixtures ---------------------------------------------------------------

fn user_msg(content: &str) -> AiChatMessage {
    AiChatMessage::new(AiChatRole::User, content)
}

fn assistant_msg(content: &str) -> AiChatMessage {
    AiChatMessage::new(AiChatRole::Assistant, content)
}

fn memory(reference: &str) -> AiMemoryReference {
    AiMemoryReference::new(
        reference,
        format!("id-{reference}"),
        "subject",
        "fact",
        "user",
    )
}

fn full_snapshot() -> AiChatContextSnapshot {
    AiChatContextSnapshot {
        editor_sql: Some("select 1".to_string()),
        selection: Some("x".to_string()),
        last_error: Some("boom".to_string()),
        result_preview: Some(AiChatResultPreview {
            columns: vec!["a".to_string()],
            rows: Vec::new(),
        }),
    }
}

fn draft(request_id: &str, content: &str) -> AiPendingDraft {
    AiPendingDraft::new(request_id, content)
}

fn question_block_msg() -> AiChatMessage {
    assistant_msg("Pick one.")
        .id("m2")
        .questions(vec![AiQuestion::new("q1", "Pick one.")
            .option("a", "A")
            .option("b", "B")])
}

struct Probe {
    panel: AiChatPanelState,
    dark: bool,
    inserted_sql: Vec<String>,
    ran_code: Vec<(String, String)>,
    new_queries: Vec<String>,
    ai_help: usize,
    workflow_help: usize,
    settings_opens: usize,
}

impl Probe {
    fn new() -> Self {
        Self {
            panel: AiChatPanelState::new("sess-1"),
            dark: true,
            inserted_sql: Vec::new(),
            ran_code: Vec::new(),
            new_queries: Vec::new(),
            ai_help: 0,
            workflow_help: 0,
            settings_opens: 0,
        }
    }

    /// Configured, with a live session — the state most tests start from.
    fn configured() -> Self {
        let mut probe = Self::new();
        probe.panel.set_is_configured(true);
        assert!(probe.panel.ensure_session());
        probe
    }

    /// Configured with messages, skills, templates, profiles, a
    /// snapshot and a KV context — the rich panel the geometry tests use.
    fn rich() -> Self {
        let mut probe = Self::configured();
        probe.panel.set_messages(vec![
            user_msg("select * from users").id("m1"),
            assistant_msg("Here are the rows.").id("m2"),
        ]);
        probe.panel.set_templates(vec![
            AiPromptTemplate::new("t1", "Review"),
            AiPromptTemplate::new("t2", "Explain"),
        ]);
        probe.panel.set_profiles(vec![
            AiProfile::new("p1", "Main", "gpt-4").is_default(true),
            AiProfile::new("p2", "Local", "llama").safety_gate(AiSafetyGate::local_trust()),
        ]);
        probe.panel.set_active_profile_id("p1");
        probe
            .panel
            .set_kv_context(Some("user:42".to_string()), Some("KEY user:42".to_string()));
        probe.panel.sync_auto_context(Some(&full_snapshot()));
        probe.panel.area_mut().set_loaded_skills(vec![
            crate::components::ai::AiLoadedSkill::new("sql-review").id("s1"),
            crate::components::ai::AiLoadedSkill::new("explain"),
        ]);
        probe
    }
}

fn panel_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let panel = AiChatPanel::<Probe>::new(&state.panel)
        .panel_mut(|s: &mut Probe| &mut s.panel)
        .on_insert_sql(|s: &mut Probe, sql: &str| s.inserted_sql.push(sql.to_string()))
        .on_run_code(|s: &mut Probe, sql: &str, dialect: &str| {
            s.ran_code.push((sql.to_string(), dialect.to_string()));
        })
        .on_new_query(|s: &mut Probe, sql: &str| s.new_queries.push(sql.to_string()))
        .on_open_ai_help(|s: &mut Probe| s.ai_help += 1)
        .on_open_workflow_help(|s: &mut Probe| s.workflow_help += 1)
        .on_open_settings(|s: &mut Probe| s.settings_opens += 1)
        .build(state.dark, &mut layer);
    rune_kit::root(
        col()
            .key("probe-page")
            .w_full()
            .h_full()
            .padding(16.0)
            .child(panel),
        layer,
    )
}

/// The same panel with the state accessor but no host callbacks: the
/// `if let Some(action)` None arms of every pass-through.
fn panel_only_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let panel = AiChatPanel::<Probe>::new(&state.panel)
        .panel_mut(|s: &mut Probe| &mut s.panel)
        .build(state.dark, &mut layer);
    rune_kit::root(
        col()
            .key("probe-page")
            .w_full()
            .h_full()
            .padding(16.0)
            .child(panel),
        layer,
    )
}

fn runtime_panel_only(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, panel_only_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

/// The same panel with no accessor and no host callbacks: the
/// children render unwired, covering the `None` branches.
fn bare_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let panel = AiChatPanel::<Probe>::new(&state.panel).build(state.dark, &mut layer);
    rune_kit::root(
        col()
            .key("probe-page")
            .w_full()
            .h_full()
            .padding(16.0)
            .child(panel),
        layer,
    )
}

fn runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, panel_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

fn runtime_bare(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, bare_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// ---- session key + init -----------------------------------------------------

#[test]
fn desired_session_key_prefers_connection_then_db_session_then_default() {
    let mut panel = AiChatPanelState::new("sess-1");
    assert_eq!(panel.desired_session_key(), "sess-1::");
    panel.set_connection_id(Some("conn-9".to_string()));
    assert_eq!(panel.desired_session_key(), "conn-9::");
    // JS `||`: an empty connection id falls through to the db session id.
    panel.set_connection_id(Some(String::new()));
    assert_eq!(panel.desired_session_key(), "sess-1::");
    panel.set_connection_id(None);
    panel.set_database(Some("mydb".to_string()));
    assert_eq!(panel.desired_session_key(), "sess-1::mydb");
    let empty = AiChatPanelState::new("");
    assert_eq!(empty.desired_session_key(), "default::");
}

#[test]
fn ensure_session_inits_once_and_rekeys_on_connection_change() {
    let mut panel = AiChatPanelState::new("sess-1");
    // No session before initChat: no welcome line renders.
    assert_eq!(panel.session_key(), None);
    assert!(!panel.list().has_session());
    assert!(panel.ensure_session());
    assert_eq!(panel.session_key(), Some("sess-1::"));
    assert!(panel.list().has_session());
    assert!(!panel.ensure_session(), "the same key does not re-init");
    // A reconnect changes dbSessionId but the connection id owns the key.
    panel.set_connection_id(Some("conn-9".to_string()));
    assert!(panel.ensure_session());
    assert_eq!(panel.session_key(), Some("conn-9::"));
    // A fresh session starts empty.
    panel.set_messages(vec![user_msg("hi").id("m1")]);
    panel.set_connection_id(Some("conn-10".to_string()));
    assert!(panel.ensure_session());
    assert!(panel.list().messages().is_empty());
    assert_eq!(panel.request_id(), None);
}

#[test]
fn template_resets_on_connection_and_database_type_change_only() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_templates(vec![AiPromptTemplate::new("t1", "Review")]);
    panel.header_mut().choose_template("t1");
    assert_eq!(panel.header().template_id(), "t1");
    // A database change alone does not reset (not in the effect's deps).
    panel.set_database(Some("mydb".to_string()));
    assert_eq!(panel.header().template_id(), "t1");
    panel.set_connection_id(Some("c1".to_string()));
    assert_eq!(
        panel.header().template_id(),
        "",
        "a connection change returns to automatic selection"
    );
    panel.header_mut().choose_template("t1");
    panel.set_database_type(Some("postgres".to_string()));
    assert_eq!(
        panel.header().template_id(),
        "",
        "a database-type change resets too"
    );
    panel.header_mut().choose_template("t1");
    panel.set_connection_id(Some("c1".to_string()));
    assert_eq!(
        panel.header().template_id(),
        "t1",
        "setting the same connection id again does not reset"
    );
    panel.set_database_type(Some("postgres".to_string()));
    assert_eq!(
        panel.header().template_id(),
        "t1",
        "setting the same database type again does not reset"
    );
}

// ---- snapshot sync ----------------------------------------------------------

#[test]
fn snapshot_sync_seeds_updates_and_never_resurrects() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.sync_auto_context(None);
    assert_eq!(panel.area().snapshot(), None);
    let full = full_snapshot();
    panel.sync_auto_context(Some(&full));
    assert_eq!(panel.area().snapshot(), Some(&full));
    // The user removes a field; the same host snapshot must not bring it back.
    panel.area_mut().remove_snapshot_field("editorSql");
    assert_eq!(panel.area().snapshot().unwrap().editor_sql, None);
    panel.sync_auto_context(Some(&full));
    assert_eq!(
        panel.area().snapshot().unwrap().editor_sql,
        None,
        "an unchanged host value never resurrects a removed field"
    );
    // A changed host value does resurrect it, with the new value.
    let mut changed = full.clone();
    changed.editor_sql = Some("select 2".to_string());
    panel.sync_auto_context(Some(&changed));
    assert_eq!(
        panel.area().snapshot().unwrap().editor_sql.as_deref(),
        Some("select 2")
    );
    // An empty host string deletes the field, as JS truthiness does.
    let mut emptied = changed.clone();
    emptied.editor_sql = Some(String::new());
    panel.sync_auto_context(Some(&emptied));
    assert_eq!(panel.area().snapshot().unwrap().editor_sql, None);
    // The host snapshot going away clears the chips.
    panel.sync_auto_context(None);
    assert_eq!(panel.area().snapshot(), None);
}

#[test]
fn snapshot_sync_goes_null_when_nothing_with_content_is_left() {
    let mut panel = AiChatPanelState::new("sess-1");
    let blank = AiChatContextSnapshot {
        editor_sql: Some("   ".to_string()),
        selection: None,
        last_error: None,
        result_preview: None,
    };
    panel.sync_auto_context(Some(&blank));
    assert_eq!(
        panel.area().snapshot(),
        None,
        "a whitespace-only snapshot has no content"
    );
}

// ---- draft bridge -----------------------------------------------------------

#[test]
fn draft_bridge_ignores_unconfigured_and_processed_requests() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.handle_draft_request(Some(&draft("d-1", "hello")));
    assert_eq!(
        panel.area().input_text(),
        "",
        "AI not configured: the draft is not consumed"
    );
    assert!(panel.consumed_draft_ids().is_empty());
    panel.set_is_configured(true);
    panel.handle_draft_request(Some(&draft("d-1", "hello")));
    assert_eq!(panel.area().input_text(), "hello");
    assert_eq!(panel.consumed_draft_ids(), ["d-1".to_string()]);
    panel.area_mut().set_input(String::new());
    panel.handle_draft_request(Some(&draft("d-1", "hello again")));
    assert_eq!(
        panel.area().input_text(),
        "",
        "an already-processed request id is ignored"
    );
    assert_eq!(panel.consumed_draft_ids().len(), 1);
}

#[test]
fn draft_bridge_parks_conflict_when_input_is_not_empty() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    panel.area_mut().set_input("old text".to_string());
    panel.handle_draft_request(Some(&draft("d-1", "new draft")));
    assert_eq!(
        panel.area().pending_conflict(),
        Some(&draft("d-1", "new draft")),
        "a non-empty input parks the draft as a conflict"
    );
    assert_eq!(panel.area().input_text(), "old text");
    assert!(
        panel.consumed_draft_ids().is_empty(),
        "a parked conflict acks nothing"
    );
    // Dismissing, then re-delivering the same request, prefills it.
    panel.dismiss_conflict();
    assert_eq!(panel.area().pending_conflict(), None);
    panel.area_mut().set_input(String::new());
    panel.handle_draft_request(Some(&draft("d-1", "new draft")));
    assert_eq!(panel.area().input_text(), "new draft");
    assert_eq!(panel.consumed_draft_ids(), ["d-1".to_string()]);
}

#[test]
fn draft_bridge_prefills_and_acks_when_not_streaming() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    panel.header_mut().set_tab(AiChatTab::Workflows);
    panel.handle_draft_request(Some(&draft("d-1", "prefill me")));
    assert_eq!(panel.area().input_text(), "prefill me");
    assert_eq!(panel.header().tab(), AiChatTab::Chat);
    assert_eq!(panel.consumed_draft_ids(), ["d-1".to_string()]);
    assert!(panel.take_focus_input_request(), "prefill asks for focus");
    assert!(!panel.take_focus_input_request(), "the flag is consumed");
}

#[test]
fn draft_bridge_prefills_without_ack_while_streaming() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_streaming(true);
    panel.handle_draft_request(Some(&draft("d-1", "prefill me")));
    assert_eq!(panel.area().input_text(), "prefill me");
    assert!(
        panel.consumed_draft_ids().is_empty(),
        "a streaming prefill defers the ack"
    );
    // The processed mark means the ack never arrives on later runs either.
    panel.set_streaming(false);
    panel.handle_draft_request(Some(&draft("d-1", "prefill me")));
    assert!(
        panel.consumed_draft_ids().is_empty(),
        "React's early return skips the deferred ack"
    );
}

#[test]
fn resolve_conflict_replaces_appends_and_acks() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    panel.area_mut().set_input("old".to_string());
    panel.handle_draft_request(Some(&draft("d-1", "new")));
    assert_eq!(
        panel.resolve_conflict(ConflictResolution::Replace),
        Some("d-1".to_string())
    );
    assert_eq!(panel.area().input_text(), "new");
    assert_eq!(panel.area().pending_conflict(), None);
    assert_eq!(panel.consumed_draft_ids(), ["d-1".to_string()]);
    assert!(panel.take_focus_input_request());

    panel.area_mut().set_input("old".to_string());
    panel.handle_draft_request(Some(&draft("d-2", "new")));
    assert_eq!(
        panel.resolve_conflict(ConflictResolution::Append),
        Some("d-2".to_string())
    );
    assert_eq!(
        panel.area().input_text(),
        format!("old{APPEND_SEPARATOR}new"),
        "append joins after the chat.draft.appendSeparator separator"
    );
    assert_eq!(panel.resolve_conflict(ConflictResolution::Replace), None);
}

// ---- send / stop / cancel ---------------------------------------------------

#[test]
fn send_gates_on_blank_and_streaming() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    assert_eq!(panel.send(), None, "an empty draft sends nothing");
    panel.area_mut().set_input("   ".to_string());
    assert_eq!(panel.send(), None, "a blank draft sends nothing");
    panel.area_mut().set_input("hello".to_string());
    panel.set_streaming(true);
    assert_eq!(panel.send(), None, "no send while streaming");
    assert_eq!(
        panel.area().input_text(),
        "hello",
        "a gated send leaves the draft alone"
    );
}

#[test]
fn send_records_the_composed_message_and_clears() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_kv_context(Some("user:42".to_string()), Some("KEY user:42".to_string()));
    panel.set_templates(vec![AiPromptTemplate::new("t1", "Review")]);
    panel.header_mut().choose_template("t1");
    panel.header_mut().set_agent_mode(AiAgentMode::Agent);
    panel.sync_auto_context(Some(&full_snapshot()));
    panel.area_mut().set_context_items_count(2);
    panel.area_mut().set_input("  hello  ".to_string());
    let sent = panel.send().expect("a non-blank draft sends");
    assert_eq!(sent.content, "hello\n\nKEY user:42");
    assert_eq!(sent.raw_content, "hello");
    assert_eq!(sent.agent_mode, AiAgentMode::Agent);
    assert_eq!(sent.template_id.as_deref(), Some("t1"));
    assert_eq!(sent.memory_mode, AiMemoryMode::Auto);
    assert_eq!(sent.memory_scope, AiMemoryScope::User);
    assert_eq!(sent.auto_context, Some(full_snapshot()));
    assert_eq!(sent.context_items, 2);
    assert_eq!(sent.target_claim, None, "the editor grant is not wired");
    assert_eq!(panel.sent_messages().len(), 1);
    assert_eq!(panel.area().input_text(), "", "the draft clears after send");
    assert_eq!(
        panel.area().snapshot(),
        None,
        "the snapshot clears after send"
    );
    assert_eq!(panel.area().context_items_count(), 0);
    assert_eq!(panel.list().messages().len(), 1);
    assert_eq!(panel.list().messages()[0].role, AiChatRole::User);
    assert_eq!(
        panel.list().messages()[0].content,
        "hello",
        "the appended bubble shows what was typed, not the composed text"
    );
    assert!(panel.header().has_messages());
}

#[test]
fn send_without_kv_or_template_or_snapshot_is_plain() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.area_mut().set_input("hello".to_string());
    let sent = panel.send().expect("a non-blank draft sends");
    assert_eq!(sent.content, "hello");
    assert_eq!(
        sent.template_id, None,
        "an empty template id sends as undefined"
    );
    assert_eq!(sent.auto_context, None);
}

#[test]
fn send_answers_submits_the_formatted_payload() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_messages(vec![user_msg("ask").id("m1"), question_block_msg()]);
    panel
        .list_mut()
        .question_state_mut(1)
        .toggle("q1", "a", false);
    let sent = panel.send_answers(1).expect("an answered block sends");
    assert_eq!(sent.content, "[q1] Pick one.\nA");
    assert_eq!(sent.template_id, None);
    assert_eq!(sent.auto_context, None);
    assert_eq!(panel.list().messages().len(), 3);
    assert_eq!(panel.send_answers(1), None, "the block cannot send twice");
    assert_eq!(
        panel.send_answers(99),
        None,
        "an unknown index sends nothing"
    );
}

#[test]
fn stop_and_unmount_cancel_record_the_in_flight_request() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.stop();
    assert!(
        panel.cancelled_request_ids().is_empty(),
        "no run in flight: cancelChat(undefined) records nothing"
    );
    panel.set_request_id(Some("r-1".to_string()));
    panel.stop();
    assert_eq!(panel.cancelled_request_ids(), ["r-1".to_string()]);
    panel.cancel_in_flight();
    assert_eq!(
        panel.cancelled_request_ids(),
        ["r-1".to_string(), "r-1".to_string()],
        "unmount cancel is a second receipt for the same run"
    );
}

#[test]
fn clear_chat_mirrors_the_store_reset() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_messages(vec![user_msg("hi").id("m1")]);
    panel.set_streaming(true);
    panel.set_stream_content("partial".to_string());
    panel.set_stream_mcp_tool_name(Some("tool/x".to_string()));
    panel.set_memory_candidate_count(Some(2));
    panel.set_request_id(Some("r-1".to_string()));
    panel
        .area_mut()
        .set_loaded_skills(vec![crate::components::ai::AiLoadedSkill::new(
            "sql-review",
        )]);
    panel.clear_chat();
    assert_eq!(panel.clear_chat_presses(), 1);
    assert!(panel.list().messages().is_empty());
    assert!(!panel.list().is_streaming());
    assert_eq!(panel.list().stream_content(), "");
    assert_eq!(panel.list().stream_mcp_tool_name(), None);
    assert_eq!(panel.list().memory_candidate_count(), None);
    assert!(panel.area().loaded_skills().is_empty());
    assert_eq!(panel.request_id(), None);
    assert!(!panel.header().has_messages());
}

// ---- remember ---------------------------------------------------------------

#[test]
fn remember_records_a_candidate_or_shows_disabled() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_memory_enabled(false);
    assert!(!panel.handle_remember(&assistant_msg("hello")));
    assert_eq!(
        panel.list().remember_notice().unwrap_or_default(),
        MEMORY_DISABLED
    );
    assert!(panel.memory_create_requests().is_empty());

    panel.set_memory_enabled(true);
    assert!(
        !panel.handle_remember(&assistant_msg("   ")),
        "blank is a no-op"
    );
    let long = format!("{}\nsecond line", "x".repeat(100));
    assert!(panel.handle_remember(&assistant_msg(&long)));
    let request = &panel.memory_create_requests()[0];
    assert_eq!(request.scope_type, "user");
    assert_eq!(request.connection_id, None);
    assert_eq!(request.kind, "preference");
    assert_eq!(request.subject.len(), 80, "the subject caps at 80 chars");
    assert_eq!(request.content, long);
    assert_eq!(request.source, "explicit");
    assert_eq!(request.source_ref.as_deref(), Some("sess-1::"));
    assert_eq!(request.status, "candidate");
    assert_eq!(request.sensitivity, "normal");
    assert_eq!(request.confidence, 0.6);
    assert_eq!(
        panel.list().remember_notice().unwrap_or_default(),
        REMEMBERED
    );

    panel.set_connection_id(Some("conn-9".to_string()));
    assert!(panel.handle_remember(&assistant_msg("\n\nFirst line\nsecond")));
    let request = &panel.memory_create_requests()[1];
    assert_eq!(request.scope_type, "connection");
    assert_eq!(request.connection_id.as_deref(), Some("conn-9"));
    assert_eq!(request.subject, "First line", "the first non-blank line");

    panel.report_remember_failed();
    assert_eq!(
        panel.list().remember_notice().unwrap_or_default(),
        REMEMBER_FAILED
    );
}

// ---- streaming guard, error normalisation, memory mode, unread -------------

#[test]
fn streaming_is_never_seeded_without_a_session() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_streaming(true);
    assert!(
        !panel.list().is_streaming() && !panel.area().is_streaming(),
        "streaming with no session is not seeded"
    );
    assert!(panel.ensure_session());
    panel.set_streaming(true);
    assert!(panel.list().is_streaming() && panel.area().is_streaming());
}

#[test]
fn empty_string_errors_normalise_to_none_at_the_seam() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_session_migration_error(Some(String::new()));
    assert_eq!(
        panel.header().session_migration_error(),
        None,
        "S3 D1: an empty migration error renders no banner"
    );
    panel.set_session_migration_error(Some("boom".to_string()));
    assert_eq!(
        panel.header().session_migration_error(),
        Some("boom"),
        "a real error still passes through"
    );
    panel.set_share_error(Some(String::new()));
    assert_eq!(
        panel.header().share_error(),
        None,
        "S3 D1: an empty share error renders no span"
    );
}

#[test]
fn effective_memory_mode_prefers_the_prop_then_the_switch() {
    let mut panel = AiChatPanelState::new("sess-1");
    assert_eq!(panel.effective_memory_mode(), AiMemoryMode::Auto);
    panel.set_memory_enabled(false);
    assert_eq!(panel.effective_memory_mode(), AiMemoryMode::Off);
    panel.set_memory_mode(Some(AiMemoryMode::Ask));
    assert_eq!(
        panel.effective_memory_mode(),
        AiMemoryMode::Ask,
        "the prop wins over the switch"
    );
}

#[test]
fn store_arrivals_count_unread_only_away_from_the_bottom() {
    let mut panel = Probe::configured().panel;
    panel.list_mut().set_at_bottom(false);
    panel.set_messages(vec![user_msg("a").id("m1"), assistant_msg("b").id("m2")]);
    assert_eq!(panel.list().unread_count(), 2);
    panel.set_messages(vec![user_msg("a").id("m1"), assistant_msg("b").id("m2")]);
    assert_eq!(
        panel.list().unread_count(),
        2,
        "re-seeding the same messages adds nothing"
    );
    panel.list_mut().set_at_bottom(true);
    assert_eq!(panel.list().unread_count(), 0);
}

#[test]
fn seeders_and_accessors_round_trip() {
    let mut panel = AiChatPanelState::new("sess-1");
    assert_eq!(panel.db_session_id(), "sess-1");
    assert_eq!(panel.database(), None);
    panel.set_database(Some("mydb".to_string()));
    assert_eq!(panel.database(), Some("mydb"));
    assert_eq!(panel.database_type(), None);
    assert_eq!(panel.sql_dialect(), None);
    panel.set_sql_dialect(Some("postgres".to_string()));
    assert_eq!(panel.sql_dialect(), Some("postgres"));
    assert_eq!(panel.connection_id(), None);
    assert_eq!(panel.editor_session_id(), None);
    panel.set_editor_session_id(Some("ed-1".to_string()));
    assert_eq!(panel.editor_session_id(), Some("ed-1"));
    assert_eq!(panel.memory_scope(), AiMemoryScope::User);
    panel.set_memory_scope(AiMemoryScope::Connection);
    assert_eq!(panel.memory_scope(), AiMemoryScope::Connection);
    assert!(panel.memory_enabled());
    panel.set_memory_mode(Some(AiMemoryMode::Ask));
    assert_eq!(panel.effective_memory_mode(), AiMemoryMode::Ask);
    assert_eq!(panel.kv_key_name(), None);
    panel.set_session_history_open(true);
    assert!(panel.header().session_history_open());
    // A None draft request is a no-op.
    panel.handle_draft_request(None);
    assert_eq!(panel.area().input_text(), "");
    // Stream seeders forward to the list.
    assert!(panel.ensure_session());
    panel.set_stream_reasoning("thinking".to_string());
    assert_eq!(panel.list().stream_reasoning(), "thinking");
    panel.set_stream_mcp_tool_name(Some("tool/x".to_string()));
    assert_eq!(panel.list().stream_mcp_tool_name(), Some("tool/x"));
    panel.set_stream_memory_used(vec![memory("M1")]);
    panel.set_session_memory_used(vec![memory("M2")]);
    assert_eq!(
        panel.list().streaming_message().memory_used,
        vec![memory("M1")],
        "the stream memory wins over the session memory"
    );
    panel.set_session_migration_success(2);
    assert_eq!(panel.header().session_migration_success(), 2);
    panel.set_can_share(true);
    panel.set_sharing(true);
    assert!(panel.header().sharing());
    panel.list_mut().set_remember_notice(Some("n".to_string()));
    panel.dismiss_remember_notice();
    assert_eq!(panel.list().remember_notice(), None);
}

// ---- unconfigured view ------------------------------------------------------

#[test]
fn unconfigured_view_shows_exact_text_and_reports_the_settings_press() {
    let rt = runtime(Probe::new(), 960.0, 640.0);
    assert!(has_key(&rt, "ai-not-configured"));
    assert!(!has_key(&rt, "ai-chat-panel"));
    assert_eq!(
        element_text(&rt, "ai-not-configured-text"),
        NOT_CONFIGURED,
        "the literal zh-CN text from locales/zh-CN/ai.ts + core.ts"
    );
    assert!(any_text(&rt, GO_TO_CONFIGURE), "the settings button label");
    // The centred column: the text sits on the window's centre line.
    let text_rect = element(&rt, "ai-not-configured-text").rect;
    let centre_x = text_rect[0] + text_rect[2] / 2.0;
    assert!(
        (centre_x - 480.0).abs() <= 2.0,
        "the unconfigured text is centred: centre_x {centre_x}, expected 480"
    );
    let icon_rect = element(&rt, "ai-not-configured-icon").rect;
    let icon_centre_x = icon_rect[0] + icon_rect[2] / 2.0;
    assert!(
        (icon_centre_x - 480.0).abs() <= 2.0,
        "the Sparkles glyph is centred: centre_x {icon_centre_x}"
    );
    let mut rt = rt;
    click(&mut rt, "ai-not-configured-settings");
    assert_eq!(rt.state.settings_opens, 1);
}

#[test]
fn configured_panel_assembles_header_list_and_input() {
    let rt = runtime(Probe::configured(), 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-panel"));
    assert!(!has_key(&rt, "ai-not-configured"));
    assert!(has_key(&rt, "ai-chat-header"));
    assert!(has_key(&rt, "ai-chat-message-list"));
    assert!(has_key(&rt, "ai-chat-input-area"));
    assert!(any_text(&rt, t("chat.welcome")), "the empty state shows");
}

// ---- tabs + mounts ----------------------------------------------------------

#[test]
fn tab_switch_mounts_the_workflows_shell_and_back() {
    let mut rt = runtime(Probe::configured(), 960.0, 640.0);
    click(&mut rt, "ai-tab-workflows");
    assert_eq!(rt.state.panel.header().tab(), AiChatTab::Workflows);
    assert!(has_key(&rt, "ai-chat-workflows"));
    assert!(has_key(&rt, "ai-workflows-panel"), "T334's mount point");
    assert!(!has_key(&rt, "ai-chat-message-list"));
    assert!(!has_key(&rt, "ai-chat-input-area"));
    assert!(has_key(&rt, "ai-open-workflow-help"));
    assert!(!has_key(&rt, "ai-open-ai-help"));
    click(&mut rt, "ai-tab-chat");
    assert_eq!(rt.state.panel.header().tab(), AiChatTab::Chat);
    assert!(has_key(&rt, "ai-chat-message-list"));
    assert!(has_key(&rt, "ai-chat-input-area"));
    assert!(!has_key(&rt, "ai-chat-workflows"));
}

#[test]
fn history_toggle_mounts_and_unmounts_the_history_shell() {
    let mut rt = runtime(Probe::configured(), 960.0, 640.0);
    assert!(!has_key(&rt, "ai-session-history"));
    click(&mut rt, "ai-session-history-toggle");
    assert!(has_key(&rt, "ai-session-history"), "T195's mount point");
    assert!(rt.state.panel.header().session_history_open());
    click(&mut rt, "ai-session-history-toggle");
    assert!(!has_key(&rt, "ai-session-history"));
}

#[test]
fn help_buttons_report_through_the_shell() {
    let mut rt = runtime(Probe::configured(), 960.0, 640.0);
    click(&mut rt, "ai-open-ai-help");
    assert_eq!(rt.state.ai_help, 1);
    click(&mut rt, "ai-tab-workflows");
    click(&mut rt, "ai-open-workflow-help");
    assert_eq!(rt.state.workflow_help, 1);
}

#[test]
fn agent_mode_switches_through_the_shell() {
    let mut rt = runtime(Probe::configured(), 960.0, 640.0);
    click(&mut rt, "ai-agent-mode-agent");
    assert_eq!(rt.state.panel.header().agent_mode(), AiAgentMode::Agent);
    assert_eq!(
        element_text(&rt, "ai-agent-mode-hint"),
        t("chat.mode.agentHint")
    );
    click(&mut rt, "ai-agent-mode-ask");
    assert_eq!(rt.state.panel.header().agent_mode(), AiAgentMode::Ask);
}

// ---- send / stop through the UI --------------------------------------------

#[test]
fn typing_and_clicking_send_records_the_send_and_shows_the_bubble() {
    let mut rt = runtime(Probe::configured(), 960.0, 640.0);
    click(&mut rt, "ai-input-text");
    rt.paste("  hello  ");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "ai-input-submit");
    assert_eq!(rt.state.panel.sent_messages().len(), 1);
    assert_eq!(rt.state.panel.sent_messages()[0].content, "hello");
    assert_eq!(rt.state.panel.area().input_text(), "");
    assert!(
        any_text(&rt, "hello"),
        "the user bubble shows the sent text"
    );
    // Enter in the field sends too.
    click(&mut rt, "ai-input-text");
    rt.paste("again");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.panel.sent_messages().len(), 2);
    assert_eq!(rt.state.panel.sent_messages()[1].content, "again");
}

#[test]
fn stop_button_cancels_the_in_flight_run() {
    let mut probe = Probe::configured();
    probe.panel.set_request_id(Some("r-9".to_string()));
    probe.panel.set_streaming(true);
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "ai-input-stop"));
    click(&mut rt, "ai-input-stop");
    assert_eq!(rt.state.panel.cancelled_request_ids(), ["r-9".to_string()]);
}

#[test]
fn clear_button_empties_the_session_through_the_shell() {
    let mut probe = Probe::configured();
    probe.panel.set_messages(vec![
        user_msg("hi").id("m1"),
        assistant_msg("hello").id("m2"),
    ]);
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(any_text(&rt, "hello"));
    click(&mut rt, "ai-clear-chat");
    assert_eq!(rt.state.panel.clear_chat_presses(), 1);
    assert!(rt.state.panel.list().messages().is_empty());
    assert!(any_text(&rt, t("chat.welcome")), "the empty state returns");
}

// ---- snapshot chips + draft conflict through the UI ------------------------

#[test]
fn snapshot_chips_remove_fields_through_the_shell() {
    let mut probe = Probe::configured();
    probe.panel.sync_auto_context(Some(&full_snapshot()));
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "ai-auto-context-chips"));
    assert!(has_key(&rt, "ai-auto-context-sql"));
    click(&mut rt, "ai-auto-context-sql-remove");
    assert!(!has_key(&rt, "ai-auto-context-sql"));
    assert!(has_key(&rt, "ai-auto-context-selection"));
    for key in [
        "ai-auto-context-selection-remove",
        "ai-auto-context-error-remove",
        "ai-auto-context-result-remove",
    ] {
        click(&mut rt, key);
    }
    assert!(
        !has_key(&rt, "ai-auto-context-chips"),
        "removing the last field drops the whole row"
    );
}

#[test]
fn draft_conflict_bar_resolves_through_the_shell() {
    let mut probe = Probe::configured();
    probe.panel.area_mut().set_input("old".to_string());
    probe
        .panel
        .handle_draft_request(Some(&draft("d-1", "new draft")));
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "draft-conflict-bar"));
    click(&mut rt, "draft-replace");
    assert_eq!(rt.state.panel.area().input_text(), "new draft");
    assert!(!has_key(&rt, "draft-conflict-bar"));
    assert_eq!(rt.state.panel.consumed_draft_ids(), ["d-1".to_string()]);
}

#[test]
fn draft_conflict_bar_appends_through_the_shell() {
    let mut probe = Probe::configured();
    probe.panel.area_mut().set_input("old".to_string());
    probe
        .panel
        .handle_draft_request(Some(&draft("d-1", "new draft")));
    let mut rt = runtime(probe, 960.0, 640.0);
    click(&mut rt, "draft-append");
    assert_eq!(
        rt.state.panel.area().input_text(),
        format!("old{APPEND_SEPARATOR}new draft")
    );
    assert_eq!(rt.state.panel.consumed_draft_ids(), ["d-1".to_string()]);
    assert!(
        !has_key(&rt, "draft-cancel"),
        "the bar is gone after resolve"
    );
}

// ---- list wiring through the UI --------------------------------------------

#[test]
fn jump_to_bottom_pill_clears_unread_through_the_shell() {
    let mut probe = Probe::configured();
    probe
        .panel
        .set_messages(vec![user_msg("a").id("m1"), assistant_msg("b").id("m2")]);
    // Seeding while at the bottom adds no unread; drive away first.
    probe.panel.list_mut().set_at_bottom(false);
    probe.panel.list_mut().note_new_messages(2);
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "jump-to-bottom"));
    assert_eq!(element_text(&rt, "jump-to-bottom-count"), "2");
    click(&mut rt, "jump-to-bottom");
    assert!(rt.state.panel.list().at_bottom());
    assert_eq!(rt.state.panel.list().unread_count(), 0);
    assert!(!has_key(&rt, "jump-to-bottom"));
}

#[test]
fn memory_candidate_toast_dismisses_through_the_shell() {
    let mut probe = Probe::configured();
    probe.panel.set_memory_candidate_count(Some(3));
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "ai-memory-candidate-toast"));
    assert!(any_text(
        &rt,
        &t("chat.memoryCandidates").replacen("{count}", "3", 1)
    ));
    click(&mut rt, "ai-memory-candidate-dismiss");
    assert!(!has_key(&rt, "ai-memory-candidate-toast"));
    assert_eq!(rt.state.panel.list().memory_candidate_count(), None);
}

#[test]
fn remember_button_records_a_candidate_through_the_shell() {
    let mut probe = Probe::configured();
    probe
        .panel
        .set_messages(vec![assistant_msg("First line\nsecond").id("m1")]);
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "ai-memory-remember"));
    click(&mut rt, "ai-memory-remember");
    assert_eq!(rt.state.panel.memory_create_requests().len(), 1);
    assert_eq!(
        rt.state.panel.memory_create_requests()[0].subject,
        "First line"
    );
    assert!(has_key(&rt, "ai-memory-remember-toast"));
    assert_eq!(
        element_text(&rt, "ai-memory-remember-toast-text"),
        REMEMBERED
    );
    click(&mut rt, "ai-memory-remember-dismiss");
    assert!(!has_key(&rt, "ai-memory-remember-toast"));
    assert_eq!(rt.state.panel.list().remember_notice(), None);
}

#[test]
fn question_block_submit_sends_answers_through_the_shell() {
    let mut probe = Probe::configured();
    probe
        .panel
        .set_messages(vec![user_msg("ask").id("m1"), question_block_msg()]);
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "question-block"));
    click(&mut rt, "question-option-q1-a");
    click(&mut rt, "question-submit");
    assert_eq!(rt.state.panel.sent_messages().len(), 1);
    assert_eq!(
        rt.state.panel.sent_messages()[0].content,
        "[q1] Pick one.\nA"
    );
    assert!(
        !has_key(&rt, "question-submit"),
        "the submitted block folds away (S1 keeps an empty question-block shell)"
    );
    assert!(!has_key(&rt, "question-option-q1-a"));
}

// ---- geometry + unwired -----------------------------------------------------

#[test]
fn no_horizontal_overflow_at_both_sizes_both_themes() {
    for (w, h, dark, label) in [
        (960.0, 640.0, true, "960x640 dark"),
        (960.0, 640.0, false, "960x640 light"),
        (1440.0, 816.0, true, "1440x816 dark"),
        (1440.0, 816.0, false, "1440x816 light"),
    ] {
        let mut probe = Probe::rich();
        probe.dark = dark;
        let rt = runtime(probe, w, h);
        assert_no_horizontal_overflow(&rt, w, label);
    }
    // The unconfigured view, same sweep.
    for (w, h, dark, label) in [
        (960.0, 640.0, true, "unconfigured 960x640 dark"),
        (960.0, 640.0, false, "unconfigured 960x640 light"),
    ] {
        let mut probe = Probe::new();
        probe.dark = dark;
        let rt = runtime(probe, w, h);
        assert_no_horizontal_overflow(&rt, w, label);
    }
    // The workflows tab, same sweep.
    let mut probe = Probe::rich();
    probe.panel.header_mut().set_tab(AiChatTab::Workflows);
    let rt = runtime(probe, 960.0, 640.0);
    assert_no_horizontal_overflow(&rt, 960.0, "workflows 960x640 dark");
}

#[test]
fn unwired_panel_still_renders() {
    // A fully unwired panel cannot render its input area in debug: the
    // TextField guard (`ui/field.rs`) panics on a field without
    // `on_edit`, by design — a production caller always wires
    // `panel_mut`. The unwired path is covered where no text field is
    // mounted: the unconfigured view renders, and its settings button
    // stays inert without a callback.
    let mut rt = runtime_bare(Probe::new(), 960.0, 640.0);
    assert!(has_key(&rt, "ai-not-configured"));
    assert!(any_text(&rt, GO_TO_CONFIGURE));
    click(&mut rt, "ai-not-configured-settings");
    assert_eq!(rt.state.settings_opens, 0);
}

#[test]
fn template_and_profile_selects_choose_through_the_shell() {
    let mut rt = runtime(Probe::rich(), 960.0, 640.0);
    // Template: the trigger opens the listbox, option1 is "Review" (t1).
    click(&mut rt, "ai-template-select-field-trigger");
    assert!(rt.state.panel.header().template_open());
    assert!(has_key(&rt, "ai-template-select-field-content"));
    click(&mut rt, "option1");
    assert_eq!(rt.state.panel.header().template_id(), "t1");
    assert!(!rt.state.panel.header().template_open());
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("Review")),
        "the trigger now shows the chosen template"
    );
    // Profile: option1 is "Local" (p2).
    click(&mut rt, "ai-profile-select-field-trigger");
    assert!(rt.state.panel.header().profile_open());
    click(&mut rt, "option1");
    assert_eq!(rt.state.panel.header().active_profile_id(), "p2");
    assert!(!rt.state.panel.header().profile_open());
    // Escape closes a listbox through the dismissal layer (on_close).
    click(&mut rt, "ai-template-select-field-trigger");
    assert!(rt.state.panel.header().template_open());
    press(&mut rt, Key::Escape);
    assert!(
        !rt.state.panel.header().template_open(),
        "Escape closes the template listbox"
    );
    click(&mut rt, "ai-profile-select-field-trigger");
    assert!(rt.state.panel.header().profile_open());
    press(&mut rt, Key::Escape);
    assert!(
        !rt.state.panel.header().profile_open(),
        "Escape closes the profile listbox"
    );
}

#[test]
fn bubble_toggles_flip_through_the_shell() {
    let mut probe = Probe::configured();
    probe.panel.set_messages(vec![
        AiChatMessage::new(AiChatRole::Tool, "tool output").id("m0"),
        assistant_msg("here is the answer")
            .id("m1")
            .reasoning("because")
            .memory_used(vec![memory("M1")]),
    ]);
    let mut rt = runtime(probe, 960.0, 640.0);
    // Tool role: collapsed until the toggle is pressed.
    assert!(!has_key(&rt, "chat-bubble-tool-content"));
    click(&mut rt, "tool-role-toggle");
    assert!(has_key(&rt, "chat-bubble-tool-content"));
    assert_eq!(
        element_text(&rt, "chat-bubble-tool-content-text"),
        "tool output"
    );
    // Reasoning: hidden until the toggle is pressed.
    assert!(!has_key(&rt, "chat-bubble-reasoning-text"));
    click(&mut rt, "chat-bubble-reasoning");
    assert_eq!(element_text(&rt, "chat-bubble-reasoning-text"), "because");
    // Memory chip: collapsed until the chip is pressed.
    assert!(!has_key(&rt, "ai-memory-expanded"));
    click(&mut rt, "ai-memory-chip");
    assert!(has_key(&rt, "ai-memory-expanded"));
    assert!(has_key(&rt, "ai-memory-item-M1"));
}

#[test]
fn draft_conflict_cancel_dismisses_through_the_shell() {
    let mut probe = Probe::configured();
    probe.panel.area_mut().set_input("old".to_string());
    probe
        .panel
        .handle_draft_request(Some(&draft("d-1", "new draft")));
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "draft-conflict-bar"));
    click(&mut rt, "draft-cancel");
    assert!(!has_key(&rt, "draft-conflict-bar"));
    assert_eq!(
        rt.state.panel.area().input_text(),
        "old",
        "dismissing leaves the input untouched"
    );
    assert!(
        rt.state.panel.consumed_draft_ids().is_empty(),
        "dismissing acks nothing"
    );
    assert_eq!(rt.state.panel.area().pending_conflict(), None);
}

#[test]
fn unconfigured_settings_button_fires_on_enter() {
    let mut rt = runtime(Probe::new(), 960.0, 640.0);
    focus_by_tab(&mut rt, "ai-not-configured-settings");
    press(&mut rt, Key::Text("x".into()));
    assert_eq!(
        rt.state.settings_opens, 0,
        "a non-Enter key does not fire the button"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.settings_opens, 1);
}

#[test]
fn share_switch_toggles_through_the_shell() {
    let mut probe = Probe::configured();
    probe.panel.set_can_share(true);
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(!rt.state.panel.header().sharing());
    click(&mut rt, "ai-share-editor-toggle");
    assert!(rt.state.panel.header().sharing());
}

#[test]
fn host_callbacks_absent_still_renders() {
    let mut probe = Probe::rich();
    probe.panel.set_sql_dialect(Some("postgres".to_string()));
    let rt = runtime_panel_only(probe, 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-panel"));
    assert!(has_key(&rt, "ai-chat-header"));
    assert!(has_key(&rt, "ai-chat-message-list"));
    assert!(has_key(&rt, "ai-chat-input-area"));
    assert!(has_key(&rt, "ai-template-select"));
    assert!(has_key(&rt, "ai-profile-select"));
    let rt = runtime_panel_only(Probe::new(), 960.0, 640.0);
    assert!(has_key(&rt, "ai-not-configured"));
}
