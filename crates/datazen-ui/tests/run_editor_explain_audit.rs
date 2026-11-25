//! Independent audit for T231: `runEditorExplain.ts` port.
//!
//! The coder pins the React cases. These tests fill branches the React suite
//! does not reach: a succeeded terminal view with no plan chunk, the exact
//! lowercase string views of every reused DTO enum, and the truthiness filter
//! on each optional envelope field.

use serde_json::json;

use datazen_ui::session::editor_session_controller::{
    ApiErrorCode, ExecutionReceipt, ExecutionState, SessionError,
};
use datazen_ui::session::execution_projection::{
    ArtifactChunk, EffectOutcome, ExecutionErrorCode, ExecutionView, PublishedResultChunk,
    ResultCompleteness,
};
use datazen_ui::session::run_editor_explain::{
    explain_command_call, explain_failure_message, start, ExplainEffect, ExplainFailure,
    ExplainInput, ExplainOutcome, ExplainRequest,
};

fn request() -> ExplainRequest {
    ExplainRequest {
        key: "p1".to_owned(),
        connection_id: "cfg-1".to_owned(),
        sql: "SELECT 1".to_owned(),
        database: None,
        schema: None,
        params: Vec::new(),
    }
}

fn receipt() -> ExecutionReceipt {
    ExecutionReceipt {
        execution_id: "explain-1".to_owned(),
        stream_id: "stream-1".to_owned(),
        state: ExecutionState::Queued,
    }
}

fn view(state: ExecutionState, code: Option<ExecutionErrorCode>, effect: EffectOutcome) -> ExecutionView {
    ExecutionView {
        execution_id: "explain-1".to_owned(),
        state,
        effect_outcome: effect,
        provenance: None,
        artifact_ids: Vec::new(),
        result_completeness: ResultCompleteness::Complete,
        truncation_reason: None,
        error_code: code,
        runtime_binding: None,
    }
}

fn chunk(payload: serde_json::Value) -> PublishedResultChunk {
    PublishedResultChunk {
        chunk: ArtifactChunk {
            artifact_id: "plan-1".to_owned(),
            chunk_index: "0".to_owned(),
            total_chunks: Some("1".to_owned()),
            published_chunk_count: None,
            published_byte_length: None,
            offset: "0".to_owned(),
            bytes: Vec::new(),
            result_completeness: ResultCompleteness::Complete,
        },
        source: None,
        payload,
    }
}

/// React: a succeeded view with no `commandResult`/`planText` chunk throws
/// `Explain result unavailable`. Pin that the machine returns the typed
/// `ResultUnavailable` failure (and still disposes in `finally`).
#[test]
fn succeeded_view_without_a_plan_chunk_is_result_unavailable() {
    let (mut run, _) = start(request());
    let _ = run.step(ExplainInput::ControllerReady);
    let _ = run.step(ExplainInput::Executed(Ok(receipt())));
    let _ = run.step(ExplainInput::Projected);
    let step = run.step(ExplainInput::Recovered {
        view: Some(view(ExecutionState::Succeeded, None, EffectOutcome::Completed)),
        chunks: vec![chunk(json!({ "output": { "kind": "queryResult", "data": { "planText": "no" } } }))],
    });
    assert_eq!(step.effects, vec![ExplainEffect::Refresh]);
    let step = run.step(ExplainInput::Refreshed);
    assert_eq!(
        step.outcome,
        Some(ExplainOutcome::Failed(ExplainFailure::ResultUnavailable))
    );
    assert_eq!(step.effects, vec![ExplainEffect::Dispose]);
}

/// The local lowercase string views must exactly mirror React's interpolated
/// `state / errorCode / effectOutcome` values, including `undefined` for a
/// missing error code.
#[test]
fn failure_message_covers_every_reused_dto_variant() {
    let states = [
        (ExecutionState::Queued, "queued"),
        (ExecutionState::Running, "running"),
        (ExecutionState::CancelRequested, "cancelRequested"),
        (ExecutionState::Succeeded, "succeeded"),
        (ExecutionState::Failed, "failed"),
        (ExecutionState::Cancelled, "cancelled"),
    ];
    for (state, text) in states {
        let message = explain_failure_message(&view(state, None, EffectOutcome::NotStarted));
        assert_eq!(message, format!("{text}: undefined; effect: notStarted"));
    }

    let codes = [
        (ExecutionErrorCode::SqlError, "sqlError"),
        (ExecutionErrorCode::ProtocolError, "protocolError"),
        (ExecutionErrorCode::Cancelled, "cancelled"),
        (ExecutionErrorCode::Timeout, "timeout"),
        (ExecutionErrorCode::ResourceLost, "resourceLost"),
        (ExecutionErrorCode::PipelineAborted, "pipelineAborted"),
        (ExecutionErrorCode::HostRejected, "hostRejected"),
    ];
    for (code, text) in codes {
        let message = explain_failure_message(&view(
            ExecutionState::Failed,
            Some(code),
            EffectOutcome::Unknown,
        ));
        assert_eq!(message, format!("failed: {text}; effect: unknown"));
    }

    for (effect, text) in [
        (EffectOutcome::NotStarted, "notStarted"),
        (EffectOutcome::Completed, "completed"),
        (EffectOutcome::RolledBack, "rolledBack"),
        (EffectOutcome::PartiallyApplied, "partiallyApplied"),
        (EffectOutcome::Unknown, "unknown"),
    ] {
        let message = explain_failure_message(&view(ExecutionState::Failed, None, effect));
        assert_eq!(message, format!("failed: undefined; effect: {text}"));
    }
}

/// React omits `params` when empty and `database`/`schema` when falsy. Cover
/// the mixed cases (params with no database, and an empty schema) that the
/// coder's all-or-nothing cases leave open.
#[test]
fn envelope_filters_each_optional_field_independently() {
    let call = explain_command_call(
        "SELECT :id",
        &[("id".to_owned(), json!(73))],
        None,
        Some(""),
    );
    assert_eq!(call.command, "explain");
    assert_eq!(call.input, r#"{"sql":"SELECT :id","params":{"id":73}}"#);

    let call = explain_command_call(
        "SELECT 1",
        &[],
        Some("A"),
        None,
    );
    assert_eq!(call.input, r#"{"sql":"SELECT 1","database":"A"}"#);

    let call = explain_command_call(
        "SELECT 1",
        &[],
        None,
        Some("s"),
    );
    assert_eq!(call.input, r#"{"sql":"SELECT 1","schema":"s"}"#);
}

/// A backend execute failure is mapped to a typed `Session` failure (the
/// effect-stream discrepancy is reported separately as D1).
#[test]
fn execute_error_maps_to_the_typed_session_failure() {
    let (mut run, _) = start(request());
    let _ = run.step(ExplainInput::ControllerReady);
    let step = run.step(ExplainInput::Executed(Err(SessionError {
        code: ApiErrorCode::SessionLost,
        message: "gone".to_owned(),
    })));
    assert_eq!(
        step.outcome,
        Some(ExplainOutcome::Failed(ExplainFailure::Session(SessionError {
            code: ApiErrorCode::SessionLost,
            message: "gone".to_owned(),
        })))
    );
    assert_eq!(run.phase(), "done");
}
