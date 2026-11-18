//! Rune ports of `packages/frontend/src/components/ai/**`.
//!
//! | React source | Rune module |
//! |---|---|
//! | `AiChatPanel.tsx` (leaf slice, T009-S1) | `ai_chat_panel` |
//! | `AiChatPanel.tsx` (`ChatBubble`, T009-S2) | `chat_bubble` |
//!
//! These components currently have no production consumer in the Rune app;
//! they are intended to be wired by the `AiChatPanel` / `ChatBubble` track
//! (T009 slices after S1). Public(crate) re-exports are kept for that
//! upcoming integration.

#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub(crate) use ai_chat_panel::{
    format_mcp_tool_display_name, has_snapshot_content, AiChatContextSnapshot, AiChatResultPreview,
    AiQuestion, AiQuestionOption, Answer, AutoContextChip, QuestionBlock, QuestionBlockState,
};
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub(crate) use chat_bubble::{
    AiChatMessage, AiChatRole, AiMemoryReference, ChatBubble, ChatBubbleState,
};

pub(crate) mod ai_chat_panel;
pub(crate) mod chat_bubble;
