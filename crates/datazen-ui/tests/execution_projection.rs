//! T122: `packages/frontend/src/lib/session/ExecutionProjection.ts` port.
//!
//! The React unit journeys (`sessionJourney.test.ts` result-recovery block and
//! `executionRecoveryJourney.test.ts`) are driven through emitted effects and
//! host feeds — no fake async, no weakened assertions. Every expected value is
//! the exact React one (status mapping, chunk boundaries, error/cancel
//! terminal states, cursor/epoch rules).

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

fn source(exec: &str) -> StatementResultSource {
    StatementResultSource {
        execution_id: exec.to_owned(),
        statement_index: serde_json::Number::from(0),
        writable_mapping: WritableMapping::Verified,
    }
}

fn chunk(index: &str, total: Option<&str>, published: Option<&str>, bytes: Vec<u8>) -> ArtifactChunk {
    ArtifactChunk {
        artifact_id: "a".to_owned(),
        chunk_index: index.to_owned(),
        total_chunks: total.map(str::to_owned),
        published_chunk_count: published.map(str::to_owned),
        published_byte_length: None,
        offset: "0".to_owned(),
        bytes,
        result_completeness: ResultCompleteness::Truncated,
    }
}

fn view(state: ExecutionState, artifacts: &[&str], binding: Option<RuntimeResultBinding>) -> ExecutionView {
    ExecutionView {
        execution_id: "e".to_owned(),
        state,
        effect_outcome: EffectOutcome::Unknown,
        provenance: Some(ResultProvenance {
            requested_target: ExecutionTarget {
                connection_id: "c".to_owned(),
                namespace: NamespaceTarget {
                    database: Some("A".to_owned()),
                    ..Default::default()
                },
                object: None,
            },
        }),
        artifact_ids: artifacts.iter().map(|id| (*id).to_owned()).collect(),
        result_completeness: ResultCompleteness::Truncated,
        truncation_reason: Some("cancelled".to_owned()),
        error_code: Some(ExecutionErrorCode::Cancelled),
        runtime_binding: binding,
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

/// Feed one `resultChunk` event and resolve its single artifact read.
fn accept_chunk(p: &mut ExecutionProjection, seq: &str, index: &str, served: ArtifactChunk) {
    let mut effects = Vec::new();
    p.accept(event(seq, index, "e"), &mut effects);
    let id = read_call(&effects);
    p.on_read_artifact(id, Ok(served), &mut effects);
}

/// Resolve a `recover()` all the way through its execution view and the chunk-0
/// metadata read (single-artifact fixture).
fn finish_single_artifact_recover(p: &mut ExecutionProjection, served: ArtifactChunk, state: ExecutionState) {
    let effects = p.recover();
    let id = get_call(&effects);
    let mut effects = Vec::new();
    p.on_get_execution(id, Ok(view(state, &["a"], None)), &mut effects);
    let id = read_call(&effects);
    p.on_read_artifact(id, Ok(served), &mut effects);
}

// --- 1. stream reset rebuilds and clears the cursor -------------------------

#[test]
fn stream_reset_rebuilds_and_clears_the_cursor_even_below_the_old_sequence() {
    for reset_sequence in ["1", "101"] {
        let mut p = projection();
        accept_chunk(&mut p, "100", "0", chunk("0", Some("1"), None, vec![1]));
        assert_eq!(p.chunks().len(), 1);
        assert_eq!(p.sequence(), Some("100"));

        let mut effects = Vec::new();
        let mut reset = event(reset_sequence, "0", "e");
        reset.payload = ConnectionEvent::StreamResetRequired { reason: "cursor unavailable".into() };
        p.accept(reset, &mut effects);
        // The reset clears the cursor *before* dedupe, so it always rebuilds.
        assert_eq!(p.sequence(), None);
        let get = get_call(&effects);
        p.on_get_execution(get, Ok(view(ExecutionState::Cancelled, &["a"], None)), &mut effects);
        let read = read_call(&effects);
        p.on_read_artifact(read, Ok(chunk("0", Some("1"), None, vec![1])), &mut effects);

        assert_eq!(p.chunks().len(), 1, "reset sequence {reset_sequence} rebuilds one chunk");
        assert_eq!(p.sequence(), None, "recover() does not advance the cursor");

        // consume() re-subscribes from the beginning (afterSequence: null), and
        // a cancelled view is terminal so the closed iterator does not re-subscribe.
        let effects = p.consume();
        match effects.first() {
            Some(ProjectionEffect::SubscribeEvents { stream_id, after_sequence, .. }) => {
                assert_eq!(stream_id, "stream");
                assert_eq!(after_sequence, &None, "a cleared cursor resumes from null");
            }
            other => panic!("expected SubscribeEvents, got {other:?}"),
        }
        let sub = sub_call(&effects);
        let mut effects = Vec::new();
        p.on_event(sub, StreamItem::Done, &mut effects);
        let get = get_call(&effects);
        p.on_get_execution(get, Ok(view(ExecutionState::Cancelled, &["a"], None)), &mut effects);
        let read = read_call(&effects);
        p.on_read_artifact(read, Ok(chunk("0", Some("1"), None, vec![1])), &mut effects);
        assert!(
            !effects.iter().any(|e| matches!(e, ProjectionEffect::SubscribeEvents { .. })),
            "a terminal (cancelled) view must not re-subscribe after the iterator closes"
        );
        assert_eq!(p.chunks().len(), 1);
        let _ = p.dispose();
    }
}

// --- 2. a bad first chunk never poisons the publishable prefix --------------

#[test]
fn a_mismatched_first_chunk_is_rejected_without_poisoning_the_prefix() {
    let mut p = projection();
    let effects = p.recover();
    let get = get_call(&effects);
    let mut effects = Vec::new();
    p.on_get_execution(get, Ok(view(ExecutionState::Failed, &["a"], None)), &mut effects);
    let read = read_call(&effects);
    // The host returns chunk index "1" for a request of chunk 0.
    p.on_read_artifact(read, Ok(chunk("1", Some("1"), None, vec![2])), &mut effects);

    assert_eq!(
        p.error().map(ProjectionError::message).as_deref(),
        Some("Artifact read returned a different chunk")
    );
    assert_eq!(p.chunks().len(), 0, "a mismatched chunk is not published");

    // A clean retry publishes the prefix.
    finish_single_artifact_recover(
        &mut p,
        chunk("0", Some("1"), None, vec![2]),
        ExecutionState::Failed,
    );
    assert_eq!(p.chunks().len(), 1);
    assert_eq!(p.chunks()[0].chunk.chunk_index, "0");
}

// --- 3. dedupe, retry, gap recovery and binding invalidation ----------------

#[test]
fn dedup_retry_gap_recovery_and_binding_invalidation_keep_the_old_target() {
    let mut p = projection();

    // The 2^53+1 sequence must survive intact (CM-01 precision).
    accept_chunk(
        &mut p,
        "9007199254740993",
        "0",
        chunk("0", None, Some("3"), vec![1]),
    );
    // A replay is deduplicated before any read is emitted (React returns early
    // when `next <= sequence`), so the second accept produces no effect at all.
    let mut replay = Vec::new();
    p.accept(event("9007199254740993", "0", "e"), &mut replay);
    assert!(replay.is_empty(), "a replayed sequence emits no artifact read");
    assert_eq!(p.chunks().len(), 1, "a replayed sequence is deduplicated");
    assert_eq!(p.sequence(), Some("9007199254740993"));

    // The next event's read fails; the cursor must not advance.
    let mut effects = Vec::new();
    p.accept(event("9007199254740994", "1", "e"), &mut effects);
    let read = read_call(&effects);
    p.on_read_artifact(read, Err(ProjectionError::Host("disconnected".into())), &mut effects);
    assert_eq!(p.error().map(ProjectionError::message).as_deref(), Some("disconnected"));
    assert_eq!(p.sequence(), Some("9007199254740993"), "a failed read leaves the cursor put");
    assert_eq!(p.chunks().len(), 1);

    // A gap forces recover() first, which fetches the unread blocks 1 and 2.
    let mut effects = Vec::new();
    p.accept(event("9007199254740996", "2", "e"), &mut effects);
    let get = get_call(&effects);
    p.on_get_execution(
        get,
        Ok(view(
            ExecutionState::Running,
            &["a"],
            Some(RuntimeResultBinding {
                handle: handle("s", "epoch"),
                resource_binding_id: "r".to_owned(),
                execution_id: "e".to_owned(),
            }),
        )),
        &mut effects,
    );
    let read = read_call(&effects);
    p.on_read_artifact(read, Ok(chunk("0", None, Some("3"), vec![1])), &mut effects);
    let read = read_call(&effects);
    p.on_read_artifact(read, Ok(chunk("1", None, Some("3"), vec![1])), &mut effects);
    let read = read_call(&effects);
    p.on_read_artifact(read, Ok(chunk("2", None, Some("3"), vec![1])), &mut effects);

    assert_eq!(p.chunks().len(), 3, "the gap rebuilds chunks 0, 1 and 2");
    assert_eq!(p.sequence(), Some("9007199254740996"));
    assert_eq!(p.error(), None, "recover() clears the projected error");
    assert_eq!(p.view().map(|v| v.effect_outcome), Some(EffectOutcome::Unknown));
    assert_eq!(
        p.view()
            .and_then(|v| v.provenance.as_ref())
            .and_then(|p| p.requested_target.namespace.database.as_deref()),
        Some("A"),
        "the old target survives a recovery"
    );

    // Epoch + execution identity gate the writable binding.
    assert!(p.writable_binding(Some(&handle("s", "epoch"))).is_some());
    assert!(p.writable_binding(Some(&handle("new", "epoch"))).is_none());
    p.invalidate_runtime_binding();
    assert!(p.writable_binding(Some(&handle("s", "epoch"))).is_none());
    let effects = p.recover();
    let get = get_call(&effects);
    let mut effects = Vec::new();
    p.on_get_execution(
        get,
        Ok(view(
            ExecutionState::Running,
            &["a"],
            Some(RuntimeResultBinding {
                handle: handle("s", "epoch"),
                resource_binding_id: "r".to_owned(),
                execution_id: "e".to_owned(),
            }),
        )),
        &mut effects,
    );
    let read = read_call(&effects);
    p.on_read_artifact(read, Ok(chunk("0", Some("1"), None, vec![1])), &mut effects);
    assert!(p.writable_binding(Some(&handle("s", "epoch"))).is_none(), "invalidation is permanent");
    let _ = p.dispose();
}

// --- 4. cancel / error terminal projection ----------------------------------

#[test]
fn recover_projects_the_cancelled_terminal_view() {
    let mut p = projection();
    accept_chunk(&mut p, "100", "0", chunk("0", Some("1"), None, vec![1]));
    assert_eq!(p.chunks().len(), 1);

    let effects = p.recover();
    let get = get_call(&effects);
    let mut effects = Vec::new();
    p.on_get_execution(get, Ok(view(ExecutionState::Cancelled, &["a"], None)), &mut effects);
    let read = read_call(&effects);
    p.on_read_artifact(read, Ok(chunk("0", Some("1"), None, vec![1])), &mut effects);

    assert_eq!(p.view().map(|v| v.state), Some(ExecutionState::Cancelled));
    assert_eq!(p.view().and_then(|v| v.error_code), Some(ExecutionErrorCode::Cancelled));
    assert_eq!(p.view().and_then(|v| v.truncation_reason.as_deref()), Some("cancelled"));
    let _ = p.dispose();
}

#[test]
fn an_execution_changed_event_carries_the_state_and_normalizes_a_binding() {
    let mut p = projection();
    let mut effects = Vec::new();
    let mut changed = event("1", "0", "e");
    changed.payload = ConnectionEvent::ExecutionChanged {
        execution: view(
            ExecutionState::Succeeded,
            &[],
            Some(RuntimeResultBinding {
                handle: handle("s", "epoch"),
                resource_binding_id: "r".to_owned(),
                execution_id: "e".to_owned(),
            }),
        ),
    };
    p.accept(changed, &mut effects);
    assert_eq!(p.view().map(|v| v.state), Some(ExecutionState::Succeeded));
    assert!(p.writable_binding(Some(&handle("s", "epoch"))).is_some());

    // A foreign execution id is ignored and does not advance the cursor.
    let mut foreign = event("2", "0", "other");
    foreign.payload = ConnectionEvent::ExecutionChanged {
        execution: ExecutionView {
            execution_id: "other".to_owned(),
            ..view(ExecutionState::Failed, &[], None)
        },
    };
    p.accept(foreign, &mut effects);
    assert_eq!(p.view().map(|v| v.state), Some(ExecutionState::Succeeded));
    assert_eq!(p.sequence(), Some("1"));
    let _ = p.dispose();
}

// --- 5. artifact chunk handling / embedded source ---------------------------

#[test]
fn published_adopts_only_a_valid_embedded_source_and_keeps_binary_opaque() {
    let mut p = projection();
    let json = br#"{"source":{"executionId":"e","statementIndex":0,"writableMapping":"readOnly"}}"#;
    finish_single_artifact_recover(
        &mut p,
        chunk("0", Some("1"), None, json.to_vec()),
        ExecutionState::Succeeded,
    );
    let published = p.chunks()[0];
    let adopted = published.source.as_ref().expect("a valid embedded source is adopted");
    assert_eq!(adopted.execution_id, "e");
    assert_eq!(adopted.writable_mapping, WritableMapping::ReadOnly);
    assert!(published.payload.is_object());

    // A foreign execution id is not adopted.
    let mut p = projection();
    let foreign = br#"{"source":{"executionId":"other","statementIndex":0,"writableMapping":"verified"}}"#;
    finish_single_artifact_recover(
        &mut p,
        chunk("0", Some("1"), None, foreign.to_vec()),
        ExecutionState::Succeeded,
    );
    assert!(p.chunks()[0].source.is_none());

    // An opaque binary chunk stays null (JSON.parse throws on the wire).
    let mut p = projection();
    finish_single_artifact_recover(
        &mut p,
        chunk("0", Some("1"), None, vec![1]),
        ExecutionState::Succeeded,
    );
    assert!(p.chunks()[0].payload.is_null());
}

#[test]
fn chunks_sort_by_artifact_then_numeric_chunk_index() {
    let mut p = projection();
    for (seq, artifact, index) in [("1", "b", "10"), ("2", "a", "2"), ("3", "a", "10")] {
        let mut e = event(seq, index, "e");
        e.payload = ConnectionEvent::ResultChunk {
            artifact_id: artifact.to_owned(),
            chunk_index: index.to_owned(),
            source: source("e"),
        };
        let mut effects = Vec::new();
        p.accept(e, &mut effects);
        let id = read_call(&effects);
        let mut served = chunk(index, Some("1"), None, vec![1]);
        served.artifact_id = artifact.to_owned();
        p.on_read_artifact(id, Ok(served), &mut effects);
    }
    let order: Vec<(String, String)> = p
        .chunks()
        .iter()
        .map(|c| (c.chunk.artifact_id.clone(), c.chunk.chunk_index.clone()))
        .collect();
    assert_eq!(
        order,
        vec![
            ("a".to_owned(), "2".to_owned()),
            ("a".to_owned(), "10".to_owned()),
            ("b".to_owned(), "10".to_owned()),
        ],
        "chunk '10' sorts after '2' numerically, not lexicographically"
    );
}

// --- 6. session callback / phases / metadata guard --------------------------

#[test]
fn on_session_fires_only_for_matching_original_and_envelope_handles() {
    let mut p = projection();
    let fires = Rc::new(Cell::new(0));
    let counter = Rc::clone(&fires);
    p.set_on_session(Box::new(move |_| counter.set(counter.get() + 1)));

    let mut e = event("1", "0", "e");
    e.payload = ConnectionEvent::SessionChanged { session: datazen_ui::session::editor_session_controller::SessionView {
        handle: handle("s", "epoch"),
        ..session_view_fixture()
    } };
    let mut effects = Vec::new();
    p.accept(e, &mut effects);
    assert_eq!(fires.get(), 1, "matching handles deliver onSession");

    let mut e = event("2", "0", "e");
    e.session_handle = Some(handle("new", "epoch"));
    e.payload = ConnectionEvent::SessionChanged { session: datazen_ui::session::editor_session_controller::SessionView {
        handle: handle("s", "epoch"),
        ..session_view_fixture()
    } };
    p.accept(e, &mut effects);
    assert_eq!(fires.get(), 1, "a foreign envelope session handle is ignored");

    let mut e = event("3", "0", "e");
    e.payload = ConnectionEvent::SessionChanged { session: datazen_ui::session::editor_session_controller::SessionView {
        handle: handle("s", "other-epoch"),
        ..session_view_fixture()
    } };
    p.accept(e, &mut effects);
    assert_eq!(fires.get(), 1, "an epoch-changed session handle is ignored");
    let _ = p.dispose();
}

fn session_view_fixture() -> datazen_ui::session::editor_session_controller::SessionView {
    use datazen_ui::session::editor_session_controller::*;
    SessionView {
        handle: handle("s", "epoch"),
        connection_id: "c".to_owned(),
        config_revision: "1".to_owned(),
        owner: OwnerRef::Editor {
            client_instance_id: "client".to_owned(),
            editor_session_id: "editor".to_owned(),
        },
        initial_target: target_fixture(),
        observed_context: SessionContext {
            namespace: NamespaceTarget::default(),
            search_path: None,
            effective_identity: None,
            transaction_state: TransactionState::None,
            autocommit: Some(true),
            confidence: ContextConfidence::Confirmed,
        },
        context_revision: "1".to_owned(),
        state: SessionState::Ready,
        attachment_state: AttachmentState::Attached,
        active_execution_id: None,
        expires_at: None,
    }
}

fn target_fixture() -> ExecutionTarget {
    ExecutionTarget {
        connection_id: "c".to_owned(),
        namespace: NamespaceTarget::default(),
        object: None,
    }
}

#[test]
fn missing_publication_metadata_is_projected_as_a_fixed_error() {
    let mut p = projection();
    let effects = p.recover();
    let get = get_call(&effects);
    let mut effects = Vec::new();
    p.on_get_execution(get, Ok(view(ExecutionState::Running, &["a"], None)), &mut effects);
    let read = read_call(&effects);
    p.on_read_artifact(read, Ok(chunk("0", None, None, vec![1])), &mut effects);
    assert_eq!(
        p.error().map(ProjectionError::message).as_deref(),
        Some("Artifact publication metadata unavailable")
    );
    assert_eq!(p.chunks().len(), 0);
}

#[test]
fn read_phases_record_execution_view_then_artifact_read() {
    let mut p = projection();
    let effects = p.recover();
    assert_eq!(p.read_phases(), &[ReadPhase::ExecutionView]);
    let get = get_call(&effects);
    let mut effects = Vec::new();
    p.on_get_execution(get, Ok(view(ExecutionState::Running, &["a"], None)), &mut effects);
    assert_eq!(p.read_phases(), &[ReadPhase::ExecutionView, ReadPhase::ArtifactRead]);
}

// --- 7. subscription generation / resubscribe -------------------------------

#[test]
fn consume_stops_the_previous_stream_and_a_live_view_resubscribes() {
    let mut p = projection();
    let first = p.consume();
    let sub = sub_call(&first);
    assert_eq!(p.generation(), 1);
    assert!(!first.iter().any(|e| matches!(e, ProjectionEffect::CloseStream { .. })));

    // Closing the iterator recovers; a running (non-terminal) view re-subscribes.
    let mut effects = Vec::new();
    p.on_event(sub, StreamItem::Done, &mut effects);
    let get = get_call(&effects);
    p.on_get_execution(get, Ok(view(ExecutionState::Running, &["a"], None)), &mut effects);
    let read = read_call(&effects);
    p.on_read_artifact(read, Ok(chunk("0", Some("1"), None, vec![1])), &mut effects);
    assert!(
        effects.iter().any(|e| matches!(e, ProjectionEffect::SubscribeEvents { .. })),
        "a live view must re-subscribe after the stream closes"
    );

    let second = p.consume();
    assert!(
        second.iter().any(|e| matches!(e, ProjectionEffect::CloseStream { .. })),
        "a second consume stops the still-open first stream"
    );
    assert_eq!(p.generation(), 2);

    // A stale delivery from the old subscription is ignored.
    let before = p.publish_count();
    let mut stale = Vec::new();
    p.on_event(sub, StreamItem::Error(ProjectionError::Host("late".into())), &mut stale);
    assert!(stale.is_empty());
    assert_eq!(p.publish_count(), before);
}
