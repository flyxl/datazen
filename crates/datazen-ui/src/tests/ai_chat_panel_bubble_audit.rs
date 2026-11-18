//! Independent audit of T009-S2 (the `AiChatPanel` `ChatBubble` ported in
//! `574e39f7e`).
//!
//! Every expectation below is re-derived from the React source
//! (`packages/frontend/src/components/ai/AiChatPanel.tsx:992–1210`,
//! `packages/frontend/src/types/ai.ts:286,446`,
//! `packages/frontend/src/locales/zh-CN/ai.ts`,
//! `packages/frontend/src/components/ai/AiMessageContent.tsx:66`) with fresh
//! vectors — different message ids, memory references, question ids and
//! labels from the coder's tests — so a value copied from the implementation
//! under test cannot pass by construction. Runtime facts are read off the
//! laid-out element tree (`rt.elements()` rects / `item_selected` / `fill` /
//! `opacity` / `loop_phase`) and the accessibility dump, never off the
//! component's own report of itself. Geometry is double-observed: rect
//! bounds + sibling overlap from `rt.elements()`, and hit-test centres from
//! `rt.find()`, a second layout-engine channel.
//!
//! Findings recorded here (all non-blocking; see the audit report):
//!
//! * D1 — `open_memory_citation("")` opens the list, while React's
//!   `handleMemoryCitationClick` returns early on an empty reference
//!   (`if (!reference) return`). Unreachable from any real citation today:
//!   the placeholder has no links, and T169's links are built from
//!   non-empty `memory.reference` values. Pinned below so the behaviour
//!   cannot silently change; fix with the T169 wiring (or a one-line
//!   empty guard).
//! * D2 — the tool / reasoning / memory toggles carry one icon (the
//!   semantic glyph) where React shows a chevron plus the glyph; the
//!   open/closed state rides the button's `expanded` slot and the state
//!   word. Deliberate kit limitation, documented in the port's module
//!   docs; the state is observable through `expanded` and the label.
//! * D3 — React's expanded reasoning text and memory section carry a
//!   left / top rule; Rune has no single-side border, so both render as
//!   indented blocks without the rule. Documented in the port's module
//!   docs; geometry unchanged.
//!
//! Deliberate divergences inherited from S1 and pinned, not failed:
//!
//! * Typing free text into a **multi-select** question and then pressing an
//!   option replaces the draft with the single pick instead of spreading
//!   the string into a character array as React's `toggleOption` would
//!   (the React path is broken in that corner).
//! * Content/reasoning gates use `is_empty()` only, no `trim()`, so the
//!   S1-noted `trim` divergence (Rust keeps U+FEFF, JS strips it) cannot
//!   change any gate here.

use rune::{col, Color, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

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

/// Second observation channel: every keyed element's hit-test centre (from
/// `rt.find`, the pointer layer) must itself lie inside the window.
fn assert_find_centres_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let Some(key) = e.key.clone() else { continue };
        let Some((x, y)) = rt.find(&key) else {
            continue;
        };
        if x < -0.5 || y < -0.5 || x > w + 0.5 || y > h + 0.5 {
            outside.push(format!("{key} centre ({x:.1}, {y:.1})"));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} hit-test centre(s) out of window {w}x{h}: {outside:?}",
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

// ---- fresh fixtures ----------------------------------------------------------

fn assistant(content: &str) -> AiChatMessage {
    AiChatMessage::new(AiChatRole::Assistant, content)
}

fn user(content: &str) -> AiChatMessage {
    AiChatMessage::new(AiChatRole::User, content)
}

fn system(content: &str) -> AiChatMessage {
    AiChatMessage::new(AiChatRole::System, content)
}

/// Fresh references: different ids, subjects, kinds and scopes from the
/// coder's M1/M2 fixtures.
fn memory_k3() -> AiMemoryReference {
    AiMemoryReference::new(
        "K3",
        "mem-k3-9f2",
        "prefers tabs over spaces",
        "fact",
        "connection",
    )
}

fn memory_k4() -> AiMemoryReference {
    AiMemoryReference::new(
        "K4",
        "mem-k4-71b",
        "prod-eu-2 is read-only",
        "preference",
        "org",
    )
}

fn memory_k5() -> AiMemoryReference {
    AiMemoryReference::new("K5", "mem-k5-04d", "deploys on Fridays", "rule", "user")
}

fn audit_questions() -> Vec<AiQuestion> {
    vec![
        AiQuestion::new("e1", "Which engine?")
            .option("sq", "SQLite")
            .option("dk", "DuckDB"),
        AiQuestion::new("e2", "Which jobs?")
            .allow_multiple(true)
            .option("mg", "Migrate")
            .option("in", "Inspect"),
        AiQuestion::new("e3", "Notes?"),
    ]
}

struct AuditProbe {
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
    sql_hook_calls: Vec<&'static str>,
}

fn probe(message: AiChatMessage) -> AuditProbe {
    let block = QuestionBlockState::new(&message.questions);
    AuditProbe {
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
        sql_hook_calls: Vec::new(),
    }
}

fn bubble_el(state: &AuditProbe) -> El {
    let mut bubble = ChatBubble::<AuditProbe>::new(&state.message, &state.bubble)
        .streaming(state.streaming)
        .last_assistant(state.last_assistant)
        .sql_dialect("mysql")
        .on_insert_sql(|s: &mut AuditProbe, _| s.sql_hook_calls.push("on_insert_sql"))
        .on_run_code(|s: &mut AuditProbe, _, _| s.sql_hook_calls.push("on_run_code"))
        .on_new_query(|s: &mut AuditProbe, _| s.sql_hook_calls.push("on_new_query"))
        .on_toggle_reasoning(|s: &mut AuditProbe| s.bubble.toggle_reasoning())
        .on_toggle_tool(|s: &mut AuditProbe| s.bubble.toggle_tool())
        .on_toggle_memory(|s: &mut AuditProbe| s.bubble.toggle_memory())
        .on_cite_memory(|s: &mut AuditProbe, reference: &str| {
            s.bubble.open_memory_citation(reference);
        });
    if state.wire_remember {
        bubble = bubble.on_remember(|s: &mut AuditProbe, m: &AiChatMessage| {
            s.remembered.push(m.content.clone());
        });
    }
    if state.wire_questions {
        let questions = state.message.questions.clone();
        bubble = bubble.with_questions(
            &state.block,
            |s: &mut AuditProbe, qid, oid, multi| s.block.toggle(qid, oid, multi),
            |s: &mut AuditProbe, qid: &str| s.block.custom_edit(qid),
            move |s: &mut AuditProbe| {
                if let Some(payload) = s.block.submit(&questions) {
                    s.answered.push(payload);
                }
            },
        );
    }
    bubble.build(state.dark)
}

fn bubble_view(state: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    col()
        .key("probe-page")
        .w_full()
        .h_full()
        .padding(16.0)
        .child(bubble_el(state))
}

fn runtime(probe: AuditProbe, w: f32, h: f32) -> Runtime<AuditProbe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, bubble_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// ---- i18n: keys exist in zh-CN/ai.ts with these exact values ---------------

#[test]
fn audit_bubble_i18n_keys_exist_and_match_the_react_locale() {
    // Values read from packages/frontend/src/locales/zh-CN/ai.ts:
    // toolRole:165, collapsed:166, expanded:167, reasoning:157,
    // memory.used:70, rememberThis:71, thinking:151, cancelled:29.
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
        assert!(has(key), "{key} must exist in the generated zh-CN table");
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

// ---- ICON_BOOK_OPEN: registry entry is the installed lucide glyph ----------

#[test]
fn audit_icon_book_open_matches_the_installed_lucide_book_open() {
    // Paths verified against the repo's installed lucide-react@0.487.0
    // (node_modules/lucide-react/dist/esm/icons/book-open.js).
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

// ---- discipline: no bare unwrap/expect, no oklch, no todo in new code ------

#[test]
fn audit_product_source_has_no_bare_unwrap_expect_oklch_or_todo() {
    let src = include_str!("../components/ai/chat_bubble.rs");
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for literal in [
        "unwrap()",
        "expect(",
        "oklch(",
        "oklcha(",
        "todo!()",
        "unimplemented!()",
    ] {
        assert!(
            !code.contains(literal),
            "`{literal}` must not appear in the new product code"
        );
    }
    // `unwrap_or_default` is the only `unwrap` shape present, and it is not
    // a bare unwrap: the reasoning text falls back to "" when absent.
    assert!(code.contains("unwrap_or_default()"));
}

// ---- pure predicates: fresh vectors, including the system role -------------

#[test]
fn audit_predicates_match_react_gates_on_fresh_vectors() {
    // Tool early return.
    assert!(ChatBubbleState::is_tool_bubble(&AiChatMessage::new(
        AiChatRole::Tool,
        "pg_dump --help"
    )));
    assert!(!ChatBubbleState::is_tool_bubble(&AiChatMessage::new(
        AiChatRole::Tool,
        ""
    )));
    assert!(!ChatBubbleState::is_tool_bubble(&system("note")));

    // Pulse: streaming + no content + non-empty reasoning.
    let pulsing = assistant("").reasoning("still thinking");
    assert!(ChatBubbleState::is_pulsing(&pulsing, true));
    assert!(!ChatBubbleState::is_pulsing(&pulsing, false));
    assert!(!ChatBubbleState::is_pulsing(
        &assistant("").reasoning(" "),
        false
    ));
    // A whitespace-only reasoning string is truthy in JS, so it pulses.
    assert!(ChatBubbleState::is_pulsing(
        &assistant("").reasoning(" "),
        true
    ));
    assert!(!ChatBubbleState::is_pulsing(&assistant(" "), true));

    // Reasoning gate: " " is truthy, "" is falsy.
    assert!(ChatBubbleState::has_reasoning(
        &assistant("x").reasoning(" ")
    ));
    assert!(!ChatBubbleState::has_reasoning(
        &assistant("x").reasoning("")
    ));

    // Remember gate: the system role is not the user, so React offers it.
    let sys = system("here is the plan");
    assert!(ChatBubbleState::show_remember(&sys, false, true, true));
    assert!(!ChatBubbleState::show_remember(&sys, true, true, true));
    assert!(!ChatBubbleState::show_remember(&sys, false, false, true));

    // Questions gate: system qualifies (!isUser), tool does not reach it
    // in the render (early return) but the predicate itself is role-blind
    // beyond `!= User`.
    let sys_q = system("pick one").questions(audit_questions());
    assert!(ChatBubbleState::has_questions(&sys_q, false));
    assert!(!ChatBubbleState::has_questions(&sys_q, true));
}

// ---- tool role: fresh content, a11y name, early-return consequences --------

#[test]
fn audit_tool_toggle_label_state_word_and_a11y_name() {
    let mut rt = runtime(
        probe(AiChatMessage::new(AiChatRole::Tool, "pg_dump --help")),
        960.0,
        640.0,
    );
    assert!(has_key(&rt, "tool-role-toggle"));
    assert!(
        !has_key(&rt, "chat-bubble-body"),
        "tool early return: no body"
    );
    assert_eq!(
        a11y_label(&rt, "tool-role-toggle").as_deref(),
        Some("工具输出 已折叠"),
        "accessible name joins toolRole + collapsed"
    );
    assert_eq!(element(&rt, "tool-role-toggle").expanded, Some(false));
    click(&mut rt, "tool-role-toggle");
    assert!(has_key(&rt, "chat-bubble-tool-content"));
    assert_eq!(
        a11y_label(&rt, "tool-role-toggle").as_deref(),
        Some("工具输出 已展开")
    );
    assert_eq!(element(&rt, "tool-role-toggle").expanded, Some(true));
    assert!(
        texts(&rt).iter().any(|text| text == "pg_dump --help"),
        "expanded tool box shows the raw output"
    );
    // The expanded box honours the 90% cap: content box is 928pt wide.
    let content = element(&rt, "chat-bubble-tool-content");
    assert!(
        content.rect[2] <= 0.9 * 928.0 + 1.0,
        "tool content width {} exceeds the 90% cap: {:?}",
        content.rect[2],
        content.rect
    );
}

#[test]
fn audit_tool_early_return_skips_memory_remember_and_cancelled() {
    // React's `if (isTool && message.content)` returns before the memory
    // chip, the remember button and the cancelled marker are ever reached.
    let mut p = probe(
        AiChatMessage::new(AiChatRole::Tool, "tool output")
            .memory_used(vec![memory_k3()])
            .cancelled(true),
    );
    p.last_assistant = true;
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "tool-role-toggle"));
    assert!(!has_key(&rt, "ai-memory-chip"));
    assert!(!has_key(&rt, "ai-memory-remember"));
    assert!(!has_key(&rt, "chat-message-cancelled"));
}

#[test]
fn audit_tool_message_without_content_falls_through_to_the_normal_bubble() {
    let rt = runtime(
        probe(AiChatMessage::new(AiChatRole::Tool, "").reasoning("why empty")),
        960.0,
        640.0,
    );
    assert!(!has_key(&rt, "tool-role-toggle"));
    assert!(has_key(&rt, "chat-bubble-body"));
    assert!(has_key(&rt, "chat-bubble-reasoning"));
}

// ---- reasoning: keyboard, whitespace truthiness, user role ------------------

#[test]
fn audit_reasoning_toggle_works_by_keyboard_and_hides_until_open() {
    let mut rt = runtime(
        probe(assistant("done").reasoning("because the index was missing")),
        960.0,
        640.0,
    );
    assert!(has_key(&rt, "chat-bubble-reasoning"));
    assert!(!has_key(&rt, "chat-bubble-reasoning-text"));
    press(&mut rt, Key::Tab);
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("chat-bubble-reasoning"),
        "Tab reaches the reasoning toggle first"
    );
    press(&mut rt, Key::Enter);
    assert!(has_key(&rt, "chat-bubble-reasoning-text"));
    assert_eq!(element(&rt, "chat-bubble-reasoning").expanded, Some(true));
    assert!(
        texts(&rt)
            .iter()
            .any(|text| text == "because the index was missing"),
        "the open section shows the reasoning verbatim"
    );
    press(&mut rt, Key::Text(" ".into()));
    assert!(
        !has_key(&rt, "chat-bubble-reasoning-text"),
        "Space closes it again, exactly once"
    );
    assert_eq!(element(&rt, "chat-bubble-reasoning").expanded, Some(false));
}

#[test]
fn audit_whitespace_reasoning_is_truthy_and_shows_its_toggle() {
    // JS `message.reasoning && …`: " " is truthy. Rune's `is_empty()` gate
    // agrees, so the toggle renders for a whitespace-only reasoning.
    let rt = runtime(probe(assistant("done").reasoning(" ")), 960.0, 640.0);
    assert!(has_key(&rt, "chat-bubble-reasoning"));
}

#[test]
fn audit_user_message_with_reasoning_still_shows_the_reasoning_section() {
    // React gates the reasoning block on `message.reasoning` alone, not on
    // the role — a user message with reasoning shows the toggle too.
    let mut rt = runtime(
        probe(user("my query").reasoning("user-side note")),
        960.0,
        640.0,
    );
    assert!(has_key(&rt, "chat-bubble-reasoning"));
    click(&mut rt, "chat-bubble-reasoning");
    assert!(
        texts(&rt).iter().any(|text| text == "user-side note"),
        "the reasoning text opens for a user message as well"
    );
}

// ---- content placeholder: SQL hooks stored, never called --------------------

#[test]
fn audit_placeholder_shows_fenced_sql_raw_and_never_calls_sql_hooks() {
    let fenced = "```sql\nSELECT * FROM orders;\n```";
    let rt = runtime(probe(assistant(fenced)), 960.0, 640.0);
    assert!(has_key(&rt, "chat-bubble-content-placeholder"));
    assert!(has_key(&rt, "chat-bubble-content"));
    assert!(
        texts(&rt).iter().any(|text| text == fenced),
        "the fenced block renders as raw text, not as a code block"
    );
    assert!(
        !rt.elements()
            .iter()
            .any(|e| e.key.as_deref().is_some_and(|key| {
                key.contains("code") || key.contains("sql-block") || key.contains("ai-code")
            })),
        "nothing here pretends to be AiCodeBlock"
    );
    assert!(
        rt.state.sql_hook_calls.is_empty(),
        "onInsertSql / onRunCode / onNewQuery are stored only, never called: {:?}",
        rt.state.sql_hook_calls
    );
    // No citation link exists in the placeholder, so the cite hook cannot
    // fire from the UI either.
    assert!(
        !texts(&rt).iter().any(|text| text.contains("#memory-")),
        "the placeholder carries no #memory- link to click"
    );
}

#[test]
fn audit_content_gates_use_emptiness_not_trimming() {
    // Whitespace-only content is truthy in JS and non-empty in Rune.
    let ws = runtime(probe(assistant(" \t ")), 960.0, 640.0);
    assert!(has_key(&ws, "chat-bubble-content"));
    // Two U+FEFFs: non-empty in both runtimes (JS trim would strip them,
    // but the gate never trims — this is the S1 trim divergence made
    // irrelevant, pinned for both spellings).
    let feff = runtime(probe(assistant("\u{feff}\u{feff}")), 960.0, 640.0);
    assert!(has_key(&feff, "chat-bubble-content"));
    // Empty content renders no content node at all.
    let empty = runtime(probe(assistant("")), 960.0, 640.0);
    assert!(!has_key(&empty, "chat-bubble-content"));
}

// ---- streaming: spinner channels, pulse, and the no-reasoning case ---------

#[test]
fn audit_thinking_spinner_carries_its_label_and_vanishes_with_content() {
    let mut p = probe(assistant("").reasoning("still working"));
    p.streaming = true;
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "chat-bubble-thinking"));
    assert!(has_key(&rt, "chat-bubble-thinking-label"));
    assert_eq!(
        a11y_label(&rt, "chat-bubble-thinking").as_deref(),
        Some("思考中…"),
        "the spinner root (role=progressbar) carries chat.thinking as its accessible label"
    );
    assert!(texts(&rt).iter().any(|text| text == "思考中…"));

    // Content arrived: no spinner, no pulse.
    let mut arrived = probe(assistant("partial answer").reasoning("still working"));
    arrived.streaming = true;
    let rt2 = runtime(arrived, 960.0, 640.0);
    assert!(!has_key(&rt2, "chat-bubble-thinking"));
    assert_eq!(element(&rt2, "chat-bubble-body").loop_phase, None);
}

#[test]
fn audit_streaming_without_reasoning_shows_neither_spinner_nor_pulse() {
    // React: both the pulse class and the spinner require `message.reasoning`.
    let mut p = probe(assistant(""));
    p.streaming = true;
    let rt = runtime(p, 960.0, 640.0);
    assert!(!has_key(&rt, "chat-bubble-thinking"));
    let body = element(&rt, "chat-bubble-body");
    assert_eq!(body.loop_phase, None, "no reasoning, no pulse loop");
    assert_eq!(body.opacity, 1.0);
}

#[test]
fn audit_pulse_actually_moves_opacity_over_a_full_period() {
    let mut p = probe(assistant("").reasoning("digging through the schema"));
    p.streaming = true;
    let mut rt = runtime(p, 960.0, 640.0);
    let body = element(&rt, "chat-bubble-body");
    assert!(
        body.loop_phase.is_some(),
        "the streaming bubble carries a looping opacity track: {body:?}"
    );
    // Sample the layout engine's reported opacity over a full 2s period.
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
    // Control: the same message settled stays opaque with no loop.
    let mut settled = runtime(probe(assistant("done").reasoning("digging")), 960.0, 640.0);
    assert_eq!(element(&settled, "chat-bubble-body").loop_phase, None);
    settled.frame(1.0);
    assert_eq!(element(&settled, "chat-bubble-body").opacity, 1.0);
}

// ---- memory: counts, rows, chip a11y, user gate -----------------------------

fn memory_message() -> AiChatMessage {
    assistant("See [K3] and [K4] for details.").memory_used(vec![memory_k3(), memory_k4()])
}

#[test]
fn audit_memory_chip_counts_one_and_three_and_names_itself() {
    let one = runtime(
        probe(assistant("x").memory_used(vec![memory_k3()])),
        960.0,
        640.0,
    );
    assert!(
        texts(&one).iter().any(|text| text == "已使用 1 条记忆"),
        "singular count formats through chat.memory.used"
    );
    assert_eq!(
        a11y_label(&one, "ai-memory-chip").as_deref(),
        Some("已使用 1 条记忆"),
        "the chip's accessible name is its label"
    );

    let three = runtime(
        probe(assistant("x").memory_used(vec![memory_k3(), memory_k4(), memory_k5()])),
        960.0,
        640.0,
    );
    assert!(
        texts(&three).iter().any(|text| text == "已使用 3 条记忆"),
        "the count is the list length, not a hardcoded 2"
    );
}

#[test]
fn audit_memory_list_rows_render_reference_subject_kind_and_scope() {
    let mut rt = runtime(probe(memory_message()), 960.0, 640.0);
    assert!(!has_key(&rt, "ai-memory-expanded"));
    click(&mut rt, "ai-memory-chip");
    assert!(has_key(&rt, "ai-memory-expanded"));
    for key in ["ai-memory-item-K3", "ai-memory-item-K4"] {
        assert!(has_key(&rt, key), "{key} is listed");
    }
    let all = texts(&rt);
    for fragment in [
        "[K3]",
        "prefers tabs over spaces",
        "fact · connection",
        "[K4]",
        "prod-eu-2 is read-only",
        "preference · org",
    ] {
        assert!(
            all.iter().any(|text| text == fragment),
            "memory item shows {fragment:?}: {all:?}"
        );
    }
    // The reference is rendered in the mono font, like React's font-mono span.
    let reference = element(&rt, "ai-memory-item-K3-reference");
    assert_eq!(reference.text.as_deref(), Some("[K3]"));
}

#[test]
fn audit_memory_chip_hidden_for_user_messages_even_with_memories() {
    let rt = runtime(
        probe(user("hi").memory_used(vec![memory_k3()])),
        960.0,
        640.0,
    );
    assert!(!has_key(&rt, "ai-memory-chip"));
}

#[test]
fn audit_memory_chip_shows_for_the_system_role() {
    // React gates the chip on `!isUser` only — a system message with
    // memories shows it too.
    let rt = runtime(
        probe(system("note").memory_used(vec![memory_k3()])),
        960.0,
        640.0,
    );
    assert!(has_key(&rt, "ai-memory-chip"));
}

// ---- citation jump: settle_all reproduction + empty-reference finding -------

#[test]
fn audit_direct_state_poke_needs_settle_all_before_it_renders() {
    // Reproduces the coder's claim: poking `rt.state` directly does not
    // mark the runtime dirty, so `frame` alone re-renders nothing and the
    // list stays closed until `settle_all` runs.
    let mut rt = runtime(probe(memory_message()), 960.0, 640.0);
    assert!(!has_key(&rt, "ai-memory-expanded"));
    rt.state.bubble.open_memory_citation("K4");
    rt.frame(0.0);
    assert!(
        !has_key(&rt, "ai-memory-expanded"),
        "a direct poke without settle_all must not re-render — if this fails, the coder's settle_all claim is wrong"
    );
    rt.settle_all();
    rt.frame(0.0);
    assert!(
        has_key(&rt, "ai-memory-expanded"),
        "after settle_all the jump expands the list"
    );
    let focused = element(&rt, "ai-memory-item-K4");
    assert!(focused.item_selected, "the jumped-to item reports selected");
    assert_ne!(
        focused.fill,
        Color::TRANSPARENT,
        "the jumped-to item paints its accent wash"
    );
    let other = element(&rt, "ai-memory-item-K3");
    assert!(!other.item_selected, "the sibling stays unselected");
    assert_eq!(other.fill, Color::TRANSPARENT);
}

#[test]
fn audit_citation_jump_to_an_unknown_reference_opens_but_selects_nothing() {
    // React sets open regardless and its querySelector simply finds nothing.
    let mut state = ChatBubbleState::default();
    state.open_memory_citation("K99");
    assert!(state.memory_open());
    assert_eq!(state.memory_focused(), Some("K99"));
    let mut rt = runtime(probe(memory_message()), 960.0, 640.0);
    rt.state.bubble.open_memory_citation("K99");
    rt.settle_all();
    rt.frame(0.0);
    assert!(has_key(&rt, "ai-memory-expanded"));
    assert!(!element(&rt, "ai-memory-item-K3").item_selected);
    assert!(!element(&rt, "ai-memory-item-K4").item_selected);
}

#[test]
fn audit_empty_reference_jump_opens_the_list_unlike_react() {
    // Finding D1, pinned: React's handler returns early on an empty
    // reference (`if (!reference) return`), so the list stays closed.
    // Rune opens it and focuses nothing. Unreachable from any real
    // citation today (links are built from non-empty references), so this
    // is recorded, not failed — fix with the T169 wiring.
    let mut state = ChatBubbleState::default();
    state.open_memory_citation("");
    assert!(
        state.memory_open(),
        "current Rune behaviour: an empty jump still opens the list"
    );
    assert_eq!(state.memory_focused(), Some(""));
    let mut rt = runtime(probe(memory_message()), 960.0, 640.0);
    rt.state.bubble.open_memory_citation("");
    rt.settle_all();
    rt.frame(0.0);
    assert!(has_key(&rt, "ai-memory-expanded"));
    assert!(!element(&rt, "ai-memory-item-K3").item_selected);
    assert!(!element(&rt, "ai-memory-item-K4").item_selected);
}

// ---- remember: full matrix, repeat presses, system role ---------------------

#[test]
fn audit_remember_matrix_reports_only_when_every_conjunct_holds() {
    // Positive: shown, click reports the exact message, twice on two clicks.
    let mut p = probe(assistant("keep this answer"));
    p.last_assistant = true;
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-memory-remember"));
    assert!(
        texts(&rt).iter().any(|text| text == "记住这条"),
        "the button carries chat.rememberThis"
    );
    click(&mut rt, "ai-memory-remember");
    click(&mut rt, "ai-memory-remember");
    assert_eq!(
        rt.state.remembered,
        vec![
            "keep this answer".to_string(),
            "keep this answer".to_string()
        ],
        "each press reports the message; React calls onRemember per click"
    );

    // One missing conjunct each: hidden.
    let cases: Vec<(&str, AuditProbe)> = vec![
        ("not last", {
            let mut q = probe(assistant("keep this answer"));
            q.last_assistant = false;
            q
        }),
        ("streaming", {
            let mut q = probe(assistant("keep this answer"));
            q.last_assistant = true;
            q.streaming = true;
            q
        }),
        ("empty content", {
            let mut q = probe(assistant(""));
            q.last_assistant = true;
            q
        }),
        ("unwired", {
            let mut q = probe(assistant("keep this answer"));
            q.last_assistant = true;
            q.wire_remember = false;
            q
        }),
        ("user role", {
            let mut q = probe(user("keep this answer"));
            q.last_assistant = true;
            q
        }),
    ];
    for (label, q) in cases {
        assert!(
            !has_key(&mut runtime(q, 960.0, 640.0), "ai-memory-remember"),
            "remember hidden when {label}"
        );
    }
}

#[test]
fn audit_remember_shows_for_the_system_role_when_last_assistant() {
    // React: `!isUser && isLastAssistant && …` — the system role qualifies.
    let mut p = probe(system("here is the plan"));
    p.last_assistant = true;
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-memory-remember"));
}

// ---- cancelled: marker for user and system roles too ------------------------

#[test]
fn audit_cancelled_marker_shows_for_user_and_system_roles() {
    let user_rt = runtime(probe(user("partial").cancelled(true)), 960.0, 640.0);
    assert!(has_key(&user_rt, "chat-message-cancelled"));
    assert!(
        texts(&user_rt).iter().any(|text| text == "已停止生成。"),
        "the marker carries chat.cancelled for the user role too"
    );
    let sys_rt = runtime(probe(system("partial").cancelled(true)), 960.0, 640.0);
    assert!(has_key(&sys_rt, "chat-message-cancelled"));
    let not_cancelled = runtime(probe(assistant("done")), 960.0, 640.0);
    assert!(!has_key(&not_cancelled, "chat-message-cancelled"));
}

// ---- questions: wiring seam, fresh payload, S1 divergence inherited ----------

#[test]
fn audit_questions_block_needs_the_with_questions_wiring() {
    // React renders the block only when `onAnswerQuestions` is supplied;
    // Rune's equivalent is `with_questions`. Without the wiring the block
    // stays hidden even when every other gate holds.
    let mut p = probe(assistant("Which one?").questions(audit_questions()));
    p.last_assistant = true;
    p.wire_questions = false;
    let rt = runtime(p, 960.0, 640.0);
    assert!(!has_key(&rt, "question-block"));
}

#[test]
fn audit_questions_block_forwards_a_fresh_payload_and_hides_while_streaming() {
    let mut p = probe(assistant("Which one?").questions(audit_questions()));
    p.last_assistant = true;
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "question-block"));
    assert!(has_key(&rt, "question-option-e1-dk"));
    click(&mut rt, "question-option-e1-dk");
    click(&mut rt, "question-submit");
    assert_eq!(
        rt.state.answered,
        vec!["[e1] Which engine?\nDuckDB".to_string()],
        "submit forwards the formatted answers to onAnswerQuestions"
    );

    let mut streaming = probe(assistant("Which one?").questions(audit_questions()));
    streaming.last_assistant = true;
    streaming.streaming = true;
    assert!(!has_key(
        &mut runtime(streaming, 960.0, 640.0),
        "question-block"
    ));

    let mut not_last = probe(assistant("Which one?").questions(audit_questions()));
    not_last.last_assistant = false;
    assert!(!has_key(
        &mut runtime(not_last, 960.0, 640.0),
        "question-block"
    ));
}

#[test]
fn audit_questions_block_renders_for_the_system_role_when_last_assistant() {
    // React: `hasQuestions && isLastAssistant && onAnswerQuestions` — the
    // system role is not the user, so it qualifies.
    let mut p = probe(system("pick one").questions(audit_questions()));
    p.last_assistant = true;
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "question-block"));
    assert!(has_key(&rt, "question-option-e1-sq"));
}

#[test]
fn audit_multi_select_free_text_then_option_replaces_the_draft() {
    // S1's divergence, inherited through the reuse: React would spread the
    // typed string into a character array; Rune replaces the draft with the
    // single selection.
    let mut p = probe(assistant("Which one?").questions(audit_questions()));
    p.last_assistant = true;
    let mut rt = runtime(p, 960.0, 640.0);
    click(&mut rt, "question-custom-input-e2");
    rt.paste("write my own");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "question-option-e2-mg");
    assert_eq!(
        rt.state.block.custom_text("e2"),
        "",
        "pressing an option clears the draft, as in S1"
    );
    click(&mut rt, "question-submit");
    assert_eq!(
        rt.state.answered,
        vec!["[e2] Which jobs?\nMigrate".to_string()],
        "the option replaces the typed text instead of spreading it"
    );
}

// ---- alignment and geometry: fresh vectors, dual channel ---------------------

#[test]
fn audit_user_bubble_right_assistant_left_with_fresh_short_text() {
    let user_rt = runtime(probe(user("ok")), 960.0, 640.0);
    let user_body = element(&user_rt, "chat-bubble-body");
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

    let asst_rt = runtime(probe(assistant("sure thing")), 960.0, 640.0);
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
fn audit_long_content_wraps_inside_ninety_percent_at_both_sizes() {
    let long = "The query planner chose a sequential scan over twelve thousand rows because the index on created_at was dropped during last night's migration; rebuilding it should bring the report back under a second for the dashboard charts.";
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = runtime(probe(assistant(long)), w, h);
        assert_in_window(&rt, w, h, "long assistant content");
        assert_find_centres_in_window(&rt, w, h, "long assistant content");
        let body = element(&rt, "chat-bubble-body");
        let cap = 0.9 * (w - 32.0) + 1.0;
        assert!(
            body.rect[2] <= cap,
            "bubble width {} exceeds the 90% cap {cap} at {w}x{h}: {:?}",
            body.rect[2],
            body.rect
        );
    }
}

fn audit_rich_message() -> AiChatMessage {
    assistant(
        "The report query scans the whole orders table because the created_at index is gone; rebuild it and the planner switches back to an index scan.",
    )
    .reasoning("First I read the schema, then the explain output.")
    .questions(audit_questions())
    .memory_used(vec![memory_k3(), memory_k4(), memory_k5()])
}

fn audit_rich_probe(dark: bool) -> AuditProbe {
    let mut p = probe(audit_rich_message());
    p.last_assistant = true;
    p.dark = dark;
    p.bubble.toggle_reasoning();
    p.bubble.toggle_memory();
    p
}

#[test]
fn audit_rich_bubble_stays_inside_three_windows_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0), (800.0, 600.0)] {
        for dark in [true, false] {
            let rt = runtime(audit_rich_probe(dark), w, h);
            let theme = if dark { "dark" } else { "light" };
            assert_in_window(&rt, w, h, &format!("rich bubble {w}x{h} {theme}"));
            assert_find_centres_in_window(&rt, w, h, &format!("rich bubble {w}x{h} {theme}"));
            assert_no_sibling_overlap(&rt, &format!("rich bubble {w}x{h} {theme}"));
            assert!(has_key(&rt, "ai-memory-expanded"));
            assert!(has_key(&rt, "question-block"));
            assert!(has_key(&rt, "ai-memory-remember"));
            assert!(has_key(&rt, "chat-bubble-reasoning-text"));
        }
    }
}

#[test]
fn audit_tool_and_user_bubbles_stay_inside_the_minimum_window() {
    for dark in [true, false] {
        let mut tool = probe(AiChatMessage::new(AiChatRole::Tool, "pg_dump --help"));
        tool.dark = dark;
        tool.bubble.toggle_tool();
        let rt = runtime(tool, 960.0, 640.0);
        assert_in_window(&rt, 960.0, 640.0, "expanded tool bubble");
        assert_find_centres_in_window(&rt, 960.0, 640.0, "expanded tool bubble");
        assert_no_sibling_overlap(&rt, "expanded tool bubble");

        let mut user_p = probe(user("SELECT 1"));
        user_p.dark = dark;
        let user_rt = runtime(user_p, 960.0, 640.0);
        assert_in_window(&user_rt, 960.0, 640.0, "user bubble");
        assert_no_sibling_overlap(&user_rt, "user bubble");
    }
}

#[test]
fn audit_multi_line_content_grows_the_bubble() {
    let single = runtime(probe(assistant("one line")), 960.0, 640.0);
    let single_h = element(&single, "chat-bubble-body").rect[3];
    let multi = runtime(probe(assistant("alpha\nbeta\ngamma\ndelta")), 960.0, 640.0);
    let multi_h = element(&multi, "chat-bubble-body").rect[3];
    assert!(
        multi_h > single_h + 10.0,
        "four lines ({multi_h}pt) must be taller than one ({single_h}pt)"
    );
}
