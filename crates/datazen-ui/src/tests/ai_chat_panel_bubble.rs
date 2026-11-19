//! T009-S2: AiChatPanel `ChatBubble` (`AiChatPanel.tsx:992–1210`).
//!
//! Pure-logic tests call the state machine directly; runtime tests render
//! the bubble in a probe `Runtime` and read back the laid-out element tree
//! and the accessibility dump — never the component's own report of itself.
//! Geometry is asserted on `rt.elements()` rects and on the reported
//! `opacity` / `loop_phase`, which come from the layout engine and the
//! render clock, not from the code under test.
//!
//! The assistant content renders as a plain-text placeholder until
//! `AiMessageContent` (T169) lands — see `chat_bubble.rs` — so the tests
//! pin the placeholder key (`chat-bubble-content-placeholder`) and the raw
//! text, and never claim Markdown / SQL rendering. The citation jump is
//! pinned through the state transition (`open_memory_citation`) plus its
//! render effect, because the placeholder has no clickable citation link
//! yet; the `on_cite_memory` hook itself cannot be exercised until T169
//! provides the link.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::ai::{
    AiChatMessage, AiChatRole, AiMemoryReference, AiQuestion, ChatBubble, ChatBubbleState,
    QuestionBlockState,
};
use crate::i18n::{has, t};
use crate::icons::ICON_BOOK_OPEN;

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

fn focused_key<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
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

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
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
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

fn contains(outer: [f32; 4], inner: [f32; 4]) -> bool {
    let (ox, oy, ow, oh) = (outer[0], outer[1], outer[2], outer[3]);
    let (ix, iy, iw, ih) = (inner[0], inner[1], inner[2], inner[3]);
    ix >= ox - 0.5 && iy >= oy - 0.5 && ix + iw <= ox + ow + 0.5 && iy + ih <= oy + oh + 0.5
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

/// Siblings under the same parent path never overlap, except when one rect
/// contains the other (a child inside its parent's box is layering, not a
/// collision). Copied from `base_geometry.rs`'s `assert_siblings`, scoped
/// to the probe page; no allow-list — a collision here is a real defect.
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

fn assistant(content: &str) -> AiChatMessage {
    AiChatMessage::new(AiChatRole::Assistant, content)
}

fn user(content: &str) -> AiChatMessage {
    AiChatMessage::new(AiChatRole::User, content)
}

fn memory(reference: &str, subject: &str) -> AiMemoryReference {
    AiMemoryReference::new(
        reference,
        format!("id-{reference}"),
        subject,
        "preference",
        "user",
    )
}

fn sample_questions() -> Vec<AiQuestion> {
    vec![
        AiQuestion::new("q1", "Which database?")
            .option("o1", "Postgres")
            .option("o2", "MySQL"),
        AiQuestion::new("q2", "Anything else?"),
    ]
}

struct Probe {
    message: AiChatMessage,
    streaming: bool,
    last_assistant: bool,
    dark: bool,
    bubble: ChatBubbleState,
    block: QuestionBlockState,
    wire_remember: bool,
    wire_questions: bool,
    remembered: Vec<String>,
    answered: Vec<String>,
}

fn probe(message: AiChatMessage) -> Probe {
    let block = QuestionBlockState::new(&message.questions);
    Probe {
        message,
        streaming: false,
        last_assistant: false,
        dark: true,
        bubble: ChatBubbleState::default(),
        block,
        wire_remember: true,
        wire_questions: true,
        remembered: Vec::new(),
        answered: Vec::new(),
    }
}

fn bubble_el(state: &Probe) -> El {
    let mut bubble = ChatBubble::<Probe>::new(&state.message, &state.bubble)
        .streaming(state.streaming)
        .last_assistant(state.last_assistant)
        .sql_dialect("postgres")
        .on_insert_sql(|_, _| {})
        .on_run_code(|_, _, _| {})
        .on_new_query(|_, _| {})
        .on_toggle_reasoning(|s: &mut Probe| s.bubble.toggle_reasoning())
        .on_toggle_tool(|s: &mut Probe| s.bubble.toggle_tool())
        .on_toggle_memory(|s: &mut Probe| s.bubble.toggle_memory())
        .on_cite_memory(|s: &mut Probe, reference: &str| {
            s.bubble.open_memory_citation(reference);
        });
    if state.wire_remember {
        bubble = bubble.on_remember(|s: &mut Probe, m: &AiChatMessage| {
            s.remembered.push(m.content.clone());
        });
    }
    if state.wire_questions {
        let questions = state.message.questions.clone();
        bubble = bubble.with_questions(
            &state.block,
            |s: &mut Probe, qid, oid, multi| s.block.toggle(qid, oid, multi),
            |s: &mut Probe, qid: &str| s.block.custom_edit(qid),
            move |s: &mut Probe| {
                if let Some(payload) = s.block.submit(&questions) {
                    s.answered.push(payload);
                }
            },
        );
    }
    bubble.build(state.dark)
}

fn bubble_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    col()
        .key("probe-page")
        .w_full()
        .h_full()
        .padding(16.0)
        .child(bubble_el(state))
}

fn runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, bubble_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// ---- ChatBubbleState: pure predicates ----------------------------------------

#[test]
fn builder_setters_keep_the_message_identity_and_memory_score() {
    let message = assistant("answer").id("m-1");
    assert_eq!(message.id.as_deref(), Some("m-1"));
    let reference = memory("M1", "prefers snake_case").score(0.9);
    assert_eq!(reference.score, 0.9);
    assert_eq!(reference.reference, "M1");
}

#[test]
fn tool_bubble_only_when_tool_role_has_content() {
    let tool_with_content = AiChatMessage::new(AiChatRole::Tool, "ls -la");
    assert!(ChatBubbleState::is_tool_bubble(&tool_with_content));
    let tool_empty = AiChatMessage::new(AiChatRole::Tool, "");
    assert!(
        !ChatBubbleState::is_tool_bubble(&tool_empty),
        "a tool message with no content falls through to the normal bubble"
    );
    assert!(!ChatBubbleState::is_tool_bubble(&assistant("hi")));
    assert!(!ChatBubbleState::is_tool_bubble(&user("hi")));
}

#[test]
fn pulsing_only_while_streaming_with_reasoning_and_no_content() {
    let streaming_reasoning = assistant("").reasoning("thinking…");
    assert!(ChatBubbleState::is_pulsing(&streaming_reasoning, true));
    assert!(
        !ChatBubbleState::is_pulsing(&streaming_reasoning, false),
        "a settled message never pulses"
    );
    let with_content = assistant("partial").reasoning("thinking…");
    assert!(
        !ChatBubbleState::is_pulsing(&with_content, true),
        "content arrived — the pulse stops"
    );
    let no_reasoning = assistant("");
    assert!(
        !ChatBubbleState::is_pulsing(&no_reasoning, true),
        "no reasoning, nothing to pulse over"
    );
    let empty_reasoning = assistant("").reasoning("");
    assert!(
        !ChatBubbleState::is_pulsing(&empty_reasoning, true),
        "an empty reasoning string is falsy, as in React"
    );
}

#[test]
fn has_reasoning_ignores_a_missing_or_empty_reasoning_string() {
    assert!(!ChatBubbleState::has_reasoning(&assistant("hi")));
    assert!(!ChatBubbleState::has_reasoning(
        &assistant("hi").reasoning("")
    ));
    assert!(ChatBubbleState::has_reasoning(
        &assistant("hi").reasoning("why")
    ));
}

#[test]
fn remember_gate_needs_every_conjunct() {
    let message = assistant("answer");
    assert!(ChatBubbleState::show_remember(&message, false, true, true));
    assert!(
        !ChatBubbleState::show_remember(&message, true, true, true),
        "streaming hides the button"
    );
    assert!(
        !ChatBubbleState::show_remember(&message, false, false, true),
        "only the last assistant message offers it"
    );
    assert!(
        !ChatBubbleState::show_remember(&message, false, true, false),
        "no onRemember, no button"
    );
    assert!(
        !ChatBubbleState::show_remember(&assistant(""), false, true, true),
        "no content, nothing to remember"
    );
    assert!(
        !ChatBubbleState::show_remember(&user("hi"), false, true, true),
        "the user never remembers their own message"
    );
}

#[test]
fn questions_gate_excludes_user_and_streaming_messages() {
    let message = assistant("answer").questions(sample_questions());
    assert!(ChatBubbleState::has_questions(&message, false));
    assert!(
        !ChatBubbleState::has_questions(&message, true),
        "no questions block while streaming"
    );
    let user_message = user("hi").questions(sample_questions());
    assert!(
        !ChatBubbleState::has_questions(&user_message, false),
        "the user never gets a questions block"
    );
    assert!(!ChatBubbleState::has_questions(&assistant("answer"), false));
}

#[test]
fn citation_jump_opens_the_list_and_selects_the_reference() {
    let mut state = ChatBubbleState::default();
    assert!(!state.memory_open());
    assert_eq!(state.memory_focused(), None);
    state.open_memory_citation("M2");
    assert!(state.memory_open(), "the jump expands the list");
    assert_eq!(state.memory_focused(), Some("M2"));
    state.open_memory_citation("M9");
    assert!(state.memory_open());
    assert_eq!(
        state.memory_focused(),
        Some("M9"),
        "the jump still opens the list for an unknown reference — React sets open regardless and its querySelector simply finds nothing"
    );
}

#[test]
fn toggles_flip_each_section_independently() {
    let mut state = ChatBubbleState::default();
    assert!(!state.reasoning_open());
    assert!(state.tool_collapsed(), "the tool section starts collapsed");
    assert!(!state.memory_open());
    state.toggle_reasoning();
    assert!(state.reasoning_open());
    assert!(state.tool_collapsed());
    assert!(!state.memory_open());
    state.toggle_tool();
    assert!(!state.tool_collapsed());
    state.toggle_memory();
    assert!(state.memory_open());
    state.toggle_reasoning();
    assert!(!state.reasoning_open());
}

// ---- i18n --------------------------------------------------------------------

#[test]
fn the_bubble_keys_exist_and_resolve_to_chinese() {
    for key in [
        "chat.toolRole",
        "chat.collapsed",
        "chat.expanded",
        "chat.reasoning",
        "chat.memory.used",
        "chat.rememberThis",
        "chat.thinking",
        "chat.cancelled",
    ] {
        assert!(has(key), "{key} must exist in the zh-CN table");
    }
    assert_eq!(t("chat.toolRole"), "工具输出");
    assert_eq!(t("chat.collapsed"), "已折叠");
    assert_eq!(t("chat.expanded"), "已展开");
    assert_eq!(t("chat.reasoning"), "思考过程");
    assert_eq!(t("chat.memory.used"), "已使用 {count} 条记忆");
    assert_eq!(t("chat.rememberThis"), "记住这条");
    assert_eq!(t("chat.thinking"), "思考中…");
    assert_eq!(t("chat.cancelled"), "已停止生成。");
}

// ---- ICON_BOOK_OPEN ----------------------------------------------------------

#[test]
fn audit_icon_book_open_is_the_lucide_book_open_glyph() {
    assert_eq!(ICON_BOOK_OPEN.name, "book-open");
    assert_eq!(ICON_BOOK_OPEN.view, 24.0);
    assert!(
        ICON_BOOK_OPEN.body.contains("M12 7v14"),
        "book-open spine: {}",
        ICON_BOOK_OPEN.body
    );
    assert!(
        ICON_BOOK_OPEN.body.contains(
            "M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z"
        ),
        "book-open cover: {}",
        ICON_BOOK_OPEN.body
    );
}

// ---- tool role ---------------------------------------------------------------

#[test]
fn tool_bubble_starts_collapsed_and_expands_on_toggle() {
    let mut rt = runtime(
        probe(AiChatMessage::new(AiChatRole::Tool, "tool output here")),
        960.0,
        640.0,
    );
    assert!(has_key(&rt, "tool-role-toggle"));
    assert!(!has_key(&rt, "chat-bubble-tool-content"));
    let all = texts(&rt);
    assert!(
        all.iter().any(|text| text == "工具输出 已折叠"),
        "toggle shows chat.toolRole + chat.collapsed: {all:?}"
    );
    assert_eq!(
        element(&rt, "tool-role-toggle").expanded,
        Some(false),
        "collapsed is observable as a11y expanded=false"
    );
    assert_eq!(
        a11y_label(&rt, "tool-role-toggle").as_deref(),
        Some("工具输出 已折叠"),
        "the accessible name is the full toggle label"
    );
    click(&mut rt, "tool-role-toggle");
    assert!(has_key(&rt, "chat-bubble-tool-content"));
    assert!(
        texts(&rt).iter().any(|text| text == "工具输出 已展开"),
        "toggle shows chat.expanded once open"
    );
    assert_eq!(
        element(&rt, "tool-role-toggle").expanded,
        Some(true),
        "open is observable as a11y expanded=true"
    );
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("tool output here")),
        "the expanded tool box shows the content"
    );
    click(&mut rt, "tool-role-toggle");
    assert!(!has_key(&rt, "chat-bubble-tool-content"));
    assert_eq!(
        element(&rt, "tool-role-toggle").expanded,
        Some(false),
        "a second click collapses it again"
    );
}

#[test]
fn tool_toggle_activates_on_enter() {
    let mut rt = runtime(
        probe(AiChatMessage::new(AiChatRole::Tool, "x")),
        960.0,
        640.0,
    );
    press(&mut rt, Key::Tab);
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("tool-role-toggle"),
        "Tab reaches the tool toggle"
    );
    press(&mut rt, Key::Enter);
    assert!(
        has_key(&rt, "chat-bubble-tool-content"),
        "Enter expands the tool output exactly once"
    );
}

// ---- content placeholder -----------------------------------------------------

#[test]
fn assistant_content_renders_as_the_plain_text_placeholder() {
    let mut rt = runtime(probe(assistant("SELECT * FROM users;")), 960.0, 640.0);
    assert!(
        has_key(&rt, "chat-bubble-content-placeholder"),
        "assistant content is the AiMessageContent placeholder until T169"
    );
    assert!(has_key(&rt, "chat-bubble-content"));
    assert!(
        texts(&rt).iter().any(|text| text == "SELECT * FROM users;"),
        "the placeholder shows the raw content verbatim"
    );
    // No SQL rendering is faked: the placeholder carries no code-block key.
    assert!(
        !rt.elements().iter().any(|e| e
            .key
            .as_deref()
            .is_some_and(|key| key.contains("code") || key.contains("sql-block"))),
        "nothing here pretends to be AiCodeBlock"
    );
}

#[test]
fn user_content_is_plain_text_with_no_placeholder_wrapper() {
    let mut rt = runtime(probe(user("hello there")), 960.0, 640.0);
    assert!(has_key(&rt, "chat-bubble-content"));
    assert!(
        !has_key(&rt, "chat-bubble-content-placeholder"),
        "user content is final in React (a plain <pre>), not a placeholder"
    );
}

#[test]
fn whitespace_only_content_still_renders() {
    // React gates on `message.content` truthiness: a whitespace-only string
    // is truthy, so it renders. Rune mirrors that with `is_empty()` only —
    // no `trim()` — so the S1-noted trim divergence (Rust keeps U+FEFF, JS
    // strips it) cannot change this gate.
    let mut rt = runtime(probe(assistant("   ")), 960.0, 640.0);
    assert!(has_key(&rt, "chat-bubble-content"));
    let feff = runtime(probe(assistant("\u{feff}")), 960.0, 640.0);
    assert!(
        has_key(&feff, "chat-bubble-content"),
        "a lone U+FEFF is non-empty content in both runtimes"
    );
}

// ---- alignment -----------------------------------------------------------------

#[test]
fn user_bubble_sits_on_the_right_and_assistant_on_the_left() {
    let user_rt = runtime(probe(user("hi")), 960.0, 640.0);
    let user_body = element(&user_rt, "chat-bubble-body");
    // Probe page: 16pt padding, so the content box is 16..944.
    assert!(
        (user_body.rect[0] + user_body.rect[2] - 944.0).abs() <= 1.0,
        "user bubble hugs the right edge: {:?}",
        user_body.rect
    );
    assert!(
        user_body.rect[0] > 480.0,
        "a short user bubble does not stretch to 90%: {:?}",
        user_body.rect
    );

    let asst_rt = runtime(probe(assistant("hello")), 960.0, 640.0);
    let asst_body = element(&asst_rt, "chat-bubble-body");
    assert!(
        (asst_body.rect[0] - 16.0).abs() <= 1.0,
        "assistant bubble hugs the left edge: {:?}",
        asst_body.rect
    );
    assert_ne!(
        user_body.fill, asst_body.fill,
        "user (accent wash) and assistant (surface-alt) paint differently"
    );
}

#[test]
fn long_content_wraps_inside_ninety_percent() {
    let long = "Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua enim ad minim veniam quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat duis aute irure dolor in reprehenderit.";
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = runtime(probe(assistant(long)), w, h);
        assert_in_window(&rt, w, h, "long assistant content");
        let body = element(&rt, "chat-bubble-body");
        // Content box is the window minus 16pt probe padding on each side;
        // React caps the bubble at 90% of it.
        let cap = 0.9 * (w - 32.0) + 1.0;
        assert!(
            body.rect[2] <= cap,
            "bubble width {} exceeds the 90% cap {cap} at {w}x{h}: {:?}",
            body.rect[2],
            body.rect
        );
    }
}

// ---- reasoning -----------------------------------------------------------------

#[test]
fn reasoning_starts_closed_and_opens_on_toggle() {
    let mut rt = runtime(
        probe(assistant("answer").reasoning("first I checked the schema")),
        960.0,
        640.0,
    );
    assert!(has_key(&rt, "chat-bubble-reasoning"));
    assert!(!has_key(&rt, "chat-bubble-reasoning-text"));
    assert_eq!(
        element(&rt, "chat-bubble-reasoning").expanded,
        Some(false),
        "the reasoning toggle reports expanded=false while closed"
    );
    assert!(
        texts(&rt).iter().any(|text| text == "思考过程"),
        "the toggle carries chat.reasoning"
    );
    click(&mut rt, "chat-bubble-reasoning");
    assert!(has_key(&rt, "chat-bubble-reasoning-text"));
    assert_eq!(
        element(&rt, "chat-bubble-reasoning").expanded,
        Some(true),
        "the reasoning toggle reports expanded=true once open"
    );
    assert!(
        texts(&rt)
            .iter()
            .any(|text| text == "first I checked the schema"),
        "the open section shows the reasoning text"
    );
    click(&mut rt, "chat-bubble-reasoning");
    assert!(!has_key(&rt, "chat-bubble-reasoning-text"));
}

#[test]
fn no_reasoning_means_no_reasoning_section() {
    let mut rt = runtime(probe(assistant("answer")), 960.0, 640.0);
    assert!(!has_key(&rt, "chat-bubble-reasoning"));
    let empty = runtime(probe(assistant("answer").reasoning("")), 960.0, 640.0);
    assert!(
        !has_key(&empty, "chat-bubble-reasoning"),
        "an empty reasoning string is falsy in React too"
    );
}

// ---- streaming -----------------------------------------------------------------

#[test]
fn streaming_without_content_shows_the_thinking_spinner() {
    let mut p = probe(assistant("").reasoning("working on it"));
    p.streaming = true;
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "chat-bubble-thinking"));
    assert!(has_key(&rt, "chat-bubble-thinking-label"));
    assert!(
        texts(&rt).iter().any(|text| text == "思考中…"),
        "the spinner carries chat.thinking"
    );
    assert!(
        !has_key(&rt, "chat-bubble-content"),
        "no content yet, so no content node"
    );
    // Once content arrives the spinner goes away.
    let mut with_content = probe(assistant("partial").reasoning("working on it"));
    with_content.streaming = true;
    let mut rt2 = runtime(with_content, 960.0, 640.0);
    assert!(
        !has_key(&mut rt2, "chat-bubble-thinking"),
        "content arrived — no thinking placeholder"
    );
}

#[test]
fn pulsing_bubble_loops_opacity_while_a_settled_one_does_not() {
    let mut p = probe(assistant("").reasoning("working on it"));
    p.streaming = true;
    let mut rt = runtime(p, 960.0, 640.0);
    let body = element(&rt, "chat-bubble-body");
    assert!(
        body.loop_phase.is_some(),
        "the streaming bubble carries a looping opacity track: {body:?}"
    );
    // Sample the reported opacity over a full 2s pulse period: it must
    // actually move (down toward 0.5 and back), not sit at 1.0.
    let mut seen_low = false;
    let mut seen_high = false;
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
        let opacity = element(&rt, "chat-bubble-body").opacity;
        if opacity < 0.9 {
            seen_low = true;
        }
        if opacity > 0.9 {
            seen_high = true;
        }
    }
    assert!(
        seen_low && seen_high,
        "the pulse never moved: opacity stayed at one end for a full period"
    );

    // Control: the same message, settled, has no loop and stays opaque.
    let mut settled = runtime(
        probe(assistant("done").reasoning("working on it")),
        960.0,
        640.0,
    );
    let settled_body = element(&settled, "chat-bubble-body");
    assert_eq!(
        settled_body.loop_phase, None,
        "a settled bubble carries no loop"
    );
    assert_eq!(settled_body.opacity, 1.0);
    settled.frame(1.0);
    assert_eq!(
        element(&settled, "chat-bubble-body").opacity,
        1.0,
        "a settled bubble stays opaque over time"
    );
}

// ---- memory --------------------------------------------------------------------

fn memory_message() -> AiChatMessage {
    assistant("The answer uses [M1] and [M2].").memory_used(vec![
        memory("M1", "prefers snake_case"),
        memory("M2", "orders table is huge"),
    ])
}

#[test]
fn memory_chip_shows_the_count_and_expands_the_list() {
    let mut rt = runtime(probe(memory_message()), 960.0, 640.0);
    assert!(has_key(&rt, "ai-memory-chip"));
    assert!(!has_key(&rt, "ai-memory-expanded"));
    assert!(
        texts(&rt).iter().any(|text| text == "已使用 2 条记忆"),
        "the chip formats chat.memory.used with the count"
    );
    assert_eq!(
        element(&rt, "ai-memory-chip").expanded,
        Some(false),
        "the chip reports expanded=false while closed"
    );
    click(&mut rt, "ai-memory-chip");
    assert!(has_key(&rt, "ai-memory-expanded"));
    assert_eq!(
        element(&rt, "ai-memory-chip").expanded,
        Some(true),
        "the chip reports expanded=true once open"
    );
    for key in ["ai-memory-item-M1", "ai-memory-item-M2"] {
        assert!(has_key(&rt, key), "{key} is listed");
    }
    let all = texts(&rt);
    for fragment in [
        "[M1]",
        "prefers snake_case",
        "preference · user",
        "[M2]",
        "orders table is huge",
    ] {
        assert!(
            all.iter().any(|text| text == fragment),
            "memory item shows {fragment:?}: {all:?}"
        );
    }
    click(&mut rt, "ai-memory-chip");
    assert!(!has_key(&rt, "ai-memory-expanded"));
}

#[test]
fn memory_section_is_hidden_for_user_messages_and_empty_lists() {
    let user_rt = runtime(
        probe(user("hi").memory_used(vec![memory("M1", "x")])),
        960.0,
        640.0,
    );
    assert!(
        !has_key(&user_rt, "ai-memory-chip"),
        "React gates the chip on !isUser"
    );
    let empty = runtime(probe(assistant("answer")), 960.0, 640.0);
    assert!(!has_key(&empty, "ai-memory-chip"), "no memories, no chip");
}

#[test]
fn citation_jump_expands_the_list_and_selects_the_item() {
    // The placeholder content has no clickable citation link yet, so the
    // jump is driven the way the future AiMessageContent wiring will drive
    // it: straight into the state machine, then the render is observed.
    // The poke bypasses the runtime's event handlers, so `settle_all` is
    // needed to mark the state dirty — `frame` only re-runs the view when
    // state changed, and a direct mutation does not set that flag.
    let mut rt = runtime(probe(memory_message()), 960.0, 640.0);
    assert!(!has_key(&rt, "ai-memory-expanded"));
    rt.state.bubble.open_memory_citation("M2");
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        has_key(&rt, "ai-memory-expanded"),
        "the jump expands the list"
    );
    let focused = element(&rt, "ai-memory-item-M2");
    assert!(focused.item_selected, "the jumped-to item reports selected");
    assert_ne!(
        focused.fill,
        rune::Color::TRANSPARENT,
        "the jumped-to item paints its accent wash"
    );
    let other = element(&rt, "ai-memory-item-M1");
    assert!(!other.item_selected, "the sibling stays unselected");
    assert_eq!(other.fill, rune::Color::TRANSPARENT);
}

// ---- remember ------------------------------------------------------------------

#[test]
fn remember_button_only_shows_when_every_gate_holds() {
    // Full gates: shown, and the click reports the message.
    let mut p = probe(assistant("remember me"));
    p.last_assistant = true;
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-memory-remember"));
    assert!(
        texts(&rt).iter().any(|text| text == "记住这条"),
        "the button carries chat.rememberThis"
    );
    click(&mut rt, "ai-memory-remember");
    assert_eq!(rt.state.remembered, vec!["remember me".to_string()]);

    // One missing conjunct each: hidden.
    let mut not_last = probe(assistant("remember me"));
    not_last.last_assistant = false;
    assert!(!has_key(
        &mut runtime(not_last, 960.0, 640.0),
        "ai-memory-remember"
    ));

    let mut streaming = probe(assistant("remember me"));
    streaming.last_assistant = true;
    streaming.streaming = true;
    assert!(!has_key(
        &mut runtime(streaming, 960.0, 640.0),
        "ai-memory-remember"
    ));

    let mut empty = probe(assistant(""));
    empty.last_assistant = true;
    assert!(!has_key(
        &mut runtime(empty, 960.0, 640.0),
        "ai-memory-remember"
    ));

    let mut unwired = probe(assistant("remember me"));
    unwired.last_assistant = true;
    unwired.wire_remember = false;
    assert!(!has_key(
        &mut runtime(unwired, 960.0, 640.0),
        "ai-memory-remember"
    ));

    let mut user_message = probe(user("remember me"));
    user_message.last_assistant = true;
    assert!(!has_key(
        &mut runtime(user_message, 960.0, 640.0),
        "ai-memory-remember"
    ));
}

#[test]
fn remember_button_activates_on_enter() {
    let mut p = probe(assistant("remember me"));
    p.last_assistant = true;
    let mut rt = runtime(p, 960.0, 640.0);
    for _ in 0..8 {
        if focused_key(&rt).as_deref() == Some("ai-memory-remember") {
            break;
        }
        press(&mut rt, Key::Tab);
    }
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("ai-memory-remember"),
        "Tab reaches the remember button"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.remembered, vec!["remember me".to_string()]);
}

// ---- cancelled -----------------------------------------------------------------

#[test]
fn cancelled_marker_shows_when_the_run_was_cancelled() {
    let mut rt = runtime(probe(assistant("partial").cancelled(true)), 960.0, 640.0);
    assert!(has_key(&rt, "chat-message-cancelled"));
    assert!(
        texts(&rt).iter().any(|text| text == "已停止生成。"),
        "the marker carries chat.cancelled"
    );
    let not_cancelled = runtime(probe(assistant("done")), 960.0, 640.0);
    assert!(
        !has_key(&not_cancelled, "chat-message-cancelled"),
        "no marker without cancellation"
    );
}

// ---- questions (S1 reuse) ------------------------------------------------------

#[test]
fn questions_block_renders_under_the_last_assistant_bubble_and_forwards_answers() {
    let mut p = probe(assistant("Which one?").questions(sample_questions()));
    p.last_assistant = true;
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "question-block"));
    assert!(has_key(&rt, "question-option-q1-o1"));
    click(&mut rt, "question-option-q1-o2");
    click(&mut rt, "question-submit");
    assert_eq!(
        rt.state.answered,
        vec!["[q1] Which database?\nMySQL".to_string()],
        "submit forwards the formatted answers to onAnswerQuestions"
    );

    // Streaming hides the block even for the last assistant message.
    let mut streaming = probe(assistant("Which one?").questions(sample_questions()));
    streaming.last_assistant = true;
    streaming.streaming = true;
    assert!(!has_key(
        &mut runtime(streaming, 960.0, 640.0),
        "question-block"
    ));

    // Not the last assistant message: hidden.
    let mut not_last = probe(assistant("Which one?").questions(sample_questions()));
    not_last.last_assistant = false;
    assert!(!has_key(
        &mut runtime(not_last, 960.0, 640.0),
        "question-block"
    ));
}

#[test]
fn typing_then_picking_an_option_replaces_the_multi_draft() {
    // S1's divergence, inherited through the reuse: React would spread the
    // typed string into a character array (or throw); Rune replaces the
    // draft with the single selection.
    let questions = vec![AiQuestion::new("q1", "What do you need?")
        .allow_multiple(true)
        .option("a", "Migrate")
        .option("b", "Inspect")];
    let mut p = probe(assistant("Which one?").questions(questions));
    p.last_assistant = true;
    let mut rt = runtime(p, 960.0, 640.0);
    click(&mut rt, "question-custom-input-q1");
    rt.paste("something else");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "question-option-q1-a");
    assert_eq!(
        rt.state.block.custom_text("q1"),
        "",
        "pressing an option clears the draft, as in S1"
    );
    click(&mut rt, "question-submit");
    assert_eq!(
        rt.state.answered,
        vec!["[q1] What do you need?\nMigrate".to_string()],
        "the option replaces the typed text instead of spreading it"
    );
}

// ---- geometry ------------------------------------------------------------------

fn rich_message() -> AiChatMessage {
    assistant(
        "The orders table has 12,400 rows. The slow query scans all of them because the created_at index is missing; add it and the planner switches to an index scan.",
    )
    .reasoning("First I checked the schema, then the query plan.")
    .questions(sample_questions())
    .memory_used(vec![
        memory("M1", "prefers snake_case"),
        memory("M2", "orders table is huge"),
    ])
}

fn rich_probe(dark: bool) -> Probe {
    let mut p = probe(rich_message());
    p.last_assistant = true;
    p.dark = dark;
    p.bubble.toggle_reasoning();
    p.bubble.toggle_memory();
    p
}

#[test]
fn bubble_stays_inside_both_window_sizes_in_both_themes_without_sibling_overlap() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let rt = runtime(rich_probe(dark), w, h);
            let theme = if dark { "dark" } else { "light" };
            assert_in_window(&rt, w, h, &format!("rich bubble {w}x{h} {theme}"));
            assert_no_sibling_overlap(&rt, &format!("rich bubble {w}x{h} {theme}"));
            // The expanded memory list and the questions block are both laid out.
            assert!(has_key(&rt, "ai-memory-expanded"));
            assert!(has_key(&rt, "question-block"));
            assert!(has_key(&rt, "ai-memory-remember"));
        }
    }
}

#[test]
fn tool_and_user_bubbles_stay_inside_the_minimum_window() {
    for dark in [true, false] {
        let mut tool = probe(AiChatMessage::new(AiChatRole::Tool, "tool output here"));
        tool.dark = dark;
        tool.bubble.toggle_tool();
        let rt = runtime(tool, 960.0, 640.0);
        assert_in_window(&rt, 960.0, 640.0, "expanded tool bubble");
        assert_no_sibling_overlap(&rt, "expanded tool bubble");

        let mut user_p = probe(user("SELECT 1"));
        user_p.dark = dark;
        let user_rt = runtime(user_p, 960.0, 640.0);
        assert_in_window(&user_rt, 960.0, 640.0, "user bubble");
        assert_no_sibling_overlap(&user_rt, "user bubble");
    }
}

#[test]
fn multi_line_content_grows_the_bubble() {
    let single = runtime(probe(assistant("one line")), 960.0, 640.0);
    let single_h = element(&single, "chat-bubble-body").rect[3];
    let multi = runtime(
        probe(assistant("line one\nline two\nline three")),
        960.0,
        640.0,
    );
    let multi_h = element(&multi, "chat-bubble-body").rect[3];
    assert!(
        multi_h > single_h + 10.0,
        "three lines ({multi_h}pt) must be taller than one ({single_h}pt)"
    );
}
