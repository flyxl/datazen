//! Message-list state — T009-S4 (`AiChatPanel.tsx:642–790`).
//!
//! The view-model placeholder for the `aiStore` slice (T088, not done):
//! messages, streaming fields, the session memory, both notices and the
//! scroll position. The builder lives in `ai_chat_message_list.rs`; the
//! split keeps each file under the 800-line cap. See that module's docs
//! for the key convention and the deliberate boundaries.

// The panel view (a later T009 slice) is the production consumer; until it
// lands the component tests are the only callers, so the dead-code lint is
// silenced here rather than in a crate-wide allow.
#![allow(dead_code)]

use super::ai_chat_panel::{format_mcp_tool_display_name, QuestionBlockState};
use super::chat_bubble::{AiChatMessage, AiChatRole, AiMemoryReference, ChatBubbleState};
use crate::i18n::t;

/// `useAutoScroll`'s default threshold: within 120pt of the bottom counts
/// as at the bottom (`hooks/useAutoScroll.ts`).
pub(crate) const AUTO_SCROLL_THRESHOLD: f32 = 120.0;

/// The mutable state behind an [`AiChatMessageList`]: the view-model
/// placeholder for the `aiStore` slice (T088, not done). The caller owns
/// one of these (in `AppState` once the panel lands; in the probe state in
/// tests) and hands `&state` to the builder each frame; presses call back
/// into the caller, which drives the state machine. Defaults mirror
/// React's initials: a session exists, nothing streaming, at the bottom,
/// no unread, no notices.
#[derive(Clone, Debug)]
pub(crate) struct AiChatMessageListState {
    has_session: bool,
    messages: Vec<AiChatMessage>,
    bubble_states: Vec<ChatBubbleState>,
    question_states: Vec<QuestionBlockState>,
    fallback_bubble_state: ChatBubbleState,
    fallback_question_state: QuestionBlockState,
    streaming_bubble_state: ChatBubbleState,
    is_streaming: bool,
    stream_content: String,
    stream_reasoning: String,
    stream_mcp_tool_name: Option<String>,
    stream_memory_used: Vec<AiMemoryReference>,
    session_memory_used: Vec<AiMemoryReference>,
    memory_candidate_count: Option<u32>,
    remember_notice: Option<String>,
    at_bottom: bool,
    unread_count: u32,
}

impl Default for AiChatMessageListState {
    fn default() -> Self {
        Self {
            has_session: true,
            messages: Vec::new(),
            bubble_states: Vec::new(),
            question_states: Vec::new(),
            fallback_bubble_state: ChatBubbleState::default(),
            fallback_question_state: QuestionBlockState::default(),
            streaming_bubble_state: ChatBubbleState::default(),
            is_streaming: false,
            stream_content: String::new(),
            stream_reasoning: String::new(),
            stream_mcp_tool_name: None,
            stream_memory_used: Vec::new(),
            session_memory_used: Vec::new(),
            memory_candidate_count: None,
            remember_notice: None,
            // `useAutoScroll`: `useState(true)`.
            at_bottom: true,
            unread_count: 0,
        }
    }
}

impl AiChatMessageListState {
    pub(crate) fn has_session(&self) -> bool {
        self.has_session
    }

    pub(crate) fn messages(&self) -> &[AiChatMessage] {
        &self.messages
    }

    pub(crate) fn is_streaming(&self) -> bool {
        self.is_streaming
    }

    pub(crate) fn stream_content(&self) -> &str {
        &self.stream_content
    }

    pub(crate) fn stream_reasoning(&self) -> &str {
        &self.stream_reasoning
    }

    pub(crate) fn stream_mcp_tool_name(&self) -> Option<&str> {
        self.stream_mcp_tool_name.as_deref()
    }

    pub(crate) fn memory_candidate_count(&self) -> Option<u32> {
        self.memory_candidate_count
    }

    pub(crate) fn remember_notice(&self) -> Option<&str> {
        self.remember_notice.as_deref()
    }

    pub(crate) fn at_bottom(&self) -> bool {
        self.at_bottom
    }

    pub(crate) fn unread_count(&self) -> u32 {
        self.unread_count
    }

    /// React's empty-state gate: a session exists, no messages, not
    /// streaming (`AiChatPanel.tsx:688`).
    pub(crate) fn show_empty(&self) -> bool {
        self.has_session && self.messages.is_empty() && !self.is_streaming
    }

    /// React's jump-to-bottom gate: away from the bottom with unread
    /// messages (`AiChatPanel.tsx:759`). See the module docs: this is a
    /// dead gate in React as shipped (`unread_count` starts 0 and
    /// nothing increments it until the panel slice wires
    /// [`AiChatMessageListState::note_new_messages`]).
    pub(crate) fn show_jump_to_bottom(&self) -> bool {
        !self.at_bottom && self.unread_count > 0
    }

    /// React's streaming-bubble gate: streaming with content or reasoning
    /// already arrived (`AiChatPanel.tsx:727–728`). JS truthiness — a
    /// whitespace-only string is truthy — so `is_empty()` only, no trim.
    pub(crate) fn show_streaming_bubble(&self) -> bool {
        self.is_streaming && (!self.stream_content.is_empty() || !self.stream_reasoning.is_empty())
    }

    /// React's thinking-row gate: streaming with neither content nor
    /// reasoning yet (`AiChatPanel.tsx:741–743`).
    pub(crate) fn show_thinking_row(&self) -> bool {
        self.is_streaming && self.stream_content.is_empty() && self.stream_reasoning.is_empty()
    }

    /// React's message effective-memory rule (`AiChatPanel.tsx:696–702`):
    /// the last assistant message inherits the session memory when it
    /// carries none of its own. See the module docs for the `[]`-vs-
    /// absent divergence this cannot express.
    pub(crate) fn effective_message(&self, index: usize) -> Option<AiChatMessage> {
        let message = self.messages.get(index)?.clone();
        let is_last = index + 1 == self.messages.len();
        if message.role == AiChatRole::Assistant
            && is_last
            && message.memory_used.is_empty()
            && !self.session_memory_used.is_empty()
        {
            let mut inherited = message;
            inherited.memory_used = self.session_memory_used.clone();
            Some(inherited)
        } else {
            Some(message)
        }
    }

    /// React's streaming bubble message (`AiChatPanel.tsx:730–735`):
    /// assistant role, the streamed content / reasoning, memory from the
    /// stream falling back to the session.
    pub(crate) fn streaming_message(&self) -> AiChatMessage {
        let mut message = AiChatMessage::new(AiChatRole::Assistant, self.stream_content.clone());
        if !self.stream_reasoning.is_empty() {
            message = message.reasoning(self.stream_reasoning.clone());
        }
        if !self.stream_memory_used.is_empty() {
            message = message.memory_used(self.stream_memory_used.clone());
        } else if !self.session_memory_used.is_empty() {
            message = message.memory_used(self.session_memory_used.clone());
        }
        message
    }

    /// The thinking-row label (`AiChatPanel.tsx:746–750`): the MCP tool
    /// name, display-formatted, while a tool call is in flight, else
    /// `chat.thinking`.
    pub(crate) fn thinking_label(&self) -> String {
        match &self.stream_mcp_tool_name {
            Some(name) => {
                t("chat.callingMcpTool").replacen("{name}", &format_mcp_tool_display_name(name), 1)
            }
            None => t("chat.thinking").to_string(),
        }
    }

    /// The bubble state for message `index`; the fallback when the caller
    /// drives an index `set_messages` never sized (see
    /// [`AiChatMessageListState::set_messages`]).
    pub(crate) fn bubble_state(&self, index: usize) -> &ChatBubbleState {
        self.bubble_states
            .get(index)
            .unwrap_or(&self.fallback_bubble_state)
    }

    pub(crate) fn bubble_state_mut(&mut self, index: usize) -> &mut ChatBubbleState {
        match self.bubble_states.get_mut(index) {
            Some(state) => state,
            None => &mut self.fallback_bubble_state,
        }
    }

    /// The questions state for message `index`; same fallback rule as
    /// [`AiChatMessageListState::bubble_state`].
    pub(crate) fn question_state(&self, index: usize) -> &QuestionBlockState {
        self.question_states
            .get(index)
            .unwrap_or(&self.fallback_question_state)
    }

    pub(crate) fn question_state_mut(&mut self, index: usize) -> &mut QuestionBlockState {
        match self.question_states.get_mut(index) {
            Some(state) => state,
            None => &mut self.fallback_question_state,
        }
    }

    pub(crate) fn streaming_bubble_state(&self) -> &ChatBubbleState {
        &self.streaming_bubble_state
    }

    pub(crate) fn streaming_bubble_state_mut(&mut self) -> &mut ChatBubbleState {
        &mut self.streaming_bubble_state
    }

    // -- VM seeding: the store writes the panel slice will make once T088
    // lands. Until then tests (and the panel slice's placeholder) drive
    // these directly. --

    pub(crate) fn set_has_session(&mut self, has_session: bool) {
        self.has_session = has_session;
    }

    /// Seed the message list. Per-message bubble and question states are
    /// (re)created to match — answers do not survive a re-seed, so the
    /// panel slice seeds once per session, not per frame.
    pub(crate) fn set_messages(&mut self, messages: Vec<AiChatMessage>) {
        self.bubble_states = messages
            .iter()
            .map(|_| ChatBubbleState::default())
            .collect();
        self.question_states = messages
            .iter()
            .map(|message| QuestionBlockState::new(&message.questions))
            .collect();
        self.messages = messages;
    }

    pub(crate) fn set_streaming(&mut self, is_streaming: bool) {
        self.is_streaming = is_streaming;
    }

    pub(crate) fn set_stream_content(&mut self, content: impl Into<String>) {
        self.stream_content = content.into();
    }

    pub(crate) fn set_stream_reasoning(&mut self, reasoning: impl Into<String>) {
        self.stream_reasoning = reasoning.into();
    }

    pub(crate) fn set_stream_mcp_tool_name(&mut self, name: Option<String>) {
        self.stream_mcp_tool_name = name;
    }

    pub(crate) fn set_stream_memory_used(&mut self, memory: Vec<AiMemoryReference>) {
        self.stream_memory_used = memory;
    }

    pub(crate) fn set_session_memory_used(&mut self, memory: Vec<AiMemoryReference>) {
        self.session_memory_used = memory;
    }

    pub(crate) fn set_memory_candidate_count(&mut self, count: Option<u32>) {
        self.memory_candidate_count = count;
    }

    /// React's `dismissMemoryCandidateNotice`: the store clears the
    /// notice; here the VM clears its count.
    pub(crate) fn dismiss_memory_candidate(&mut self) {
        self.memory_candidate_count = None;
    }

    pub(crate) fn set_remember_notice(&mut self, notice: Option<String>) {
        self.remember_notice = notice;
    }

    /// React's `setRememberNotice(null)` on the remember toast's dismiss.
    pub(crate) fn dismiss_remember_notice(&mut self) {
        self.remember_notice = None;
    }

    /// React's `useAutoScroll` `checkAtBottom`: the distance from the
    /// bottom decides, and reaching the bottom clears the unread count.
    /// The threshold is the hook's 120pt default.
    pub(crate) fn on_scroll(&mut self, distance_from_bottom: f32) {
        self.at_bottom = distance_from_bottom <= AUTO_SCROLL_THRESHOLD;
        if self.at_bottom {
            self.unread_count = 0;
        }
    }

    /// The viewport's `on_at_bottom` report: same state transition as
    /// [`AiChatMessageListState::on_scroll`] at the boundary.
    pub(crate) fn set_at_bottom(&mut self, at_bottom: bool) {
        self.at_bottom = at_bottom;
        if at_bottom {
            self.unread_count = 0;
        }
    }

    /// `useTrackUnread` (`hooks/useAutoScroll.ts`): new messages add to
    /// the unread count only while away from the bottom. React's panel
    /// never calls this helper — the seam exists so the panel slice can
    /// wire it when the store reports arrivals.
    pub(crate) fn note_new_messages(&mut self, count: u32) {
        if !self.at_bottom {
            self.unread_count = self.unread_count.saturating_add(count);
        }
    }

    /// React's `jumpToBottom`: scroll to the end (the caller issues
    /// `Tx::scroll_to_bottom` on `ai-chat-message-list-viewport`) and
    /// reset the scroll state.
    pub(crate) fn jump_to_bottom(&mut self) {
        self.at_bottom = true;
        self.unread_count = 0;
    }
}

