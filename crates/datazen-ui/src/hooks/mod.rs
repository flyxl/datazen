//! Ports of `packages/frontend/src/hooks/` — view glue that keeps a state
//! machine and a small rendering half (useConfirmDialog, useAiEditorGrant,
//! then the T124 trio useAiKeyboard/useAutoScroll/useClipboardCopyFeedback;
//! T158 adds useColumnPresentationSession; the remaining hooks land with
//! their own tasks).

pub(crate) mod confirm_dialog;
pub(crate) mod use_ai_editor_grant;
pub(crate) mod use_ai_keyboard;
pub(crate) mod use_auto_scroll;
pub(crate) mod use_clipboard_copy_feedback;
pub(crate) mod use_column_presentation_session;
