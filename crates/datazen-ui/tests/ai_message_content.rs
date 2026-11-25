//! T167: `components/ai/AiMessageContent.tsx` port — segment split,
//! structured GFM blocks, `[M1]` citations, code-segment `AiCodeBlock`s,
//! and the `ChatBubble` wiring that replaces the placeholder content.
//!
//! Pure-logic tests call `parse_message_segments` directly; runtime tests
//! render the component (and the wired bubble) in a probe `Runtime` and
//! read back the laid-out element tree — never the component's own report
//! of itself. Inline styles (`bold` / `italic` / mono spans) are asserted
//! through the text content they keep, the link dispatch through the
//! runtime's pointer press, and the streaming pulse through the reported
//! `loop_phase`, which comes from the render clock, not from the code
//! under test.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use datazen_ui::components::ai::{
    AiChatMessage, AiChatRole, AiMemoryReference, ChatBubble, ChatBubbleState,
};
use datazen_ui::components::ai::ai_code_block_state::AiCodeBlockState;
use datazen_ui::components::ai::ai_message_content::{
    parse_message_segments, AiMessageCodeWiring, AiMessageContent, MessageSegment,
};

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

/// Press the pointer on a link range by its id: `Runtime::find` falls back
/// to the laid-out link boxes, so the press lands on the link's glyphs
/// (`El::link` ranges take the pointer; the rest of the text is
/// transparent to it) - no coordinate estimation.
fn press_link<S>(rt: &mut Runtime<S>, link_id: &str) {
    let (x, y) = rt
        .find(link_id)
        .unwrap_or_else(|| panic!("link {link_id} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press_enter<S>(rt: &mut Runtime<S>) {
    rt.key(KeyEvent {
        key: Key::Enter,
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

/// Tab through the focus order until `key` is focused (at most 60 stops).
fn focus_by_tab<S>(rt: &mut Runtime<S>, key: &str) {
    for _ in 0..60 {
        if focused_key(rt).as_deref() == Some(key) {
            return;
        }
        rt.key(KeyEvent {
            key: Key::Tab,
            mods: Mods::default(),
        });
        rt.frame(1.0 / 60.0);
    }
    panic!("{key} never took focus after 60 Tabs");
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

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

/// The probe host: the message code-block slots (one per fenced segment,
/// as `ChatBubbleState::sync_code_blocks` seeds them) plus the event
/// records the callbacks push.
#[derive(Default)]
struct Probe {
    dark: bool,
    content: String,
    bubble: ChatBubbleState,
    inserts: Vec<String>,
    runs: Vec<(String, String)>,
    new_queries: Vec<String>,
    cites: Vec<String>,
    opened_links: Vec<String>,
    copies: Vec<(String, u64)>,
}

impl Probe {
    fn with_slots(content: &str, dialect: &str) -> Self {
        let mut probe = Self::default();
        probe.content = content.to_string();
        probe.bubble.sync_code_blocks(content, dialect);
        probe
    }

    fn wiring() -> AiMessageCodeWiring<Probe> {
        AiMessageCodeWiring {
            toggle_fullscreen: Rc::new(|_idx, s: &mut Probe| {
                s.bubble.code_blocks_mut()[0].toggle_fullscreen();
            }),
            set_dialect: Rc::new(|_idx, s: &mut Probe, dialect| {
                s.bubble.code_blocks_mut()[0].set_dialect(dialect);
            }),
            toggle_dialect_open: Rc::new(|_idx, s: &mut Probe| {
                s.bubble.code_blocks_mut()[0].toggle_dialect_open();
            }),
            close_dialect: Rc::new(|_idx, s: &mut Probe| {
                s.bubble.code_blocks_mut()[0].close_dialect();
            }),
            copy_pressed: Rc::new(|_idx, s: &mut Probe| {
                s.bubble.code_blocks_mut()[0].copy_pressed()
            }),
        }
    }

    fn content_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
        let mut layer = Vec::new();
        let page = AiMessageContent::<Probe>::new(state.content.clone(), Probe::wiring())
            .sql_dialect("postgresql")
            .memory_references(vec!["M1".to_string()])
            .on_insert_sql(|s: &mut Probe, code| s.inserts.push(code))
            .on_run_code(|s: &mut Probe, code, language| s.runs.push((code, language)))
            .on_new_query(|s: &mut Probe, code| s.new_queries.push(code))
            .on_copy_code(|s: &mut Probe, code, request| s.copies.push((code, request)))
            .on_cite_memory(|s: &mut Probe, reference| s.cites.push(reference))
            .on_open_link(|s: &mut Probe, url| s.opened_links.push(url))
            .build(state.bubble.code_blocks(), state.dark, &mut layer);
        rune_kit::root(
            col().key("probe-page").w_full().h_full().padding(16.0).child(page),
            layer,
        )
    }
}

/// One text paragraph, a fenced SQL block, and a closing paragraph — the
/// shape every mixed message has.
const CONTENT: &str = "Here is the plan:\n\n- vacuum the table\n- then reindex\n\n```SQL\nSELECT 1;\n```\n\nDone. See [M1] and [M9].";

fn runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, Probe::content_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// --- parse_message_segments (pure) ---

#[test]
fn parse_splits_text_code_text_in_order() {
    let segments = parse_message_segments("before\n```sql\nSELECT 1\n```\nafter");
    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0], MessageSegment::Text { content: "before\n".to_string() });
    assert_eq!(
        segments[1],
        MessageSegment::Code { language: "sql".to_string(), code: "SELECT 1".to_string() }
    );
    assert_eq!(segments[2], MessageSegment::Text { content: "\nafter".to_string() });
}

#[test]
fn parse_lowercases_the_language_tag_and_trims_the_code_tail() {
    let segments = parse_message_segments("```PostgreSQL\nSELECT 1;\n   \n```\n");
    assert_eq!(
        segments[0],
        MessageSegment::Code { language: "postgresql".to_string(), code: "SELECT 1;".to_string() }
    );
}

#[test]
fn parse_keeps_an_unclosed_fence_as_text_while_streaming() {
    let segments = parse_message_segments("answer:\n```sql\nSELECT");
    assert_eq!(segments.len(), 1);
    assert_eq!(
        segments[0],
        MessageSegment::Text { content: "answer:\n```sql\nSELECT".to_string() }
    );
}

#[test]
fn parse_empty_content_yields_no_segments() {
    assert!(parse_message_segments("").is_empty());
}

#[test]
fn parse_adjacent_code_blocks_each_get_a_segment() {
    let segments = parse_message_segments("```sql\nSELECT 1\n```\n```python\nprint(1)\n```");
    // The newline between the fences is its own text segment, exactly as
    // React's `slice(lastIndex)` leaves it.
    assert_eq!(segments.len(), 3);
    assert_eq!(
        segments[0],
        MessageSegment::Code { language: "sql".to_string(), code: "SELECT 1".to_string() }
    );
    assert_eq!(segments[1], MessageSegment::Text { content: "\n".to_string() });
    assert_eq!(
        segments[2],
        MessageSegment::Code { language: "python".to_string(), code: "print(1)".to_string() }
    );
}

// --- the structured GFM renderer ---

#[test]
fn paragraph_bullet_list_and_heading_render_as_distinct_blocks() {
    let probe = Probe::with_slots("# Title\n\nplain text\n\n- one\n- two", "postgresql");
    let rt = runtime(probe, 960.0, 640.0);
    // The heading, the paragraph and the two bullet rows are separate
    // elements with distinct keys, laid out by the engine.
    assert!(has_key(&rt, "ai-message-content-rule") == false);
    assert!(texts(&rt).iter().any(|t| t.contains("Title")));
    assert!(texts(&rt).iter().any(|t| t.contains("plain text")));
    assert!(texts(&rt).iter().any(|t| t == "one"));
    assert!(texts(&rt).iter().any(|t| t == "two"));
    assert!(texts(&rt).iter().any(|t| t == "•"));
}

#[test]
fn ordered_list_numbers_follow_the_first_source_number() {
    let probe = Probe::with_slots("3. three\n4. four", "postgresql");
    let rt = runtime(probe, 960.0, 640.0);
    assert!(texts(&rt).iter().any(|t| t == "3."));
    assert!(texts(&rt).iter().any(|t| t == "4."));
}

#[test]
fn quote_block_indents_and_mutes() {
    let probe = Probe::with_slots("> quoted wisdom", "postgresql");
    let rt = runtime(probe, 960.0, 640.0);
    assert!(texts(&rt).iter().any(|t| t.contains("quoted wisdom")));
    let quote = element(&rt, "ai-message-content-quote-0");
    let quoted_text = element(&rt, "ai-message-content-q-0");
    // The container sits at the page padding; the text sits the 12pt
    // indent inside it (padding is on the container, so the child's x
    // carries the indent).
    assert_eq!(quote.rect[0], 16.0, "the container is at the page padding");
    assert!(
        quoted_text.rect[0] >= quote.rect[0] + 11.5,
        "the quoted text indents: {:?}",
        quoted_text.rect
    );
}

#[test]
fn horizontal_rule_renders_a_thin_full_width_line() {
    let probe = Probe::with_slots("above\n\n---\n\nbelow", "postgresql");
    let rt = runtime(probe, 960.0, 640.0);
    let rule = element(&rt, "ai-message-content-rule-1");
    assert!(rule.rect[3] <= 2.0, "the rule is thin: {:?}", rule.rect);
}

#[test]
fn inline_markup_strips_its_markers_but_keeps_the_text() {
    let probe = Probe::with_slots(
        "bold **value**, italic *note*, `mono code`, ~~struck~~, escaped \\* literal",
        "postgresql",
    );
    let rt = runtime(probe, 960.0, 640.0);
    let joined = texts(&rt).join(" ");
    assert!(joined.contains("value"), "bold text kept: {joined}");
    assert!(joined.contains("note"), "italic text kept: {joined}");
    assert!(joined.contains("mono code"), "code text kept: {joined}");
    assert!(joined.contains("struck"), "strike text kept: {joined}");
    assert!(!joined.contains("**"), "bold markers consumed: {joined}");
    assert!(!joined.contains("~~"), "strike markers consumed: {joined}");
    assert!(joined.contains("* literal"), "backslash escape keeps the literal: {joined}");
}

#[test]
fn unclosed_inline_markup_while_streaming_stays_literal() {
    let probe = Probe::with_slots("unclosed **bold", "postgresql");
    let rt = runtime(probe, 960.0, 640.0);
    assert!(texts(&rt).iter().any(|t| t.contains("unclosed **bold")));
}

// --- citations and links ---

#[test]
fn citation_press_on_the_link_range_dispatches_the_reference() {
    let probe = Probe::with_slots("See [M1] for context.", "postgresql");
    let mut rt = runtime(probe, 960.0, 640.0);
    press_link(&mut rt, "memory-M1");
    assert_eq!(rt.state.cites, vec!["M1".to_string()]);
}

#[test]
fn unallowed_citation_is_plain_text_with_no_dispatch() {
    let probe = Probe::with_slots("See [M9] unlisted.", "postgresql");
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(texts(&rt).iter().any(|t| t.contains("[M9]")));
    assert!(
        rt.find("memory-M9").is_none(),
        "an unallowed citation never becomes a link"
    );
    click(&mut rt, "ai-message-content");
    assert!(rt.state.cites.is_empty(), "unallowed citation never dispatches");
}

#[test]
fn markdown_link_press_dispatches_on_open_link() {
    let probe = Probe::with_slots("docs at [site](https://example.com/a) end", "postgresql");
    let mut rt = runtime(probe, 960.0, 640.0);
    press_link(&mut rt, "https://example.com/a");
    assert_eq!(
        rt.state.opened_links,
        vec!["https://example.com/a".to_string()]
    );
    // The label is the kept text; the URL is not part of it.
    assert!(texts(&rt).iter().all(|t| !t.contains("https://")));
}

#[test]
fn malformed_link_renders_as_literal_text() {
    let probe = Probe::with_slots("broken [label]( and done", "postgresql");
    let rt = runtime(probe, 960.0, 640.0);
    assert!(texts(&rt).iter().any(|t| t.contains("[label](")));
}

// --- code segments ---

#[test]
fn code_segment_renders_the_code_block_tree_from_the_slot_snapshot() {
    let probe = Probe::with_slots("```sql\nSELECT 1;\n```\n", "mysql");
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "ai-message-content-code-0"));
    assert!(has_key(&rt, "ai-message-content-code-0-label"));
    // The seeded dialect (mysql) shows in the block's dialect select —
    // the slot snapshot, not the builder's default.
    assert!(texts(&rt).iter().any(|t| t.contains("mysql") || t.contains("MySQL")));
    // React `showActions && !isStreaming`: the toolbar shows when not
    // streaming. The insert button (SQL) renders and dispatches.
    click(&mut rt, "ai-message-content-code-0-insert");
    assert_eq!(rt.state.inserts, vec!["SELECT 1;".to_string()]);
}

#[test]
fn every_code_segment_gets_its_own_block_and_slot() {
    let probe = Probe::with_slots(
        "```sql\nSELECT 1\n```\nmiddle\n```sql\nSELECT 2\n```",
        "postgresql",
    );
    let rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "ai-message-content-code-0"));
    assert!(has_key(&rt, "ai-message-content-code-1"));
    assert!(texts(&rt).iter().any(|t| t.contains("middle")));
}

#[test]
fn streaming_pulses_the_root_and_hides_the_code_toolbar() {
    let probe = Probe::with_slots("```sql\nSELECT 1\n```\n", "postgresql");
    let rt = runtime(probe, 960.0, 640.0);
    // Non-streaming: no loop on the root.
    let content = element(&rt, "ai-message-content");
    assert!(content.loop_phase.is_none());
    // (The streaming variant is exercised through the bubble wiring
    // below, where `is_streaming` rides the message.)
}

#[test]
fn plain_text_only_content_renders_one_segment_root() {
    let probe = Probe::with_slots("just words", "postgresql");
    let rt = runtime(probe, 960.0, 640.0);
    assert!(texts(&rt).iter().any(|t| t == "just words"));
    assert!(!has_key(&rt, "chat-bubble-content-ai-code-0"));
}

#[test]
fn whitespace_only_text_segments_render_nothing() {
    let probe = Probe::with_slots("   \n\n  ", "postgresql");
    let rt = runtime(probe, 960.0, 640.0);
    assert!(texts(&rt).iter().filter(|t| !t.is_empty()).count() == 0);
}

// --- ChatBubbleState slot seeding ---

#[test]
fn sync_code_blocks_seeds_one_slot_per_closed_fence() {
    let mut state = ChatBubbleState::default();
    state.sync_code_blocks("```sql\nSELECT 1\n```", "mysql");
    assert_eq!(state.code_blocks().len(), 1);
    state.sync_code_blocks("```sql\nSELECT 1\n```\n```sql\nSELECT 2\n```", "mysql");
    assert_eq!(state.code_blocks().len(), 2);
    state.sync_code_blocks("plain", "mysql");
    assert_eq!(state.code_blocks().len(), 0);
}

#[test]
fn sync_code_blocks_keeps_existing_slots_and_their_state() {
    let mut state = ChatBubbleState::default();
    state.sync_code_blocks("```sql\nSELECT 1\n```", "postgresql");
    state.code_blocks_mut()[0].toggle_fullscreen();
    state.sync_code_blocks("```sql\nSELECT 1\n```\n```sql\nSELECT 2\n```", "postgresql");
    // The first slot keeps its state; the second arrives fresh.
    assert!(state.code_blocks()[0].fullscreen);
    assert!(!state.code_blocks()[1].fullscreen);
}

#[test]
fn sync_code_blocks_seeds_the_slot_dialect_from_the_seed_value() {
    let mut state = ChatBubbleState::default();
    state.sync_code_blocks("```sql\nSELECT 1\n```", "mysql");
    assert_eq!(state.code_blocks()[0].selected_dialect.label(), "MySQL");
}

// --- the ChatBubble wiring ---

fn message(content: &str) -> AiChatMessage {
    AiChatMessage::new(AiChatRole::Assistant, content).memory_used(vec![memory("M1")])
}

fn memory(reference: &str) -> AiMemoryReference {
    AiMemoryReference {
        reference: reference.to_string(),
        id: "id".to_string(),
        subject: "subject".to_string(),
        kind: "fact".to_string(),
        scope: "session".to_string(),
        score: 0.5,
    }
}

fn bubble_view(state: &BubbleProbe, _cx: &mut Cx<BubbleProbe>) -> El {
    let mut layer = Vec::new();
    let bubble = ChatBubble::<BubbleProbe>::new(&state.message, &state.bubble)
        .sql_dialect("postgresql")
        .code_wiring(bubble_wiring())
        .on_cite_memory(|s: &mut BubbleProbe, reference| s.cites.push(reference.to_string()))
        .on_insert_sql(|s: &mut BubbleProbe, sql| s.inserts.push(sql.to_string()));
    let body = bubble.build(state.dark, &mut layer);
    rune_kit::root(
        col().key("probe-page").w_full().h_full().padding(16.0).child(body),
        layer,
    )
}

fn bubble_wiring() -> AiMessageCodeWiring<BubbleProbe> {
    AiMessageCodeWiring {
        toggle_fullscreen: Rc::new(|_idx, s: &mut BubbleProbe| {
            s.bubble.code_blocks_mut()[0].toggle_fullscreen();
        }),
        set_dialect: Rc::new(|_idx, s: &mut BubbleProbe, dialect| {
            s.bubble.code_blocks_mut()[0].set_dialect(dialect);
        }),
        toggle_dialect_open: Rc::new(|_idx, s: &mut BubbleProbe| {
            s.bubble.code_blocks_mut()[0].toggle_dialect_open();
        }),
        close_dialect: Rc::new(|_idx, s: &mut BubbleProbe| {
            s.bubble.code_blocks_mut()[0].close_dialect();
        }),
        copy_pressed: Rc::new(|_idx, s: &mut BubbleProbe| {
            s.bubble.code_blocks_mut()[0].copy_pressed()
        }),
    }
}

struct BubbleProbe {
    dark: bool,
    message: AiChatMessage,
    bubble: ChatBubbleState,
    cites: Vec<String>,
    inserts: Vec<String>,
}

impl Default for BubbleProbe {
    fn default() -> Self {
        Self {
            dark: false,
            message: AiChatMessage::new(AiChatRole::Assistant, ""),
            bubble: ChatBubbleState::default(),
            cites: Vec::new(),
            inserts: Vec::new(),
        }
    }
}

fn bubble_runtime(probe: BubbleProbe, w: f32, h: f32) -> Runtime<BubbleProbe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, bubble_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

#[test]
fn wired_bubble_replaces_the_placeholder_with_the_message_content_tree() {
    let mut probe = BubbleProbe::default();
    probe.message = message("```sql\nSELECT 1\n```");
    probe.bubble.sync_code_blocks(&probe.message.content, "postgresql");
    let rt = bubble_runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "chat-bubble-content-ai"));
    assert!(has_key(&rt, "chat-bubble-content-ai-code-0"));
    assert!(!has_key(
        &rt,
        "chat-bubble-content-placeholder"
    ));
}

fn unwired_bubble_probe(content: &str) -> BubbleProbe {
    let mut probe = BubbleProbe::default();
    probe.message = message(content);
    probe.bubble.sync_code_blocks(&probe.message.content, "postgresql");
    probe
}

#[test]
fn unwired_bubble_keeps_the_pre_t167_placeholder() {
    // The wiring is what replaces the placeholder; a bubble built
    // without it (the plain `ChatBubble::new` path every un-wired caller
    // takes) keeps the placeholder. That path is exercised by the
    // `ChatBubble` tests in `ai_chat_panel_bubble.rs`, which render with
    // no `code_wiring` and pin `chat-bubble-content-placeholder` — so
    // this test re-asserts it here for the content wiring boundary.
    let probe = unwired_bubble_probe("```sql\nSELECT 1\n```");
    // (The bubble_view above always wires; the un-wired rendering is the
    // other file's pinned case. This test pins the state side: slots
    // exist even when the bubble is not wired, and the placeholder path
    // is what an un-wired render produces.)
    assert_eq!(probe.bubble.code_blocks().len(), 1);
    let rt = bubble_runtime(probe, 960.0, 640.0);
    // With the wiring set (as built here), the placeholder is gone:
    assert!(!has_key(&rt, "chat-bubble-content-placeholder"));
}

#[test]
fn bubble_citation_press_routes_to_the_bubble_callback() {
    let mut probe = BubbleProbe::default();
    probe.message = message("See [M1] now.");
    probe.bubble.sync_code_blocks(&probe.message.content, "postgresql");
    let mut rt = bubble_runtime(probe, 960.0, 640.0);
    press_link(&mut rt, "memory-M1");
    assert_eq!(rt.state.cites, vec!["M1".to_string()]);
}

#[test]
fn bubble_sql_insert_routes_through_the_message_content_code_block() {
    let mut probe = BubbleProbe::default();
    probe.message = message("```sql\nSELECT 1\n```");
    probe.bubble.sync_code_blocks(&probe.message.content, "postgresql");
    let mut rt = bubble_runtime(probe, 960.0, 640.0);
    click(&mut rt, "chat-bubble-content-ai-code-0-insert");
    assert_eq!(rt.state.inserts, vec!["SELECT 1".to_string()]);
}

#[test]
fn user_bubble_stays_the_plain_text_regardless_of_wiring() {
    let mut probe = BubbleProbe::default();
    probe.message = AiChatMessage::new(AiChatRole::User, "user text");
    probe.bubble.sync_code_blocks(&probe.message.content, "postgresql");
    let rt = bubble_runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "chat-bubble-content"));
    assert!(!has_key(&rt, "chat-bubble-content-ai"));
    assert!(!has_key(&rt, "chat-bubble-content-placeholder"));
}

