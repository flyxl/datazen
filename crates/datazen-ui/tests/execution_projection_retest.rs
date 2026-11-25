//! T122 independent RE-audit (retest of the D1 fix, 2daad6c32).
//!
//! D1 (the unused `Pending::call_id` method) is closed by the per-item
//! `#[allow(dead_code)]` annotation; this file does not re-litigate it. Instead
//! it pins projection branches the earlier suites never exercised: the
//! `reconnect()` (`Resume::ThenConsume`) flow, invalidation surviving a later
//! `executionChanged`, the null-handle cases of `writable_binding`, and
//! `dispose()` clearing listeners while stopping the stream. All assertions are
//! on observable effects / published state, never helper self-reports.

use std::cell::Cell;
use std::rc::Rc;

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

fn source() -> StatementResultSource {
    StatementResultSource {
        execution_id: "e".to_owned(),
        statement_index: serde_json::Number::from(0),
        writable_mapping: WritableMapping::Verified,
    }
}

fn chunk(index: &str, total: &str, bytes: Vec<u8>) -> ArtifactChunk {
    ArtifactChunk {
        artifact_id: "a".to_owned(),
        chunk_index: index.to_owned(),
        total_chunks: Some(total.to_owned()),
        published_chunk_count: None,
        published_byte_length: None,
        offset: "0".to_owned(),
        bytes,
        result_completeness: ResultCompleteness::Truncated,
    }
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

fn event(seq: &str, index: &str) -> EventEnvelope {
    EventEnvelope {
        stream_id: "stream".to_owned(),
        sequence: seq.to_owned(),
        runtime_epoch: "epoch".to_owned(),
        execution_id: Some("e".to_owned()),
        job_id: None,
        session_handle: Some(handle("s", "epoch")),
        context_revision: Some("1".to_owned()),
        payload: ConnectionEvent::ResultChunk {
            artifact_id: "a".to_owned(),
            chunk_index: index.to_owned(),
            source: source(),
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

fn accept_chunk(p: &mut ExecutionProjection, seq: &str, index: &str, served: ArtifactChunk) {
    let mut effects = Vec::new();
    p.accept(event(seq, index), &mut effects);
    let id = read_call(&effects);
    p.on_read_artifact(id, Ok(served), &mut effects);
}

// 1. reconnect = unsubscribe (gen bump + CloseStream) -> recover -> subscribe
//    from the established cursor. This is the `Resume::ThenConsume` branch,
//    which no earlier suite exercised.
#[test]
fn reconnect_unsubscribes_recovers_then_subscribes_from_cursor() {
    let mut p = projection();
    let first = p.consume();
    let sub = sub_call(&first);
    accept_chunk(&mut p, "7", "0", chunk("0", "1", vec![1]));
    assert_eq!(p.sequence(), Some("7"));

    let effects = p.reconnect();
    assert_eq!(p.generation(), 2, "reconnect bumps the generation exactly once");
    assert!(
        effects.iter().any(|e| matches!(e, ProjectionEffect::CloseStream {
            stream_call_id, ..
        } if *stream_call_id == sub)),
        "reconnect closes the previous subscription"
    );
    let get = get_call(&effects);
    let mut fed = Vec::new();
    p.on_get_execution(get, Ok(view(ExecutionState::Running, &["a"], None)), &mut fed);
    let read = read_call(&fed);
    p.on_read_artifact(read, Ok(chunk("0", "1", vec![1])), &mut fed);
    match fed.iter().find(|e| matches!(e, ProjectionEffect::SubscribeEvents { .. })) {
        Some(ProjectionEffect::SubscribeEvents { after_sequence, .. }) => {
            assert_eq!(after_sequence, &Some("7".to_owned()), "reconnect resumes from the cursor");
        }
        other => panic!("expected a resubscribe after reconnect, got {other:?}"),
    }
}

// 2. `invalidateRuntimeBinding()` is permanent: a later executionChanged that
//    carries a valid binding must still publish a nulled one. This is the
//    `runtime_invalidated` branch of `process_payload(ExecutionChanged)`.
#[test]
fn invalidation_nulls_the_binding_on_a_later_execution_changed() {
    let mut p = projection();
    p.invalidate_runtime_binding();

    let mut e = event("1", "0");
    e.payload = ConnectionEvent::ExecutionChanged {
        execution: view(ExecutionState::Running, &[], Some(binding("e"))),
    };
    p.accept(e, &mut Vec::new());

    assert!(
        p.view().and_then(|v| v.runtime_binding.as_ref()).is_none(),
        "an invalidated projection nulls the binding on executionChanged"
    );
    assert!(p.writable_binding(Some(&handle("s", "epoch"))).is_none());
}

// 3. `writableBinding`: a null current handle, a null original handle, or an
//    epoch-mismatched current handle are all never writable — only the exact
//    (session id, epoch) triple passes.
#[test]
fn writable_binding_rejects_null_and_mismatched_handles() {
    let mut p = projection();
    let mut e = event("1", "0");
    e.payload = ConnectionEvent::ExecutionChanged {
        execution: view(ExecutionState::Running, &[], Some(binding("e"))),
    };
    p.accept(e, &mut Vec::new());

    assert!(p.writable_binding(None).is_none(), "a null current handle is never writable");
    assert!(p.writable_binding(Some(&handle("s", "other"))).is_none(), "epoch mismatch is rejected");
    assert!(p.writable_binding(Some(&handle("other", "epoch"))).is_none(), "session mismatch is rejected");
    assert!(p.writable_binding(Some(&handle("s", "epoch"))).is_some(), "the exact handle is writable");

    // A projection with no original handle can never produce a writable binding.
    let mut p = ExecutionProjection::new(
        "desktop".to_owned(),
        ExecutionReceipt {
            execution_id: "e".to_owned(),
            stream_id: "stream".to_owned(),
            state: ExecutionState::Queued,
        },
        None,
    );
    let mut e = event("1", "0");
    e.payload = ConnectionEvent::ExecutionChanged {
        execution: view(ExecutionState::Running, &[], Some(binding("e"))),
    };
    p.accept(e, &mut Vec::new());
    assert!(p.writable_binding(Some(&handle("s", "epoch"))).is_none(), "a null original handle is never writable");
}

// 4. dispose() removes every listener and stops the active stream. A publish
//    after disposal (an executionChanged) must not reach the former listener.
#[test]
fn dispose_clears_listeners_and_stops_the_stream() {
    let calls = Rc::new(Cell::new(0));
    let counter = Rc::clone(&calls);
    let mut p = projection();
    p.subscribe(Box::new(move || counter.set(counter.get() + 1)));

    let first = p.consume();
    let sub = sub_call(&first);
    let before_gen = p.generation();

    let effects = p.dispose();
    assert_eq!(p.generation(), before_gen + 1, "dispose bumps the generation");
    assert!(
        effects.iter().any(|e| matches!(e, ProjectionEffect::CloseStream {
            stream_call_id, ..
        } if *stream_call_id == sub)),
        "dispose closes the active subscription"
    );

    // This accept publishes; the cleared listener must not fire.
    let mut e = event("1", "0");
    e.payload = ConnectionEvent::ExecutionChanged {
        execution: view(ExecutionState::Running, &[], None),
    };
    p.accept(e, &mut Vec::new());
    assert_eq!(calls.get(), 0, "dispose removed the listener");
}
