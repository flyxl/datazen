//! Independent audit for T311: `targetCommand.ts` port.
//!
//! Fill branches the coder does not drive: a recovery answer with no view, a
//! terminal failure with no error code, non-array `rows` in a rows chunk, and
//! a non-array `columns` field in the schema payload.

use serde_json::{json, Value};

use datazen_ui::session::editor_session_controller::{
    CommandCall, ExecutionReceipt, ExecutionState, ExecutionTarget, NamespaceTarget, SubmissionToken,
};
use datazen_ui::session::execution_projection::{
    ArtifactChunk, EffectOutcome, ExecutionView, PublishedResultChunk, ResultCompleteness,
};
use datazen_ui::session::target_command::{
    schema_from_result, schema_call, ResponseKind, TargetCommandEffect, TargetCommandError,
    TargetCommandInput, TargetCommandOutcome, TargetConnectionProfile, TargetRun,
};

fn target() -> ExecutionTarget {
    ExecutionTarget {
        connection_id: "cfg-1".to_owned(),
        namespace: NamespaceTarget::default(),
        object: None,
    }
}

fn admitted(run: &mut TargetRun) {
    let _ = run.step(TargetCommandInput::Profiles(vec![TargetConnectionProfile {
        connection_id: "cfg-1".to_owned(),
        enabled: true,
        config_revision: "7".to_owned(),
    }]));
    let _ = run.step(TargetCommandInput::Token(SubmissionToken {
        idempotency_key: "token-1".to_owned(),
        expires_at: 999,
    }));
    let _ = run.step(TargetCommandInput::Execute {
        result: Ok(ExecutionReceipt {
            execution_id: "execution-1".to_owned(),
            stream_id: "stream-1".to_owned(),
            state: ExecutionState::Queued,
        }),
        now_ms: 0,
    });
}

fn chunk(payload: Value) -> PublishedResultChunk {
    PublishedResultChunk {
        chunk: ArtifactChunk {
            artifact_id: "artifact-1".to_owned(),
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

/// React's loop: if `projection.view` is still null after a successful recover,
/// sleep 50ms and loop. The machine re-emits Recover without settling.
#[test]
fn a_recovery_answer_without_a_view_keeps_recovering() {
    let (mut run, _) = TargetRun::new(target(), call(), ResponseKind::CommandResult);
    admitted(&mut run);
    let step = run.step(TargetCommandInput::Recovered {
        view: None,
        chunks: Vec::new(),
    });
    assert_eq!(step.effects, vec![TargetCommandEffect::Recover]);
    assert_eq!(step.outcome, None);
    assert_eq!(run.phase(), "projection");
}

/// React interpolates `${view.errorCode}` directly, so an absent code becomes
/// the literal "undefined". Pin that (and the exact `effect` suffix).
#[test]
fn a_terminal_failure_without_an_error_code_reports_undefined() {
    let (mut run, _) = TargetRun::new(target(), call(), ResponseKind::CommandResult);
    admitted(&mut run);
    let step = run.step(TargetCommandInput::Recovered {
        view: Some(ExecutionView {
            execution_id: "execution-1".to_owned(),
            state: ExecutionState::Failed,
            effect_outcome: EffectOutcome::Unknown,
            provenance: None,
            artifact_ids: Vec::new(),
            result_completeness: ResultCompleteness::Complete,
            truncation_reason: None,
            error_code: None,
            runtime_binding: None,
        }),
        chunks: Vec::new(),
    });
    assert_eq!(
        step.outcome,
        Some(TargetCommandOutcome::Failed(TargetCommandError::Execution {
            message: "undefined; effect: unknown".to_owned()
        }))
    );
}

/// React's rows extraction requires `Array.isArray(output.rows)`; a non-array
/// `rows` value contributes nothing rather than failing the run.
#[test]
fn rows_extraction_skips_non_array_rows() {
    let (mut run, _) = TargetRun::new(target(), call(), ResponseKind::Rows);
    admitted(&mut run);
    let step = run.step(TargetCommandInput::Recovered {
        view: Some(ExecutionView {
            execution_id: "execution-1".to_owned(),
            state: ExecutionState::Succeeded,
            effect_outcome: EffectOutcome::Completed,
            provenance: None,
            artifact_ids: Vec::new(),
            result_completeness: ResultCompleteness::Complete,
            truncation_reason: None,
            error_code: None,
            runtime_binding: None,
        }),
        chunks: vec![
            chunk(json!({ "output": { "kind": "rows", "rows": "not-an-array" } })),
            chunk(json!({ "output": { "kind": "rows", "rows": [[7]] } })),
        ],
    });
    assert_eq!(
        step.outcome,
        Some(TargetCommandOutcome::Value(json!([[7]]))),
        "only the array-shaped rows are projected"
    );
}

/// `readTargetSchema` rejects a schema whose `columns` is not an array.
#[test]
fn schema_result_requires_an_array_columns_field() {
    assert_eq!(
        schema_from_result(Some(&json!({ "schema": { "columns": "x" } }))),
        Err(TargetCommandError::InvalidSchemaResult)
    );
    // Also pin the literal envelope command/key set the coder already checks.
    assert_eq!(schema_call("users", "app", None).command, "get_table_schema");
}

fn call() -> CommandCall {
    CommandCall {
        command: "get_table_schema".to_owned(),
        input: "{}".to_owned(),
    }
}
