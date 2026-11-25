//! T122 independent audit (tester, not coder).
//!
//! Coverage-driven probes of branches the shipped `execution_projection.rs`
//! tests do not pin: the full terminal-state truth table on the iterator-done
//! resubscribe gate, `streamResetRequired` arriving *through the subscription*
//! (the `Resubscribe` resume), host/identity error projection (no swallow),
//! the multi-artifact `count == 0` boundary and exclusive `1..count` fan-out,
//! embedded-`source` rejection branches, the binding execution-id guard and
//! the artifact-id identity check. Every assertion is on observable state /
//! emitted effects, never a helper self-report.

use datazen_ui::session::execution_projection::*;

fn handle(id: &str, epoch: &str) -> SessionHandle {
    SessionHandle { db_session_id: id.to_owned(), runtime_epoch: epoch.to_owned() }
}

fn projection() -> ExecutionProjection {
    ExecutionProjection::new(
        "desktop".to_owned(),
        ExecutionReceipt {
            execution_id: "e".to_owned(),
            stream_id: "stream".to_owned(),
            state: ExecutionState::Queued,
        },
        Some(handle("s", "epoch")),
    )
}

fn source(exec: &str) -> StatementResultSource {
    StatementResultSource {
        execution_id: exec.to_owned(),
        statement_index: serde_json::Number::from(0),
        writable_mapping: WritableMapping::Verified,
    }
}

fn chunk_for(
    artifact: &str,
    index: &str,
    total: Option<&str>,
    published: Option<&str>,
    bytes: Vec<u8>,
) -> ArtifactChunk {
    ArtifactChunk {
        artifact_id: artifact.to_owned(),
        chunk_index: index.to_owned(),
        total_chunks: total.map(str::to_owned),
        published_chunk_count: published.map(str::to_owned),
        published_byte_length: None,
        offset: "0".to_owned(),
        bytes,
        result_completeness: ResultCompleteness::Truncated,
    }
}

fn chunk(index: &str, total: Option<&str>, bytes: Vec<u8>) -> ArtifactChunk {
    chunk_for("a", index, total, None, bytes)
}

fn binding(exec: &str) -> RuntimeResultBinding {
    RuntimeResultBinding {
        handle: handle("s", "epoch"),
        resource_binding_id: "r".to_owned(),
        execution_id: exec.to_owned(),
    }
}

fn view(state: ExecutionState, artifacts: &[&str], rb: Option<RuntimeResultBinding>) -> ExecutionView {
    ExecutionView {
        execution_id: "e".to_owned(),
        state,
        effect_outcome: EffectOutcome::Unknown,
        provenance: None,
        artifact_ids: artifacts.iter().map(|id| (*id).to_owned()).collect(),
        result_completeness: ResultCompleteness::Truncated,
        truncation_reason: None,
        error_code: None,
        runtime_binding: rb,
    }
}

fn event(seq: &str, index: &str, exec: &str) -> EventEnvelope {
    EventEnvelope {
        stream_id: "stream".to_owned(),
        sequence: seq.to_owned(),
        runtime_epoch: "epoch".to_owned(),
        execution_id: Some(exec.to_owned()),
        job_id: None,
        session_handle: Some(handle("s", "epoch")),
        context_revision: Some("1".to_owned()),
        payload: ConnectionEvent::ResultChunk {
            artifact_id: "a".to_owned(),
            chunk_index: index.to_owned(),
            source: source(exec),
        },
    }
}

fn get_call(effects: &[ProjectionEffect]) -> u64 {
    effects
        .iter()
        .find_map(|e| match e {
            ProjectionEffect::GetExecution { call_id, .. } => Some(*call_id),
            _ => None,
        })
        .expect("expected a GetExecution effect")
}

fn read_call(effects: &[ProjectionEffect]) -> u64 {
    effects
        .iter()
        .rev()
        .find_map(|e| match e {
            ProjectionEffect::ReadArtifact { call_id, .. } => Some(*call_id),
            _ => None,
        })
        .expect("expected a ReadArtifact effect")
}

fn sub_call(effects: &[ProjectionEffect]) -> u64 {
    effects
        .iter()
        .find_map(|e| match e {
            ProjectionEffect::SubscribeEvents { call_id, .. } => Some(*call_id),
            _ => None,
        })
        .expect("expected a SubscribeEvents effect")
}

fn has_sub(effects: &[ProjectionEffect]) -> bool {
    effects.iter().any(|e| matches!(e, ProjectionEffect::SubscribeEvents { .. }))
}

fn accept_chunk(p: &mut ExecutionProjection, seq: &str, index: &str, served: ArtifactChunk) {
    let mut effects = Vec::new();
    p.accept(event(seq, index, "e"), &mut effects);
    let id = read_call(&effects);
    p.on_read_artifact(id, Ok(served), &mut effects);
}

fn settle_single_artifact_recover(p: &mut ExecutionProjection, served: ArtifactChunk, state: ExecutionState) {
    let effects = p.recover();
    let id = get_call(&effects);
    let mut effects = Vec::new();
    p.on_get_execution(id, Ok(view(state, &["a"], None)), &mut effects);
    let id = read_call(&effects);
    p.on_read_artifact(id, Ok(served), &mut effects);
}

/// Whether a terminal-state view resubscribes after the stream iterator closes.
fn resubscribes_after_done(state: ExecutionState) -> bool {
    let mut p = projection();
    let first = p.consume();
    let sub = sub_call(&first);
    let mut effects = Vec::new();
    p.on_event(sub, StreamItem::Done, &mut effects);
    let get = get_call(&effects);
    let mut fed = Vec::new();
    p.on_get_execution(get, Ok(view(state, &["a"], None)), &mut fed);
    let read = read_call(&fed);
    p.on_read_artifact(read, Ok(chunk("0", Some("1"), vec![1])), &mut fed);
    has_sub(&fed)
}

// 1. Terminal-state truth table — React gate is exactly
//    ['succeeded','failed','cancelled']; CancelRequested is NOT terminal.
#[test]
fn terminal_gate_covers_succeeded_failed_cancelled_only() {
    for state in [ExecutionState::Succeeded, ExecutionState::Failed, ExecutionState::Cancelled] {
        assert!(!resubscribes_after_done(state), "{state:?} is terminal: must not resubscribe");
    }
    for state in [ExecutionState::Queued, ExecutionState::Running, ExecutionState::CancelRequested] {
        assert!(resubscribes_after_done(state), "{state:?} is live: must resubscribe");
    }
}

// 2. streamResetRequired delivered through the subscription must clear the
//    cursor before dedupe and resubscribe from afterSequence:null once recover
//    completes (the `Resubscribe` resume branch).
#[test]
fn reset_via_stream_resubscribes_from_null_after_recover() {
    let mut p = projection();
    let first = p.consume();
    let sub = sub_call(&first);
    accept_chunk(&mut p, "5", "0", chunk("0", Some("1"), vec![1]));
    assert_eq!(p.sequence(), Some("5"));

    let mut reset = event("1", "0", "e"); // below the old cursor: must still rebuild
    reset.payload = ConnectionEvent::StreamResetRequired { reason: "restart".into() };
    let mut effects = Vec::new();
    p.on_event(sub, StreamItem::Event(reset), &mut effects);
    assert_eq!(p.sequence(), None, "reset clears the cursor before dedupe");
    assert!(!has_sub(&effects), "resubscribe happens only after recover completes");

    let get = get_call(&effects);
    let mut fed = Vec::new();
    p.on_get_execution(get, Ok(view(ExecutionState::Running, &["a"], None)), &mut fed);
    let read = read_call(&fed);
    p.on_read_artifact(read, Ok(chunk("0", Some("1"), vec![1])), &mut fed);
    match fed.iter().find(|e| matches!(e, ProjectionEffect::SubscribeEvents { .. })) {
        Some(ProjectionEffect::SubscribeEvents { after_sequence, .. }) => {
            assert_eq!(after_sequence, &None, "re-subscribe resumes from zero");
        }
        other => panic!("expected a resubscribe after recover, got {other:?}"),
    }
}

// 3. Host failure and execution-identity mismatch must be projected as errors
//    and must NOT start the chunk fan-out (no swallowed failure).
#[test]
fn host_and_identity_errors_are_projected_without_chunk_fanout() {
    let mut p = projection();
    let effects = p.recover();
    let get = get_call(&effects);
    let mut fed = Vec::new();
    let mut bad = view(ExecutionState::Running, &[], None);
    bad.execution_id = "other".into();
    p.on_get_execution(get, Ok(bad), &mut fed);
    assert_eq!(
        p.error().map(ProjectionError::message).as_deref(),
        Some("Execution identity mismatch")
    );
    assert!(p.view().is_none());
    assert!(!fed.iter().any(|e| matches!(e, ProjectionEffect::ReadArtifact { .. })));

    let mut p = projection();
    let effects = p.recover();
    let get = get_call(&effects);
    let mut fed = Vec::new();
    p.on_get_execution(get, Err(ProjectionError::Host("boom".into())), &mut fed);
    assert_eq!(p.error().map(ProjectionError::message).as_deref(), Some("boom"));
    assert!(p.view().is_none());
    assert!(!fed.iter().any(|e| matches!(e, ProjectionEffect::ReadArtifact { .. })));
}

// 4. A count-0 artifact publishes nothing; the next artifact's 1..count loop
//    is exclusive at the top boundary (indices 0,1 only when totalChunks=2).
#[test]
fn zero_count_artifact_is_skipped_and_chunk_fanout_is_exclusive() {
    let mut p = projection();
    let effects = p.recover();
    let get = get_call(&effects);
    let mut fed = Vec::new();
    p.on_get_execution(get, Ok(view(ExecutionState::Running, &["a", "b"], None)), &mut fed);

    let id = read_call(&fed);
    p.on_read_artifact(id, Ok(chunk_for("a", "0", Some("0"), None, vec![1])), &mut fed);
    let id = read_call(&fed);
    p.on_read_artifact(id, Ok(chunk_for("b", "0", Some("2"), None, vec![2])), &mut fed);
    let id = read_call(&fed);
    p.on_read_artifact(id, Ok(chunk_for("b", "1", Some("2"), None, vec![3])), &mut fed);

    let published: Vec<(String, String)> = p
        .chunks()
        .iter()
        .map(|c| (c.chunk.artifact_id.clone(), c.chunk.chunk_index.clone()))
        .collect();
    assert_eq!(
        published,
        vec![("b".to_owned(), "0".to_owned()), ("b".to_owned(), "1".to_owned())],
        "artifact a has totalChunks=0 (nothing published); b publishes indices 0..2 exclusive"
    );
    assert_eq!(p.chunks().len(), 2, "no off-by-one extra chunk at index 2");
    let last_read = fed.iter().rev().find_map(|e| match e {
        ProjectionEffect::ReadArtifact { chunk_index, .. } => Some(chunk_index.clone()),
        _ => None,
    });
    assert_eq!(
        last_read.as_deref(),
        Some("1"),
        "the 1..count fan-out must be exclusive: no ReadArtifact emitted at index == totalChunks"
    );
}

// 5. Embedded source adoption guards: mapping must be verified/readOnly,
//    statementIndex must be a JSON number; binding execution id must match.
#[test]
fn embedded_source_and_binding_execution_guards_reject_invalid_inputs() {
    let mut p = projection();
    let bad_map = br#"{"source":{"executionId":"e","statementIndex":0,"writableMapping":"pending"}}"#;
    settle_single_artifact_recover(&mut p, chunk("0", Some("1"), bad_map.to_vec()), ExecutionState::Running);
    assert!(p.chunks()[0].source.is_none(), "writableMapping outside verified/readOnly is not adopted");

    let mut p = projection();
    let bad_index = br#"{"source":{"executionId":"e","statementIndex":"0","writableMapping":"verified"}}"#;
    settle_single_artifact_recover(&mut p, chunk("0", Some("1"), bad_index.to_vec()), ExecutionState::Running);
    assert!(p.chunks()[0].source.is_none(), "a string statementIndex is not adopted");

    let mut p = projection();
    let mut e = event("1", "0", "e");
    e.payload = ConnectionEvent::ExecutionChanged {
        execution: view(ExecutionState::Succeeded, &[], Some(binding("other"))),
    };
    p.accept(e, &mut Vec::new());
    assert!(
        p.writable_binding(Some(&handle("s", "epoch"))).is_none(),
        "a binding whose execution id differs from the receipt is never writable"
    );
}

// 6. A host chunk with the wrong artifact id is rejected exactly like a wrong
//    chunk index, and never enters the published projection.
#[test]
fn wrong_artifact_id_chunk_is_rejected_without_publishing() {
    let mut p = projection();
    let mut effects = Vec::new();
    p.accept(event("1", "0", "e"), &mut effects);
    let id = read_call(&effects);
    let mut wrong = chunk("0", Some("1"), vec![1]);
    wrong.artifact_id = "other".into();
    p.on_read_artifact(id, Ok(wrong), &mut effects);
    assert_eq!(
        p.error().map(ProjectionError::message).as_deref(),
        Some("Artifact read returned a different chunk")
    );
    assert!(p.chunks().is_empty());
    assert_eq!(p.sequence(), None, "a failed read leaves the cursor put");
}
