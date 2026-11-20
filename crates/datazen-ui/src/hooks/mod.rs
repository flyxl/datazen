//! Ports of `packages/frontend/src/hooks/` — view glue that keeps a state
//! machine and a small rendering half (useConfirmDialog, then
//! useAiEditorGrant; the remaining hooks land with their own tasks).

pub(crate) mod confirm_dialog;
pub(crate) mod use_ai_editor_grant;
