//! `ChatBubble` — T009-S2, the second slice of T009 (`AiChatPanel`).
//!
//! Ported from `packages/frontend/src/components/ai/AiChatPanel.tsx`
//! 992–1210 (`function ChatBubble`), reusing S1's `QuestionBlock`
//! (`ai_chat_panel.rs`) for the questions block. The panel itself is a
//! later slice: this file has no `aiStore` wiring and is not mounted in
//! `app.rs`'s `Section` dispatch.
//!
//! | React source | Rune item |
//! |---|---|
//! | `AiChatMessage` (`types/ai.ts:446`) | [`AiChatMessage`] |
//! | `AiMemoryReference` (`types/ai.ts:286`) | [`AiMemoryReference`] |
//! | `ChatBubble` (992–1210) | [`ChatBubble`] + [`ChatBubbleState`] |
//!
//! # State ownership
//!
//! Like S1's `QuestionBlock`, the component is a pure render + event router:
//! the caller owns a [`ChatBubbleState`] (reasoning open, tool collapsed,
//! memory open, citation-focused item) and passes it in as `&state` each
//! frame; presses call back into the caller, which drives the state machine.
//! The pure predicates ([`ChatBubbleState::is_tool_bubble`],
//! [`ChatBubbleState::is_pulsing`], …) are unit-testable without a runtime.
//!
//! # Key convention
//!
//! Keys are React's `data-testid` values verbatim where React has one:
//! `tool-role-toggle`, `ai-memory-chip`, `ai-memory-expanded`,
//! `ai-memory-item-{reference}`, `ai-memory-remember`,
//! `chat-message-cancelled`. The bubble root (`chat-bubble`), the content
//! placeholder (`chat-bubble-content-placeholder`), the reasoning toggle /
//! text (`chat-bubble-reasoning`, `chat-bubble-reasoning-text`), the tool
//! content (`chat-bubble-tool-content`) and the thinking spinner
//! (`chat-bubble-thinking`, label at `chat-bubble-thinking-label`) have no
//! React testid; their keys are internal and documented here so the panel
//! slice can find them.
//!
//! # Deliberate boundaries
//!
//! * **Assistant content renders through `AiMessageContent` when the code
//!   wiring is set (T167).** React renders assistant content through
//!   `AiMessageContent` (`AiMessageContent.tsx`): GFM Markdown, fenced
//!   SQL blocks via `AiCodeBlock`, and `[M1]`-style memory citations. The
//!   port lives in [`super::ai_message_content`]; the bubble takes it
//!   through [`ChatBubble::code_wiring`] and renders the caller-owned
//!   code-block slot snapshot under `chat-bubble-content-ai`. An un-wired
//!   bubble keeps the pre-T167 plain-text placeholder under
//!   `chat-bubble-content-placeholder` exactly (no SQL rendering is faked
//!   for callers that have not wired the state slots). The user role is
//!   final: React renders user content as a plain `<pre>`, which is what
//!   the bubble shows for it.
//! * **Memory citations route through `on_cite_memory` (T167).** React
//!   links `[M1]` inside the rendered Markdown and the bubble's click
//!   handler expands the memory list and scrolls the item into view. The
//!   port's citation link dispatches [`ChatBubble::on_cite_memory`], whose
//!   caller drives [`ChatBubbleState::open_memory_citation`] (expand the
//!   list + mark the item selected).
//!   Rune has no DOM `scrollIntoView`/`focus()`; the selected item is
//!   expressed with `focus_selected` (observed as `item_selected` on the
//!   laid-out element) plus an accent wash fill, the same channel S1 uses
//!   for a selected option button.
//! * **Remember creates nothing.** The button only reports the message
//!   through `on_remember`; memory creation / persistence is not this
//!   slice's job and is not implemented here.
//! * **`animate-pulse` is a real loop, not a static tint.** React pulses
//!   the bubble while streaming with reasoning but no content yet; Rune
//!   renders that with `LoopSpec::opacity(2.0, 0.0, 0.5, 1.0)` — the
//!   Tailwind `pulse` keyframes (2s, opacity 1 → 0.5 → 1). The loop is
//!   observed through the layout engine's reported `opacity` / `loop_phase`
//!   on the laid-out element, not through the component's own report.
//! * **Toggle buttons carry one icon.** React's toggles show a chevron
//!   (direction) plus a semantic glyph (Wrench / Sparkles / BookOpen);
//!   kit `Button` takes a single icon, so the semantic glyph is kept and
//!   the open/closed state rides the button's `expanded` slot (a11y
//!   `expanded` + ghost pressed fill while open) and the state word
//!   (`chat.collapsed` / `chat.expanded`).
//! * **No left-only borders.** React draws the expanded reasoning text
//!   with a left rule and the memory section with a top rule; Rune has no
//!   single-side border, so both render as indented blocks without the
//!   rule. The geometry is unchanged.
//! * **Content emptiness is JS truthiness, not trimming.** React gates on
//!   `message.content` (a whitespace-only string is truthy) and on
//!   `message.reasoning` (an empty string is falsy). Rune mirrors that
//!   with `is_empty()` only — no `trim()` — so the S1-noted `trim`
//!   divergence (Rust `trim` keeps U+FEFF, JS `trim` strips it) cannot
//!   change any gate here.
//! * **S1's divergences are inherited, not relitigated.** The multi-select
//!   draft behaviour (typing free text then pressing an option replaces
//!   the draft with the single selection instead of spreading the string
//!   into a character array, as React would) lives in S1's
//!   `QuestionBlockState` and is reused verbatim through the questions
//!   wiring below.

// The panel view (a later T009 slice) is the production consumer; until it
// lands the component tests are the only callers, so the dead-code lint is
// silenced here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, Color, El, Key, KeyEvent, LoopSpec, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use super::ai_chat_panel::{AiQuestion, QuestionBlock, QuestionBlockState};
use super::ai_message_content::{AiMessageCodeWiring, AiMessageContent};
use crate::components::ai::ai_code_block_state::AiCodeBlockState;
use crate::i18n::t;
use crate::icons::{ICON_BOOK_OPEN, ICON_SPARKLES, ICON_TOOLS};
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};

/// React's `AiChatMessage` role (`types/ai.ts:446`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// React's `AiMemoryReference` (`types/ai.ts:286`): non-secret citation
/// metadata for one memory injected into a chat run. Only `reference`,
/// `subject`, `kind` and `scope` are displayed; `id` is kept because React
/// keys the list rows `${id}-${reference}`, and `score` because a later
/// slice may sort or filter on it.
#[derive(Clone, Debug, PartialEq)]
pub struct AiMemoryReference {
    pub reference: String,
    pub id: String,
    pub subject: String,
    pub kind: String,
    pub scope: String,
    pub score: f64,
}

impl AiMemoryReference {
    pub fn new(
        reference: impl Into<String>,
        id: impl Into<String>,
        subject: impl Into<String>,
        kind: impl Into<String>,
        scope: impl Into<String>,
    ) -> Self {
        Self {
            reference: reference.into(),
            id: id.into(),
            subject: subject.into(),
            kind: kind.into(),
            scope: scope.into(),
            score: 0.0,
        }
    }

    pub fn score(mut self, score: f64) -> Self {
        self.score = score;
        self
    }
}

/// React's `AiChatMessage` (`types/ai.ts:446`): one chat turn. Only the
/// fields `ChatBubble` reads are carried — `toolCalls`, `toolCallId`,
/// `status` and `createdAt` belong to the panel / store slices.
#[derive(Clone, Debug, PartialEq)]
pub struct AiChatMessage {
    pub id: Option<String>,
    pub role: AiChatRole,
    pub content: String,
    pub reasoning: Option<String>,
    pub questions: Vec<AiQuestion>,
    pub cancelled: bool,
    pub memory_used: Vec<AiMemoryReference>,
}

impl AiChatMessage {
    pub fn new(role: AiChatRole, content: impl Into<String>) -> Self {
        Self {
            id: None,
            role,
            content: content.into(),
            reasoning: None,
            questions: Vec::new(),
            cancelled: false,
            memory_used: Vec::new(),
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn reasoning(mut self, reasoning: impl Into<String>) -> Self {
        self.reasoning = Some(reasoning.into());
        self
    }

    pub fn questions(mut self, questions: Vec<AiQuestion>) -> Self {
        self.questions = questions;
        self
    }

    pub fn cancelled(mut self, cancelled: bool) -> Self {
        self.cancelled = cancelled;
        self
    }

    pub fn memory_used(mut self, memory_used: Vec<AiMemoryReference>) -> Self {
        self.memory_used = memory_used;
        self
    }
}

/// The mutable state behind a [`ChatBubble`]: which collapsible sections are
/// open, and which memory item a citation jump selected.
///
/// The caller owns one of these (in `AppState` once the panel lands; in the
/// probe state in tests) and hands `&state` to the builder each frame.
/// Behaviour mirrors `ChatBubble`'s React hooks exactly: `toolCollapsed`
/// starts `true`, the other sections start closed, and a citation jump
/// opens the memory list.
#[derive(Clone, Debug)]
pub struct ChatBubbleState {
    reasoning_open: bool,
    tool_collapsed: bool,
    memory_open: bool,
    memory_focused: Option<String>,
    /// The `AiCodeBlock` state slots of this bubble's message content,
    /// one per fenced code segment (T167). Seeded / kept in length with
    /// the message by [`ChatBubbleState::sync_code_blocks`]; the panel
    /// wiring owns the mutations the code-block events drive.
    code_blocks: Vec<AiCodeBlockState>,
}

impl Default for ChatBubbleState {
    fn default() -> Self {
        Self {
            reasoning_open: false,
            // React: `useState(true)`.
            tool_collapsed: true,
            memory_open: false,
            memory_focused: None,
            code_blocks: Vec::new(),
        }
    }
}

impl ChatBubbleState {
    /// The code-block state slots (one per fenced code segment).
    pub fn code_blocks(&self) -> &[AiCodeBlockState] {
        &self.code_blocks
    }

    /// The code-block state slots, mutable: the panel wiring routes the
    /// `AiCodeBlock` events here (T167). A missing slot (a lagging seed)
    /// makes the event a no-op, matching the conservative default the
    /// render falls back to.
    pub fn code_blocks_mut(&mut self) -> &mut [AiCodeBlockState] {
        &mut self.code_blocks
    }

    /// Brings the code-block slots in step with the message content:
    /// grows to the fenced-code segment count (seeding each slot's
    /// dialect from `sql_dialect`) and keeps existing slots otherwise —
    /// a slot's fullscreen / dialect / copy feedback belongs to its
    /// block alone (the `AiCodeBlock` docs), so slots are never reused
    /// across a re-split.
    pub fn sync_code_blocks(&mut self, content: &str, sql_dialect: &str) {
        let needed = crate::components::ai::ai_message_content::parse_message_segments(content)
            .iter()
            .filter(|segment| matches!(segment, crate::components::ai::ai_message_content::MessageSegment::Code { .. }))
            .count();
        while self.code_blocks.len() < needed {
            self.code_blocks.push(AiCodeBlockState::new(sql_dialect));
        }
        self.code_blocks.truncate(needed);
    }

    pub fn reasoning_open(&self) -> bool {
        self.reasoning_open
    }

    pub fn tool_collapsed(&self) -> bool {
        self.tool_collapsed
    }

    pub fn memory_open(&self) -> bool {
        self.memory_open
    }

    /// The reference the last citation jump selected, if any.
    pub fn memory_focused(&self) -> Option<&str> {
        self.memory_focused.as_deref()
    }

    pub fn toggle_reasoning(&mut self) {
        self.reasoning_open = !self.reasoning_open;
    }

    pub fn toggle_tool(&mut self) {
        self.tool_collapsed = !self.tool_collapsed;
    }

    pub fn toggle_memory(&mut self) {
        self.memory_open = !self.memory_open;
    }

    /// React's `handleMemoryCitationClick`, minus the DOM half: expand the
    /// memory list and mark `reference` as the focused item. React also
    /// scrolls the item into view and focuses it (`scrollIntoView` +
    /// `focus()` after a tick); Rune has neither, so the selection is the
    /// whole effect — the list renders the focused item selected, and a
    /// reference with no matching item simply selects nothing.
    pub fn open_memory_citation(&mut self, reference: &str) {
        self.memory_open = true;
        self.memory_focused = Some(reference.to_string());
    }

    /// React's `message.reasoning && …` gate: a missing or empty reasoning
    /// string shows no reasoning section.
    pub fn has_reasoning(message: &AiChatMessage) -> bool {
        message
            .reasoning
            .as_deref()
            .is_some_and(|reasoning| !reasoning.is_empty())
    }

    /// React's tool early return: a `tool` message with content renders
    /// only the collapsible toggle + content, none of the normal bubble.
    /// A tool message with empty content falls through to the normal path.
    pub fn is_tool_bubble(message: &AiChatMessage) -> bool {
        message.role == AiChatRole::Tool && !message.content.is_empty()
    }

    /// React's `animate-pulse` gate: streaming, no content yet, but
    /// reasoning is already arriving.
    pub fn is_pulsing(message: &AiChatMessage, is_streaming: bool) -> bool {
        is_streaming && message.content.is_empty() && Self::has_reasoning(message)
    }

    /// React's `hasQuestions`: a non-user message that is not streaming and
    /// carries questions.
    pub fn has_questions(message: &AiChatMessage, is_streaming: bool) -> bool {
        message.role != AiChatRole::User && !is_streaming && !message.questions.is_empty()
    }

    /// React's remember-button gate: non-user, last assistant message, not
    /// streaming, has content, and the caller supplied `onRemember`.
    pub fn show_remember(
        message: &AiChatMessage,
        is_streaming: bool,
        is_last_assistant: bool,
        has_on_remember: bool,
    ) -> bool {
        message.role != AiChatRole::User
            && is_last_assistant
            && !is_streaming
            && !message.content.is_empty()
            && has_on_remember
    }
}

/// The questions wiring a [`ChatBubble`] forwards into S1's `QuestionBlock`:
/// the block's state plus the three callbacks its builder needs. The
/// bubble renders the block only when React would — see
/// [`ChatBubbleState::has_questions`] plus `isLastAssistant` and a supplied
/// submit — and the caller drives submit through
/// [`QuestionBlockState::submit`], forwarding the formatted payload to
/// `onAnswerQuestions`.
pub struct ChatBubbleQuestions<'a, S: 'static> {
    state: &'a QuestionBlockState,
    on_toggle: Rc<dyn Fn(&mut S, &str, &str, bool)>,
    on_custom_edit: Rc<dyn for<'b, 'c> Fn(&'b mut S, &'c str) -> &'b mut TextEdit>,
    on_submit: Rc<dyn Fn(&mut S)>,
}

/// `ChatBubble` (AiChatPanel.tsx:992–1210): one chat message with its
/// reasoning section, memory chip + list, remember button, streaming
/// placeholder, cancelled marker, and — for the last assistant message —
/// S1's `QuestionBlock`. The caller owns the [`ChatBubbleState`] and wires
/// the callbacks; the bubble itself keeps no state.
pub struct ChatBubble<'a, S: 'static> {
    message: &'a AiChatMessage,
    state: &'a ChatBubbleState,
    is_streaming: bool,
    is_last_assistant: bool,
    sql_dialect: Option<String>,
    on_insert_sql: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_run_code: Option<Rc<dyn Fn(&mut S, &str, &str)>>,
    on_new_query: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_toggle_reasoning: Option<Rc<dyn Fn(&mut S)>>,
    on_toggle_tool: Option<Rc<dyn Fn(&mut S)>>,
    on_toggle_memory: Option<Rc<dyn Fn(&mut S)>>,
    on_cite_memory: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_remember: Option<Rc<dyn Fn(&mut S, &AiChatMessage)>>,
    questions: Option<ChatBubbleQuestions<'a, S>>,
    /// T167: the `AiCodeBlock` event wiring for the message content's
    /// code segments. Without it the assistant content stays the plain
    /// placeholder (the panel wiring provides it in production).
    code_wiring: Option<AiMessageCodeWiring<S>>,
    /// The host's clipboard write starter for a code segment's copy
    /// button; forwarded to `AiMessageContent`'s `on_copy_code`.
    on_copy_code: Option<Rc<dyn Fn(&mut S, String, u64)>>,
    /// A markdown link press in the content; React's browser navigates
    /// the `href`, the host decides here.
    on_open_link: Option<Rc<dyn Fn(&mut S, String)>>,
}

impl<'a, S: 'static> ChatBubble<'a, S> {
    pub fn new(message: &'a AiChatMessage, state: &'a ChatBubbleState) -> Self {
        Self {
            message,
            state,
            is_streaming: false,
            is_last_assistant: false,
            sql_dialect: None,
            on_insert_sql: None,
            on_run_code: None,
            on_new_query: None,
            on_toggle_reasoning: None,
            on_toggle_tool: None,
            on_toggle_memory: None,
            on_cite_memory: None,
            on_remember: None,
            questions: None,
            code_wiring: None,
            on_copy_code: None,
            on_open_link: None,
        }
    }

    pub fn streaming(mut self, is_streaming: bool) -> Self {
        self.is_streaming = is_streaming;
        self
    }

    pub fn last_assistant(mut self, is_last_assistant: bool) -> Self {
        self.is_last_assistant = is_last_assistant;
        self
    }

    /// The SQL dialect for `AiMessageContent`'s code blocks (T169). Stored
    /// only until that slice lands.
    pub fn sql_dialect(mut self, sql_dialect: impl Into<String>) -> Self {
        self.sql_dialect = Some(sql_dialect.into());
        self
    }

    /// `AiMessageContent`'s `onInsertSql` (T169). Stored only; nothing in
    /// this slice renders SQL or calls it.
    pub fn on_insert_sql(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_insert_sql = Some(Rc::new(f));
        self
    }

    /// `AiMessageContent`'s `onRunCode` (T169). Stored only.
    pub fn on_run_code(mut self, f: impl Fn(&mut S, &str, &str) + 'static) -> Self {
        self.on_run_code = Some(Rc::new(f));
        self
    }

    /// `AiMessageContent`'s `onNewQuery` (T169). Stored only.
    pub fn on_new_query(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_new_query = Some(Rc::new(f));
        self
    }

    /// Called when the reasoning toggle is pressed; the caller drives
    /// [`ChatBubbleState::toggle_reasoning`].
    pub fn on_toggle_reasoning(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_reasoning = Some(Rc::new(f));
        self
    }

    /// Called when the tool-role toggle is pressed; the caller drives
    /// [`ChatBubbleState::toggle_tool`].
    pub fn on_toggle_tool(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_tool = Some(Rc::new(f));
        self
    }

    /// Called when the memory chip is pressed; the caller drives
    /// [`ChatBubbleState::toggle_memory`].
    pub fn on_toggle_memory(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_memory = Some(Rc::new(f));
        self
    }

    /// Called with a memory `reference` when a `#memory-<reference>`
    /// citation inside the content is activated; the caller drives
    /// [`ChatBubbleState::open_memory_citation`]. No element calls this
    /// yet — the placeholder content has no links (see the module docs).
    pub fn on_cite_memory(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_cite_memory = Some(Rc::new(f));
        self
    }

    /// Called with the message when the remember button is pressed; the
    /// caller owns memory creation / persistence.
    pub fn on_remember(mut self, f: impl Fn(&mut S, &AiChatMessage) + 'static) -> Self {
        self.on_remember = Some(Rc::new(f));
        self
    }

    /// T167: the `AiCodeBlock` event wiring for the content's code
    /// segments (`AiMessageCodeWiring`). With this set, the assistant
    /// content renders through `AiMessageContent` (the placeholder goes
    /// away); without it the placeholder stays, so un-wired callers
    /// keep the exact pre-T167 rendering.
    pub fn code_wiring(mut self, wiring: AiMessageCodeWiring<S>) -> Self {
        self.code_wiring = Some(wiring);
        self
    }

    /// The host's clipboard write starter for a code segment's copy
    /// button (React's `navigator.clipboard`); without it the copy button
    /// renders but does nothing.
    pub fn on_copy_code(
        mut self,
        f: impl Fn(&mut S, String, u64) + 'static,
    ) -> Self {
        self.on_copy_code = Some(Rc::new(f));
        self
    }

    /// A markdown link press in the content (React's browser navigates
    /// the `href`; the host opens it).
    pub fn on_open_link(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_open_link = Some(Rc::new(f));
        self
    }

    /// Wire S1's `QuestionBlock` under the bubble. The caller supplies the
    /// block's state and the three callbacks its builder needs; submit
    /// should drive [`QuestionBlockState::submit`] and forward the payload
    /// to `onAnswerQuestions`.
    pub fn with_questions(
        mut self,
        state: &'a QuestionBlockState,
        on_toggle: impl Fn(&mut S, &str, &str, bool) + 'static,
        on_custom_edit: impl for<'b, 'c> Fn(&'b mut S, &'c str) -> &'b mut TextEdit + 'static,
        on_submit: impl Fn(&mut S) + 'static,
    ) -> Self {
        self.questions = Some(ChatBubbleQuestions {
            state,
            on_toggle: Rc::new(on_toggle),
            on_custom_edit: Rc::new(on_custom_edit),
            on_submit: Rc::new(on_submit),
        });
        self
    }

    /// Wire Space (kit `on_press`) and Enter on a built button element, the
    /// way S1's chip and option buttons do — kit buttons fire on Space,
    /// React's `<button>` fires on Enter too.
    fn wire_button(el: El, action: &Option<Rc<dyn Fn(&mut S)>>) -> El {
        let Some(action) = action else {
            return el;
        };
        let press = Rc::clone(action);
        let keyed = Rc::clone(action);
        el.on_press(move |state: &mut S| press(state)).on_key(
            move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                if event.key == Key::Enter {
                    keyed(state);
                    true
                } else {
                    false
                }
            },
        )
    }

    pub fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        if ChatBubbleState::is_tool_bubble(self.message) {
            return self.build_tool(dark);
        }
        self.build_message(dark, layer)
    }

    /// React's tool early return: the toggle plus, when expanded, the tool
    /// output in a muted box.
    fn build_tool(self, dark: bool) -> El {
        let k = kit(dark);
        let collapsed = self.state.tool_collapsed;
        let label = format!(
            "{} {}",
            t("chat.toolRole"),
            t(if collapsed {
                "chat.collapsed"
            } else {
                "chat.expanded"
            })
        );
        let label_text = label.clone();
        let toggle = Self::wire_button(
            Button::new(label)
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .icon(&ICON_TOOLS)
                .expanded(!collapsed)
                .label_with(move |font, _ink| text(label_text).font(font).color(k.c.text_muted))
                .build(k)
                .key("tool-role-toggle"),
            &self.on_toggle_tool,
        );
        let mut root = col()
            .key("chat-bubble")
            .w_full()
            .min_w(0.0)
            .gap(4.0)
            .child(toggle);
        if !collapsed {
            root = root.child(
                col()
                    .key("chat-bubble-tool-content")
                    .w_frac(0.9)
                    .min_w(0.0)
                    .ml(16.0)
                    .gap(4.0)
                    .px(12.0)
                    .py(8.0)
                    .radius(k.radius.md)
                    .border(1.0, k.c.border_base)
                    .fill(k.c.layer_01)
                    .child(
                        text(self.message.content.clone())
                            .key("chat-bubble-tool-content-text")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted)
                            .wrap(),
                    ),
            );
        }
        root
    }

    fn build_message(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let message = self.message;
        let is_user = message.role == AiChatRole::User;
        let pulsing = ChatBubbleState::is_pulsing(message, self.is_streaming);

        // The bubble body: hug its content inside a 90%-wide slot (React's
        // `max-w-[90%]`), right-aligned for the user role.
        let mut body = col()
            .key("chat-bubble-body")
            .min_w(0.0)
            .gap(8.0)
            .px(12.0)
            .py(8.0)
            .radius(k.radius.md)
            .fill(if is_user {
                // React: `bg-accent/20`.
                k.alpha(k.c.bg_accent, 0.20)
            } else {
                // React: `bg-surface-alt`.
                k.c.layer_01
            });
        if is_user {
            body = body.self_end();
        } else {
            body = body.self_start();
        }
        if pulsing {
            // Tailwind `animate-pulse`: 2s, opacity 1 → 0.5 → 1.
            body = body.looping(LoopSpec::opacity(2.0, 0.0, 0.5, 1.0));
        }

        if ChatBubbleState::has_reasoning(message) {
            body = body.child(self.reasoning_section(dark));
        }

        if !message.content.is_empty() {
            body = body.child(self.content_section(dark, layer));
        }

        if !is_user && !message.memory_used.is_empty() {
            body = body.child(self.memory_section(dark));
        }

        if ChatBubbleState::show_remember(
            message,
            self.is_streaming,
            self.is_last_assistant,
            self.on_remember.is_some(),
        ) {
            body = body.child(self.remember_button(dark));
        }

        // Streaming with reasoning but no content yet: the thinking spinner.
        if message.content.is_empty()
            && self.is_streaming
            && ChatBubbleState::has_reasoning(message)
        {
            body = body.child(
                Spinner::new("chat-bubble-thinking")
                    .size(SpinnerSize::Sm)
                    .tone(SpinnerTone::Muted)
                    .label(t("chat.thinking"))
                    .build(dark),
            );
        }

        if message.cancelled {
            body = body.child(
                text(t("chat.cancelled"))
                    .key("chat-message-cancelled")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        }

        let mut row_el = row().key("chat-bubble-row").w_full().min_w(0.0);
        if is_user {
            row_el = row_el.justify_end();
        }
        row_el = row_el.child(
            col()
                .key("chat-bubble-slot")
                .w_frac(0.9)
                .min_w(0.0)
                .child(body),
        );

        let mut root = col().key("chat-bubble").w_full().min_w(0.0).gap(8.0);
        root = root.child(row_el);

        if ChatBubbleState::has_questions(message, self.is_streaming) && self.is_last_assistant {
            if let Some(questions) = self.questions {
                let block = QuestionBlock::new(message.questions.clone(), questions.state)
                    .on_toggle(move |state: &mut S, qid, oid, multi| {
                        (questions.on_toggle)(state, qid, oid, multi);
                    })
                    .on_custom_edit(move |state: &mut S, qid: &str| {
                        (questions.on_custom_edit)(state, qid)
                    })
                    .on_submit(move |state: &mut S| {
                        (questions.on_submit)(state);
                    })
                    .build(dark);
                root = root.child(block);
            }
        }
        root
    }

    /// The reasoning toggle + expanded reasoning text.
    fn reasoning_section(&self, dark: bool) -> El {
        let k = kit(dark);
        let open = self.state.reasoning_open;
        let label = t("chat.reasoning").to_string();
        let toggle = Self::wire_button(
            Button::new(label.clone())
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .icon(&ICON_SPARKLES)
                .expanded(open)
                .label_with(move |font, _ink| text(label).font(font).color(k.c.text_muted))
                .build(k)
                .key("chat-bubble-reasoning"),
            &self.on_toggle_reasoning,
        );
        let mut section = col()
            .key("chat-bubble-reasoning-section")
            .min_w(0.0)
            .gap(4.0)
            .child(toggle);
        if open {
            // No single-side border in Rune: the text is indented instead
            // of carrying React's left rule (see the module docs).
            let reasoning = self.message.reasoning.clone().unwrap_or_default();
            section = section.child(
                col()
                    .key("chat-bubble-reasoning-text-wrap")
                    .min_w(0.0)
                    .ml(4.0)
                    .pl(8.0)
                    .child(
                        text(reasoning)
                            .key("chat-bubble-reasoning-text")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted)
                            .wrap(),
                    ),
            );
        }
        section
    }

    /// The message body. The user role is final (React renders a plain
    /// `<pre>`); the assistant role renders through `AiMessageContent`
    /// when the T167 code wiring is set — text segments as structured GFM
    /// blocks, code segments as `AiCodeBlock`s with the wiring's event
    /// routing and this state's slot snapshot — and stays the plain-text
    /// placeholder for un-wired callers (the exact pre-T167 rendering).
    fn content_section(&self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let is_user = self.message.role == AiChatRole::User;
        let content = text(self.message.content.clone())
            .key("chat-bubble-content")
            .font(k.ty.text_xsmall)
            .color(if is_user {
                k.c.text_base
            } else {
                k.c.text_muted
            })
            .wrap();
        if is_user {
            return content;
        }
        if let Some(wiring) = self.code_wiring.clone() {
            let mut builder = AiMessageContent::<S>::new(self.message.content.clone(), wiring)
                .is_streaming(self.is_streaming)
                .key("chat-bubble-content-ai");
            if let Some(dialect) = self.sql_dialect.clone() {
                builder = builder.sql_dialect(dialect);
            }
            let references: Vec<String> = self
                .message
                .memory_used
                .iter()
                .map(|memory| memory.reference.clone())
                .collect();
            if !references.is_empty() {
                builder = builder.memory_references(references);
            }
            if let Some(on_insert_sql) = self.on_insert_sql.clone() {
                builder =
                    builder.on_insert_sql(move |s: &mut S, code: String| on_insert_sql(s, &code));
            }
            if let Some(on_run_code) = self.on_run_code.clone() {
                builder = builder.on_run_code(move |s: &mut S, code: String, language: String| {
                    on_run_code(s, &code, &language);
                });
            }
            if let Some(on_new_query) = self.on_new_query.clone() {
                builder =
                    builder.on_new_query(move |s: &mut S, code: String| on_new_query(s, &code));
            }
            if let Some(on_copy_code) = self.on_copy_code.clone() {
                builder =
                    builder.on_copy_code(move |s: &mut S, code: String, request: u64| {
                        on_copy_code(s, code, request);
                    });
            }
            if let Some(on_cite_memory) = self.on_cite_memory.clone() {
                builder =
                    builder.on_cite_memory(move |s: &mut S, reference: String| {
                        on_cite_memory(s, &reference);
                    });
            }
            if let Some(on_open_link) = self.on_open_link.clone() {
                builder =
                    builder.on_open_link(move |s: &mut S, url: String| on_open_link(s, url));
            }
            return builder.build(self.state.code_blocks(), dark, layer);
        }
        col()
            .key("chat-bubble-content-placeholder")
            .min_w(0.0)
            .child(content)
    }

    /// The memory chip + expanded memory list. The chip label formats
    /// `chat.memory.used` (`已使用 {count} 条记忆`) — `t()` leaves the
    /// `{count}` placeholder in place, so the caller formats it, exactly
    /// as React's `t('…', { count })` does.
    fn memory_section(&self, dark: bool) -> El {
        let k = kit(dark);
        let open = self.state.memory_open;
        let count = self.message.memory_used.len();
        let label = t("chat.memory.used").replacen("{count}", &count.to_string(), 1);
        let label_text = label.clone();
        let chip = Self::wire_button(
            Button::new(label)
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .icon(&ICON_BOOK_OPEN)
                .expanded(open)
                .label_with(move |font, _ink| text(label_text).font(font).color(k.c.text_muted))
                .build(k)
                .key("ai-memory-chip"),
            &self.on_toggle_memory,
        );
        // No single-side top border in Rune (see the module docs); the
        // section is spaced from the content above it by the body's gap.
        let mut section = col()
            .key("chat-bubble-memory-section")
            .min_w(0.0)
            .gap(4.0)
            .child(chip);
        if open {
            let mut list = col().key("ai-memory-expanded").min_w(0.0).gap(2.0);
            for memory in &self.message.memory_used {
                let focused =
                    self.state.memory_focused.as_deref() == Some(memory.reference.as_str());
                let reference = format!("[{}]", memory.reference);
                let detail = format!("{} · {}", memory.kind, memory.scope);
                list = list.child(
                    row()
                        .key(format!("ai-memory-item-{}", memory.reference))
                        .center_y()
                        .gap(4.0)
                        .min_w(0.0)
                        .px(6.0)
                        .py(4.0)
                        .radius(k.radius.sm)
                        .focus_selected(focused)
                        .fill(if focused {
                            k.alpha(k.c.text_accent, 0.15)
                        } else {
                            Color::TRANSPARENT
                        })
                        .child(
                            text(reference)
                                .key(format!("ai-memory-item-{}-reference", memory.reference))
                                .font(k.ty.mono)
                                .color(k.c.text_muted),
                        )
                        .child(
                            text(memory.subject.clone())
                                .key(format!("ai-memory-item-{}-subject", memory.reference))
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_base)
                                .wrap()
                                .min_w(0.0)
                                .shrink(),
                        )
                        .child(
                            text(detail)
                                .key(format!("ai-memory-item-{}-detail", memory.reference))
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_faint),
                        ),
                );
            }
            section = section.child(list);
        }
        section
    }

    /// The remember button. Rendered only when
    /// [`ChatBubbleState::show_remember`] holds; pressing it reports the
    /// message through `on_remember` — creation / persistence is the
    /// caller's job.
    fn remember_button(&self, dark: bool) -> El {
        let k = kit(dark);
        let label = t("chat.rememberThis").to_string();
        let label_text = label.clone();
        let message = self.message.clone();
        let message_keyed = self.message.clone();
        // Unreachable through `build_message` — `show_remember` already
        // requires the callback — but keep the fallback unkeyed so a
        // future caller can never observe a phantom `ai-memory-remember`.
        let Some(on_remember) = &self.on_remember else {
            return col().min_w(0.0);
        };
        let press = Rc::clone(on_remember);
        let keyed = Rc::clone(on_remember);
        Button::new(label)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .icon(&ICON_BOOK_OPEN)
            .label_with(move |font, _ink| text(label_text).font(font).color(k.c.text_muted))
            .build(k)
            .key("ai-memory-remember")
            .on_press(move |state: &mut S| press(state, &message))
            .on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                if event.key == Key::Enter {
                    keyed(state, &message_keyed);
                    true
                } else {
                    false
                }
            })
    }
}
