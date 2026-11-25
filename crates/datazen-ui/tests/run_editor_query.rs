//! T252: `lib/session/runEditorQuery.ts` port — real branch tests.
//!
//! Pinned values come from `runEditorQueryArtifacts.test.ts`: pane `pane`,
//! receipt `execution-1`, artifact `artifact-1`, and the three terminal states.
//! Every answer goes through `QueryRun::step`, with no async or fake timers.

use serde_json::{json, Value};

use datazen_ui::session::editor_session_controller::{
    ApiErrorCode, CommandCall, ExecutionReceipt, ExecutionState, ExecutionTarget, NamespaceTarget,
    OwnerRef, SessionError,
};
use datazen_ui::session::execution_projection::{
    ArtifactChunk, EffectOutcome, ExecutionErrorCode, ExecutionView, PublishedResultChunk,
    ResultCompleteness, StatementResultSource, WritableMapping,
};
use datazen_ui::session::run_editor_query::{
    execution_error_message, query_command_call, start, QueryEffect, QueryEvent, QueryFailure,
    QueryInput, QueryOutcome, QueryRequest, QuerySettings,
};

fn request(params: bool, limited: bool) -> QueryRequest {
    QueryRequest {
        key: "pane".to_owned(),
        connection_id: "connection".to_owned(),
        sql: "SELECT 1".to_owned(),
        database: Some("db".to_owned()),
        schema: None,
        params: if params {
            vec![("id".to_owned(), json!(73))]
        } else {
            Vec::new()
        },
        settings: QuerySettings {
            limit_select_results: limited,
            query_result_limit: 250,
        },
    }
}

fn receipt() -> ExecutionReceipt {
    ExecutionReceipt {
        execution_id: "execution-1".to_owned(),
        stream_id: "stream-1".to_owned(),
        state: ExecutionState::Queued,
    }
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

fn rows_chunk() -> PublishedResultChunk {
    chunk(json!({
        "output": { "kind": "rows", "statementIndex": 0, "rows": [[1]],
                    "columns": [{ "name": "n", "dataType": "int4", "nullable": false }] }
    }))
}

fn terminal(state: ExecutionState, completeness: ResultCompleteness) -> ExecutionView {
    ExecutionView {
        execution_id: "execution-1".to_owned(),
        state,
        effect_outcome: EffectOutcome::Unknown,
        provenance: None,
        artifact_ids: vec!["artifact-1".to_owned()],
        result_completeness: completeness,
        truncation_reason: None,
        error_code: Some(ExecutionErrorCode::SqlError),
        runtime_binding: None,
    }
}

fn drive_to_recovery(run: &mut datazen_ui::session::run_editor_query::QueryRun) {
    let _ = run.step(QueryInput::ControllerReady);
    let _ = run.step(QueryInput::Executed(Ok(receipt())));
    let _ = run.step(QueryInput::Projected);
}

#[test]
fn envelope_pins_record_history_params_limit_and_target() {
    let call = query_command_call(&request(true, true));
    assert_eq!(
        call,
        CommandCall {
            command: "query".to_owned(),
            input: r#"{"sql":"SELECT 1","recordHistory":true,"params":{"id":73},"limit":250,"database":"db"}"#
                .to_owned(),
        },
        "React query envelope with bind values and a result limit"
    );
    let bare = query_command_call(&request(false, false));
    assert_eq!(
        bare.input,
        r#"{"sql":"SELECT 1","recordHistory":true,"database":"db"}"#,
        "no params and no limit when the setting is off"
    );
}

#[test]
fn resolves_the_same_pane_controller_with_the_normalized_target() {
    let (_, step) = start(request(false, false));
    assert_eq!(
        step.effects,
        vec![QueryEffect::GetController {
            key: "pane".to_owned(),
            target: ExecutionTarget {
                connection_id: "connection".to_owned(),
                namespace: NamespaceTarget {
                    database: Some("db".to_owned()),
                    catalog: None,
                    schema: None,
                    path: Vec::new(),
                },
                object: None,
            },
            owner: OwnerRef::Editor {
                client_instance_id: String::new(),
                editor_session_id: "pane".to_owned(),
            },
        }],
        "one controller per pane key with the panel-resolved target"
    );
}

#[test]
fn preserves_authoritative_artifact_references_before_terminal_completion() {
    for (state, completeness) in [
        (ExecutionState::Succeeded, ResultCompleteness::Complete),
        (ExecutionState::Failed, ResultCompleteness::Truncated),
        (ExecutionState::Cancelled, ResultCompleteness::Truncated),
    ] {
        let (mut run, _) = start(request(false, false));
        drive_to_recovery(&mut run);

        let step = run.step(QueryInput::Recovered {
            view: Some(terminal(state, completeness.clone())),
            chunks: vec![rows_chunk()],
            writable_binding: None,
        });
        let published = step.events.iter().find_map(|event| match event {
            QueryEvent::PublishedArtifacts { artifacts } => Some(artifacts),
            _ => None,
        });
        assert_eq!(
            published.cloned(),
            Some(datazen_ui::session::run_editor_query::ArtifactRefs {
                execution_id: "execution-1".to_owned(),
                artifact_ids: vec!["artifact-1".to_owned()],
                result_completeness: completeness,
            }),
            "{state:?}: authoritative artifact references are published before the outcome"
        );
        assert!(
            step.events
                .iter()
                .any(|event| matches!(event, QueryEvent::StatementSnapshot { index: 0, .. })),
            "{state:?}: the published prefix is emitted"
        );
        assert_eq!(step.effects, vec![QueryEffect::Refresh]);

        let step = run.step(QueryInput::Refreshed { total_time_ms: 12 });
        let has_done = step
            .events
            .iter()
            .any(|event| matches!(event, QueryEvent::Done { .. }));
        assert_eq!(has_done, state == ExecutionState::Succeeded, "done only on success");
        if state == ExecutionState::Succeeded {
            assert_eq!(step.outcome, Some(QueryOutcome::Completed));
        } else {
            assert!(matches!(step.outcome, Some(QueryOutcome::Failed(_))));
        }
        assert_eq!(step.effects, vec![QueryEffect::Dispose], "finally disposes");
    }
}

#[test]
fn emits_execution_started_before_the_projection_and_no_dispose_on_execute_failure() {
    let (mut run, _) = start(request(false, false));
    let step = run.step(QueryInput::ControllerReady);
    assert_eq!(
        step.events,
        Vec::new(),
        "no event before the execution receipt exists"
    );
    let step = run.step(QueryInput::Executed(Ok(receipt())));
    assert_eq!(
        step.events,
        vec![QueryEvent::ExecutionStarted {
            execution_id: "execution-1".to_owned()
        }]
    );

    let (mut failed, _) = start(request(false, false));
    let _ = failed.step(QueryInput::ControllerReady);
    let error = SessionError {
        code: ApiErrorCode::SessionLost,
        message: "gone".to_owned(),
    };
    let step = failed.step(QueryInput::Executed(Err(error.clone())));
    assert_eq!(
        step.effects,
        Vec::new(),
        "no projection exists on execute failure, so no dispose (React parity)"
    );
    assert_eq!(
        step.outcome,
        Some(QueryOutcome::Failed(QueryFailure::Session(error)))
    );
}

#[test]
fn failure_diagnostic_prefers_an_execution_error_chunk_then_the_view_code() {
    let mut view = terminal(ExecutionState::Failed, ResultCompleteness::Truncated);
    assert_eq!(
        execution_error_message(&view, &[]),
        "failed: sqlError; effect: unknown"
    );

    let diagnostic = chunk(json!({
        "output": { "kind": "executionError", "message": "constraint violated" }
    }));
    assert_eq!(
        execution_error_message(&view, &[diagnostic]),
        "failed: constraint violated; effect: unknown"
    );

    view.error_code = None;
    assert_eq!(
        execution_error_message(&view, &[]),
        "failed: Execution outcome unavailable; effect: unknown",
        "React fallback sentence when no diagnostic and no errorCode"
    );
}

#[test]
fn statement_snapshots_extract_columns_rows_and_a_verified_binding() {
    let source = StatementResultSource {
        execution_id: "execution-1".to_owned(),
        statement_index: serde_json::Number::from(0),
        writable_mapping: WritableMapping::Verified,
    };
    let mut with_source = rows_chunk();
    with_source.source = Some(source);
    let binding = datazen_ui::session::execution_projection::RuntimeResultBinding {
        handle: datazen_ui::session::execution_projection::SessionHandle {
            db_session_id: "s".to_owned(),
            runtime_epoch: "epoch".to_owned(),
        },
        resource_binding_id: "binding-1".to_owned(),
        execution_id: "execution-1".to_owned(),
    };

    let (mut run, _) = start(request(false, false));
    drive_to_recovery(&mut run);
    let step = run.step(QueryInput::Recovered {
        view: None,
        chunks: vec![with_source.clone()],
        writable_binding: Some(binding.clone()),
    });
    let snapshot = step.events.iter().find_map(|event| match event {
        QueryEvent::StatementSnapshot { result, .. } => Some(result),
        _ => None,
    });
    let snapshot = snapshot.expect("a statement snapshot is emitted");
    assert_eq!(snapshot.sql, "");
    assert_eq!(snapshot.rows, vec![vec![json!(1)]]);
    assert_eq!(snapshot.columns.len(), 1);
    assert_eq!(snapshot.columns[0].name, "n");
    assert_eq!(snapshot.runtime_binding, Some(binding.clone()));

    // A second identical recovery emits no duplicate snapshot.
    let step = run.step(QueryInput::Recovered {
        view: None,
        chunks: vec![with_source],
        writable_binding: Some(binding),
    });
    assert!(
        !step
            .events
            .iter()
            .any(|event| matches!(event, QueryEvent::StatementSnapshot { .. })),
        "unchanged results are not re-emitted"
    );
}

#[test]
fn recover_failure_never_resubmits_sql() {
    let (mut run, _) = start(request(false, false));
    let mut effects = Vec::new();
    effects.extend(run.step(QueryInput::ControllerReady).effects);
    effects.extend(run.step(QueryInput::Executed(Ok(receipt()))).effects);
    effects.extend(run.step(QueryInput::Projected).effects);
    let retry = run.step(QueryInput::RecoverFailed);
    effects.extend(retry.effects.clone());
    assert_eq!(retry.effects, vec![QueryEffect::Recover]);
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, QueryEffect::Execute { .. }))
            .count(),
        1,
        "recovery retries the same execution, never a new SQL submission"
    );
}

#[test]
fn stale_inputs_do_not_move_the_run() {
    let (mut run, _) = start(request(false, false));
    let step = run.step(QueryInput::Refreshed { total_time_ms: 0 });
    assert_eq!(step.effects, Vec::new());
    assert_eq!(step.events, Vec::new());
    assert_eq!(step.outcome, None);
    assert_eq!(run.phase(), "awaitController");
}
