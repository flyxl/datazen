//! Independent audit for T273: `sessionPrompts.ts` port.
//!
//! The coder's suite compares results against the module's own `PROMPT_*`
//! constants, so mutating a constant's value would stay green. These tests
//! pin the exact literal React key strings and fill the regex / fallback
//! branches the React cases do not reach.

use datazen_ui::session::editor_session_controller::{ApiErrorCode, SessionError};
use datazen_ui::session::query_panel_session::{SwitchError, SwitchResult};
use datazen_ui::session::session_prompts::{
    projection_error_prompt_key, session_error_prompt_key, switch_result_prompt_key, PromptError,
};

fn failed(code: &str) -> SwitchResult {
    SwitchResult::Failed {
        error: SwitchError {
            code: code.to_owned(),
            message: "error".to_owned(),
        },
    }
}

/// Every returned key must be the literal React string, not whatever the
/// module's constant currently holds.
#[test]
fn prompt_keys_are_the_exact_react_literals() {
    assert_eq!(
        switch_result_prompt_key(&SwitchResult::Conflict { session: None }),
        Some("query.session.revisionConflict")
    );
    assert_eq!(
        switch_result_prompt_key(&failed("SessionLost")),
        Some("query.session.disconnected")
    );
    assert_eq!(
        switch_result_prompt_key(&failed("Other")),
        Some("query.session.switchFailed")
    );
    assert_eq!(
        session_error_prompt_key(&PromptError::api("SessionLost", "")),
        "query.session.disconnected"
    );
    assert_eq!(
        session_error_prompt_key(&PromptError::api("ContextConflict", "")),
        "query.session.revisionConflict"
    );
    assert_eq!(
        session_error_prompt_key(&PromptError::api("ResourceBusy", "")),
        "query.session.cancelledRetry"
    );
    assert_eq!(
        session_error_prompt_key(&PromptError::error("boom")),
        "query.session.executionFailed"
    );
    assert_eq!(
        projection_error_prompt_key(&PromptError::error("boom")),
        "query.session.executionFailed"
    );
}

/// React: `/disconnect|lost|network/i.test(error.message)`. Cover the `lost`
/// alternative, case-insensitivity, and an ApiError whose message is shaped
/// like a disconnect but whose code is not SessionLost (ApiError extends
/// Error in JS, so the regex applies to it too).
#[test]
fn projection_disconnect_detection_covers_lost_and_case_insensitivity() {
    for message in ["session lost", "DISCONNECTED", "Network drop", "a Lost socket"] {
        assert_eq!(
            projection_error_prompt_key(&PromptError::error(message)),
            "query.session.disconnected",
            "{message:?} matches the React disconnect regex"
        );
    }
    assert_eq!(
        projection_error_prompt_key(&PromptError::api("HostRejected", "peer disconnected")),
        "query.session.disconnected",
        "the message regex applies to ApiError messages as well"
    );
}

/// An unknown ApiError code and the remaining T083 battery codes fall back to
/// the generic execution-failure copy; also pin the exact string adaptation
/// of `RuntimeEpochMismatch` / `SessionNotFound`.
#[test]
fn unknown_codes_and_remaining_battery_codes_fall_back() {
    assert_eq!(
        session_error_prompt_key(&PromptError::api("HostRejected", "x")),
        "query.session.executionFailed"
    );
    for code in [ApiErrorCode::SessionNotFound, ApiErrorCode::RuntimeEpochMismatch] {
        let adapted = PromptError::from_session(&SessionError {
            code,
            message: "x".to_owned(),
        });
        match &adapted {
            PromptError::ApiError { code, .. } => assert_eq!(
                session_error_prompt_key(&adapted),
                "query.session.executionFailed",
                "adapted code {code:?} is not one of the three mapped codes"
            ),
            other => panic!("from_session must produce an ApiError, got {other:?}"),
        }
    }
}
