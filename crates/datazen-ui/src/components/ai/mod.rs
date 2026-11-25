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
//! | `AiCodeBlock.tsx` (T060) | `ai_code_block` (+ `ai_code_block_state`) |
//! | `DiagnosisPanel.tsx` (T239) | `diagnosis_panel` |
//!
//! These components are wired by the `AiChatPanel` / `ChatBubble` track
//! (T009 slices after S1). The production consumer is the `Section`
//! wiring (T009-S7): `views::ai_chat_aside` mounts the S6 shell in the
//! workspace, and `AppState::sync_ai_chat_panel` (see
//! `ai_chat_state.rs`) drives its host sync.

#[allow(unused_imports)] // the AiMessageContent wiring (T169) is the production consumer
pub use ai_code_block::AiCodeBlock;
#[allow(unused_imports)] // the AiMessageContent wiring (T169) is the production consumer
pub use ai_code_block_state::{
    apply_dialect_choice, code_block_label, display_label, is_sql_code_block, looks_like_sql,
    run_language, AiCodeBlockProps, AiCodeBlockState, SqlDialect, COPIED_FEEDBACK_MS,
};
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub use ai_chat_panel::{
    format_mcp_tool_display_name, has_snapshot_content, AiChatContextSnapshot, AiChatResultPreview,
    AiQuestion, AiQuestionOption, Answer, AutoContextChip, QuestionBlock, QuestionBlockState,
};
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub use chat_bubble::{
    AiChatMessage, AiChatRole, AiMemoryReference, ChatBubble, ChatBubbleState,
};
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub use ai_chat_header::{
    detect_safety_preset, AiAgentMode, AiChatHeader, AiChatHeaderState, AiChatTab, AiProfile,
    AiPromptTemplate, AiSafetyGate, AiSafetyPreset,
};
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub use ai_chat_message_list::AiChatMessageList;
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub use ai_chat_message_list_state::{AiChatMessageListState, AUTO_SCROLL_THRESHOLD};
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub use ai_chat_input_area::AiChatInputArea;
#[allow(unused_imports)] // consumers will be added by the AiChatPanel wiring
pub use ai_chat_input_area_state::{
    AiChatInputAreaState, AiInjection, AiLoadedSkill, AiPendingDraft, ConflictResolution,
};
#[allow(unused_imports)] // the Section wiring (T009-S7) is the production consumer
pub use ai_chat_panel_shell::AiChatPanel;
#[allow(unused_imports)] // the Section wiring (T009-S7) is the production consumer
pub use ai_chat_panel_state::AiChatPanelState;
#[allow(unused_imports)] // the Section wiring (T009-S7) is the production consumer
pub use ai_chat_panel_types::{
    AiMemoryCreateRequest, AiMemoryMode, AiMemoryScope, AiSentMessage,
};
#[allow(unused_imports)] // the modal_layer mount is the production consumer
pub use ai_write_proposal_host::{
    AiWriteApplyReceipt, AiWriteApplyRequest, AiWriteImpact, AiWriteProposal,
    AiWriteProposalHostState, AiWriteStatement,
};
#[allow(unused_imports)] // the modal_layer mount is the production consumer
pub use ai_write_proposal_host_view::AiWriteProposalHost;

pub mod ai_chat_header;
pub mod ai_code_block;
pub mod ai_code_block_state;
pub mod ai_message_content;
pub mod ai_chat_input_area;
pub mod ai_chat_input_area_state;
pub mod ai_chat_message_list;
pub mod ai_chat_message_list_state;
pub mod ai_chat_panel;
pub mod ai_chat_panel_shell;
pub mod ai_chat_panel_state;
pub mod ai_chat_panel_state_seed;
pub mod ai_chat_panel_types;
pub mod ai_write_proposal_host;
pub mod ai_write_proposal_host_view;
pub mod chat_bubble;
pub mod ai_egress_notice;
pub mod ai_session_history;
pub mod ai_input;
pub mod context_picker;
pub mod diagnosis_panel;
pub mod explain_panel;
// T280: the NL2SQL panel (the natural-language-to-SQL composer).
pub mod nl2_sql_panel;
// T299: the natural-language filter input (the smart-filter composer).
pub mod nl_filter_input;

pub use ai_egress_notice::{AiEgressNotice, ContextItem, ContextKind};
pub use explain_panel::{
    raw_explain_table, AnalyzeExplainRequest, ExplainPanel, ExplainPanelVm, RawExplainTable,
};
/// T239: the failed query's AI diagnosis strip (the unconfigured notice,
/// the header with the manual re-diagnose and close controls, and the four
/// body states). The query workspace wiring task is the mount point.
pub use diagnosis_panel::{
    diagnose_request, DiagnosisPanel, DiagnosisPanelState, DiagnoseRequest, BODY_MAX_HEIGHT,
};
#[allow(unused_imports)] // the AiInput wiring task is the production consumer
pub use context_picker::{
    matches_query, table_label, ContextEntry, ContextPicker, ContextPickerState, PickerCategory,
    PickerKeyOutcome, PickerPosition, PickerRow, PickerView, MAX_RECENT,
};
#[allow(unused_imports)] // the panel shell / state wiring are the consumers
pub use ai_message_content::{AiMessageCodeWiring, AiMessageContent, MessageSegment};
