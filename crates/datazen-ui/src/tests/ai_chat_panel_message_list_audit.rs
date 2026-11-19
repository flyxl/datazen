//! T009-S4 independent audit: AiChatPanel message list
//! (`AiChatPanel.tsx:642–790`, `hooks/useAutoScroll.ts`).
//!
//! Fresh vectors, written against the React source and the React zh-CN
//! locale packs — not against the coder's test file. Where a value can be
//! pinned without going through the component's own `t()` helper it is
//! pinned against the literal zh-CN string from
//! `packages/frontend/src/locales/zh-CN/ai.ts`, so a wrong table entry
//! cannot hide behind a self-referential comparison.
//!
//! Geometry is asserted on `rt.elements()` rects, which come from the
//! layout engine, not from the code under test. Vertical overflow is
//! expected inside the scroll viewport and is not asserted there; the
//! horizontal span is asserted on every element.
//!
//! Divergences pinned here on purpose (non-blocking; see the audit
//! report): D1 — an empty remember notice (`Some("")`) renders an empty
//! toast row, where React's `{rememberNotice && ...}` hides it; D2 — a
//! stream memory of `[]` falls back to the session memory, where React's
//! `??` would keep the empty list (same `[]`-vs-absent class as the
//! documented `msg.memoryUsed` divergence).

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Tx};

use crate::components::ai::{
    AiChatMessage, AiChatMessageList, AiChatMessageListState, AiChatRole, AiMemoryReference,
    AiQuestion, Answer, AUTO_SCROLL_THRESHOLD,
};
use crate::i18n::{has, t};

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

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn count_key<S>(rt: &Runtime<S>, key: &str) -> usize {
    rt.elements()
        .iter()
        .filter(|e| !e.exiting && e.key.as_deref() == Some(key))
        .count()
}

fn find_json<'a>(node: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    if node.get("key").and_then(|v| v.as_str()) == Some(key) {
        return Some(node);
    }
    node.get("children")?
        .as_array()?
        .iter()
        .find_map(|child| find_json(child, key))
}

fn a11y_label<S>(rt: &Runtime<S>, key: &str) -> Option<String> {
    let json = rt.accessibility_json();
    let node = find_json(&json, key)?;
    node.get("name").and_then(|v| v.as_str()).map(str::to_owned)
}

/// Every laid-out element stays inside the window horizontally; the
/// scroll viewport's content may extend below the window vertically, so
/// only the horizontal span is checked here.
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

fn assert_rect_inside<S>(rt: &Runtime<S>, key: &str, w: f32, h: f32, label: &str) {
    let r = element(rt, key).rect;
    assert!(
        r[0] >= -0.5 && r[1] >= -0.5 && r[0] + r[2] <= w + 0.5 && r[1] + r[3] <= h + 0.5,
        "{label}: {key} rect {r:?} escapes {w}x{h}"
    );
}

fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn zero_area(rect: [f32; 4]) -> bool {
    rect[2] <= 0.5 || rect[3] <= 0.5
}

fn key_of(element: &rune::ElementInfo) -> String {
    element.key.clone().unwrap_or_else(|| element.path.clone())
}

fn contains(outer: [f32; 4], inner: [f32; 4]) -> bool {
    let (ox, oy, ow, oh) = (outer[0], outer[1], outer[2], outer[3]);
    let (ix, iy, iw, ih) = (inner[0], inner[1], inner[2], inner[3]);
    ix >= ox - 0.5 && iy >= oy - 0.5 && ix + iw <= ox + ow + 0.5 && iy + ih <= oy + oh + 0.5
}

/// Sibling elements (same parent, neither containing the other) must not
/// overlap — the layout engine's rects, not the component's own report.
fn assert_no_sibling_overlap<S>(rt: &Runtime<S>, label: &str) {
    let elements = rt.elements();
    let nodes: Vec<&rune::ElementInfo> = elements
        .iter()
        .filter(|element| element.path == "probe-page" || element.path.starts_with("probe-page/"))
        .collect();
    let mut collisions = Vec::new();
    for (index, first) in nodes.iter().enumerate() {
        for second in &nodes[index + 1..] {
            if parent_path(&first.path) != parent_path(&second.path) {
                continue;
            }
            if zero_area(first.rect) || zero_area(second.rect) {
                continue;
            }
            if contains(first.rect, second.rect) || contains(second.rect, first.rect) {
                continue;
            }
            let overlap_w = (first.rect[0] + first.rect[2]).min(second.rect[0] + second.rect[2])
                - first.rect[0].max(second.rect[0]);
            let overlap_h = (first.rect[1] + first.rect[3]).min(second.rect[1] + second.rect[3])
                - first.rect[1].max(second.rect[1]);
            if overlap_w > 0.5 && overlap_h > 0.5 {
                collisions.push(format!(
                    "{} {:?} x {} {:?}",
                    key_of(first),
                    first.rect,
                    key_of(second),
                    second.rect
                ));
            }
        }
    }
    assert!(
        collisions.is_empty(),
        "{label}: {} sibling collision(s): {collisions:#?}",
        collisions.len()
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

#[derive(Default)]
struct AuditProbe {
    list: AiChatMessageListState,
    dark: bool,
    candidate_dismisses: usize,
    remember_dismisses: usize,
    jumps: usize,
    remembers: Vec<String>,
    reasoning_toggles: Vec<usize>,
    tool_toggles: Vec<usize>,
    memory_toggles: Vec<usize>,
    inserts: Vec<String>,
    runs: Vec<(String, String)>,
    new_queries: Vec<String>,
    answers: Vec<(usize, String)>,
    at_bottom_reports: Vec<bool>,
    wire_at_bottom: bool,
}

fn audit_probe() -> AuditProbe {
    AuditProbe {
        list: AiChatMessageListState::default(),
        dark: false,
        ..AuditProbe::default()
    }
}

fn list_view(state: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    let mut list = AiChatMessageList::<AuditProbe>::new(&state.list)
        .sql_dialect("mysql")
        .on_insert_sql(|s: &mut AuditProbe, sql: &str| s.inserts.push(sql.to_string()))
        .on_run_code(|s: &mut AuditProbe, sql: &str, dialect: &str| {
            s.runs.push((sql.to_string(), dialect.to_string()));
        })
        .on_new_query(|s: &mut AuditProbe, sql: &str| s.new_queries.push(sql.to_string()))
        .on_toggle_reasoning(|s: &mut AuditProbe, index| {
            s.reasoning_toggles.push(index);
            s.list.bubble_state_mut(index).toggle_reasoning();
        })
        .on_toggle_tool(|s: &mut AuditProbe, index| {
            s.tool_toggles.push(index);
            s.list.bubble_state_mut(index).toggle_tool();
        })
        .on_toggle_memory(|s: &mut AuditProbe, index| {
            s.memory_toggles.push(index);
            s.list.bubble_state_mut(index).toggle_memory();
        })
        .on_toggle_question(|s: &mut AuditProbe, index, qid, oid, multi| {
            s.list.question_state_mut(index).toggle(qid, oid, multi);
        })
        .on_custom_edit(|s: &mut AuditProbe, index, qid: &str| {
            s.list.question_state_mut(index).custom_edit(qid)
        })
        .on_submit_questions(|s: &mut AuditProbe, index| {
            let questions = s.list.messages()[index].questions.clone();
            if let Some(payload) = s.list.question_state_mut(index).submit(&questions) {
                s.answers.push((index, payload));
            }
        })
        .on_remember(|s: &mut AuditProbe, message: &AiChatMessage| {
            s.remembers.push(message.content.clone());
        })
        .on_dismiss_memory_candidate(|s: &mut AuditProbe| {
            s.candidate_dismisses += 1;
            s.list.dismiss_memory_candidate();
        })
        .on_dismiss_remember_notice(|s: &mut AuditProbe| {
            s.remember_dismisses += 1;
            s.list.dismiss_remember_notice();
        })
        .on_jump_to_bottom(|s: &mut AuditProbe, tx: &mut Tx| {
            s.jumps += 1;
            s.list.jump_to_bottom();
            tx.scroll_to_bottom("ai-chat-message-list-viewport");
        });
    // The viewport's at-bottom report clears the unread count when it
    // fires, so it stays unwired unless a test asks for it — otherwise
    // the jump pill would vanish under the test's own feet.
    if state.wire_at_bottom {
        list = list.on_at_bottom(|s: &mut AuditProbe, at: bool| {
            s.at_bottom_reports.push(at);
            s.list.set_at_bottom(at);
        });
    }
    rune_kit::root(
        col()
            .key("probe-page")
            .w_full()
            .h_full()
            .padding(16.0)
            .child(list.build(state.dark)),
        Vec::new(),
    )
}

fn runtime(probe: AuditProbe, w: f32, h: f32) -> Runtime<AuditProbe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, list_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// ---- AiChatMessageListState: pure logic, fresh vectors ----------------------

#[test]
fn audit_scroll_threshold_boundaries_and_set_at_bottom() {
    assert_eq!(
        AUTO_SCROLL_THRESHOLD, 120.0,
        "the hook's default threshold, verbatim (useAutoScroll.ts)"
    );
    let mut state = AiChatMessageListState::default();
    state.on_scroll(119.9);
    assert!(
        state.at_bottom(),
        "just inside the threshold counts as bottom"
    );
    state.on_scroll(120.0);
    assert!(
        state.at_bottom(),
        "exactly at the threshold counts as bottom"
    );
    state.on_scroll(120.01);
    assert!(!state.at_bottom(), "just past the threshold is away");
    // A negative distance (scrolled past the end) is still at the bottom.
    state.on_scroll(-5.0);
    assert!(state.at_bottom());
    // Zero new messages while away add nothing and show no pill.
    state.on_scroll(500.0);
    state.note_new_messages(0);
    assert_eq!(state.unread_count(), 0);
    assert!(!state.show_jump_to_bottom());
    // set_at_bottom(false) keeps the unread count; only reaching the
    // bottom clears it, as the hook's second effect does.
    state.note_new_messages(4);
    state.set_at_bottom(false);
    assert_eq!(state.unread_count(), 4);
    assert!(state.show_jump_to_bottom());
    state.set_at_bottom(true);
    assert_eq!(state.unread_count(), 0);
    assert!(!state.show_jump_to_bottom());
    // Jump resets even from the pinned state, and is idempotent.
    state.jump_to_bottom();
    state.jump_to_bottom();
    assert!(state.at_bottom() && state.unread_count() == 0);
}

#[test]
fn audit_streaming_gates_are_off_without_streaming() {
    let mut state = AiChatMessageListState::default();
    // Seeded content alone never opens the bubble or the thinking row:
    // React gates both on `chatSession?.isStreaming`.
    state.set_stream_content("draft".to_string());
    state.set_stream_reasoning("why".to_string());
    assert!(!state.show_streaming_bubble());
    assert!(!state.show_thinking_row());
    state.set_streaming(true);
    assert!(state.show_streaming_bubble() && !state.show_thinking_row());
    // Whitespace-only reasoning is truthy in JS, like whitespace content.
    state.set_stream_content(String::new());
    state.set_stream_reasoning(" ".to_string());
    assert!(
        state.show_streaming_bubble(),
        "reasoning of a single space still opens the bubble"
    );
}

#[test]
fn audit_effective_message_inheritance_is_last_message_only() {
    let mut state = AiChatMessageListState::default();
    state.set_session_memory_used(vec![memory("M1")]);
    // The last message is a user message: the earlier assistant message
    // never inherits, even though it is the last assistant.
    state.set_messages(vec![
        assistant_msg("answer").id("m1"),
        user_msg("thanks").id("m2"),
    ]);
    assert!(
        state
            .effective_message(0)
            .is_some_and(|m| m.memory_used.is_empty()),
        "inheritance keys on the last message, not the last assistant"
    );
    assert!(
        state
            .effective_message(1)
            .is_some_and(|m| m.memory_used.is_empty()),
        "a user message never inherits"
    );
    // A lone assistant message is the last message, so it inherits.
    state.set_messages(vec![assistant_msg("solo").id("m1")]);
    assert_eq!(
        state.effective_message(0).map(|m| m.memory_used),
        Some(vec![memory("M1")]),
    );
    // An empty session memory inherits nothing (stays visibly empty).
    state.set_session_memory_used(Vec::new());
    assert!(
        state
            .effective_message(0)
            .is_some_and(|m| m.memory_used.is_empty()),
        "no session memory: nothing to inherit"
    );
    assert_eq!(state.effective_message(9), None, "out of range is None");
}

#[test]
fn audit_streaming_message_reasoning_empty_means_absent() {
    let mut state = AiChatMessageListState::default();
    state.set_stream_content("draft".to_string());
    state.set_stream_reasoning(String::new());
    let message = state.streaming_message();
    assert_eq!(
        message.reasoning, None,
        "React passes `reasoning || undefined`"
    );
    assert_eq!(message.content, "draft");
    assert_eq!(message.role, AiChatRole::Assistant);
    // D2 (pinned, non-blocking): an empty stream memory list falls back
    // to the session memory here, while React's `??` would keep the
    // empty list — the same []-vs-absent class as the documented
    // msg.memoryUsed divergence.
    state.set_session_memory_used(vec![memory("M1")]);
    state.set_stream_memory_used(Vec::new());
    assert_eq!(
        state.streaming_message().memory_used,
        vec![memory("M1")],
        "empty stream memory currently falls back to the session memory"
    );
}

#[test]
fn audit_thinking_label_formats_multi_segment_tool_names() {
    let mut state = AiChatMessageListState::default();
    state.set_stream_mcp_tool_name(Some("a/b/c/d".to_string()));
    assert_eq!(
        state.thinking_label(),
        t("chat.callingMcpTool").replacen("{name}", "c/d", 1),
        "four segments show everything after the second slash (S1's formatter)"
    );
    state.set_stream_mcp_tool_name(Some("a/b".to_string()));
    assert!(
        state.thinking_label().contains("a/b"),
        "fewer than three segments keep the whole name"
    );
    state.set_stream_mcp_tool_name(None);
    assert_eq!(state.thinking_label(), t("chat.thinking"));
}

#[test]
fn audit_reseed_drops_per_message_answers() {
    let mut state = AiChatMessageListState::default();
    let question = AiQuestion::new("q1", "Pick one")
        .option("a", "Alpha")
        .option("b", "Beta");
    let message = assistant_msg("Which?").id("m2").questions(vec![question]);
    state.set_messages(vec![user_msg("hi").id("m1"), message]);
    state.bubble_state_mut(1).toggle_reasoning();
    state.question_state_mut(1).toggle("q1", "b", false);
    assert!(state.bubble_state(1).reasoning_open());
    assert_eq!(
        state.question_state(1).answer("q1"),
        Some(&Answer::Single("b".to_string()))
    );
    // Re-seeding recreates the per-message states: answers do not survive,
    // as the set_messages docs say — the panel slice seeds once per session.
    let question = AiQuestion::new("q1", "Pick one")
        .option("a", "Alpha")
        .option("b", "Beta");
    let message = assistant_msg("Which?").id("m2").questions(vec![question]);
    state.set_messages(vec![user_msg("hi").id("m1"), message]);
    assert!(!state.bubble_state(1).reasoning_open());
    assert_eq!(
        state.question_state(1).answer("q1"),
        Some(&Answer::Single(String::new())),
        "the answer resets to the initial empty single"
    );
    assert!(!state.question_state(1).submitted());
}

#[test]
fn audit_out_of_range_state_accessors_do_not_panic() {
    let mut state = AiChatMessageListState::default();
    state.set_messages(vec![user_msg("hi").id("m1")]);
    // Writing through an out-of-range index lands on the shared fallback
    // and leaves the real message 0 state alone.
    state.bubble_state_mut(7).toggle_reasoning();
    assert!(
        state.bubble_state(7).reasoning_open(),
        "fallback took the toggle"
    );
    assert!(
        !state.bubble_state(0).reasoning_open(),
        "message 0 is untouched by the fallback write"
    );
    assert!(!state.question_state(9).submitted());
    assert_eq!(state.question_state(9).custom_text("q1"), "");
    // Dismissing with nothing to dismiss is a no-op, twice over.
    state.dismiss_memory_candidate();
    state.dismiss_memory_candidate();
    state.dismiss_remember_notice();
    state.dismiss_remember_notice();
    assert_eq!(state.memory_candidate_count(), None);
    assert_eq!(state.remember_notice(), None);
}

// ---- i18n: literal zh-CN strings from the React packs -----------------------

#[test]
fn audit_slice_strings_match_the_react_zh_cn_pack_literally() {
    for key in [
        "chat.memoryCandidates",
        "chat.dismiss",
        "chat.welcome",
        "chat.thinking",
        "chat.callingMcpTool",
        "chat.jumpToBottom",
    ] {
        assert!(has(key), "{key} must exist in the zh-CN pack");
    }
    assert_eq!(
        t("chat.welcome"),
        "你好！我是你的数据库助手。你可以问我关于 SQL 查询、数据库概念或数据分析的问题。"
    );
    assert_eq!(t("chat.thinking"), "思考中…");
    assert_eq!(
        t("chat.callingMcpTool").replacen("{name}", "search", 1),
        "正在调用 MCP 工具 search…"
    );
    assert_eq!(
        t("chat.memoryCandidates").replacen("{count}", "7", 1),
        "发现 7 条可记住的内容，请到“设置 → AI → 记忆”确认。"
    );
    assert_eq!(t("chat.jumpToBottom"), "回到底部");
    assert_eq!(t("chat.dismiss"), "关闭");
}

// ---- rendering: toasts, fresh vectors ---------------------------------------

#[test]
fn audit_candidate_count_zero_still_renders_its_toast() {
    // React gates on the notice object, not the count: a notice with
    // count 0 still renders its toast.
    let mut p = audit_probe();
    p.list.set_memory_candidate_count(Some(0));
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-memory-candidate-toast"));
    assert_eq!(
        element_text(&rt, "ai-memory-candidate-toast-text"),
        "发现 0 条可记住的内容，请到“设置 → AI → 记忆”确认。"
    );
    // The dismiss button carries React's aria-label, chat.dismiss.
    assert_eq!(
        a11y_label(&rt, "ai-memory-candidate-dismiss").as_deref(),
        Some("关闭"),
        "dismiss button exposes the chat.dismiss accessible name"
    );
}

#[test]
fn audit_empty_remember_notice_renders_and_is_a_divergence() {
    // D1 (pinned, non-blocking): React's `{rememberNotice && ...}`
    // hides an empty string, but the VM renders `Some("")` as an empty
    // toast row. Unreachable from handleRemember — it only ever sets a
    // non-empty i18n string or null — so this pins today's behaviour
    // rather than failing the slice.
    let mut p = audit_probe();
    p.list.set_remember_notice(Some(String::new()));
    let rt = runtime(p, 960.0, 640.0);
    assert!(
        has_key(&rt, "ai-memory-remember-toast"),
        "an empty notice currently renders its (empty) toast row"
    );
    assert_eq!(element_text(&rt, "ai-memory-remember-toast-text"), "");
}

#[test]
fn audit_remember_dismiss_fires_on_enter_and_candidate_on_space() {
    let mut p = audit_probe();
    p.list.set_memory_candidate_count(Some(1));
    p.list.set_remember_notice(Some("已记住。".to_string()));
    let mut rt = runtime(p, 960.0, 640.0);
    // Space (kit on_press) dismisses the candidate toast.
    focus_by_tab(&mut rt, "ai-memory-candidate-dismiss");
    press(&mut rt, Key::Text(" ".into()));
    assert_eq!(rt.state.candidate_dismisses, 1);
    assert!(!has_key(&rt, "ai-memory-candidate-toast"));
    // Enter dismisses the remember toast.
    focus_by_tab(&mut rt, "ai-memory-remember-dismiss");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.remember_dismisses, 1);
    assert!(!has_key(&rt, "ai-memory-remember-toast"));
}

// ---- rendering: remember button gating --------------------------------------

#[test]
fn audit_remember_button_only_on_the_last_assistant() {
    // Last message is a user message: no remember button anywhere.
    let mut p = audit_probe();
    p.list.set_messages(vec![
        assistant_msg("answer").id("m1"),
        user_msg("thanks").id("m2"),
    ]);
    let rt = runtime(p, 960.0, 640.0);
    assert!(
        !has_key(&rt, "ai-memory-remember"),
        "the remember button belongs to the last assistant only"
    );
    // Two assistants: exactly one remember button, on the last.
    let mut p = audit_probe();
    p.list.set_messages(vec![
        assistant_msg("first").id("m1"),
        assistant_msg("second").id("m2"),
    ]);
    let mut rt = runtime(p, 960.0, 640.0);
    assert_eq!(count_key(&rt, "ai-memory-remember"), 1);
    click(&mut rt, "ai-memory-remember");
    assert_eq!(rt.state.remembers, vec!["second".to_string()]);
    // While streaming, the last assistant is not "last" any more:
    // React's isLastAssistant includes !isStreaming.
    let mut p = audit_probe();
    p.list.set_messages(vec![
        user_msg("q").id("m1"),
        assistant_msg("answer").id("m2"),
    ]);
    p.list.set_streaming(true);
    p.list.set_stream_content("more".to_string());
    let rt = runtime(p, 960.0, 640.0);
    assert!(
        !has_key(&rt, "ai-memory-remember"),
        "streaming clears isLastAssistant, so no remember button"
    );
}

// ---- rendering: memory inheritance end to end -------------------------------

#[test]
fn audit_inherited_session_memory_renders_the_chip() {
    let mut p = audit_probe();
    p.list.set_session_memory_used(vec![memory("M1")]);
    p.list.set_messages(vec![assistant_msg("answer").id("m1")]);
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(
        has_key(&rt, "ai-memory-chip"),
        "the last assistant inherits the session memory, so the chip renders"
    );
    click(&mut rt, "ai-memory-chip");
    assert_eq!(rt.state.memory_toggles, vec![0]);
    assert!(has_key(&rt, "ai-memory-item-M1"));
}

#[test]
fn audit_streaming_bubble_shows_the_session_memory_chip() {
    let mut p = audit_probe();
    p.list.set_session_memory_used(vec![memory("M7")]);
    p.list.set_streaming(true);
    p.list.set_stream_content("drafting".to_string());
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-streaming-bubble"));
    assert!(
        has_key(&rt, "ai-memory-chip"),
        "the streaming bubble falls back to the session memory"
    );
    // React passes no handlers to the streaming ChatBubble, so the chip
    // renders unwired: pressing it expands nothing.
    click(&mut rt, "ai-memory-chip");
    assert!(
        !has_key(&rt, "ai-memory-item-M7"),
        "the streaming bubble's chip has no toggle wired, as in React"
    );
}

// ---- rendering: streaming exclusivity ---------------------------------------

#[test]
fn audit_thinking_row_and_streaming_bubble_are_exclusive_in_render() {
    let mut p = audit_probe();
    p.list.set_streaming(true);
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-streaming-thinking"));
    assert!(!has_key(&rt, "ai-chat-streaming-bubble"));
    assert_eq!(
        element_text(&rt, "ai-chat-streaming-thinking-label"),
        "思考中…"
    );
    let mut p = audit_probe();
    p.list.set_streaming(true);
    p.list.set_stream_content("partial".to_string());
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-streaming-bubble"));
    assert!(!has_key(&rt, "ai-chat-streaming-thinking"));
    // The streaming bubble shows the streamed content as its bubble text.
    let bubbles: Vec<_> = rt
        .elements()
        .into_iter()
        .filter(|e| e.key.as_deref() == Some("chat-bubble-content"))
        .collect();
    assert_eq!(bubbles.len(), 1);
    assert_eq!(bubbles[0].text.as_deref(), Some("partial"));
}

// ---- rendering: questions gating --------------------------------------------

#[test]
fn audit_questions_hidden_when_not_the_last_assistant() {
    // Questions ride on the first assistant; the last message is a user
    // message, so no questions block renders.
    let question = AiQuestion::new("q1", "Pick one")
        .option("a", "Alpha")
        .option("b", "Beta");
    let message = assistant_msg("Which?").id("m1").questions(vec![question]);
    let mut p = audit_probe();
    p.list
        .set_messages(vec![message, user_msg("b then").id("m2")]);
    let rt = runtime(p, 960.0, 640.0);
    assert!(
        !has_key(&rt, "question-block"),
        "questions render only under the last assistant"
    );
    // Questions on the last assistant while streaming: isLastAssistant is
    // false, so the block stays hidden.
    let question = AiQuestion::new("q1", "Pick one")
        .option("a", "Alpha")
        .option("b", "Beta");
    let message = assistant_msg("Which?").id("m2").questions(vec![question]);
    let mut p = audit_probe();
    p.list.set_messages(vec![user_msg("hi").id("m1"), message]);
    p.list.set_streaming(true);
    p.list.set_stream_content("more".to_string());
    let rt = runtime(p, 960.0, 640.0);
    assert!(
        !has_key(&rt, "question-block"),
        "streaming hides the questions block with the last-assistant gate"
    );
}

// ---- rendering: index routing beyond message 0 ------------------------------

#[test]
fn audit_bubble_toggle_reports_the_second_message_index() {
    let mut p = audit_probe();
    p.list.set_messages(vec![
        user_msg("first").id("m1"),
        assistant_msg("with reasoning")
            .id("m2")
            .reasoning("because"),
    ]);
    let mut rt = runtime(p, 960.0, 640.0);
    click(&mut rt, "chat-bubble-reasoning");
    assert_eq!(rt.state.reasoning_toggles, vec![1]);
    assert!(
        rt.state.list.bubble_state(1).reasoning_open(),
        "the toggle drove message 1's bubble state"
    );
    assert!(
        !rt.state.list.bubble_state(0).reasoning_open(),
        "message 0 is untouched"
    );
}

// ---- rendering: empty state + anchor in light mode --------------------------

#[test]
fn audit_empty_list_in_light_mode_shows_the_literal_welcome() {
    let rt = runtime(audit_probe(), 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-chat-empty"),
        "你好！我是你的数据库助手。你可以问我关于 SQL 查询、数据库概念或数据分析的问题。"
    );
    assert!(has_key(&rt, "messages-end"), "anchor is always present");
    assert!(!has_key(&rt, "chat-bubble"));
    assert!(!has_key(&rt, "jump-to-bottom"));
}

// ---- rendering: jump pill literals ------------------------------------------

#[test]
fn audit_jump_pill_shows_the_literal_label_and_two_digit_count() {
    let mut p = audit_probe();
    p.list
        .set_messages(vec![user_msg("q").id("m1"), assistant_msg("a").id("m2")]);
    p.list.on_scroll(500.0);
    p.list.note_new_messages(12);
    let rt = runtime(p, 960.0, 640.0);
    assert_eq!(element_text(&rt, "jump-to-bottom-label"), "回到底部");
    assert_eq!(element_text(&rt, "jump-to-bottom-count"), "12");
    assert_eq!(
        a11y_label(&rt, "jump-to-bottom").as_deref(),
        Some("回到底部 12"),
        "the pill exposes its label plus the unread count to assistive tech"
    );
}

// ---- geometry: light mode, both sizes, full content -------------------------

#[test]
fn audit_light_mode_full_list_geometry() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut p = audit_probe();
        p.list.set_messages(vec![
            user_msg("select * from users where id = 1").id("m1"),
            assistant_msg("Here are the rows you asked for.").id("m2"),
        ]);
        p.list.set_memory_candidate_count(Some(2));
        p.list.set_remember_notice(Some("已记住。".to_string()));
        p.list.on_scroll(500.0);
        p.list.note_new_messages(4);
        let rt = runtime(p, w, h);
        let label = format!("light {w}x{h} full list");
        assert_no_horizontal_overflow(&rt, w, &label);
        assert_rect_inside(&rt, "ai-chat-message-list-viewport", w, h, &label);
        assert_rect_inside(&rt, "ai-memory-candidate-toast", w, h, &label);
        assert_rect_inside(&rt, "ai-memory-remember-toast", w, h, &label);
        assert_rect_inside(&rt, "jump-to-bottom", w, h, &label);
        assert_no_sibling_overlap(&rt, &label);
        // Toasts stack candidate-first, above the viewport; the pill is
        // stacked directly under the viewport (not absolutely positioned).
        let candidate = element(&rt, "ai-memory-candidate-toast").rect;
        let remember = element(&rt, "ai-memory-remember-toast").rect;
        let viewport = element(&rt, "ai-chat-message-list-viewport").rect;
        let pill = element(&rt, "jump-to-bottom").rect;
        assert!(
            candidate[1] + candidate[3] <= remember[1] + 0.5,
            "{label}: candidate toast {candidate:?} must sit above remember toast {remember:?}"
        );
        assert!(
            remember[1] + remember[3] <= viewport[1] + 0.5,
            "{label}: toasts {remember:?} must sit above the viewport {viewport:?}"
        );
        assert!(
            pill[1] >= viewport[1] + viewport[3] - 0.5,
            "{label}: pill {pill:?} must sit under the viewport {viewport:?}"
        );
        let centre = pill[0] + pill[2] / 2.0;
        assert!(
            (centre - w / 2.0).abs() <= 1.0,
            "{label}: pill centre {centre} should sit on the window centre {}",
            w / 2.0
        );
    }
}

#[test]
fn audit_dark_mode_long_messages_do_not_overflow() {
    let long_zh =
        "这是一段很长的中文回答，用于验证长文本在消息气泡内换行而不会横向溢出窗口。".repeat(6);
    // Realistic long tokens: a URL (break opportunities at '/') and a
    // hyphenated UUID. A single 160-character unbroken run still overflows
    // the bubble — see finding D3 in the audit report; React's
    // `<pre class="whitespace-pre-wrap">` breaks no unbroken word either,
    // so that case is left out of the passing geometry here.
    let long_url = "https://example.com/api/v1/databases/production/tables/users/columns/created_at/indexes/primary/statistics";
    let uuid = "550e8400-e29b-41d4-a716-446655440000-550e8400-e29b-41d4-a716-446655440000";
    let mut p = audit_probe();
    p.dark = true;
    p.list.set_messages(vec![
        user_msg(&long_zh).id("m1"),
        assistant_msg(&format!("see {long_url} and {uuid}")).id("m2"),
        user_msg("短消息").id("m3"),
    ]);
    p.list.set_memory_candidate_count(Some(1));
    p.list.on_scroll(500.0);
    p.list.note_new_messages(2);
    let rt = runtime(p, 960.0, 640.0);
    assert_no_horizontal_overflow(&rt, 960.0, "dark 960x640 long messages");
    assert_no_sibling_overlap(&rt, "dark 960x640 long messages");
    assert_rect_inside(
        &rt,
        "jump-to-bottom",
        960.0,
        640.0,
        "dark long messages pill",
    );
}
