//! Rune ports of `packages/frontend/src/components/ai/**`.
//!
//! | React source | Rune module |
//! |---|---|
//! | `AiChatPanel.tsx` (leaf slice, T009-S1) | `ai_chat_panel` |
//! | `AiChatPanel.tsx` (`ChatBubble`, T009-S2) | `chat_bubble` |
//! | `AiChatPanel.tsx` (header, T009-S3) | `ai_chat_header` |
//! | `AiChatPanel.tsx` (message list, T009-S4) | `ai_chat_message_list` |
//! | `AiChatPanel.tsx` (input area, T009-S5) | `ai_chat_input_area` (+ `ai_chat_input_area_state`) |
//! | `AiChatPanel.tsx` (panel shell + session wiring, T009-S6) | `ai_chat_panel_state` + `ai_chat_panel_shell` (+ `ai_chat_panel_types`, `ai_chat_panel_state_seed`) |
//!
//! These components are wired by the `AiChatPanel` / `ChatBubble` track
//! (T009 slices after S1). The production consumer is the `Section`
//! wiring (T009-S7): `views::ai_chat_aside` mounts the S6 shell in the
//! workspace, and `AppState::sync_ai_chat_panel` (see
//! `ai_chat_state.rs`) drives its host sync.

#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub(crate) use ai_chat_panel::{
    format_mcp_tool_display_name, has_snapshot_content, AiChatContextSnapshot, AiChatResultPreview,
    AiQuestion, AiQuestionOption, Answer, AutoContextChip, QuestionBlock, QuestionBlockState,
};
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub(crate) use chat_bubble::{
    AiChatMessage, AiChatRole, AiMemoryReference, ChatBubble, ChatBubbleState,
};
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub(crate) use ai_chat_header::{
    detect_safety_preset, AiAgentMode, AiChatHeader, AiChatHeaderState, AiChatTab, AiProfile,
    AiPromptTemplate, AiSafetyGate, AiSafetyPreset,
};
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub(crate) use ai_chat_message_list::AiChatMessageList;
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub(crate) use ai_chat_message_list_state::{AiChatMessageListState, AUTO_SCROLL_THRESHOLD};
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub(crate) use ai_chat_input_area::AiChatInputArea;
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub(crate) use ai_chat_input_area_state::{
    AiChatInputAreaState, AiInjection, AiLoadedSkill, AiPendingDraft, ConflictResolution,
};
#[allow(unused_imports)] // the Section wiring (T009-S7) is the production consumer
pub(crate) use ai_chat_panel_shell::AiChatPanel;
#[allow(unused_imports)] // the Section wiring (T009-S7) is the production consumer
pub(crate) use ai_chat_panel_state::AiChatPanelState;
#[allow(unused_imports)] // the Section wiring (T009-S7) is the production consumer
pub(crate) use ai_chat_panel_types::{
    AiMemoryCreateRequest, AiMemoryMode, AiMemoryScope, AiSentMessage,
};
#[allow(unused_imports)] // the modal_layer mount is the production consumer
pub(crate) use ai_write_proposal_host::{
    AiWriteApplyReceipt, AiWriteApplyRequest, AiWriteImpact, AiWriteProposal,
    AiWriteProposalHostState, AiWriteStatement,
};
#[allow(unused_imports)] // the modal_layer mount is the production consumer
pub(crate) use ai_write_proposal_host_view::AiWriteProposalHost;

pub(crate) mod ai_chat_header;
pub(crate) mod ai_chat_input_area;
pub(crate) mod ai_chat_input_area_state;
pub(crate) mod ai_chat_message_list;
pub(crate) mod ai_chat_message_list_state;
pub(crate) mod ai_chat_panel;
pub(crate) mod ai_chat_panel_shell;
pub(crate) mod ai_chat_panel_state;
pub(crate) mod ai_chat_panel_state_seed;
pub(crate) mod ai_chat_panel_types;
pub(crate) mod ai_write_proposal_host;
pub(crate) mod ai_write_proposal_host_view;
pub(crate) mod chat_bubble;
