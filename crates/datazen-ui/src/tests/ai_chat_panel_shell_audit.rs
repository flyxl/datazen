//! T009-S6 independent audit: AiChatPanel shell + session wiring.
//!
//! Fresh vectors against the React source (`AiChatPanel.tsx` 93–404,
//! 385–949; `stores/aiStore.ts` `initChatSession` / `clearChat`;
//! `hooks/useAutoScroll.ts` `useTrackUnread`; `lib/kvAiContext.ts`),
//! not a re-run of the coder's cases. Runtime tests render the panel
//! in a probe `Runtime` and read back the laid-out element tree — never
//! the component's own report of itself. Geometry is asserted on
//! `rt.elements()` rects from the layout engine. i18n is pinned through
//! `t()` against the literal zh-CN pack text.
//!
//! Round-2 retest: the two defects these tests first exposed were fixed
//! in `e22f84ea2` (`ensure_session` now resets the migration banners,
//! the candidate notice, the loaded skills and the egress summary;
//! `clear_chat` no longer dismisses the panel-local remember notice),
//! so all 35 tests below pass against the fixed implementation.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::ai::{
    AiAgentMode, AiChatContextSnapshot, AiChatMessage, AiChatPanel, AiChatPanelState,
    AiChatResultPreview, AiChatRole, AiChatTab, AiMemoryMode, AiMemoryReference, AiMemoryScope,
    AiPendingDraft, AiPromptTemplate, AiQuestion, ConflictResolution,
};
use crate::i18n::t;

// Literal zh-CN text from packages/frontend/src/locales/zh-CN/ai.ts and
// settings.ts — pinned here, then asserted through `t()` as well.
const MEMORY_DISABLED: &str = "记忆已关闭，请先在设置 → AI → 记忆 中启用。";
const REMEMBERED: &str = "已加入记忆候选队列。";
const REMEMBER_FAILED: &str = "保存记忆候选失败。";
const NOT_CONFIGURED: &str = "请先在设置中配置 AI 服务";
const GO_TO_CONFIGURE: &str = "前往配置";

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

fn any_text<S>(rt: &Runtime<S>, text: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(text))
}

fn element_text<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

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

fn draft(request_id: &str, content: &str) -> AiPendingDraft {
    AiPendingDraft::new(request_id, content)
}

struct Probe {
    panel: AiChatPanelState,
    dark: bool,
    settings_opens: usize,
}

impl Probe {
    fn new() -> Self {
        Self {
            panel: AiChatPanelState::new("sess-1"),
            dark: true,
            settings_opens: 0,
        }
    }

    fn configured() -> Self {
        let mut probe = Self::new();
        probe.panel.set_is_configured(true);
        assert!(probe.panel.ensure_session());
        probe
    }
}

fn panel_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let panel = AiChatPanel::<Probe>::new(&state.panel)
        .panel_mut(|s: &mut Probe| &mut s.panel)
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

fn runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, panel_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// ---- session key ------------------------------------------------------------

#[test]
fn audit_session_key_fallthrough_edges() {
    // Empty db session id with no connection falls to 'default'.
    assert_eq!(AiChatPanelState::new("").desired_session_key(), "default::");
    // Empty connection id falls through to the db session id.
    let mut panel = AiChatPanelState::new("sess-9");
    panel.set_connection_id(Some(String::new()));
    assert_eq!(panel.desired_session_key(), "sess-9::");
    // A database with spaces and unicode passes through verbatim.
    panel.set_database(Some("my db 数据库".to_string()));
    assert_eq!(panel.desired_session_key(), "sess-9::my db 数据库");
    // An empty database string is still an empty suffix, not 'default'.
    panel.set_database(Some(String::new()));
    assert_eq!(panel.desired_session_key(), "sess-9::");
    // A connection id wins over everything, database included.
    panel.set_connection_id(Some("conn::with::colons".to_string()));
    panel.set_database(Some("db".to_string()));
    assert_eq!(panel.desired_session_key(), "conn::with::colons::db");
}

// ---- ensure_session ---------------------------------------------------------

#[test]
fn audit_ensure_session_rekey_resets_stream_fields_and_session_memory() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_messages(vec![user_msg("hi").id("m1")]);
    panel.set_streaming(true);
    panel.set_stream_content("partial answer".to_string());
    panel.set_stream_reasoning("thinking hard".to_string());
    panel.set_stream_mcp_tool_name(Some("tool/search".to_string()));
    panel.set_stream_memory_used(vec![memory("M-stream")]);
    panel.set_session_memory_used(vec![memory("M-session")]);
    panel.set_request_id(Some("req-77".to_string()));
    panel
        .area_mut()
        .set_loaded_skills(vec![crate::components::ai::AiLoadedSkill::new(
            "sql-review",
        )
        .id("s1")]);
    panel.area_mut().set_egress(
        Some("read 2 files".to_string()),
        Some("mydb".to_string()),
        false,
        true,
    );
    // A database change re-keys the session without a connection change.
    panel.set_database(Some("otherdb".to_string()));
    assert!(panel.ensure_session());
    assert_eq!(panel.session_key(), Some("sess-1::otherdb"));
    assert!(panel.list().messages().is_empty());
    assert!(!panel.list().is_streaming());
    assert!(!panel.area().is_streaming());
    assert_eq!(panel.list().stream_content(), "");
    assert_eq!(panel.list().stream_reasoning(), "");
    assert_eq!(panel.list().stream_mcp_tool_name(), None);
    assert!(panel.list().streaming_message().memory_used.is_empty());
    assert_eq!(panel.request_id(), None);
    assert!(!panel.header().has_messages());
    // The loaded skills and the egress summary are session state too: a
    // fresh session (`newChatSession`, `stores/ai/sessions.ts:329–348`)
    // starts with `loadedSkills: []` and `egressSummary: null`.
    assert!(panel.area().loaded_skills().is_empty());
    assert_eq!(panel.area().egress_summary(), None);
}

/// React `initChatSession` (`stores/aiStore.ts:442–455`) nulls the
/// migration error/success and the memory-candidate notice when a
/// session starts: a connection switch must not leave the previous
/// session's banner or toast on screen.
#[test]
fn audit_ensure_session_rekey_clears_migration_and_candidate() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_session_migration_error(Some("migration blew up".to_string()));
    panel.set_session_migration_success(4);
    panel.set_memory_candidate_count(Some(2));
    // The panel-local remember notice is NOT session state:
    // `initChatSession` never touches it, so it survives the re-key
    // (it lives until its 4000ms timeout or a host dismiss).
    assert!(panel.handle_remember(&assistant_msg("remember this")));
    panel.set_connection_id(Some("conn-2".to_string()));
    assert!(panel.ensure_session());
    assert_eq!(
        panel.header().session_migration_error(),
        None,
        "a fresh session starts with no migration error"
    );
    assert_eq!(
        panel.header().session_migration_success(),
        0,
        "a fresh session starts with no migration success count"
    );
    assert_eq!(
        panel.list().memory_candidate_count(),
        None,
        "a fresh session starts with no candidate notice"
    );
    assert_eq!(
        panel.list().remember_notice().unwrap_or_default(),
        REMEMBERED,
        "the remember notice is panel-local and survives a re-key"
    );
    assert_eq!(
        panel.header().session_migration_error(),
        None,
        "a fresh session starts with no migration error"
    );
    assert_eq!(
        panel.header().session_migration_success(),
        0,
        "a fresh session starts with no migration success count"
    );
    assert_eq!(
        panel.list().memory_candidate_count(),
        None,
        "a fresh session starts with no candidate notice"
    );
}

/// React keeps the panel-local state across a session switch: the tab,
/// the history drawer, the draft and the template choice are not part
/// of `initChatSession`'s reset.
#[test]
fn audit_ensure_session_keeps_tab_history_input_and_template() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_templates(vec![AiPromptTemplate::new("t9", "Audit")]);
    panel.header_mut().choose_template("t9");
    panel.header_mut().set_tab(AiChatTab::Workflows);
    panel.set_session_history_open(true);
    panel.area_mut().set_input("unsent draft".to_string());
    // A database-driven re-key (no connection change, so no template reset).
    panel.set_database(Some("db2".to_string()));
    assert!(panel.ensure_session());
    assert_eq!(panel.header().template_id(), "t9");
    assert_eq!(panel.header().tab(), AiChatTab::Workflows);
    assert!(panel.header().session_history_open());
    assert_eq!(panel.area().input_text(), "unsent draft");
}

// ---- snapshot sync ----------------------------------------------------------

#[test]
fn audit_snapshot_sync_selection_and_error_only() {
    let mut panel = AiChatPanelState::new("sess-1");
    let partial = AiChatContextSnapshot {
        editor_sql: None,
        selection: Some("where id = 1".to_string()),
        last_error: Some("syntax error".to_string()),
        result_preview: None,
    };
    panel.sync_auto_context(Some(&partial));
    assert_eq!(panel.area().snapshot(), Some(&partial));
    // Changing only the error leaves the selection untouched.
    let mut changed = partial.clone();
    changed.last_error = Some("timeout".to_string());
    panel.sync_auto_context(Some(&changed));
    assert_eq!(
        panel.area().snapshot().unwrap().selection.as_deref(),
        Some("where id = 1")
    );
    assert_eq!(
        panel.area().snapshot().unwrap().last_error.as_deref(),
        Some("timeout")
    );
    panel.sync_auto_context(None);
    assert_eq!(panel.area().snapshot(), None);
}

#[test]
fn audit_snapshot_sync_empty_preview_has_no_content() {
    let mut panel = AiChatPanelState::new("sess-1");
    let empty_preview = AiChatContextSnapshot {
        editor_sql: None,
        selection: None,
        last_error: None,
        result_preview: Some(AiChatResultPreview {
            columns: Vec::new(),
            rows: Vec::new(),
        }),
    };
    panel.sync_auto_context(Some(&empty_preview));
    assert_eq!(
        panel.area().snapshot(),
        None,
        "a preview with no columns and no rows has no content"
    );
    let with_columns = AiChatContextSnapshot {
        result_preview: Some(AiChatResultPreview {
            columns: vec!["id".to_string()],
            rows: Vec::new(),
        }),
        ..Default::default()
    };
    panel.sync_auto_context(Some(&with_columns));
    assert_eq!(panel.area().snapshot(), Some(&with_columns));
}

#[test]
fn audit_snapshot_sync_keeps_whitespace_string_but_counts_it_as_empty() {
    // JS truthiness keeps a whitespace-only string in the snapshot
    // object (`if (autoContext?.selection)`), while `hasSnapshotContent`
    // trims it away — so a lone whitespace field still yields null.
    let mut panel = AiChatPanelState::new("sess-1");
    let mixed = AiChatContextSnapshot {
        editor_sql: Some("select 1".to_string()),
        selection: Some("   ".to_string()),
        last_error: None,
        result_preview: None,
    };
    panel.sync_auto_context(Some(&mixed));
    let snapshot = panel.area().snapshot().expect("editorSql has content");
    assert_eq!(snapshot.editor_sql.as_deref(), Some("select 1"));
    assert_eq!(
        snapshot.selection.as_deref(),
        Some("   "),
        "the whitespace string itself is kept, like JS truthiness keeps it"
    );
    let whitespace_only = AiChatContextSnapshot {
        editor_sql: Some("  ".to_string()),
        selection: None,
        last_error: None,
        result_preview: None,
    };
    panel.sync_auto_context(Some(&whitespace_only));
    assert_eq!(panel.area().snapshot(), None);
}

// ---- draft bridge -----------------------------------------------------------

#[test]
fn audit_draft_bridge_none_and_unconfigured_leave_tab_alone() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.header_mut().set_tab(AiChatTab::Workflows);
    panel.handle_draft_request(None);
    assert_eq!(panel.header().tab(), AiChatTab::Workflows);
    panel.handle_draft_request(Some(&draft("d-1", "hello")));
    assert_eq!(
        panel.header().tab(),
        AiChatTab::Workflows,
        "an unconfigured panel does not switch tabs for a draft"
    );
    assert_eq!(panel.area().input_text(), "");
    assert!(!panel.take_focus_input_request());
}

#[test]
fn audit_draft_bridge_whitespace_input_prefills_not_conflict() {
    // React parks a conflict only when `input.trim().length > 0`:
    // a whitespace-only draft is an empty input, so the draft prefills.
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    panel.area_mut().set_input("   \n\t  ".to_string());
    panel.handle_draft_request(Some(&draft("d-1", "prefilled")));
    assert_eq!(panel.area().pending_conflict(), None);
    assert_eq!(panel.area().input_text(), "prefilled");
    assert_eq!(panel.consumed_draft_ids(), ["d-1".to_string()]);
}

#[test]
fn audit_draft_bridge_streaming_prefill_switches_tab_and_requests_focus() {
    // Streaming defers only the ack: the prefill, the tab switch and
    // the focus request still happen (`AiChatPanel.tsx:221–249`).
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_streaming(true);
    panel.header_mut().set_tab(AiChatTab::Workflows);
    panel.handle_draft_request(Some(&draft("d-1", "while streaming")));
    assert_eq!(panel.area().input_text(), "while streaming");
    assert_eq!(panel.header().tab(), AiChatTab::Chat);
    assert!(panel.take_focus_input_request());
    assert!(panel.consumed_draft_ids().is_empty());
}

#[test]
fn audit_resolve_conflict_append_uses_pack_separator() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    panel.area_mut().set_input("line one\nline two".to_string());
    panel.handle_draft_request(Some(&draft("d-9", "drafted")));
    assert_eq!(
        panel.resolve_conflict(ConflictResolution::Append),
        Some("d-9".to_string())
    );
    let separator = t("chat.draft.appendSeparator");
    assert_eq!(
        panel.area().input_text(),
        format!("line one\nline two{separator}drafted")
    );
    assert_eq!(panel.consumed_draft_ids(), ["d-9".to_string()]);
}

#[test]
fn audit_draft_replay_after_dismiss_with_nonempty_input_reparks() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    panel.area_mut().set_input("still here".to_string());
    panel.handle_draft_request(Some(&draft("d-1", "new draft")));
    assert!(panel.area().pending_conflict().is_some());
    panel.dismiss_conflict();
    // The input is still non-empty, so a re-delivered draft parks again
    // instead of prefilling — and still acks nothing.
    panel.handle_draft_request(Some(&draft("d-1", "new draft")));
    assert_eq!(
        panel.area().pending_conflict(),
        Some(&draft("d-1", "new draft"))
    );
    assert_eq!(panel.area().input_text(), "still here");
    assert!(panel.consumed_draft_ids().is_empty());
}

// ---- send -------------------------------------------------------------------

#[test]
fn audit_send_kv_key_without_block_is_plain() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    // A key name with no block is no KV context at all
    // (`composeKvAiMessage` is the identity function for a null context).
    panel.set_kv_context(Some("user:7".to_string()), None);
    panel.area_mut().set_input("hello".to_string());
    let sent = panel.send().expect("a non-blank draft sends");
    assert_eq!(sent.content, "hello");
}

#[test]
fn audit_send_kv_whitespace_key_is_plain() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_kv_context(Some("   ".to_string()), Some("KEY block".to_string()));
    panel.area_mut().set_input("hello".to_string());
    let sent = panel.send().expect("a non-blank draft sends");
    assert_eq!(sent.content, "hello", "a blank key name composes nothing");
}

#[test]
fn audit_send_with_scope_connection_ask_mode_and_partial_snapshot() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_memory_scope(AiMemoryScope::Connection);
    panel.set_memory_mode(Some(AiMemoryMode::Ask));
    panel.header_mut().set_agent_mode(AiAgentMode::Agent);
    panel.set_templates(vec![AiPromptTemplate::new("t5", "Explain")]);
    panel.header_mut().choose_template("t5");
    let partial = AiChatContextSnapshot {
        editor_sql: None,
        selection: Some("id = 9".to_string()),
        last_error: None,
        result_preview: None,
    };
    panel.sync_auto_context(Some(&partial));
    panel.area_mut().set_input("explain this".to_string());
    let sent = panel.send().expect("a non-blank draft sends");
    assert_eq!(sent.content, "explain this");
    assert_eq!(sent.agent_mode, AiAgentMode::Agent);
    assert_eq!(sent.template_id.as_deref(), Some("t5"));
    assert_eq!(sent.memory_mode, AiMemoryMode::Ask);
    assert_eq!(sent.memory_scope, AiMemoryScope::Connection);
    assert_eq!(sent.auto_context, Some(partial));
    assert_eq!(sent.target_claim, None);
}

#[test]
fn audit_send_keeps_kv_template_and_scope_for_next_send() {
    // React's send clears only the input, the snapshot and the context
    // items; the KV prop, the template pick and the memory scope live on.
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_kv_context(Some("k:1".to_string()), Some("BLOCK".to_string()));
    panel.set_templates(vec![AiPromptTemplate::new("t1", "Review")]);
    panel.header_mut().choose_template("t1");
    panel.set_memory_scope(AiMemoryScope::Database);
    panel.area_mut().set_input("first".to_string());
    let first = panel.send().expect("first send");
    assert_eq!(first.content, "first\n\nBLOCK");
    assert_eq!(panel.kv_key_name(), Some("k:1"));
    assert_eq!(panel.header().template_id(), "t1");
    assert_eq!(panel.memory_scope(), AiMemoryScope::Database);
    panel.area_mut().set_input("second".to_string());
    let second = panel.send().expect("second send");
    assert_eq!(second.content, "second\n\nBLOCK");
    assert_eq!(second.template_id.as_deref(), Some("t1"));
    assert_eq!(panel.sent_messages().len(), 2);
}

#[test]
fn audit_send_answers_two_questions_join_with_blank_line() {
    // React's QuestionBlock joins per-question lines with '\n\n'.
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    let block = assistant_msg("Two questions.").id("m2").questions(vec![
        AiQuestion::new("q1", "First?")
            .option("a", "A")
            .option("b", "B"),
        AiQuestion::new("q2", "Second?")
            .option("c", "C")
            .option("d", "D"),
    ]);
    panel.set_messages(vec![user_msg("ask").id("m1"), block]);
    panel
        .list_mut()
        .question_state_mut(1)
        .toggle("q1", "a", false);
    panel
        .list_mut()
        .question_state_mut(1)
        .toggle("q2", "c", false);
    let sent = panel.send_answers(1).expect("answered blocks send");
    assert_eq!(sent.content, "[q1] First?\nA\n\n[q2] Second?\nC");
    assert_eq!(sent.raw_content, sent.content);
    assert_eq!(panel.list().messages().len(), 3);
}

#[test]
fn audit_send_answers_multi_select_joins_labels_with_comma() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    let block = assistant_msg("Pick any.")
        .id("m2")
        .questions(vec![AiQuestion::new("q1", "Pick.")
            .allow_multiple(true)
            .option("a", "Alpha")
            .option("b", "Beta")
            .option("c", "Gamma")]);
    panel.set_messages(vec![user_msg("ask").id("m1"), block]);
    let state = panel.list_mut().question_state_mut(1);
    state.toggle("q1", "a", true);
    state.toggle("q1", "c", true);
    let sent = panel.send_answers(1).expect("a multi-select sends");
    assert_eq!(sent.content, "[q1] Pick.\nAlpha, Gamma");
}

#[test]
fn audit_send_answers_unanswered_block_sends_nothing() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    let block = assistant_msg("Pick one.")
        .id("m2")
        .questions(vec![AiQuestion::new("q1", "Pick one.").option("a", "A")]);
    panel.set_messages(vec![user_msg("ask").id("m1"), block]);
    assert_eq!(panel.send_answers(1), None, "no answer, no send");
    assert!(panel.sent_messages().is_empty());
    assert_eq!(panel.list().messages().len(), 2);
}

// ---- stop / cancel ----------------------------------------------------------

#[test]
fn audit_stop_twice_records_two_receipts() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_request_id(Some("r-5".to_string()));
    panel.stop();
    panel.stop();
    assert_eq!(
        panel.cancelled_request_ids(),
        ["r-5".to_string(), "r-5".to_string()],
        "each stop press records its own cancel receipt"
    );
}

#[test]
fn audit_cancel_after_request_id_cleared_records_nothing() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_request_id(Some("r-5".to_string()));
    panel.stop();
    panel.set_request_id(None);
    panel.cancel_in_flight();
    assert_eq!(
        panel.cancelled_request_ids(),
        ["r-5".to_string()],
        "clearing the request id means no run is in flight any more"
    );
}

// ---- remember ---------------------------------------------------------------

#[test]
fn audit_remember_subject_counts_chars_not_bytes() {
    // React `firstLine.trim().slice(0, 80)` counts UTF-16 units; the
    // VM counts chars — identical for BMP text, and never byte-based.
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    let content = "汉".repeat(100);
    assert!(panel.handle_remember(&assistant_msg(&content)));
    let request = &panel.memory_create_requests()[0];
    assert_eq!(request.subject.chars().count(), 80);
    assert_eq!(request.subject, "汉".repeat(80));
    assert_eq!(request.content, content);
}

#[test]
fn audit_remember_empty_connection_id_is_user_scope() {
    // React: `connectionId ? 'connection' : 'user'` — an empty id is
    // falsy, so the memory is user-scoped with no connection id.
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_connection_id(Some(String::new()));
    assert!(panel.handle_remember(&assistant_msg("remember this")));
    let request = &panel.memory_create_requests()[0];
    assert_eq!(request.scope_type, "user");
    assert_eq!(request.connection_id, None);
    assert_eq!(request.source_ref.as_deref(), Some("sess-1::"));
}

#[test]
fn audit_remember_failure_overwrites_success_notice() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    assert!(panel.handle_remember(&assistant_msg("remember this")));
    assert_eq!(
        panel.list().remember_notice().unwrap_or_default(),
        REMEMBERED
    );
    panel.report_remember_failed();
    assert_eq!(
        panel.list().remember_notice().unwrap_or_default(),
        REMEMBER_FAILED
    );
    // A blank remember is a no-op and leaves the notice alone.
    assert!(!panel.handle_remember(&assistant_msg("   ")));
    assert_eq!(
        panel.list().remember_notice().unwrap_or_default(),
        REMEMBER_FAILED
    );
}

// ---- clear_chat -------------------------------------------------------------

/// The store `clearChat` (`stores/aiStore.ts:723–750`) clears the
/// store-owned candidate notice but never touches the panel-local
/// `rememberNotice` state (`AiChatPanel.tsx:296–326`): after a clear
/// the toast stays until its 4000ms timeout (or a host dismiss).
#[test]
fn audit_clear_chat_keeps_remember_notice() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    assert!(panel.handle_remember(&assistant_msg("remember this")));
    assert_eq!(
        panel.list().remember_notice().unwrap_or_default(),
        REMEMBERED
    );
    panel.clear_chat();
    assert_eq!(
        panel.list().remember_notice().unwrap_or_default(),
        REMEMBERED,
        "clearChat does not dismiss the panel-local remember toast"
    );
    assert_eq!(panel.list().memory_candidate_count(), None);
}

/// React's clear is a store action: the panel-local input, template,
/// tab and snapshot are untouched by it.
#[test]
fn audit_clear_chat_keeps_input_template_tab_and_snapshot() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.set_is_configured(true);
    assert!(panel.ensure_session());
    panel.set_templates(vec![AiPromptTemplate::new("t3", "Keep")]);
    panel.header_mut().choose_template("t3");
    panel.header_mut().set_tab(AiChatTab::Workflows);
    panel.area_mut().set_input("unsent".to_string());
    let snapshot = AiChatContextSnapshot {
        editor_sql: Some("select 9".to_string()),
        selection: None,
        last_error: None,
        result_preview: None,
    };
    panel.sync_auto_context(Some(&snapshot));
    panel.set_messages(vec![user_msg("hi").id("m1")]);
    panel.clear_chat();
    assert!(panel.list().messages().is_empty());
    assert_eq!(panel.area().input_text(), "unsent");
    assert_eq!(panel.header().template_id(), "t3");
    assert_eq!(panel.header().tab(), AiChatTab::Workflows);
    assert_eq!(panel.area().snapshot(), Some(&snapshot));
}

#[test]
fn audit_clear_chat_counts_presses() {
    let mut panel = AiChatPanelState::new("sess-1");
    panel.clear_chat();
    panel.clear_chat();
    assert_eq!(panel.clear_chat_presses(), 2);
}

// ---- unread -----------------------------------------------------------------

#[test]
fn audit_store_arrivals_shrink_then_grow_counts_only_net_new() {
    // `useTrackUnread`: the delta is counted only when the count grows;
    // shrinking resets the baseline without touching the unread total.
    let mut panel = Probe::configured().panel;
    panel.list_mut().set_at_bottom(false);
    panel.set_messages(vec![
        user_msg("a").id("m1"),
        assistant_msg("b").id("m2"),
        user_msg("c").id("m3"),
    ]);
    assert_eq!(panel.list().unread_count(), 3);
    panel.set_messages(vec![user_msg("a").id("m1")]);
    assert_eq!(
        panel.list().unread_count(),
        3,
        "shrinking the list adds no unread"
    );
    panel.set_messages(vec![
        user_msg("a").id("m1"),
        assistant_msg("b").id("m2"),
        user_msg("c").id("m3"),
        assistant_msg("d").id("m4"),
    ]);
    assert_eq!(
        panel.list().unread_count(),
        6,
        "growing 1 -> 4 adds a delta of 3"
    );
}

// ---- i18n -------------------------------------------------------------------

#[test]
fn audit_i18n_keys_used_by_panel_resolve() {
    assert_eq!(t("chat.memoryDisabled"), MEMORY_DISABLED);
    assert_eq!(t("chat.remembered"), REMEMBERED);
    assert_eq!(t("chat.rememberFailed"), REMEMBER_FAILED);
    assert_eq!(t("common.aiNotConfigured"), NOT_CONFIGURED);
    assert_eq!(t("settings.ai.goToConfigure"), GO_TO_CONFIGURE);
}

// ---- shell runtime ----------------------------------------------------------

#[test]
fn audit_history_open_survives_tab_switch() {
    // `sessionHistoryOpen` is panel-local state: leaving for the
    // workflows tab and coming back re-opens the drawer.
    let mut rt = runtime(Probe::configured(), 960.0, 640.0);
    click(&mut rt, "ai-session-history-toggle");
    assert!(has_key(&rt, "ai-session-history"));
    click(&mut rt, "ai-tab-workflows");
    assert!(!has_key(&rt, "ai-session-history"));
    click(&mut rt, "ai-tab-chat");
    assert!(
        has_key(&rt, "ai-session-history"),
        "the drawer state survives the tab switch"
    );
}

#[test]
fn audit_migration_banner_hidden_on_workflows_tab() {
    // The banners render only while `tab === 'chat'`
    // (`AiChatPanel.tsx:496–524`).
    let mut probe = Probe::configured();
    probe
        .panel
        .set_session_migration_error(Some("boom".to_string()));
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "ai-session-migration-banner"));
    assert_eq!(
        element_text(&rt, "ai-session-migration-banner-text"),
        "boom"
    );
    click(&mut rt, "ai-tab-workflows");
    assert!(!has_key(&rt, "ai-session-migration-banner"));
    click(&mut rt, "ai-tab-chat");
    assert!(has_key(&rt, "ai-session-migration-banner"));
}

#[test]
fn audit_unconfigured_hides_panel_even_with_rich_state() {
    // The `!isConfigured` early return wins over every other state.
    let mut probe = Probe::new();
    probe.panel.set_messages(vec![
        user_msg("hi").id("m1"),
        assistant_msg("hello").id("m2"),
    ]);
    probe.panel.header_mut().set_tab(AiChatTab::Workflows);
    probe
        .panel
        .set_session_migration_error(Some("boom".to_string()));
    let rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "ai-not-configured"));
    assert_eq!(element_text(&rt, "ai-not-configured-text"), NOT_CONFIGURED);
    assert!(any_text(&rt, GO_TO_CONFIGURE));
    for key in [
        "ai-chat-panel",
        "ai-chat-header",
        "ai-chat-message-list",
        "ai-chat-input-area",
        "ai-chat-workflows",
        "ai-session-migration-banner",
    ] {
        assert!(!has_key(&rt, key), "{key} must not render unconfigured");
    }
    let mut rt = rt;
    click(&mut rt, "ai-not-configured-settings");
    assert_eq!(rt.state.settings_opens, 1);
}

#[test]
fn audit_panel_children_stack_without_overlap_and_within_window() {
    for (w, h, dark, label) in [
        (960.0, 640.0, true, "960x640 dark"),
        (1440.0, 816.0, false, "1440x816 light"),
    ] {
        let mut probe = Probe::configured();
        probe.dark = dark;
        probe.panel.set_messages(vec![
            user_msg("select * from users").id("m1"),
            assistant_msg("Here are the rows.").id("m2"),
        ]);
        probe
            .panel
            .set_templates(vec![AiPromptTemplate::new("t1", "Review")]);
        probe.panel.sync_auto_context(Some(&AiChatContextSnapshot {
            editor_sql: Some("select 1".to_string()),
            selection: Some("x".to_string()),
            last_error: None,
            result_preview: None,
        }));
        let rt = runtime(probe, w, h);
        // Every laid-out node stays inside the window, both axes.
        let mut outside = Vec::new();
        for e in rt.elements() {
            if e.exiting {
                continue;
            }
            let r = e.rect;
            if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
                outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
            }
        }
        assert!(
            outside.is_empty(),
            "{label}: {} node(s) outside the {w}x{h} window: {outside:?}",
            outside.len()
        );
        // The three stacked children do not overlap each other.
        let header = element(&rt, "ai-chat-header").rect;
        let list = element(&rt, "ai-chat-message-list").rect;
        let input = element(&rt, "ai-chat-input-area").rect;
        assert!(
            header[1] + header[3] <= list[1] + 0.5,
            "{label}: header bottom {} overlaps list top {}",
            header[1] + header[3],
            list[1]
        );
        assert!(
            list[1] + list[3] <= input[1] + 0.5,
            "{label}: list bottom {} overlaps input top {}",
            list[1] + list[3],
            input[1]
        );
    }
}

#[test]
fn audit_unconfigured_column_stacks_without_overlap() {
    let rt = runtime(Probe::new(), 960.0, 640.0);
    let icon = element(&rt, "ai-not-configured-icon").rect;
    let text = element(&rt, "ai-not-configured-text").rect;
    let button = element(&rt, "ai-not-configured-settings").rect;
    assert!(
        icon[1] + icon[3] <= text[1] + 0.5,
        "icon bottom {} overlaps text top {}",
        icon[1] + icon[3],
        text[1]
    );
    assert!(
        text[1] + text[3] <= button[1] + 0.5,
        "text bottom {} overlaps button top {}",
        text[1] + text[3],
        button[1]
    );
    // The column is centred: icon, text and button share a centre line.
    for (name, r) in [("icon", icon), ("text", text), ("button", button)] {
        let centre = r[0] + r[2] / 2.0;
        assert!(
            (centre - 480.0).abs() <= 2.0,
            "{name} centre_x {centre}, expected 480"
        );
    }
}

#[test]
fn audit_send_through_ui_with_kv_chip_composes_block() {
    let mut probe = Probe::configured();
    probe
        .panel
        .set_kv_context(Some("user:42".to_string()), Some("KEY user:42".to_string()));
    probe.panel.set_messages(vec![user_msg("earlier").id("m1")]);
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "ai-kv-context-chip"));
    assert_eq!(element_text(&rt, "ai-kv-context-chip-key"), "user:42");
    click(&mut rt, "ai-input-text");
    rt.paste("hello kv");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "ai-input-submit");
    assert_eq!(rt.state.panel.sent_messages().len(), 1);
    assert_eq!(
        rt.state.panel.sent_messages()[0].content,
        "hello kv\n\nKEY user:42"
    );
    assert_eq!(rt.state.panel.sent_messages()[0].raw_content, "hello kv");
}
