//! `AiChatPanel` message list — T009-S4, the fourth slice of T009.
//!
//! Ported from `packages/frontend/src/components/ai/AiChatPanel.tsx`
//! 642–790: the memory toasts, the scrollable message list with its
//! streaming placeholder, the scroll anchor, and the jump-to-bottom pill.
//! Messages render through S2's [`ChatBubble`]; the header (S3), the
//! input area and the panel shell are other slices. This file has no
//! `aiStore` wiring and is not mounted in `app.rs`'s `Section` dispatch.
//!
//! | React source | Rune item |
//! |---|---|
//! | memory toasts (642–680) | [`AiChatMessageList::toasts`] |
//! | message list + empty state (682–756) | [`AiChatMessageList::message_viewport`] |
//! | streaming bubble (727–740) | [`AiChatMessageListState::streaming_message`] |
//! | streaming thinking row (741–755) | [`AiChatMessageList::thinking_row`] |
//! | `useAutoScroll` (`hooks/useAutoScroll.ts`) | [`AiChatMessageListState`] (`on_scroll`, `jump_to_bottom`, `note_new_messages`) |
//! | scroll anchor `messages-end` (758) | `messages-end` element |
//! | jump-to-bottom pill (759–775) | [`AiChatMessageList::jump_pill`] |
//! | `formatMcpToolDisplayName` (88–91) | S1's [`format_mcp_tool_display_name`] |
//!
//! # State ownership
//!
//! Like S1–S3, the component is a pure render + event router: the caller
//! owns an [`AiChatMessageListState`] and passes it in as `&state` each
//! frame; presses call back into the caller, which drives the state
//! machine. Per-message [`ChatBubbleState`] and [`QuestionBlockState`]
//! live in parallel vectors on the list state, resized by
//! [`AiChatMessageListState::set_messages`]; the panel slice owns them
//! once it lands and seeds them the same way.
//!
//! # View-model boundary (aiStore slice T088 not done)
//!
//! * Messages, streaming fields, the session memory, both notices and the
//!   scroll position are plain fields on [`AiChatMessageListState`],
//!   seeded through its `set_*` methods; the panel slice wires them to
//!   the real store once T088 lands.
//! * `onRemember` is only reported through `on_remember` —
//!   `handleRemember`'s memory creation, its `chat.memoryDisabled` /
//!   `chat.remembered` / `chat.rememberFailed` notices and the 4s
//!   auto-clear (`window.setTimeout(…, 4000)`) live on the panel slice,
//!   which owns the timer (a `Tx::wake_after` like `ui/copyable.rs`).
//!   This slice renders whatever `remember_notice` holds, verbatim.
//! * The candidate notice is only the store's
//!   `{ requestId, count }` count (`stores/ai/types.ts:80`); dismissing
//!   only reports through `on_dismiss_memory_candidate` — the store's
//!   `dismissMemoryCandidateNotice` is the caller's job.
//! * Assistant message content is still S2's plain-text placeholder;
//!   `onInsertSql` / `onRunCode` / `onNewQuery` and `sqlDialect` are
//!   accepted and passed through to each bubble, stored only until T169.
//! * `sendMessage` (the questions submit) is only reported through
//!   `on_submit_questions` with the message index; the caller drives
//!   `QuestionBlockState::submit` and forwards the formatted payload.
//!   (React calls that prop `onAnswerQuestions` on the bubble; the list
//!   forwards the submit, it does not own the send.)
//!
//! # Key convention
//!
//! Keys are React's `data-testid` values verbatim where React has one:
//! `ai-memory-candidate-toast`, `ai-memory-remember-toast`,
//! `messages-end`, `jump-to-bottom`. Internal keys (no React testid)
//! are documented here so the panel slice can find them:
//! `ai-chat-message-list` (root), `ai-chat-message-list-toasts`,
//! `ai-memory-candidate-dismiss`, `ai-memory-remember-dismiss`,
//! `ai-chat-message-list-viewport` (the scroll viewport —
//! `Tx::scroll_to_bottom` targets this key),
//! `ai-chat-empty`, `ai-chat-streaming-thinking`,
//! `ai-chat-streaming-thinking-label`,
//! `ai-chat-jump-to-bottom-wrap`, `jump-to-bottom-count`, and one
//! `ai-chat-message-{i}` wrapper row per message (React keys bubbles by
//! `msg.id ?? i`; the wrapper carries the index so the panel slice can
//! locate a bubble without knowing its id).
//!
//! # Deliberate boundaries
//!
//! * **The unread pill is unreachable in React today, and stays that
//!   way here.** `useAutoScroll` initialises `unreadCount` to 0 and only
//!   ever resets it to 0 — nothing in the hook increments it, and
//!   `AiChatPanel` never calls the `useTrackUnread` helper that would.
//!   So `!atBottom && unreadCount > 0` is a dead gate in React as
//!   shipped. The VM mirrors it exactly: `unread_count` starts 0 and
//!   only [`AiChatMessageListState::note_new_messages`] (the
//!   `useTrackUnread` seam, for the panel slice to call when the store
//!   reports arrivals) increments it, only while away from the bottom.
//!   No increment is invented inside this slice.
//! * **`msg.memoryUsed: []` vs absent cannot be told apart.** React
//!   gates the session-memory inheritance on `msg.memoryUsed` being
//!   present — an empty array is truthy in JS, so a message that
//!   *carries* an empty list does not inherit. S2's `AiChatMessage`
//!   stores `memory_used: Vec<_>` with no absent/empty distinction, so
//!   this slice inherits whenever the list is empty. If a caller needs
//!   the distinction, `memory_used` must become `Option<Vec<_>>` — a
//!   panel-slice decision, not a silent change here.
//! * **The pill is stacked, not absolutely positioned.** React floats
//!   it `absolute bottom-24 left-1/2` over the list; Rune has no
//!   absolute overlay inside a column, so the pill renders as a centred
//!   row directly under the viewport. The panel slice may hoist it into
//!   the `layer` when it composes the panel.
//! * **Auto-scroll rides `stick_bottom`, not the `useEffect`.** React
//!   re-scrolls on every messages / `streamContent` change while
//!   `atBottom` (`requestAnimationFrame(jumpToBottom)`); the viewport's
//!   `ScrollSpec { stick_bottom: true }` is the same behaviour — growth
//!   keeps the bottom in view while pinned — without a per-frame effect.
//!   The 120pt threshold is `useAutoScroll`'s default, kept verbatim.
//! * **No session, no empty state.** React's `chatSession?.messages`
//!   is `undefined` before `initChat`, so the welcome line only renders
//!   once a session exists; [`AiChatMessageListState::has_session`]
//!   (default `true`) carries that `?.`.

// The panel view (a later T009 slice) is the production consumer; until it
// lands the component tests are the only callers, so the dead-code lint is
// silenced here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, Color, El, Key, KeyEvent, ScrollSpec, TextEdit, Tx};
use rune_kit::{IconButton, IconButtonVariant};

use super::ai_chat_message_list_state::AiChatMessageListState;
use super::chat_bubble::{AiChatMessage, AiChatRole, ChatBubble, ChatBubbleState};
use crate::i18n::t;
use crate::icons::{ICON_ARROW_DOWN, ICON_BOOK_OPEN, ICON_X};
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};

/// `AiChatPanel`'s message list (`AiChatPanel.tsx:642–790`): the memory
/// toasts, the scrollable message list with its streaming placeholder,
/// the scroll anchor, and the jump-to-bottom pill. The caller owns the
/// [`AiChatMessageListState`] and wires the callbacks; the list itself
/// keeps no state.
pub(crate) struct AiChatMessageList<'a, S: 'static> {
    state: &'a AiChatMessageListState,
    sql_dialect: Option<String>,
    on_insert_sql: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_run_code: Option<Rc<dyn Fn(&mut S, &str, &str)>>,
    on_new_query: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_toggle_reasoning: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_toggle_tool: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_toggle_memory: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_cite_memory: Option<Rc<dyn Fn(&mut S, usize, &str)>>,
    on_remember: Option<Rc<dyn Fn(&mut S, &AiChatMessage)>>,
    on_toggle_question: Option<Rc<dyn Fn(&mut S, usize, &str, &str, bool)>>,
    on_custom_edit: Option<Rc<dyn for<'b, 'c> Fn(&'b mut S, usize, &'c str) -> &'b mut TextEdit>>,
    on_submit_questions: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_dismiss_memory_candidate: Option<Rc<dyn Fn(&mut S)>>,
    on_dismiss_remember_notice: Option<Rc<dyn Fn(&mut S)>>,
    on_jump_to_bottom: Option<Rc<dyn Fn(&mut S, &mut Tx)>>,
    on_at_bottom: Option<Rc<dyn Fn(&mut S, bool)>>,
}

impl<'a, S: 'static> AiChatMessageList<'a, S> {
    pub(crate) fn new(state: &'a AiChatMessageListState) -> Self {
        Self {
            state,
            sql_dialect: None,
            on_insert_sql: None,
            on_run_code: None,
            on_new_query: None,
            on_toggle_reasoning: None,
            on_toggle_tool: None,
            on_toggle_memory: None,
            on_cite_memory: None,
            on_remember: None,
            on_toggle_question: None,
            on_custom_edit: None,
            on_submit_questions: None,
            on_dismiss_memory_candidate: None,
            on_dismiss_remember_notice: None,
            on_jump_to_bottom: None,
            on_at_bottom: None,
        }
    }

    /// The SQL dialect for `AiMessageContent`'s code blocks (T169).
    /// Stored only until that slice lands.
    pub(crate) fn sql_dialect(mut self, sql_dialect: impl Into<String>) -> Self {
        self.sql_dialect = Some(sql_dialect.into());
        self
    }

    /// `AiMessageContent`'s `onInsertSql` (T169). Stored only.
    pub(crate) fn on_insert_sql(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_insert_sql = Some(Rc::new(f));
        self
    }

    /// `AiMessageContent`'s `onRunCode` (T169). Stored only.
    pub(crate) fn on_run_code(mut self, f: impl Fn(&mut S, &str, &str) + 'static) -> Self {
        self.on_run_code = Some(Rc::new(f));
        self
    }

    /// `AiMessageContent`'s `onNewQuery` (T169). Stored only.
    pub(crate) fn on_new_query(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_new_query = Some(Rc::new(f));
        self
    }

    /// Called with the message index when a bubble's reasoning toggle is
    /// pressed; the caller drives
    /// [`AiChatMessageListState::bubble_state_mut`] +
    /// `ChatBubbleState::toggle_reasoning`.
    pub(crate) fn on_toggle_reasoning(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_toggle_reasoning = Some(Rc::new(f));
        self
    }

    /// Called with the message index when a bubble's tool toggle is
    /// pressed; the caller drives `ChatBubbleState::toggle_tool`.
    pub(crate) fn on_toggle_tool(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_toggle_tool = Some(Rc::new(f));
        self
    }

    /// Called with the message index when a bubble's memory chip is
    /// pressed; the caller drives `ChatBubbleState::toggle_memory`.
    pub(crate) fn on_toggle_memory(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_toggle_memory = Some(Rc::new(f));
        self
    }

    /// Called with the message index and reference when a
    /// `#memory-<reference>` citation is activated; the caller drives
    /// `ChatBubbleState::open_memory_citation`. No element calls this
    /// yet — the placeholder content has no links (see S2).
    pub(crate) fn on_cite_memory(mut self, f: impl Fn(&mut S, usize, &str) + 'static) -> Self {
        self.on_cite_memory = Some(Rc::new(f));
        self
    }

    /// Called with the message when a bubble's remember button is
    /// pressed; the caller owns `handleRemember`.
    pub(crate) fn on_remember(mut self, f: impl Fn(&mut S, &AiChatMessage) + 'static) -> Self {
        self.on_remember = Some(Rc::new(f));
        self
    }

    /// The questions block's option toggle, with the message index; the
    /// caller drives `QuestionBlockState::toggle` on that message's state.
    pub(crate) fn on_toggle_question(
        mut self,
        f: impl Fn(&mut S, usize, &str, &str, bool) + 'static,
    ) -> Self {
        self.on_toggle_question = Some(Rc::new(f));
        self
    }

    /// The questions block's custom-text edit accessor, with the message
    /// index; the caller returns that message's `QuestionBlockState`
    /// draft edit.
    pub(crate) fn on_custom_edit(
        mut self,
        f: impl for<'b, 'c> Fn(&'b mut S, usize, &'c str) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_custom_edit = Some(Rc::new(f));
        self
    }

    /// The questions block's submit, with the message index; the caller
    /// drives `QuestionBlockState::submit` and forwards the payload to
    /// `sendMessage` (React's `onAnswerQuestions` on the bubble).
    pub(crate) fn on_submit_questions(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_submit_questions = Some(Rc::new(f));
        self
    }

    /// Called when the memory-candidate toast's dismiss is pressed; the
    /// caller drives the store's `dismissMemoryCandidateNotice` (here:
    /// `AiChatMessageListState::dismiss_memory_candidate`).
    pub(crate) fn on_dismiss_memory_candidate(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_dismiss_memory_candidate = Some(Rc::new(f));
        self
    }

    /// Called when the remember toast's dismiss is pressed; the caller
    /// drives `AiChatMessageListState::dismiss_remember_notice`.
    pub(crate) fn on_dismiss_remember_notice(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_dismiss_remember_notice = Some(Rc::new(f));
        self
    }

    /// Called when the jump-to-bottom pill is pressed; the caller drives
    /// [`AiChatMessageListState::jump_to_bottom`] and issues
    /// `tx.scroll_to_bottom("ai-chat-message-list-viewport")`.
    pub(crate) fn on_jump_to_bottom(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.on_jump_to_bottom = Some(Rc::new(f));
        self
    }

    /// Called with the viewport's at-bottom report; the caller drives
    /// [`AiChatMessageListState::set_at_bottom`].
    pub(crate) fn on_at_bottom(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_at_bottom = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let state = self.state;
        let mut root = col()
            .key("ai-chat-message-list")
            .w_full()
            .min_w(0.0)
            .grow(1.0)
            .min_h(0.0);
        if state.memory_candidate_count().is_some() || state.remember_notice().is_some() {
            root = root.child(self.toasts(dark));
        }
        root = root.child(self.message_viewport(dark));
        if state.show_jump_to_bottom() {
            root = root.child(self.jump_pill(dark));
        }
        root
    }

    /// The memory toasts (`AiChatPanel.tsx:642–680`): the candidate toast
    /// above the remember toast, both with a `chat.dismiss` dismiss
    /// button. React's `space-y-1 px-3 pt-2` on the wrapping div.
    fn toasts(&self, dark: bool) -> El {
        let k = kit(dark);
        let state = self.state;
        let mut toasts = col()
            .key("ai-chat-message-list-toasts")
            .w_full()
            .min_w(0.0)
            .gap(4.0)
            .px(12.0)
            .pt(8.0);
        if let Some(count) = state.memory_candidate_count() {
            let message = t("chat.memoryCandidates").replacen("{count}", &count.to_string(), 1);
            toasts = toasts.child(self.toast_row(
                dark,
                "ai-memory-candidate-toast",
                "ai-memory-candidate-dismiss",
                message,
                k.alpha(k.c.text_accent, 0.05),
                k.alpha(k.c.text_accent, 0.30),
                &self.on_dismiss_memory_candidate,
            ));
        }
        if let Some(notice) = state.remember_notice() {
            toasts = toasts.child(self.toast_row(
                dark,
                "ai-memory-remember-toast",
                "ai-memory-remember-dismiss",
                notice.to_string(),
                k.c.layer_01,
                k.c.border_base,
                &self.on_dismiss_remember_notice,
            ));
        }
        toasts
    }

    /// One toast row: a BookOpen glyph, the text, and the dismiss button.
    /// React's `text-[11px]` renders at the kit's 12pt `text_xsmall`
    /// (the kit type scale has no 11pt step, as in S3).
    fn toast_row(
        &self,
        dark: bool,
        key: &str,
        dismiss_key: &str,
        message: String,
        fill: Color,
        border: Color,
        on_dismiss: &Option<Rc<dyn Fn(&mut S)>>,
    ) -> El {
        let k = kit(dark);
        let dismiss_label = t("chat.dismiss").to_string();
        let dismiss = wire_button(
            IconButton::new(&ICON_X)
                .variant(IconButtonVariant::Ghost)
                .control_size(rune_kit::ControlSize::Small)
                .label(dismiss_label.clone())
                .help(dismiss_label)
                .build(k)
                .key(dismiss_key.to_string()),
            on_dismiss,
        );
        row()
            .key(key.to_string())
            .w_full()
            .min_w(0.0)
            .center_y()
            .gap(8.0)
            .px(8.0)
            .py(6.0)
            .radius(k.radius.sm)
            .border(1.0, border)
            .fill(fill)
            .child(
                rune::icon(&ICON_BOOK_OPEN, 12.0)
                    .color(k.c.text_accent)
                    .key(format!("{key}-icon")),
            )
            .child(
                text(message)
                    .key(format!("{key}-text"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .wrap()
                    .min_w(0.0)
                    .shrink(),
            )
            .child(dismiss)
    }

    /// The scrollable message list (`AiChatPanel.tsx:682–758`): the empty
    /// state, one [`ChatBubble`] per message, the streaming bubble or
    /// thinking row, and the `messages-end` anchor. `stick_bottom`
    /// carries React's auto-scroll-while-pinned effect (see module docs).
    fn message_viewport(&self, dark: bool) -> El {
        let k = kit(dark);
        let state = self.state;
        let mut list = col()
            .key("ai-chat-message-list-viewport")
            .w_full()
            .min_w(0.0)
            .grow(1.0)
            .min_h(0.0)
            .px(12.0)
            .py(8.0)
            .gap(8.0)
            .scroll(ScrollSpec { stick_bottom: true });
        if let Some(on_at_bottom) = &self.on_at_bottom {
            let report = Rc::clone(on_at_bottom);
            list = list.on_at_bottom(move |state: &mut S, at: bool| report(state, at));
        }
        if state.show_empty() {
            list = list.child(
                text(t("chat.welcome"))
                    .key("ai-chat-empty")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .wrap(),
            );
        }
        for index in 0..state.messages().len() {
            list = list.child(self.message_row(dark, index));
        }
        if state.show_streaming_bubble() {
            list = list.child(self.streaming_bubble(dark));
        } else if state.show_thinking_row() {
            list = list.child(self.thinking_row(dark));
        }
        // React's scroll anchor: an empty div the panel scrolls into view.
        list.child(rune::el().key("messages-end"))
    }

    /// One message row: the bubble for the effective message, wrapped so
    /// the panel slice can locate it by index. `isLastAssistant` is
    /// `role == assistant && last && !streaming` (`AiChatPanel.tsx:719–723`).
    fn message_row(&self, dark: bool, index: usize) -> El {
        let state = self.state;
        let Some(message) = state.effective_message(index) else {
            return col().min_w(0.0);
        };
        let is_last_assistant = message.role == AiChatRole::Assistant
            && index + 1 == state.messages().len()
            && !state.is_streaming();
        let bubble_state = state.bubble_state(index);
        let mut bubble = ChatBubble::new(&message, bubble_state).last_assistant(is_last_assistant);
        if let Some(dialect) = &self.sql_dialect {
            bubble = bubble.sql_dialect(dialect.clone());
        }
        if let Some(on_insert_sql) = &self.on_insert_sql {
            let f = Rc::clone(on_insert_sql);
            bubble = bubble.on_insert_sql(move |state: &mut S, sql: &str| f(state, sql));
        }
        if let Some(on_run_code) = &self.on_run_code {
            let f = Rc::clone(on_run_code);
            bubble = bubble
                .on_run_code(move |state: &mut S, sql: &str, dialect: &str| f(state, sql, dialect));
        }
        if let Some(on_new_query) = &self.on_new_query {
            let f = Rc::clone(on_new_query);
            bubble = bubble.on_new_query(move |state: &mut S, sql: &str| f(state, sql));
        }
        if let Some(on_toggle) = &self.on_toggle_reasoning {
            let f = Rc::clone(on_toggle);
            bubble = bubble.on_toggle_reasoning(move |state: &mut S| f(state, index));
        }
        if let Some(on_toggle) = &self.on_toggle_tool {
            let f = Rc::clone(on_toggle);
            bubble = bubble.on_toggle_tool(move |state: &mut S| f(state, index));
        }
        if let Some(on_toggle) = &self.on_toggle_memory {
            let f = Rc::clone(on_toggle);
            bubble = bubble.on_toggle_memory(move |state: &mut S| f(state, index));
        }
        if let Some(on_cite) = &self.on_cite_memory {
            let f = Rc::clone(on_cite);
            bubble = bubble
                .on_cite_memory(move |state: &mut S, reference: &str| f(state, index, reference));
        }
        if let Some(on_remember) = &self.on_remember {
            let f = Rc::clone(on_remember);
            bubble =
                bubble.on_remember(move |state: &mut S, message: &AiChatMessage| f(state, message));
        }
        if is_last_assistant && ChatBubbleState::has_questions(&message, false) {
            if let (Some(on_toggle), Some(on_custom_edit), Some(on_submit)) = (
                self.on_toggle_question.as_ref(),
                self.on_custom_edit.as_ref(),
                self.on_submit_questions.as_ref(),
            ) {
                let on_toggle = Rc::clone(on_toggle);
                let on_custom_edit = Rc::clone(on_custom_edit);
                let on_submit = Rc::clone(on_submit);
                let question_state = state.question_state(index);
                bubble = bubble.with_questions(
                    question_state,
                    move |state: &mut S, qid, oid, multi| on_toggle(state, index, qid, oid, multi),
                    move |state: &mut S, qid: &str| on_custom_edit(state, index, qid),
                    move |state: &mut S| on_submit(state, index),
                );
            }
        }
        col()
            .key(format!("ai-chat-message-{index}"))
            .w_full()
            .min_w(0.0)
            .child(bubble.build(dark))
    }

    /// The streaming bubble (`AiChatPanel.tsx:727–740`): no callbacks —
    /// React passes only the message, the dialect and `isStreaming`.
    fn streaming_bubble(&self, dark: bool) -> El {
        let state = self.state;
        let message = state.streaming_message();
        let bubble_state = state.streaming_bubble_state();
        // No callbacks are attached to the streaming bubble (React passes
        // none), so the state type needs an explicit annotation.
        let mut bubble: ChatBubble<'_, S> =
            ChatBubble::new(&message, bubble_state).streaming(true);
        if let Some(dialect) = &self.sql_dialect {
            bubble = bubble.sql_dialect(dialect.clone());
        }
        col()
            .key("ai-chat-streaming-bubble")
            .w_full()
            .min_w(0.0)
            .child(bubble.build(dark))
    }

    /// The thinking row (`AiChatPanel.tsx:741–755`): an `md` spinner plus
    /// the thinking / calling-tool label. React's `text-xs` renders at
    /// the kit's 12pt `text_xsmall`.
    fn thinking_row(&self, dark: bool) -> El {
        let k = kit(dark);
        let label = self.state.thinking_label();
        row()
            .key("ai-chat-streaming-thinking")
            .w_full()
            .min_w(0.0)
            .center_y()
            .gap(8.0)
            .py(8.0)
            .child(
                Spinner::new("ai-chat-streaming-thinking-spinner")
                    .size(SpinnerSize::Md)
                    .tone(SpinnerTone::Muted)
                    .build(dark),
            )
            .child(
                text(label)
                    .key("ai-chat-streaming-thinking-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
    }

    /// The jump-to-bottom pill (`AiChatPanel.tsx:759–775`): an ArrowDown
    /// glyph, the `chat.jumpToBottom` label and the unread-count badge.
    /// Stacked under the viewport instead of absolutely positioned —
    /// see the module docs. The press drives
    /// [`AiChatMessageListState::jump_to_bottom`] plus
    /// `tx.scroll_to_bottom("ai-chat-message-list-viewport")` through
    /// `on_jump_to_bottom`.
    fn jump_pill(&self, dark: bool) -> El {
        let k = kit(dark);
        let state = self.state;
        let count = state.unread_count().to_string();
        let label = t("chat.jumpToBottom").to_string();
        let pill = row()
            .key("jump-to-bottom")
            .center_y()
            .gap(6.0)
            .px(12.0)
            .py(6.0)
            .radius(k.radius.lg)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_01)
            .role("button")
            // React's pill is a <button>: a Tab stop that fires on Enter.
            // A plain row is not one until `focusable` says so (the
            // filter-chip precedent).
            .focusable(true)
            .child(
                rune::icon(&ICON_ARROW_DOWN, 12.0)
                    .color(k.c.text_muted)
                    .key("jump-to-bottom-icon"),
            )
            .child(
                text(label)
                    .key("jump-to-bottom-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base),
            )
            .child(
                text(count)
                    .key("jump-to-bottom-count")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent),
            );
        let wired = match &self.on_jump_to_bottom {
            Some(action) => {
                let press = Rc::clone(action);
                let keyed = Rc::clone(action);
                pill.on_press_tx(move |state: &mut S, tx: &mut Tx| press(state, tx))
                    .on_key(move |state: &mut S, event: &KeyEvent, tx: &mut Tx| {
                        if event.key == Key::Enter {
                            keyed(state, tx);
                            true
                        } else {
                            false
                        }
                    })
            }
            None => pill,
        };
        row()
            .key("ai-chat-jump-to-bottom-wrap")
            .w_full()
            .min_w(0.0)
            .justify_center()
            .py(4.0)
            .child(wired)
    }
}

/// Wire Space (kit `on_press`) and Enter on a built button element, the way
/// S1's chip and option buttons do — kit buttons fire on Space, React's
/// `<button>` fires on Enter too.
fn wire_button<S: 'static>(el: El, action: &Option<Rc<dyn Fn(&mut S)>>) -> El {
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
