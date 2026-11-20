//! T311: `lib/session/targetCommand.ts` port — real branch tests.
//!
//! Pinned values mirror the React source: `cfg-1`, profile revision `7`,
//! `ResourceBusy` admission retries, the three response kinds and the exact
//! failure sentences. No async or wall clock — time is supplied in inputs.

use serde_json::{json, Value};

use crate::session::editor_session_controller::{
    CommandCall, ExecutionErrorCode, ExecutionReceipt, ExecutionState, ExecutionTarget,
    NamespaceTarget, ObjectTarget, SubmissionToken,
};
use crate::session::execution_projection::{
    ArtifactChunk, EffectOutcome, ExecutionView, PublishedResultChunk,
};
use crate::session::target_command::{
    database_discovery_target, metadata_scope_key, metadata_target, schema_call, schema_from_result,
    ActiveConnectionEntry, MetadataIdentity, ResponseKind, TargetApiError, TargetCommandEffect,
    TargetCommandError, TargetCommandInput, TargetCommandOutcome, TargetCommandStep, TargetConnectionProfile,
    TargetRun,
};

fn active() -> Vec<ActiveConnectionEntry> {
    vec![ActiveConnectionEntry {
        db_session_id: "db-session".to_owned(),
        connection_id: "cfg-1".to_owned(),
        current_database: Some("app".to_owned()),
    }]
}

fn profiles() -> Vec<TargetConnectionProfile> {
    vec![TargetConnectionProfile {
        connection_id: "cfg-1".to_owned(),
        enabled: true,
        config_revision: "7".to_owned(),
    }]
}

fn target() -> ExecutionTarget {
    ExecutionTarget {
        connection_id: "cfg-1".to_owned(),
        namespace: NamespaceTarget {
            database: Some("app".to_owned()),
            schema: Some("public".to_owned()),
            catalog: None,
            path: Vec::new(),
        },
        object: Some(ObjectTarget {
            kind: "relation".to_owned(),
            name: "users".to_owned(),
            signature: None,
        }),
    }
}

fn call() -> CommandCall {
    CommandCall {
        command: "get_table_schema".to_owned(),
        input: "{}".to_owned(),
    }
}

fn token() -> SubmissionToken {
    SubmissionToken {
        idempotency_key: "token-1".to_owned(),
        expires_at: 999,
    }
}

fn receipt() -> ExecutionReceipt {
    ExecutionReceipt {
        execution_id: "execution-1".to_owned(),
        stream_id: "stream-1".to_owned(),
        state: ExecutionState::Queued,
    }
}

fn view(state: ExecutionState) -> ExecutionView {
    ExecutionView {
        execution_id: "execution-1".to_owned(),
        state,
        effect_outcome: EffectOutcome::Unknown,
        provenance: None,
        artifact_ids: vec!["artifact-1".to_owned()],
        result_completeness: crate::session::execution_projection::ResultCompleteness::Complete,
        truncation_reason: None,
        error_code: Some(ExecutionErrorCode::SqlError),
        runtime_binding: None,
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
            result_completeness: crate::session::execution_projection::ResultCompleteness::Complete,
        },
        source: None,
        payload,
    }
}

fn drive_to_admission(run: &mut TargetRun) -> TargetCommandStep {
    let step = run.step(TargetCommandInput::Profiles(profiles()));
    let step2 = run.step(TargetCommandInput::Token(token()));
    TargetCommandStep {
        effects: [step.effects, step2.effects].concat(),
        phases: [step.phases, step2.phases].concat(),
        outcome: None,
    }
}

#[test]
fn metadata_target_resolves_owner_and_defaults_the_database() {
    let from_store = metadata_target("db-session", None, Some("public"), Some("users"), None, &active())
        .expect("owner from the active store");
    assert_eq!(from_store.connection_id, "cfg-1");
    assert_eq!(from_store.namespace.database.as_deref(), Some("app"));
    assert_eq!(from_store.namespace.schema.as_deref(), Some("public"));
    assert_eq!(
        from_store.object,
        Some(ObjectTarget {
            kind: "relation".to_owned(),
            name: "users".to_owned(),
            signature: None,
        })
    );

    let explicit = metadata_target(
        "unknown-session",
        Some("override"),
        None,
        None,
        Some("cfg-2"),
        &active(),
    )
    .expect("explicit connection id wins");
    assert_eq!(explicit.connection_id, "cfg-2");
    assert_eq!(explicit.namespace.database.as_deref(), Some("override"));
    assert_eq!(explicit.namespace.schema, None);
    assert_eq!(explicit.object, None);

    let missing = metadata_target("other", None, None, None, None, &active())
        .err()
        .expect("no owner for an unknown session");
    assert_eq!(missing, TargetCommandError::MetadataOwnerUnavailable);
    assert_eq!(missing.message(), "Metadata owner unavailable");
}

#[test]
fn database_discovery_target_clears_the_namespace() {
    let discovered = database_discovery_target("db-session", None, &active())
        .expect("discovery target");
    assert_eq!(discovered.connection_id, "cfg-1");
    assert_eq!(discovered.namespace, NamespaceTarget::default());
    assert_eq!(discovered.object, None);
}

#[test]
fn schema_envelope_and_result_validation() {
    let call = schema_call("users", "app", Some("public"));
    assert_eq!(call.command, "get_table_schema");
    assert_eq!(
        call.input,
        r#"{"database":"app","schema":"public","table":"users"}"#
    );
    let valid = json!({ "schema": { "columns": [1] } });
    assert_eq!(schema_from_result(Some(&valid)), Ok(valid["schema"].clone()));
    assert_eq!(
        schema_from_result(Some(&json!({ "schema": {} }))),
        Err(TargetCommandError::InvalidSchemaResult)
    );
    assert_eq!(
        schema_from_result(None),
        Err(TargetCommandError::InvalidSchemaResult)
    );
}

#[test]
fn scope_key_orders_identity_and_profile_facts() {
    let identity = MetadataIdentity {
        organization_id: "org-1".to_owned(),
        principal_id: "user-1".to_owned(),
    };
    let profile = profiles().remove(0);
    assert_eq!(
        metadata_scope_key("desktop", &identity, &profile),
        Ok(r#"["desktop","org-1","user-1","cfg-1","7"]"#.to_owned())
    );
    let disabled = TargetConnectionProfile { enabled: false, ..profile };
    assert_eq!(
        metadata_scope_key("desktop", &identity, &disabled),
        Err(TargetCommandError::ProfileUnavailableForScope)
    );
}

#[test]
fn runner_requires_an_enabled_matching_profile() {
    for list in [
        Vec::new(),
        vec![TargetConnectionProfile {
            connection_id: "other".to_owned(),
            enabled: true,
            config_revision: "7".to_owned(),
        }],
        vec![TargetConnectionProfile {
            enabled: false,
            ..profiles().remove(0)
        }],
    ] {
        let (mut run, start) = TargetRun::new(target(), call(), ResponseKind::CommandResult);
        assert_eq!(start.phases, ["profiles"]);
        let step = run.step(TargetCommandInput::Profiles(list));
        assert_eq!(
            step.outcome,
            Some(TargetCommandOutcome::Failed(
                TargetCommandError::ProfileUnavailable
            ))
        );
        assert_eq!(run.phase(), "done");
    }
}

#[test]
fn admission_retries_resource_busy_with_the_exact_request() {
    let (mut run, start) = TargetRun::new(target(), call(), ResponseKind::CommandResult);
    let setup = drive_to_admission(&mut run);
    assert_eq!(
        [start.phases, setup.phases].concat(),
        ["profiles", "submission-token", "target-admission"]
    );
    let first = match setup.effects.last() {
        Some(TargetCommandEffect::ExecuteAtTarget { request }) => request.clone(),
        other => panic!("expected ExecuteAtTarget, got {other:?}"),
    };
    assert_eq!(first.expected_config_revision, "7");
    assert_eq!(first.idempotency_key, "token-1");
    assert_eq!(first.target, target());
    assert_eq!(first.call, call());

    let retry = run.step(TargetCommandInput::Execute {
        result: Err(TargetApiError::new("ResourceBusy", "busy")),
        now_ms: 1_000,
    });
    assert_eq!(
        retry.effects,
        vec![TargetCommandEffect::ExecuteAtTarget { request: first }],
        "the same request/token is retried before any receipt exists"
    );
    let accepted = run.step(TargetCommandInput::Execute {
        result: Ok(receipt()),
        now_ms: 1_100,
    });
    assert_eq!(accepted.effects, vec![TargetCommandEffect::Recover]);
}

#[test]
fn admission_fails_on_other_errors_and_after_the_deadline() {
    let (mut run, _) = TargetRun::new(target(), call(), ResponseKind::CommandResult);
    let _ = drive_to_admission(&mut run);
    let error = TargetApiError::new("HostRejected", "nope");
    let step = run.step(TargetCommandInput::Execute {
        result: Err(error.clone()),
        now_ms: 0,
    });
    assert_eq!(
        step.outcome,
        Some(TargetCommandOutcome::Failed(TargetCommandError::Admission(error)))
    );

    let (mut expired, _) = TargetRun::new(target(), call(), ResponseKind::CommandResult);
    let _ = drive_to_admission(&mut expired);
    let _ = expired.step(TargetCommandInput::Execute {
        result: Err(TargetApiError::new("ResourceBusy", "busy")),
        now_ms: 0,
    });
    let step = expired.step(TargetCommandInput::Execute {
        result: Err(TargetApiError::new("ResourceBusy", "busy")),
        now_ms: 10_000,
    });
    assert_eq!(
        step.outcome,
        Some(TargetCommandOutcome::Failed(TargetCommandError::Admission(
            TargetApiError::new("ResourceBusy", "busy")
        ))),
        "the 10-second acquisition bound is authoritative"
    );
}

#[test]
fn response_kinds_extract_completion_rows_or_command_result() {
    let rows_chunk = chunk(json!({
        "output": { "kind": "rows", "rows": [[1], [2]] }
    }));
    let other_chunk = chunk(json!({
        "output": { "kind": "queryResult", "rows": [[9]] }
    }));
    let data_chunk = chunk(json!({
        "output": { "kind": "commandResult", "data": { "schema": { "columns": [] } } }
    }));

    for (kind, expected) in [
        (ResponseKind::Completion, TargetCommandOutcome::Completed),
        (
            ResponseKind::Rows,
            TargetCommandOutcome::Value(json!([[1], [2]])),
        ),
        (
            ResponseKind::CommandResult,
            TargetCommandOutcome::Value(json!({ "schema": { "columns": [] } })),
        ),
    ] {
        let (mut run, _) = TargetRun::new(target(), call(), kind);
        let _ = drive_to_admission(&mut run);
        let _ = run.step(TargetCommandInput::Execute {
            result: Ok(receipt()),
            now_ms: 0,
        });
        let step = run.step(TargetCommandInput::Recovered {
            view: Some(view(ExecutionState::Succeeded)),
            chunks: vec![rows_chunk.clone(), other_chunk.clone(), data_chunk.clone()],
        });
        assert_eq!(step.outcome, Some(expected), "{kind:?} response extracted");
        assert_eq!(step.effects, vec![TargetCommandEffect::Dispose]);
    }

    let (mut missing, _) = TargetRun::new(target(), call(), ResponseKind::CommandResult);
    let _ = drive_to_admission(&mut missing);
    let _ = missing.step(TargetCommandInput::Execute {
        result: Ok(receipt()),
        now_ms: 0,
    });
    let step = missing.step(TargetCommandInput::Recovered {
        view: Some(view(ExecutionState::Succeeded)),
        chunks: vec![other_chunk],
    });
    assert_eq!(
        step.outcome,
        Some(TargetCommandOutcome::Failed(
            TargetCommandError::TypedResultUnavailable
        ))
    );
}

#[test]
fn failed_terminal_and_dispose_failure_keep_their_messages() {
    for state in [ExecutionState::Failed, ExecutionState::Cancelled] {
        let (mut run, _) = TargetRun::new(target(), call(), ResponseKind::CommandResult);
        let _ = drive_to_admission(&mut run);
        let _ = run.step(TargetCommandInput::Execute {
            result: Ok(receipt()),
            now_ms: 0,
        });
        let step = run.step(TargetCommandInput::Recovered {
            view: Some(view(state)),
            chunks: Vec::new(),
        });
        assert_eq!(
            step.outcome,
            Some(TargetCommandOutcome::Failed(TargetCommandError::Execution {
                message: "sqlError; effect: unknown".to_owned()
            })),
            "{state:?} preserves the backend outcome"
        );
    }

    let (mut run, _) = TargetRun::new(target(), call(), ResponseKind::Completion);
    let _ = drive_to_admission(&mut run);
    let step = run.step(TargetCommandInput::DisposeFailed(TargetApiError::new("Boom", "dispose failed")));
    assert_eq!(
        step.outcome,
        Some(TargetCommandOutcome::Failed(TargetCommandError::Dispose(
            TargetApiError::new("Boom", "dispose failed")
        )))
    );
    assert_eq!(step.phases, ["dispose"]);
}
