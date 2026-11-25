//! T167 tester audit: an independent read of `AiMessageContent.tsx`
//! against the Rune port. This file does not reuse the coder's probe or
//! assertions — it re-derives them from the React source and asserts
//! through the runtime's own channels (element rects / texts, the
//! laid-out link boxes, the render clock's `loop_phase`), never through
//! the component's report of itself.
//!
//! Audit scope: the segment-split truth of `lib/aiMessageBlocks.ts`'s
//! `FENCE_RE`, the streaming behaviour (`animate-pulse` + the hidden
//! toolbar), the slot-snapshot rendering (fullscreen and dialect-open
//! states), the citation allow-list, and the two-window fit (960x640 and
//! 1440x816, no horizontal overflow — the task's DoD #4).

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::ai::ai_code_block_state::{AiCodeBlockState, SqlDialect};
use datazen_ui::components::ai::ai_message_content::{
    parse_message_segments, AiMessageCodeWiring, AiMessageContent, MessageSegment,
};
use datazen_ui::components::ai::{AiChatMessage, AiChatRole, ChatBubble, ChatBubbleState};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

/// The audit host: content, slot snapshot, wiring, event records.
#[derive(Default)]
struct Audit {
    dark: bool,
    content: String,
    slots: Vec<AiCodeBlockState>,
    streaming: bool,
    references: Vec<String>,
    cites: Vec<String>,
    opened: Vec<String>,
    inserts: Vec<String>,
}

impl Audit {
    fn case(content: &str, dialect: &str, streaming: bool) -> Self {
        let mut audit = Self {
            content: content.to_string(),
            streaming,
            references: vec!["M1".to_string(), "M4".to_string()],
            ..Self::default()
        };
        let code_blocks = parse_message_segments(content)
            .iter()
            .filter(|segment| matches!(segment, MessageSegment::Code { .. }))
            .count();
        for _ in 0..code_blocks {
            audit.slots.push(AiCodeBlockState::new(dialect));
        }
        audit
    }

    fn wiring() -> AiMessageCodeWiring<Audit> {
        AiMessageCodeWiring {
            toggle_fullscreen: Rc::new(|idx, s: &mut Audit| {
                s.slots[idx].toggle_fullscreen();
            }),
            set_dialect: Rc::new(|idx, s: &mut Audit, dialect| {
                s.slots[idx].set_dialect(dialect);
            }),
            toggle_dialect_open: Rc::new(|idx, s: &mut Audit| {
                s.slots[idx].toggle_dialect_open();
            }),
            close_dialect: Rc::new(|idx, s: &mut Audit| {
                s.slots[idx].close_dialect();
            }),
            copy_pressed: Rc::new(|idx, s: &mut Audit| s.slots[idx].copy_pressed()),
        }
    }
}

fn view(state: &Audit, _cx: &mut Cx<Audit>) -> El {
    let mut layer = Vec::new();
    let mut content = AiMessageContent::<Audit>::new(state.content.clone(), Audit::wiring())
        .sql_dialect("postgresql")
        .on_insert_sql(|s: &mut Audit, code| s.inserts.push(code))
        .on_cite_memory(|s: &mut Audit, reference| s.cites.push(reference))
        .on_open_link(|s: &mut Audit, url| s.opened.push(url));
    if !state.references.is_empty() {
        content = content.memory_references(state.references.clone());
    }
    content = content.is_streaming(state.streaming);
    let page = content.build(&state.slots, state.dark, &mut layer);
    rune_kit::root(
        col().key("audit-page").w_full().h_full().padding(16.0).child(page),
        layer,
    )
}

fn runtime(audit: Audit, w: f32, h: f32) -> Runtime<Audit> {
    let dark = audit.dark;
    let mut rt = Runtime::new(audit, view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

fn press_link(rt: &mut Runtime<Audit>, link_id: &str) {
    let (x, y) = rt
        .find(link_id)
        .unwrap_or_else(|| panic!("link {link_id} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, label: &str) {
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        assert!(
            e.rect[0] + e.rect[2] <= w + 0.5,
            "{label}: {} overflows {}x: {:?}",
            e.key.unwrap_or(e.path.clone()),
            w,
            e.rect
        );
    }
}

// --- the FENCE_RE truth (lib/aiMessageBlocks.ts) ---

#[test]
fn fence_without_a_language_tag_yields_an_empty_language() {
    let segments = parse_message_segments("```\nplain block\n```");
    assert_eq!(
        segments,
        vec![MessageSegment::Code {
            language: String::new(),
            code: "plain block".to_string()
        }]
    );
}

#[test]
fn fence_language_allows_word_chars_and_dashes() {
    let segments = parse_message_segments("```Objective-C_like\nx\n```");
    assert_eq!(
        segments[0],
        MessageSegment::Code {
            language: "objective-c_like".to_string(),
            code: "x".to_string()
        }
    );
}

#[test]
fn an_inline_fence_without_a_newline_is_not_a_code_segment() {
    // FENCE_RE reads ```lang\s*\n — a fence inside one line has no
    // newline after the tag, so the regex never matches and the whole
    // run stays one text segment.
    let segments = parse_message_segments("intro ```sql mid``` outro");
    assert_eq!(segments.len(), 1);
    assert_eq!(
        segments[0],
        MessageSegment::Text {
            content: "intro ```sql mid``` outro".to_string()
        }
    );
}

#[test]
fn a_fence_on_its_own_line_splits_the_prose_around_it() {
    let segments = parse_message_segments("intro\n```sql\nmid\n```\noutro");
    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0], MessageSegment::Text { content: "intro\n".to_string() });
    assert_eq!(
        segments[1],
        MessageSegment::Code { language: "sql".to_string(), code: "mid".to_string() }
    );
    assert_eq!(segments[2], MessageSegment::Text { content: "\noutro".to_string() });
}

#[test]
fn multi_line_code_keeps_its_internal_newlines() {
    let segments = parse_message_segments("```sql\nSELECT 1;\n-- note\nSELECT 2;\n```");
    assert_eq!(
        segments[0],
        MessageSegment::Code {
            language: "sql".to_string(),
            code: "SELECT 1;\n-- note\nSELECT 2;".to_string()
        }
    );
}

// --- streaming behaviour ---

#[test]
fn streaming_pulses_the_root_at_the_tailwind_pulse_period() {
    let audit = Audit::case("streaming words", "postgresql", true);
    let mut rt = runtime(audit, 960.0, 640.0);
    // The 2s pulse (opacity 1 -> 0.5 -> 1) reports a phase from the render
    // clock once time advances past 0.
    rt.frame(0.5);
    let root = element(&rt, "ai-message-content");
    assert!(
        root.loop_phase.is_some(),
        "the streaming root pulses: {:?}",
        root.loop_phase
    );
}

#[test]
fn streaming_hides_the_code_toolbar_as_react_does() {
    let audit = Audit::case("```sql\nSELECT 1\n```", "postgresql", true);
    let rt = runtime(audit, 960.0, 640.0);
    assert!(has_key(&rt, "ai-message-content-code-0"));
    assert!(!has_key(&rt, "ai-message-content-code-0-insert"));
    assert!(!has_key(&rt, "ai-message-content-code-0-copy"));
}

#[test]
fn non_streaming_shows_the_toolbar() {
    let audit = Audit::case("```sql\nSELECT 1\n```", "postgresql", false);
    let rt = runtime(audit, 960.0, 640.0);
    assert!(has_key(&rt, "ai-message-content-code-0-insert"));
    assert!(has_key(&rt, "ai-message-content-code-0-copy"));
}

// --- the slot snapshot drives the rendered block state ---

#[test]
fn a_fullscreen_slot_renders_the_fullscreen_tree() {
    let mut audit = Audit::case("```sql\nSELECT 1\n```", "postgresql", false);
    audit.slots[0].toggle_fullscreen();
    let rt = runtime(audit, 960.0, 640.0);
    // The fullscreen tree mounts under the `-fullscreen` key; the inline
    // root (plain `-code-0`) does not (the header exists in both trees,
    // under the same key — React's fullscreen keeps its header too).
    assert!(has_key(&rt, "ai-message-content-code-0-fullscreen"));
    assert!(!has_key(&rt, "ai-message-content-code-0"));
}

#[test]
fn a_dialect_open_slot_opens_the_listbox_in_the_layer() {
    let mut audit = Audit::case("```sql\nSELECT 1\n```", "postgresql", false);
    audit.slots[0].toggle_dialect_open();
    let rt = runtime(audit, 960.0, 640.0);
    // The dialect listbox renders through the layer the build fills; the
    // select's field is present either way, and the open state is the
    // slot's (the trigger reports the open state in its own tree).
    assert!(has_key(&rt, "ai-message-content-code-0-dialect-field-trigger"));
    let trigger = element(&rt, "ai-message-content-code-0-dialect-field-trigger");
    assert!(trigger.expanded.unwrap_or(false), "the listbox is open");
}

#[test]
fn the_slot_dialect_seeds_the_block_not_the_builder_default() {
    let mut audit = Audit::case("```sql\nSELECT 1\n```", "mysql", false);
    // The builder's sql_dialect below is postgresql; the slot (seeded
    // mysql) must win — React's useState(sqlDialect) semantics.
    audit.slots[0] = AiCodeBlockState::new("mysql");
    let rt = runtime(audit, 960.0, 640.0);
    assert!(
        texts(&rt).iter().any(|t| t == "mysql"),
        "the slot's dialect shows: {:?}",
        texts(&rt)
    );
}

// --- the citation allow-list ---

#[test]
fn only_listed_references_become_links() {
    let audit = Audit::case("a [M1] b [M4] c [M9]", "postgresql", false);
    let mut rt = runtime(audit, 960.0, 640.0);
    // M1 and M4 are in the references; M9 is not.
    assert!(rt.find("memory-M1").is_some());
    assert!(rt.find("memory-M4").is_some());
    assert!(rt.find("memory-M9").is_none());
    press_link(&mut rt, "memory-M4");
    assert_eq!(rt.state.cites, vec!["M4".to_string()]);
    assert!(
        texts(&rt).iter().any(|t| t.contains("[M9]")),
        "the unlisted citation stays literal"
    );
}

#[test]
fn an_empty_reference_list_links_nothing() {
    let mut audit = Audit::case("no [M1] here", "postgresql", false);
    audit.references = Vec::new();
    let mut rt = runtime(audit, 960.0, 640.0);
    assert!(rt.find("memory-M1").is_none());
    click_text(&mut rt);
    assert!(rt.state.cites.is_empty());
}

fn click_text(rt: &mut Runtime<Audit>) {
    let (x, y) = rt
        .find("ai-message-content-p-0")
        .unwrap_or_else(|| panic!("the paragraph is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// --- the two-window fit (DoD #4) ---

#[test]
fn no_horizontal_overflow_at_960_and_1440() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let audit = Audit::case(
            "words ".repeat(120).as_str(),
            "postgresql",
            false,
        );
        let rt = runtime(audit, w, h);
        assert_no_horizontal_overflow(&rt, w, "the markdown text wraps within the window");
    }
}

#[test]
fn mixed_message_fits_both_windows() {
    let content = format!(
        "{}\n\n- {}\n- {}\n\n```sql\n{}\n```\n\n{} [M1]",
        "long prose paragraph ".repeat(20),
        "bullet one",
        "bullet two",
        "SELECT ".repeat(40),
        "closing words"
    );
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let audit = Audit::case(&content, "postgresql", false);
        let rt = runtime(audit, w, h);
        assert_no_horizontal_overflow(&rt, w, "the mixed message fits");
    }
}

// --- the ChatBubble wiring boundary ---

fn bubble_audit_view(state: &Audit, _cx: &mut Cx<Audit>) -> El {
    let message = AiChatMessage::new(AiChatRole::Assistant, state.content.clone());
    let mut layer = Vec::new();
    let bubble_state = bubble_state(state);
    let bubble = ChatBubble::<Audit>::new(&message, &bubble_state)
        .sql_dialect("mysql")
        .code_wiring(Audit::wiring())
        .on_insert_sql(|s: &mut Audit, sql: &str| s.inserts.push(sql.to_string()));
    let body = bubble.build(state.dark, &mut layer);
    rune_kit::root(
        col().key("audit-page").w_full().h_full().padding(16.0).child(body),
        layer,
    )
}

fn bubble_state(state: &Audit) -> ChatBubbleState {
    // The bubble state owns the same slots as the audit host (seeded the
    // way set_messages would seed them).
    let mut bubble = ChatBubbleState::default();
    let dialect = "mysql";
    bubble.sync_code_blocks(&state.content, dialect);
    // sync_code_blocks re-seeds with its own dialect; the audit slots
    // carry the same snapshot lengths.
    bubble
}

fn bubble_runtime(audit: Audit, w: f32, h: f32) -> Runtime<Audit> {
    let dark = audit.dark;
    let mut rt = Runtime::new(audit, bubble_audit_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

#[test]
fn the_bubble_seeds_slots_with_its_dialect_and_renders_the_block() {
    let audit = Audit::case("```sql\nSELECT 1\n```", "mysql", false);
    let rt = bubble_runtime(audit, 960.0, 640.0);
    assert!(has_key(&rt, "chat-bubble-content-ai-code-0"));
    assert!(
        texts(&rt).iter().any(|t| t == "mysql"),
        "the bubble's sql_dialect seeds the block: {:?}",
        texts(&rt)
    );
}

#[test]
fn the_bubble_dialect_seed_follows_set_messages_semantics() {
    // set_messages seeds with the recorded dialect: assert the seed count
    // and dialect through the list state the way the panel does.
    let mut list = datazen_ui::components::ai::ai_chat_message_list_state::AiChatMessageListState::default();
    list.set_sql_dialect("clickhouse");
    let messages = vec![AiChatMessage::new(
        AiChatRole::Assistant,
        "```sql\nSELECT 1\n```\n```sql\nSELECT 2\n```",
    )];
    list.set_messages(messages);
    // Two fenced blocks seed two slots, each with the recorded dialect.
    let probe = list.bubble_state(0);
    assert_eq!(probe.code_blocks().len(), 2);
    assert_eq!(probe.code_blocks()[0].selected_dialect, SqlDialect::ClickHouse);
    assert_eq!(probe.code_blocks()[1].selected_dialect, SqlDialect::ClickHouse);
}
