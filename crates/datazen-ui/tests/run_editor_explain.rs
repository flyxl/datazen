//! T231: `lib/session/runEditorExplain.ts` port — real branch tests.
//!
//! Pinned values come from `runEditorExplain.test.ts`: pane `p1::split`,
//! `cfg-1 / A / s`, bind `{ id: 73 }`, receipt `explain-1`, and the
//! `Scan owned_temp` plan. Every answer is fed through the machine's `step`;
//! nothing sleeps, spawns or re-submits SQL.

use serde_json::json;

use datazen_ui::session::editor_session_controller::{
    ApiErrorCode, CommandCall, ExecutionReceipt, ExecutionState, ExecutionTarget, NamespaceTarget,
    OwnerRef, SessionError,
};
use datazen_ui::session::execution_projection::{
    ArtifactChunk, EffectOutcome, ExecutionErrorCode, ExecutionView, PublishedResultChunk,
    ResultCompleteness,
};
use datazen_ui::session::run_editor_explain::{
    explain_command_call, explain_failure_message, explain_plan_text, start, ExplainEffect,
    ExplainFailure, ExplainInput, ExplainOutcome, ExplainPlan, ExplainRequest,
};

fn request(key: &str, sql: &str, database: Option<&str>, schema: Option<&str>, with_params: bool) -> ExplainRequest {
    ExplainRequest {
        key: key.to_owned(),
        connection_id: "cfg-1".to_owned(),
        sql: sql.to_owned(),
        database: database.map(str::to_owned),
        schema: schema.map(str::to_owned),
        params: if with_params {
            vec![("id".to_owned(), json!(73))]
        } else {
            Vec::new()
        },
    }
}

fn receipt() -> ExecutionReceipt {
    ExecutionReceipt {
        execution_id: "explain-1".to_owned(),
        stream_id: "stream-1".to_owned(),
        state: ExecutionState::Queued,
    }
}

fn view(state: ExecutionState, error_code: Option<ExecutionErrorCode>, effect: EffectOutcome) -> ExecutionView {
    ExecutionView {
        execution_id: "explain-1".to_owned(),
        state,
        effect_outcome: effect,
        provenance: None,
        artifact_ids: vec!["plan-1".to_owned()],
        result_completeness: ResultCompleteness::Complete,
        truncation_reason: None,
        error_code,
        runtime_binding: None,
    }
}

fn plan_chunk(plan_text: &str) -> PublishedResultChunk {
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
        payload: json!({ "output": { "kind": "commandResult", "data": { "planText": plan_text } } }),
    }
}

fn terminal_success() -> Vec<PublishedResultChunk> {
    vec![plan_chunk("Scan owned_temp")]
}

fn execute_calls(effects: &[ExplainEffect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, ExplainEffect::Execute { .. }))
        .count()
}

#[test]
fn explain_command_call_pins_the_envelope_and_optional_fields() {
    let call = explain_command_call(
        "SELECT :id",
        &[("id".to_owned(), json!(73))],
        Some("A"),
        Some("s"),
    );
    assert_eq!(
        call,
        CommandCall {
            command: "explain".to_owned(),
            input: r#"{"sql":"SELECT :id","params":{"id":73},"database":"A","schema":"s"}"#
                .to_owned(),
        },
        "React's explain envelope with bind values"
    );

    let bare = explain_command_call("SELECT 1", &[], Some(""), None);
    assert_eq!(
        bare.input,
        r#"{"sql":"SELECT 1"}"#,
        "empty params and a falsy database are omitted"
    );
}

#[test]
fn uses_the_same_pane_controller_and_session_command_with_bind_values() {
    let (mut run, step) = start(request("p1::split", "SELECT :id", Some("A"), Some("s"), true));
    assert_eq!(
        step.effects,
        vec![ExplainEffect::GetController {
            key: "p1::split".to_owned(),
            target: ExecutionTarget {
                connection_id: "cfg-1".to_owned(),
                namespace: NamespaceTarget {
                    database: Some("A".to_owned()),
                    catalog: None,
                    schema: Some("s".to_owned()),
                    path: Vec::new(),
                },
                object: None,
            },
            owner: OwnerRef::Editor {
                client_instance_id: String::new(),
                editor_session_id: "p1::split".to_owned(),
            },
        }],
        "the pane key selects the controller with a normalized target and its own owner"
    );

    let step = run.step(ExplainInput::ControllerReady);
    assert_eq!(
        execute_calls(&step.effects),
        1,
        "exactly one session command"
    );
    assert_eq!(
        step.effects,
        vec![ExplainEffect::Execute {
            call: CommandCall {
                command: "explain".to_owned(),
                input: r#"{"sql":"SELECT :id","params":{"id":73},"database":"A","schema":"s"}"#
                    .to_owned(),
            }
        }]
    );

    let step = run.step(ExplainInput::Executed(Ok(receipt())));
    assert_eq!(
        step.effects,
        vec![ExplainEffect::CreateProjection {
            receipt: receipt(),
            handle: None,
        }]
    );
    let step = run.step(ExplainInput::Projected);
    assert_eq!(step.effects, vec![ExplainEffect::Recover]);

    let step = run.step(ExplainInput::Recovered {
        view: Some(view(ExecutionState::Succeeded, None, EffectOutcome::Completed)),
        chunks: terminal_success(),
    });
    assert_eq!(
        step.effects,
        vec![ExplainEffect::Refresh],
        "a terminal view refreshes once"
    );
    assert_eq!(execute_calls(&step.effects), 0, "no SQL is re-submitted");

    let step = run.step(ExplainInput::Refreshed);
    assert_eq!(
        step.outcome,
        Some(ExplainOutcome::Plan(ExplainPlan {
            plan_text: "Scan owned_temp".to_owned()
        }))
    );
    assert_eq!(step.effects, vec![ExplainEffect::Dispose], "finally disposes");
    assert_eq!(run.phase(), "done");
}

#[test]
fn recovers_a_lost_artifact_read_without_resubmitting_the_plan() {
    let (mut run, step) = start(request("p1", "SELECT 1", Some("A"), None, false));
    let mut all_effects = step.effects;
    all_effects.extend(run.step(ExplainInput::ControllerReady).effects);
    all_effects.extend(run.step(ExplainInput::Executed(Ok(receipt()))).effects);
    all_effects.extend(run.step(ExplainInput::Projected).effects);

    // One failed artifact read re-emits Recover only; the execute count stays 1.
    let retry = run.step(ExplainInput::RecoverFailed);
    assert_eq!(retry.effects, vec![ExplainEffect::Recover]);
    all_effects.extend(retry.effects.clone());
    assert_eq!(execute_calls(&all_effects), 1, "the plan was never resubmitted");

    let step = run.step(ExplainInput::Recovered {
        view: Some(view(ExecutionState::Succeeded, None, EffectOutcome::Completed)),
        chunks: terminal_success(),
    });
    let step = run.step(ExplainInput::Refreshed);
    assert_eq!(
        step.outcome,
        Some(ExplainOutcome::Plan(ExplainPlan {
            plan_text: "Scan owned_temp".to_owned()
        }))
    );
    all_effects.extend(step.effects);
    assert_eq!(
        execute_calls(&all_effects),
        1,
        "two artifact reads, one execution"
    );
}

#[test]
fn preserves_failed_and_cancelled_outcomes_without_reopening_a_session() {
    for state in [ExecutionState::Failed, ExecutionState::Cancelled] {
        let (mut run, mut all_effects) = {
            let (run, step) = start(request("p1", "SELECT 1", Some("A"), None, false));
            let effects = step.effects;
            (run, effects)
        };
        all_effects.extend(run.step(ExplainInput::ControllerReady).effects);
        all_effects.extend(run.step(ExplainInput::Executed(Ok(receipt()))).effects);
        all_effects.extend(run.step(ExplainInput::Projected).effects);
        let state_name = if state == ExecutionState::Failed {
            "failed"
        } else {
            "cancelled"
        };
        let terminal = view(
            state,
            Some(ExecutionErrorCode::ResourceLost),
            EffectOutcome::Unknown,
        );
        assert_eq!(
            explain_failure_message(&terminal),
            format!("{state_name}: resourceLost; effect: unknown")
        );
        let step = run.step(ExplainInput::Recovered {
            view: Some(terminal),
            chunks: Vec::new(),
        });
        all_effects.extend(step.effects);
        let step = run.step(ExplainInput::Refreshed);
        assert_eq!(
            step.outcome,
            Some(ExplainOutcome::Failed(ExplainFailure::Execution {
                message: format!("{state_name}: resourceLost; effect: unknown")
            })),
            "{state_name} preserves the backend verdict"
        );
        assert_eq!(step.effects, vec![ExplainEffect::Dispose]);
        all_effects.extend(step.effects);
        assert_eq!(execute_calls(&all_effects), 1, "{state_name} never re-executes");
    }
}

#[test]
fn recover_exhaustion_after_twenty_one_failures() {
    let (mut run, _) = start(request("p1", "SELECT 1", Some("A"), None, false));
    let _ = run.step(ExplainInput::ControllerReady);
    let _ = run.step(ExplainInput::Executed(Ok(receipt())));
    let _ = run.step(ExplainInput::Projected);
    let mut recovers = 0;
    let mut outcome = None;
    for _ in 0..21 {
        let step = run.step(ExplainInput::RecoverFailed);
        recovers += step
            .effects
            .iter()
            .filter(|effect| matches!(effect, ExplainEffect::Recover))
            .count();
        if step.outcome.is_some() {
            outcome = step.outcome;
            assert_eq!(step.effects, vec![ExplainEffect::Dispose]);
            break;
        }
    }
    assert_eq!(recovers, 20, "20 retries before the 21st failure throws");
    assert_eq!(
        outcome,
        Some(ExplainOutcome::Failed(ExplainFailure::RecoverExhausted))
    );
}

#[test]
fn plan_scan_skips_unrelated_chunks_and_requires_a_string_plan_text() {
    let unrelated = PublishedResultChunk {
        payload: json!({ "output": { "kind": "queryResult", "data": { "planText": "nope" } } }),
        ..plan_chunk("Scan owned_temp")
    };
    let non_string = PublishedResultChunk {
        payload: json!({ "output": { "kind": "commandResult", "data": { "planText": 7 } } }),
        ..plan_chunk("Scan owned_temp")
    };
    assert_eq!(explain_plan_text(&[unrelated, non_string]), None);
    assert_eq!(
        explain_plan_text(&[non_string_with_payload(), plan_chunk("Scan owned_temp")]).as_deref(),
        Some("Scan owned_temp")
    );
}

fn non_string_with_payload() -> PublishedResultChunk {
    PublishedResultChunk {
        payload: json!({ "output": { "kind": "commandResult", "data": { "planText": 7 } } }),
        ..plan_chunk("ignored")
    }
}

#[test]
fn stale_inputs_do_not_move_the_run() {
    let (mut run, _) = start(request("p1", "SELECT 1", Some("A"), None, false));
    let step = run.step(ExplainInput::Refreshed);
    assert_eq!(step.effects, Vec::new());
    assert_eq!(step.outcome, None);
    assert_eq!(run.phase(), "awaitController");
}

#[test]
fn execute_failure_emits_no_dispose_because_no_projection_exists_yet() {
    let (mut run, _) = start(request("p1", "SELECT 1", Some("A"), None, false));
    let _ = run.step(ExplainInput::ControllerReady);
    let error = SessionError {
        code: ApiErrorCode::SessionLost,
        message: "gone".to_owned(),
    };
    let step = run.step(ExplainInput::Executed(Err(error.clone())));
    assert_eq!(
        step.effects,
        Vec::new(),
        "no projection exists on execute failure, so no dispose (React parity)"
    );
    assert_eq!(
        step.outcome,
        Some(ExplainOutcome::Failed(ExplainFailure::Session(error))),
        "the execute failure outcome is preserved"
    );
    assert_eq!(run.phase(), "done");
}
