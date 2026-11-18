//! T009-S4: AiChatPanel message list (`AiChatPanel.tsx:642–790`).
//!
//! Pure-logic tests call the state machine directly; runtime tests render
//! the list in a probe `Runtime` and read back the laid-out element tree
//! — never the component's own report of itself. Geometry is asserted on
//! `rt.elements()` rects, which come from the layout engine, not from the
//! code under test. Horizontal overflow is asserted on every element
//! (the DoD's no-horizontal-overflow rule); vertical overflow is expected
//! inside the scroll viewport and is not asserted there.
//!
//! The list is a view-model placeholder for the `aiStore` slice (T088):
//! the tests seed the state through its `set_*` methods and assert the
//! callbacks report presses, because no store exists to read back yet.
//! The 4s remember-notice auto-clear and memory creation are the panel
//! slice's job, so the tests pin the notice rendering and the dismiss
//! report, not a timer firing.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Tx};

use crate::components::ai::{
    AiChatMessage, AiChatMessageList, AiChatMessageListState, AiChatRole, AiMemoryReference,
    AiQuestion,
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

fn two_messages() -> Vec<AiChatMessage> {
    vec![
        user_msg("select * from users").id("m1"),
        assistant_msg("Here are the rows.").id("m2"),
    ]
}

#[derive(Default)]
struct Probe {
    list: AiChatMessageListState,
    dark: bool,
    wire_at_bottom: bool,
    candidate_dismisses: usize,
    remember_dismisses: usize,
    jumps: usize,
    remembers: Vec<String>,
    reasoning_toggles: Vec<usize>,
    tool_toggles: Vec<usize>,
    memory_toggles: Vec<usize>,
    cites: Vec<(usize, String)>,
    inserts: Vec<String>,
    runs: Vec<(String, String)>,
    new_queries: Vec<String>,
    answers: Vec<(usize, String)>,
    at_bottom_reports: Vec<bool>,
}

fn probe() -> Probe {
    Probe {
        list: AiChatMessageListState::default(),
        dark: true,
        ..Probe::default()
    }
}

fn list_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut list = AiChatMessageList::<Probe>::new(&state.list)
        .sql_dialect("postgres")
        .on_insert_sql(|s: &mut Probe, sql: &str| s.inserts.push(sql.to_string()))
        .on_run_code(|s: &mut Probe, sql: &str, dialect: &str| {
            s.runs.push((sql.to_string(), dialect.to_string()));
        })
        .on_new_query(|s: &mut Probe, sql: &str| s.new_queries.push(sql.to_string()))
        .on_toggle_reasoning(|s: &mut Probe, index| {
            s.reasoning_toggles.push(index);
            s.list.bubble_state_mut(index).toggle_reasoning();
        })
        .on_toggle_tool(|s: &mut Probe, index| {
            s.tool_toggles.push(index);
            s.list.bubble_state_mut(index).toggle_tool();
        })
        .on_toggle_memory(|s: &mut Probe, index| {
            s.memory_toggles.push(index);
            s.list.bubble_state_mut(index).toggle_memory();
        })
        .on_cite_memory(|s: &mut Probe, index, reference: &str| {
            s.cites.push((index, reference.to_string()));
            s.list.bubble_state_mut(index).open_memory_citation(reference);
        })
        .on_remember(|s: &mut Probe, message: &AiChatMessage| {
            s.remembers.push(message.content.clone());
        })
        .on_toggle_question(|s: &mut Probe, index, qid, oid, multi| {
            s.list.question_state_mut(index).toggle(qid, oid, multi);
        })
        .on_custom_edit(|s: &mut Probe, index, qid: &str| {
            s.list.question_state_mut(index).custom_edit(qid)
        })
        .on_submit_questions(|s: &mut Probe, index| {
            let questions = s.list.messages()[index].questions.clone();
            if let Some(payload) = s.list.question_state_mut(index).submit(&questions) {
                s.answers.push((index, payload));
            }
        })
        .on_dismiss_memory_candidate(|s: &mut Probe| {
            s.candidate_dismisses += 1;
            s.list.dismiss_memory_candidate();
        })
        .on_dismiss_remember_notice(|s: &mut Probe| {
            s.remember_dismisses += 1;
            s.list.dismiss_remember_notice();
        })
        .on_jump_to_bottom(|s: &mut Probe, tx: &mut Tx| {
            s.jumps += 1;
            s.list.jump_to_bottom();
            tx.scroll_to_bottom("ai-chat-message-list-viewport");
        });
    if state.wire_at_bottom {
        list = list.on_at_bottom(|s: &mut Probe, at: bool| {
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

fn runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, list_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

/// The same list with no callbacks wired at all: the dismiss buttons and
/// the jump pill render unwired, covering the `None` branches.
fn bare_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    rune_kit::root(
        col()
            .key("probe-page")
            .w_full()
            .h_full()
            .padding(16.0)
            .child(AiChatMessageList::<Probe>::new(&state.list).build(state.dark)),
        Vec::new(),
    )
}

fn bare_runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, bare_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// ---- AiChatMessageListState: pure logic -------------------------------------

#[test]
fn the_empty_state_needs_a_session_without_messages_or_streaming() {
    let mut state = AiChatMessageListState::default();
    assert!(state.show_empty(), "fresh VM: session, no messages");
    state.set_messages(two_messages());
    assert!(!state.show_empty(), "messages hide the welcome line");
    state.set_messages(Vec::new());
    state.set_streaming(true);
    assert!(!state.show_empty(), "streaming hides the welcome line");
    state.set_streaming(false);
    state.set_has_session(false);
    assert!(
        !state.show_empty(),
        "React's `chatSession?.messages` is undefined before initChat: no welcome without a session"
    );
}

#[test]
fn the_last_assistant_message_inherits_session_memory_only_when_it_has_none() {
    let mut state = AiChatMessageListState::default();
    state.set_session_memory_used(vec![memory("M1")]);
    state.set_messages(vec![
        assistant_msg("first").id("m1"),
        assistant_msg("last").id("m2"),
    ]);
    // The first assistant message never inherits, even with an empty list.
    assert!(
        state
            .effective_message(0)
            .is_some_and(|m| m.memory_used.is_empty()),
        "only the last assistant message inherits"
    );
    assert_eq!(
        state.effective_message(1).map(|m| m.memory_used),
        Some(vec![memory("M1")]),
        "last assistant with no own memory inherits the session memory"
    );
    // Its own memory wins over the session's.
    state.set_messages(vec![assistant_msg("last")
        .id("m2")
        .memory_used(vec![memory("M9")])]);
    assert_eq!(
        state.effective_message(0).map(|m| m.memory_used),
        Some(vec![memory("M9")]),
        "own memory is kept, session memory is not merged in"
    );
    // A user message never inherits.
    state.set_messages(vec![user_msg("hi").id("m1")]);
    assert!(
        state
            .effective_message(0)
            .is_some_and(|m| m.memory_used.is_empty()),
        "user messages never inherit session memory"
    );
}

#[test]
fn the_streaming_gates_follow_content_and_reasoning_truthiness() {
    let mut state = AiChatMessageListState::default();
    assert!(!state.show_streaming_bubble() && !state.show_thinking_row());
    state.set_streaming(true);
    assert!(
        state.show_thinking_row(),
        "streaming with nothing yet shows the thinking row"
    );
    state.set_stream_content("  ");
    assert!(
        state.show_streaming_bubble() && !state.show_thinking_row(),
        "whitespace-only content is truthy in JS: it opens the bubble, it does not stay thinking"
    );
    state.set_stream_content(String::new());
    state.set_stream_reasoning("reasoning…");
    assert!(
        state.show_streaming_bubble(),
        "reasoning alone opens the bubble"
    );
    // The plain getters report exactly what was seeded.
    assert_eq!(state.stream_content(), "");
    assert_eq!(state.stream_reasoning(), "reasoning…");
    assert_eq!(state.stream_mcp_tool_name(), None);
    assert!(state.has_session());
    assert!(state.is_streaming());
}

#[test]
fn the_streaming_message_prefers_stream_memory_over_session_memory() {
    let mut state = AiChatMessageListState::default();
    state.set_stream_content("draft");
    state.set_stream_reasoning("why");
    state.set_session_memory_used(vec![memory("M1")]);
    assert_eq!(
        state.streaming_message().memory_used,
        vec![memory("M1")],
        "no stream memory: the session memory is the fallback"
    );
    state.set_stream_memory_used(vec![memory("M2")]);
    let message = state.streaming_message();
    assert_eq!(message.memory_used, vec![memory("M2")]);
    assert_eq!(message.content, "draft");
    assert_eq!(message.reasoning.as_deref(), Some("why"));
    assert_eq!(message.role, AiChatRole::Assistant);
}

#[test]
fn the_thinking_label_formats_the_mcp_tool_name() {
    let mut state = AiChatMessageListState::default();
    assert_eq!(state.thinking_label(), t("chat.thinking"));
    state.set_stream_mcp_tool_name(Some("mcp/server/do_thing".to_string()));
    assert_eq!(
        state.thinking_label(),
        t("chat.callingMcpTool").replacen("{name}", "do_thing", 1),
        "a/b/c… shows only the part after the second slash (S1's formatter)"
    );
    state.set_stream_mcp_tool_name(Some("plain-tool".to_string()));
    assert!(
        state.thinking_label().contains("plain-tool"),
        "fewer than three segments keep the whole name"
    );
}

#[test]
fn the_auto_scroll_state_machine_mirrors_use_auto_scroll() {
    let mut state = AiChatMessageListState::default();
    assert!(state.at_bottom() && state.unread_count() == 0);
    // New messages while pinned never count as unread.
    state.note_new_messages(2);
    assert_eq!(state.unread_count(), 0, "pinned: arrivals are read");
    assert!(!state.show_jump_to_bottom());
    // 120pt is the hook's threshold, inclusive.
    state.on_scroll(120.0);
    assert!(
        state.at_bottom(),
        "exactly 120pt from the bottom still counts as at bottom"
    );
    state.on_scroll(120.5);
    assert!(
        !state.at_bottom(),
        "past the threshold the user has scrolled up"
    );
    state.note_new_messages(3);
    assert_eq!(state.unread_count(), 3);
    assert!(state.show_jump_to_bottom(), "away + unread shows the pill");
    state.note_new_messages(2);
    assert_eq!(state.unread_count(), 5, "arrivals accumulate while away");
    // Scrolling back to the bottom clears the count, as the hook does.
    state.on_scroll(0.0);
    assert!(state.at_bottom() && state.unread_count() == 0);
    assert!(!state.show_jump_to_bottom());
    // Jump resets even without a scroll event.
    state.on_scroll(500.0);
    state.note_new_messages(1);
    state.jump_to_bottom();
    assert!(state.at_bottom() && state.unread_count() == 0);
}

#[test]
fn set_messages_resizes_the_per_message_states() {
    let mut state = AiChatMessageListState::default();
    state.set_messages(two_messages());
    // Fresh per-message states match the defaults field by field.
    assert!(!state.bubble_state(0).reasoning_open());
    assert!(state.bubble_state(0).tool_collapsed());
    assert!(!state.bubble_state(0).memory_open());
    assert!(!state.bubble_state(1).reasoning_open());
    // Out-of-range indices fall back instead of panicking.
    assert!(!state.bubble_state(9).reasoning_open());
    assert!(state.bubble_state(9).tool_collapsed());
    state.bubble_state_mut(0).toggle_reasoning();
    assert!(state.bubble_state(0).reasoning_open());
    assert!(
        !state.bubble_state(1).reasoning_open(),
        "per-message state is independent"
    );
}

// ---- rendering: empty state, messages, anchor -------------------------------

#[test]
fn the_empty_list_renders_the_welcome_line_and_the_anchor() {
    let rt = runtime(probe(), 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-empty"), "welcome line is laid out");
    assert_eq!(element_text(&rt, "ai-chat-empty"), t("chat.welcome"));
    assert!(
        has_key(&rt, "messages-end"),
        "the scroll anchor is always present"
    );
    assert!(!has_key(&rt, "chat-bubble"), "no messages, no bubbles");
    assert!(!has_key(&rt, "jump-to-bottom"), "no unread while pinned");
}

#[test]
fn messages_render_as_bubbles_with_the_react_keys() {
    let mut p = probe();
    p.list.set_messages(two_messages());
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-message-0"));
    assert!(has_key(&rt, "ai-chat-message-1"));
    assert!(has_key(&rt, "messages-end"));
    assert!(
        !has_key(&rt, "ai-chat-empty"),
        "messages hide the welcome line"
    );
    assert_eq!(
        element_text(&rt, "chat-bubble-content"),
        "select * from users"
    );
    // Only the last assistant message gets the remember button (S2's gate).
    assert!(
        has_key(&rt, "ai-memory-remember"),
        "last assistant + on_remember"
    );
}

#[test]
fn the_remember_button_reports_the_last_assistant_message() {
    let mut p = probe();
    p.list.set_messages(two_messages());
    let mut rt = runtime(p, 960.0, 640.0);
    click(&mut rt, "ai-memory-remember");
    assert_eq!(
        rt.state.remembers,
        vec!["Here are the rows.".to_string()],
        "the press reports the message content, creation stays with the panel slice"
    );
}

#[test]
fn a_bubble_toggle_reports_its_message_index() {
    let mut p = probe();
    let mut first = assistant_msg("with reasoning").reasoning("because");
    first = first.id("m1");
    p.list.set_messages(vec![first, user_msg("ok").id("m2")]);
    let mut rt = runtime(p, 960.0, 640.0);
    click(&mut rt, "chat-bubble-reasoning");
    assert_eq!(rt.state.reasoning_toggles, vec![0]);
    assert!(
        rt.state.list.bubble_state(0).reasoning_open(),
        "the toggle drove message 0's bubble state, not message 1's"
    );
}

// ---- rendering: toasts -------------------------------------------------------

#[test]
fn the_candidate_toast_shows_the_count_and_dismisses() {
    let mut p = probe();
    p.list.set_memory_candidate_count(Some(3));
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-memory-candidate-toast"));
    assert_eq!(
        element_text(&rt, "ai-memory-candidate-toast-text"),
        t("chat.memoryCandidates").replacen("{count}", "3", 1)
    );
    click(&mut rt, "ai-memory-candidate-dismiss");
    assert_eq!(rt.state.candidate_dismisses, 1);
    assert!(
        !has_key(&rt, "ai-memory-candidate-toast"),
        "dismiss clears the notice"
    );
}

#[test]
fn the_remember_toast_shows_the_notice_verbatim_and_dismisses() {
    let mut p = probe();
    p.list
        .set_remember_notice(Some("已加入记忆候选队列。".to_string()));
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-memory-remember-toast"));
    assert_eq!(
        element_text(&rt, "ai-memory-remember-toast-text"),
        "已加入记忆候选队列。"
    );
    click(&mut rt, "ai-memory-remember-dismiss");
    assert_eq!(rt.state.remember_dismisses, 1);
    assert!(!has_key(&rt, "ai-memory-remember-toast"));
}

#[test]
fn both_toasts_stack_with_the_candidate_first() {
    let mut p = probe();
    p.list.set_memory_candidate_count(Some(1));
    p.list.set_remember_notice(Some("notice".to_string()));
    let rt = runtime(p, 960.0, 640.0);
    let candidate = element(&rt, "ai-memory-candidate-toast").rect;
    let remember = element(&rt, "ai-memory-remember-toast").rect;
    assert!(
        candidate[1] + candidate[3] <= remember[1] + 0.5,
        "candidate toast {candidate:?} sits above the remember toast {remember:?}, as in React"
    );
}

// ---- rendering: streaming ----------------------------------------------------

#[test]
fn the_streaming_bubble_renders_streamed_content_without_callbacks() {
    let mut p = probe();
    p.list.set_messages(vec![user_msg("q").id("m1")]);
    p.list.set_streaming(true);
    p.list.set_stream_content("partial answer".to_string());
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-streaming-bubble"));
    assert!(!has_key(&rt, "ai-chat-streaming-thinking"));
    // The streaming bubble gets no remember button: React passes no
    // onRemember to it, and it is not the last settled message.
    assert!(!has_key(&rt, "ai-memory-remember"));
    let bubbles: Vec<_> = rt
        .elements()
        .into_iter()
        .filter(|e| e.key.as_deref() == Some("chat-bubble-content"))
        .collect();
    assert_eq!(bubbles.len(), 2, "settled user message + streaming bubble");
    assert_eq!(
        bubbles[1].text.as_deref(),
        Some("partial answer"),
        "the streaming bubble shows the streamed content"
    );
}

#[test]
fn the_thinking_row_names_the_mcp_tool_or_falls_back_to_thinking() {
    let mut p = probe();
    p.list.set_streaming(true);
    p.list
        .set_stream_mcp_tool_name(Some("mcp/github/search".to_string()));
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-streaming-thinking"));
    assert_eq!(
        element_text(&rt, "ai-chat-streaming-thinking-label"),
        t("chat.callingMcpTool").replacen("{name}", "search", 1)
    );
    let mut p = probe();
    p.list.set_streaming(true);
    let rt = runtime(p, 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-chat-streaming-thinking-label"),
        t("chat.thinking")
    );
}

// ---- rendering: jump to bottom ------------------------------------------------

#[test]
fn the_jump_pill_shows_only_when_away_with_unread_and_jumps_on_press() {
    let mut p = probe();
    p.list.set_messages(two_messages());
    p.list.on_scroll(500.0);
    p.list.note_new_messages(3);
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(
        has_key(&rt, "jump-to-bottom"),
        "away + 3 unread shows the pill"
    );
    assert_eq!(element_text(&rt, "jump-to-bottom-count"), "3");
    assert_eq!(
        element_text(&rt, "jump-to-bottom-label"),
        t("chat.jumpToBottom")
    );
    click(&mut rt, "jump-to-bottom");
    assert_eq!(rt.state.jumps, 1);
    assert!(rt.state.list.at_bottom());
    assert_eq!(rt.state.list.unread_count(), 0);
    assert!(
        !has_key(&rt, "jump-to-bottom"),
        "after the jump the pill is gone"
    );
}

#[test]
fn the_jump_pill_stays_hidden_without_unread_even_when_away() {
    let mut p = probe();
    p.list.set_messages(two_messages());
    p.list.on_scroll(500.0);
    // No note_new_messages: React's hook never increments the count, so
    // the pill stays hidden — the dead gate, pinned, not worked around.
    let rt = runtime(p, 960.0, 640.0);
    assert!(
        !has_key(&rt, "jump-to-bottom"),
        "away from the bottom but zero unread: React renders no pill"
    );
}

#[test]
fn the_viewport_reports_at_bottom_to_the_caller() {
    let mut p = probe();
    // Enough messages to overflow the viewport, so the scroll state exists.
    p.list.set_messages(
        (0..30)
            .map(|i| {
                AiChatMessage::new(
                    if i % 2 == 0 {
                        AiChatRole::User
                    } else {
                        AiChatRole::Assistant
                    },
                    format!("message {i} with enough text to take a row"),
                )
                .id(format!("m{i}"))
            })
            .collect(),
    );
    p.wire_at_bottom = true;
    let mut rt = runtime(p, 960.0, 640.0);
    // The report is queued during layout and dispatched on a later frame.
    for _ in 0..5 {
        rt.frame(1.0 / 60.0);
    }
    assert!(
        rt.state.at_bottom_reports.contains(&true),
        "a list that opens pinned to the bottom reports at_bottom=true: {:?}",
        rt.state.at_bottom_reports
    );
}

// ---- rendering: questions -----------------------------------------------------

#[test]
fn the_last_assistant_questions_block_answers_through_the_list() {
    let mut p = probe();
    let question = AiQuestion::new("q1", "Pick one")
        .option("a", "Alpha")
        .option("b", "Beta");
    let message = assistant_msg("Which?").id("m2").questions(vec![question]);
    p.list.set_messages(vec![user_msg("hi").id("m1"), message]);
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(
        has_key(&rt, "question-block"),
        "questions render under the last assistant bubble"
    );
    click(&mut rt, "question-option-q1-b");
    assert!(
        rt.state.list.question_state(1).submitted() == false,
        "picking an option does not submit yet"
    );
    click(&mut rt, "question-submit");
    assert_eq!(rt.state.answers.len(), 1);
    assert_eq!(
        rt.state.answers[0].0,
        1,
        "the answer reports message index 1"
    );
    assert!(
        rt.state.answers[0].1.contains("Beta"),
        "the payload carries the picked label: {:?}",
        rt.state.answers[0].1
    );
}

#[test]
fn the_tool_message_toggle_reports_its_index_and_expands() {
    let mut p = probe();
    p.list.set_messages(vec![
        AiChatMessage::new(AiChatRole::Tool, "tool output").id("m1"),
        user_msg("ok").id("m2"),
    ]);
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "tool-role-toggle"), "tool message renders its toggle");
    assert!(
        !has_key(&rt, "chat-bubble-tool-content"),
        "the tool output starts collapsed, as in React"
    );
    click(&mut rt, "tool-role-toggle");
    assert_eq!(rt.state.tool_toggles, vec![0]);
    assert!(has_key(&rt, "chat-bubble-tool-content"), "toggle expands the output");
    assert_eq!(
        element_text(&rt, "chat-bubble-tool-content-text"),
        "tool output"
    );
}

#[test]
fn the_memory_chip_reports_its_index_and_expands_the_list() {
    let mut p = probe();
    p.list.set_messages(vec![assistant_msg("answer")
        .id("m1")
        .memory_used(vec![memory("M1")])]);
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-memory-chip"), "a message with memory renders the chip");
    assert!(!has_key(&rt, "ai-memory-expanded"), "the memory list starts closed");
    click(&mut rt, "ai-memory-chip");
    assert_eq!(rt.state.memory_toggles, vec![0]);
    assert!(has_key(&rt, "ai-memory-expanded"), "chip expands the list");
    assert!(has_key(&rt, "ai-memory-item-M1"), "the cited memory is listed");
}

#[test]
fn the_jump_pill_also_fires_on_enter() {
    let mut p = probe();
    p.list.set_messages(two_messages());
    p.list.on_scroll(500.0);
    p.list.note_new_messages(2);
    let mut rt = runtime(p, 960.0, 640.0);
    focus_by_tab(&mut rt, "jump-to-bottom");
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.jumps, 1,
        "React's <button> fires on Enter as well as on click"
    );
    assert!(!has_key(&rt, "jump-to-bottom"), "after the jump the pill is gone");
}

#[test]
fn the_candidate_dismiss_also_fires_on_enter() {
    let mut p = probe();
    p.list.set_memory_candidate_count(Some(1));
    let mut rt = runtime(p, 960.0, 640.0);
    focus_by_tab(&mut rt, "ai-memory-candidate-dismiss");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.candidate_dismisses, 1);
    assert!(!has_key(&rt, "ai-memory-candidate-toast"));
}

#[test]
fn an_unwired_list_still_renders_its_toasts_and_pill() {
    let mut p = probe();
    p.list.set_messages(two_messages());
    p.list.set_memory_candidate_count(Some(1));
    p.list.on_scroll(500.0);
    p.list.note_new_messages(1);
    let mut rt = bare_runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-memory-candidate-toast"));
    assert!(has_key(&rt, "jump-to-bottom"));
    click(&mut rt, "jump-to-bottom");
    assert_eq!(
        rt.state.jumps, 0,
        "with no jump callback wired, the press reports nothing and the pill stays"
    );
    assert!(has_key(&rt, "jump-to-bottom"));
}

#[test]
fn typing_custom_text_reaches_the_questions_state() {
    let mut p = probe();
    let question = AiQuestion::new("q1", "Pick one").option("a", "Alpha");
    let message = assistant_msg("Which?").id("m2").questions(vec![question]);
    p.list.set_messages(vec![user_msg("hi").id("m1"), message]);
    let mut rt = runtime(p, 960.0, 640.0);
    click(&mut rt, "question-custom-input-q1");
    rt.paste("free text");
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        rt.state.list.question_state(1).custom_text("q1"),
        "free text",
        "the custom draft is stored on message 1's questions state"
    );
}

// ---- i18n + geometry ----------------------------------------------------------

#[test]
fn every_key_this_slice_asks_for_exists() {
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
}

#[test]
fn the_list_and_its_pill_stay_inside_both_window_sizes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut p = probe();
        p.list.set_messages(two_messages());
        p.list.set_memory_candidate_count(Some(2));
        p.list.set_remember_notice(Some("notice".to_string()));
        p.list.on_scroll(500.0);
        p.list.note_new_messages(4);
        let rt = runtime(p, w, h);
        assert_no_horizontal_overflow(&rt, w, &format!("{w}x{h} list"));
        assert_rect_inside(
            &rt,
            "ai-chat-message-list-viewport",
            w,
            h,
            &format!("{w}x{h} viewport"),
        );
        assert_rect_inside(&rt, "jump-to-bottom", w, h, &format!("{w}x{h} pill"));
        assert_rect_inside(
            &rt,
            "ai-memory-candidate-toast",
            w,
            h,
            &format!("{w}x{h} toast"),
        );
        // The pill is centred under the viewport (stacked, not absolute —
        // see the module docs): its centre tracks the window centre.
        let pill = element(&rt, "jump-to-bottom").rect;
        let centre = pill[0] + pill[2] / 2.0;
        assert!(
            (centre - w / 2.0).abs() <= 1.0,
            "{w}x{h}: pill centre {centre} should sit on the window centre {}",
            w / 2.0
        );
    }
}
