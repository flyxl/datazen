//! Port of `packages/frontend/src/lib/session/sessionPrompts.ts`.
//!
//! Pure, dependency-free mapping from session lifecycle outcomes to the i18n
//! keys the query panel shows. React returns key strings (the caller translates
//! them), so this port returns `&'static str` keys — no rendering, no locale
//! lookup here. The DTOs (`SessionView`, `SessionState`, `SwitchResult`,
//! `SwitchError`, `SessionError`, `ApiErrorCode`) are reused from T083/T183;
//! nothing is duplicated.
//!
//! React takes `error: unknown`; Rust needs a typed stand-in to tell an
//! `ApiError` (a `code`) from a plain `Error` (message only). [`PromptError`]
//! is that stand-in. Removal stage: when a shared error DTO exposes the same
//! distinction, accept it directly and drop the constructors.
//!
//! No view consumer yet: every item carries a per-item `#[allow(dead_code)]`
//! with the removal stage "drop when the query-panel view wires these helpers".

use crate::session::editor_session_controller::{
    ApiErrorCode, SessionError, SessionState, SessionView,
};
use crate::session::query_panel_session::SwitchResult;

/// Exact React i18n keys (verified against `locales/zh-CN/query.ts`).
#[allow(dead_code)] // key constants; remove with the query-panel view.
pub const PROMPT_REVISION_CONFLICT: &str = "query.session.revisionConflict";
#[allow(dead_code)] // key constants; remove with the query-panel view.
pub const PROMPT_DISCONNECTED: &str = "query.session.disconnected";
#[allow(dead_code)] // key constants; remove with the query-panel view.
pub const PROMPT_SWITCH_FAILED: &str = "query.session.switchFailed";
#[allow(dead_code)] // key constants; remove with the query-panel view.
pub const PROMPT_CANCELLED_RETRY: &str = "query.session.cancelledRetry";
#[allow(dead_code)] // key constants; remove with the query-panel view.
pub const PROMPT_EXECUTION_FAILED: &str = "query.session.executionFailed";

/// Typed stand-in for React's `unknown` error: an `ApiError` carries a `code`,
/// a plain `Error` only a message.
#[allow(dead_code)] // stand-in; remove when a shared error DTO exposes the split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptError {
    ApiError { code: String, message: String },
    Error { message: String },
}

#[allow(dead_code)] // constructors; remove with the stand-in.
impl PromptError {
    pub fn api(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ApiError {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }

    /// Adapt the T083 `ApiError` mirror (whose battery codes live in
    /// `ApiErrorCode::Other`) into the same shape.
    pub fn from_session(error: &SessionError) -> Self {
        Self::ApiError {
            code: api_code_string(&error.code),
            message: error.message.clone(),
        }
    }
}

/// Whether a database switch must ask before leaving an open transaction. A
/// closed or lost session has no transaction left, so no confirmation.
#[allow(dead_code)] // helper; remove with the query-panel view.
pub fn needs_transaction_switch_confirm(session: &SessionView) -> bool {
    session.state != SessionState::Closed && session.state != SessionState::Lost
}

/// Prompt key for a `DatabaseSwitchResult` (silent on success or cancellation).
#[allow(dead_code)] // helper; remove with the query-panel view.
pub fn switch_result_prompt_key(result: &SwitchResult) -> Option<&'static str> {
    match result {
        SwitchResult::Switched { .. } | SwitchResult::Cancelled => None,
        SwitchResult::Conflict { .. } => Some(PROMPT_REVISION_CONFLICT),
        SwitchResult::Failed { error } => Some(match error.code.as_str() {
            "SessionLost" => PROMPT_DISCONNECTED,
            _ => PROMPT_SWITCH_FAILED,
        }),
    }
}

/// Map a session/execution error to its user-facing copy key.
#[allow(dead_code)] // helper; remove with the query-panel view.
pub fn session_error_prompt_key(error: &PromptError) -> &'static str {
    if let PromptError::ApiError { code, .. } = error {
        match code.as_str() {
            "SessionLost" => return PROMPT_DISCONNECTED,
            "ContextConflict" => return PROMPT_REVISION_CONFLICT,
            "ResourceBusy" => return PROMPT_CANCELLED_RETRY,
            _ => {}
        }
    }
    PROMPT_EXECUTION_FAILED
}

/// Map a projection error: a lost-session API error or a disconnect-shaped
/// message is a disconnect; everything else is a generic execution failure.
#[allow(dead_code)] // helper; remove with the query-panel view.
pub fn projection_error_prompt_key(error: &PromptError) -> &'static str {
    if let PromptError::ApiError { code, .. } = error {
        if code == "SessionLost" {
            return PROMPT_DISCONNECTED;
        }
    }
    let message = match error {
        PromptError::ApiError { message, .. } | PromptError::Error { message } => message,
    };
    let lowered = message.to_ascii_lowercase();
    if lowered.contains("disconnect") || lowered.contains("lost") || lowered.contains("network") {
        return PROMPT_DISCONNECTED;
    }
    PROMPT_EXECUTION_FAILED
}

/// The `SwitchError` produced by T183 carries the ApiError code as a string;
/// recognise the one code this module needs without hardcoding a dialect.
fn api_code_string(code: &ApiErrorCode) -> String {
    match code {
        ApiErrorCode::SessionLost => "SessionLost".to_owned(),
        ApiErrorCode::SessionNotFound => "SessionNotFound".to_owned(),
        ApiErrorCode::RuntimeEpochMismatch => "RuntimeEpochMismatch".to_owned(),
        ApiErrorCode::Other(code) => code.clone(),
    }
}
