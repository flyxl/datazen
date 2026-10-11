use super::convert;
use crate::registry::backend::ResourceExecution;
use datazen_application::error::{ApiError, ApiErrorCode};
use datazen_platform_api::{
    context::RequestContext,
    dto::{
        event::{ConnectionEventEnvelope, EventEnvelope},
        execution::*,
        profile::{ConnectionEvent, ResultChunkEvent},
        session::SessionHandle,
    },
    id::*,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeArtifactChunk {
    pub artifact_id: ArtifactId,
    pub chunk_index: Counter,
    pub byte_offset: Counter,
    pub columns: serde_json::Value,
    pub rows: Vec<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub source: StatementResultSource,
    pub published_chunk_count: Counter,
    pub published_byte_length: Counter,
    pub complete: bool,
}
#[derive(Clone)]
pub(super) struct ExecutionEntry {
    pub ctx: RequestContext,
    pub handle: SessionHandle,
    pub stream: StreamId,
    pub view: ExecutionView,
    pub events: Vec<ConnectionEventEnvelope>,
    sequence: u64,
    pub writer: Option<datazen_platform_api::ports::artifact::ArtifactWriter>,
    pub bytes: u64,
    pub outcome: Option<ResourceExecution>,
    pub created: std::time::Instant,
    pub subscribers: Vec<std::sync::mpsc::SyncSender<ConnectionEventEnvelope>>,
}
impl ExecutionEntry {
    fn event(&mut self, event: ConnectionEvent) {
        self.sequence = self.sequence.saturating_add(1);
        let mut envelope = EventEnvelope::new(
            self.stream.clone(),
            Counter::new(self.sequence),
            self.handle.runtime_epoch.clone(),
            event,
        );
        envelope.session_handle = Some(self.handle.clone());
        self.subscribers
            .retain(|tx| tx.try_send(envelope.clone()).is_ok());
        self.events.push(envelope);
        if self.events.len() > 4096 {
            self.events.drain(..self.events.len() - 4096);
        }
    }
    pub fn visible_to(&self, ctx: &RequestContext) -> bool {
        self.ctx.organization_id == ctx.organization_id
            && self.ctx.principal_id == ctx.principal_id
            && self.ctx.client_instance_id == ctx.client_instance_id
    }
}
/// Host-only publication capability. It accepts only executions already admitted by the use case.
#[derive(Clone, Default)]
pub struct RuntimeResultSink(
    pub(super) Arc<Mutex<HashMap<ExecutionId, ExecutionEntry>>>,
    Arc<
        std::sync::OnceLock<
            Result<Arc<crate::artifact::FileArtifactStore>, datazen_platform_api::error::PortError>,
        >,
    >,
);
impl RuntimeResultSink {
    pub fn with_artifact_store(
        &self,
        store: Arc<crate::artifact::FileArtifactStore>,
    ) -> Result<(), ApiError> {
        self.1.set(Ok(store)).map_err(|_| {
            ApiError::new(
                ApiErrorCode::ContextConflict,
                "artifact storage already initialized",
            )
        })
    }
    fn store(&self) -> Result<Arc<crate::artifact::FileArtifactStore>, ApiError> {
        self.1
            .get_or_init(|| {
                let root = std::env::temp_dir().join(format!(
                    "datazen-artifacts-{}",
                    crate::directory::DbSessionIdGenerator::new()
                        .generate()
                        .as_str()
                ));
                crate::artifact::FileArtifactStore::open(root).map(Arc::new)
            })
            .clone()
            .map_err(convert::port)
    }

    /// Reclaim settled executions after their result retention expires. Active
    /// executions retain their cancellation/ownership records until completion.
    pub(super) fn reclaim_expired(&self) {
        if let Some(Ok(store)) = self.1.get() {
            let _ = store.reclaim_expired_sync();
        }
        lock(&self.0).retain(|_, entry| {
            !entry.view.state.is_terminal()
                || entry.created.elapsed() <= std::time::Duration::from_secs(86400)
        });
    }
    pub fn publish_rows(
        &self,
        id: &ExecutionId,
        columns: serde_json::Value,
        rows: Vec<serde_json::Value>,
    ) -> Result<(), ApiError> {
        self.publish(id, columns, rows, None, None)
    }
    pub fn publish_output(
        &self,
        id: &ExecutionId,
        output: serde_json::Value,
    ) -> Result<(), ApiError> {
        self.publish(id, serde_json::json!([]), Vec::new(), Some(output), None)
    }
    /// Drivers that can observe each statement provide its context and verified relation here.
    pub fn publish_statement_rows(
        &self,
        id: &ExecutionId,
        columns: serde_json::Value,
        rows: Vec<serde_json::Value>,
        source: StatementResultSource,
    ) -> Result<(), ApiError> {
        self.publish(id, columns, rows, None, Some(source))
    }
    pub fn publish_statement_output(
        &self,
        id: &ExecutionId,
        output: serde_json::Value,
        source: StatementResultSource,
    ) -> Result<(), ApiError> {
        self.publish(
            id,
            serde_json::json!([]),
            Vec::new(),
            Some(output),
            Some(source),
        )
    }
    fn publish(
        &self,
        id: &ExecutionId,
        columns: serde_json::Value,
        rows: Vec<serde_json::Value>,
        output: Option<serde_json::Value>,
        source: Option<StatementResultSource>,
    ) -> Result<(), ApiError> {
        self.reclaim_expired();
        let mut entries = lock(&self.0);
        let total_bytes = entries
            .values()
            .fold(0u64, |sum, e| sum.saturating_add(e.bytes));
        let entry = entries
            .get_mut(id)
            .ok_or_else(|| ApiError::new(ApiErrorCode::NotFound, "execution was not admitted"))?;
        if entry.view.state.is_terminal() {
            return Err(ApiError::new(
                ApiErrorCode::SessionLost,
                "execution is already settled",
            ));
        }
        let source = match source {
            Some(source) if source.execution_id == *id => source,
            Some(_) => {
                return Err(ApiError::invalid_argument(
                    "statement source execution mismatch",
                ))
            }
            None => {
                let mut source = unknown_source(id);
                source.statement_index = output
                    .as_ref()
                    .and_then(|o| o.get("statementIndex"))
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|i| u32::try_from(i).ok())
                    .unwrap_or(0);
                source
            }
        };
        let store = self.store()?;
        if entry.writer.is_none() {
            let fingerprint = entry
                .view
                .provenance
                .as_ref()
                .map(datazen_platform_api::ports::artifact::provenance_fingerprint)
                .unwrap_or_else(|| "unobserved".into());
            let (artifact_id, writer) = store
                .create_sync(
                    &entry.ctx,
                    datazen_platform_api::ports::artifact::ArtifactSpec::new(
                        entry.ctx.organization_id.clone(),
                        entry.ctx.principal_id.clone(),
                        id.clone(),
                        fingerprint,
                    ),
                )
                .map_err(convert::port)?;
            entry.writer = Some(writer);
            entry.view.artifact_ids.push(artifact_id);
        }
        let writer = entry.writer.as_mut().ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                "artifact writer unavailable",
            )
        })?;
        let artifact_id = writer.artifact_id.clone();
        let index = writer.next_chunk_index;
        let chunk = RuntimeArtifactChunk {
            artifact_id: artifact_id.clone(),
            chunk_index: index,
            byte_offset: writer.published_byte_size,
            columns,
            rows,
            output,
            source: source.clone(),
            published_chunk_count: index.saturating_increment(),
            published_byte_length: Counter::ZERO,
            complete: false,
        };
        struct BoundedBytes(Vec<u8>);
        impl std::io::Write for BoundedBytes {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.0.len().saturating_add(bytes.len()) >= 32 * 1024 * 1024 {
                    return Err(std::io::Error::other("artifact block limit"));
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut encoded = BoundedBytes(Vec::new());
        serde_json::to_writer(&mut encoded, &chunk).map_err(|_| {
            ApiError::new(ApiErrorCode::PayloadTooLarge, "result block exceeds budget")
        })?;
        let mut bytes = encoded.0;
        bytes.push(b'\n');
        if total_bytes.saturating_add(bytes.len() as u64) > 256 * 1024 * 1024 {
            return Err(ApiError::new(
                ApiErrorCode::PayloadTooLarge,
                "result artifact budget exceeded",
            ));
        }
        store
            .append_sync(
                writer,
                datazen_platform_api::ports::artifact::ChunkPayload::new(bytes),
            )
            .map_err(convert::port)?;
        entry.bytes = writer.published_byte_size.get();
        entry.event(ConnectionEvent::ResultChunk(ResultChunkEvent {
            artifact_id,
            chunk_index: index,
            source,
        }));
        Ok(())
    }
    pub(super) fn admit(
        &self,
        ctx: RequestContext,
        handle: SessionHandle,
        stream: StreamId,
        view: ExecutionView,
    ) {
        self.reclaim_expired();
        let mut entry = ExecutionEntry {
            ctx,
            handle,
            stream,
            view,
            events: Vec::new(),
            sequence: 0,
            writer: None,
            bytes: 0,
            outcome: None,
            subscribers: Vec::new(),
            created: std::time::Instant::now(),
        };
        entry.event(ConnectionEvent::ExecutionChanged {
            execution: entry.view.clone(),
        });
        lock(&self.0).insert(entry.view.execution_id.clone(), entry);
    }
    pub(super) fn started(&self, id: &ExecutionId, resource: &str) {
        if let Some(entry) = lock(&self.0).get_mut(id) {
            entry.view.state = ExecutionState::Running;
            entry.view.runtime_binding = Some(RuntimeResultBinding {
                handle: entry.handle.clone(),
                resource_binding_id: ResourceId::new(resource),
                execution_id: id.clone(),
            });
            entry.event(ConnectionEvent::ExecutionChanged {
                execution: entry.view.clone(),
            });
        }
    }
    pub(super) fn capture(&self, outcome: ResourceExecution) {
        if let Some(entry) = lock(&self.0).get_mut(&outcome.execution_id) {
            entry.outcome = Some(outcome);
        }
    }
    pub(super) fn finish(&self, id: &ExecutionId, success: bool, resource_lost: bool) {
        if let Some(entry) = lock(&self.0).get_mut(id) {
            match entry.outcome.take() {
                Some(outcome) if success => {
                    entry.view.state =
                        convert::json_convert(outcome.state).unwrap_or(ExecutionState::Failed);
                    entry.view.effect_outcome = convert::json_convert(outcome.effect_outcome)
                        .unwrap_or(EffectOutcome::Unknown);
                    if let Some(p) = entry.view.provenance.as_mut() {
                        p.context_after = convert::context(&outcome.context_after);
                    }
                    entry.view.error_code = match entry.view.state {
                        ExecutionState::Failed => Some(ExecutionErrorCode::SqlError),
                        ExecutionState::Cancelled => Some(ExecutionErrorCode::Cancelled),
                        _ => None,
                    };
                }
                _ => {
                    entry.view.state = ExecutionState::Failed;
                    entry.view.effect_outcome = EffectOutcome::Unknown;
                    entry.view.error_code = Some(if resource_lost {
                        ExecutionErrorCode::ResourceLost
                    } else {
                        ExecutionErrorCode::HostRejected
                    });
                    entry.view.runtime_binding = None;
                    if resource_lost {
                        if let Some(provenance) = entry.view.provenance.as_mut() {
                            provenance.context_after.transaction_state =
                                datazen_platform_api::dto::session::TransactionState::Unknown;
                            provenance.context_after.confidence =
                                datazen_platform_api::dto::session::ContextConfidence::Unknown;
                        }
                    }
                }
            }
            entry.view.result_completeness = if entry.view.state == ExecutionState::Succeeded {
                ResultCompleteness::Complete
            } else {
                ResultCompleteness::Truncated
            };
            if entry.view.result_completeness == ResultCompleteness::Truncated {
                entry.view.truncation_reason = Some("executionInterrupted".into());
            }
            if let Some(writer) = entry.writer.take() {
                let reason = match entry.view.state {
                    ExecutionState::Succeeded => None,
                    ExecutionState::Cancelled => {
                        Some(datazen_platform_api::dto::artifact::AbortReason::Cancelled)
                    }
                    _ => Some(datazen_platform_api::dto::artifact::AbortReason::ExecutionFailed),
                };
                if self
                    .store()
                    .and_then(|store| store.finish_sync(writer, reason).map_err(convert::port))
                    .is_err()
                {
                    entry.view.state = ExecutionState::Failed;
                    entry.view.result_completeness = ResultCompleteness::Truncated;
                    entry.view.truncation_reason = Some("artifactFinalizationFailed".into());
                    if entry.view.error_code != Some(ExecutionErrorCode::ResourceLost) {
                        entry.view.error_code = Some(ExecutionErrorCode::HostRejected);
                    }
                }
            }
            entry.event(ConnectionEvent::ExecutionChanged {
                execution: entry.view.clone(),
            });
            entry.subscribers.clear();
        }
    }
    pub(super) fn invalidate(&self, handle: &SessionHandle) {
        for entry in lock(&self.0).values_mut().filter(|e| &e.handle == handle) {
            entry.view.runtime_binding = None;
            entry.event(ConnectionEvent::ExecutionChanged {
                execution: entry.view.clone(),
            });
        }
    }
    pub(super) fn session_event(&self, session: datazen_platform_api::dto::session::SessionView) {
        for entry in lock(&self.0)
            .values_mut()
            .filter(|e| e.handle == session.handle)
        {
            entry.event(ConnectionEvent::SessionChanged {
                session: session.clone(),
            });
        }
    }
    pub(super) fn view(
        &self,
        ctx: &RequestContext,
        id: &ExecutionId,
    ) -> Result<ExecutionView, ApiError> {
        lock(&self.0)
            .get(id)
            .filter(|e| e.visible_to(ctx))
            .map(|e| e.view.clone())
            .ok_or_else(|| ApiError::new(ApiErrorCode::NotFound, "execution unavailable"))
    }
    pub(super) fn handle(
        &self,
        ctx: &RequestContext,
        id: &ExecutionId,
    ) -> Result<SessionHandle, ApiError> {
        lock(&self.0)
            .get(id)
            .filter(|e| e.visible_to(ctx))
            .map(|e| e.handle.clone())
            .ok_or_else(|| ApiError::new(ApiErrorCode::NotFound, "execution unavailable"))
    }
    pub(super) fn subscribe(
        &self,
        ctx: &RequestContext,
        stream: &StreamId,
        after: Option<Counter>,
        cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<datazen_platform_api::ports::event::EventSubscription, ApiError> {
        let mut entries = lock(&self.0);
        let entry = entries
            .values_mut()
            .find(|e| &e.stream == stream && e.visible_to(ctx))
            .ok_or_else(|| ApiError::new(ApiErrorCode::NotFound, "stream unavailable"))?;
        let replay_gap = entry.events.first().is_some_and(|event| {
            event.sequence.get()
                > after
                    .map(|value| value.get())
                    .unwrap_or(0)
                    .saturating_add(1)
        });
        let reset = || {
            let mut envelope = EventEnvelope::new(
                entry.stream.clone(),
                Counter::new(entry.sequence),
                entry.handle.runtime_epoch.clone(),
                ConnectionEvent::StreamResetRequired {
                    reason: "eventReplayExpired".into(),
                },
            );
            envelope.session_handle = Some(entry.handle.clone());
            envelope
        };
        let events = if replay_gap {
            vec![reset()]
        } else {
            entry
                .events
                .iter()
                .filter(|e| e.sequence.get() > after.map(|v| v.get()).unwrap_or(0))
                .cloned()
                .collect::<Vec<_>>()
        };
        let (tx, rx) = std::sync::mpsc::sync_channel(256);
        if !entry.view.state.is_terminal() {
            if entry.subscribers.len() >= 64 {
                return Err(ApiError::new(
                    ApiErrorCode::ServiceUnavailable,
                    "execution subscriber limit reached",
                ));
            }
            entry.subscribers.push(tx);
        }
        Ok(Box::new(LiveSubscription {
            history: events.into_iter(),
            receiver: rx,
            cancel,
        }))
    }
    pub(super) fn chunk(
        &self,
        ctx: &RequestContext,
        artifact: &ArtifactId,
        index: Counter,
    ) -> Result<RuntimeArtifactChunk, ApiError> {
        // Artifact ACL/TTL live in the durable store, independent of a session or
        // execution tombstone. Restart recovery therefore preserves readable results.
        let store = self.store()?;
        let bytes = store
            .read_sync(ctx, artifact, index)
            .map_err(convert::port)?;
        let metadata = store.describe_sync(ctx, artifact).map_err(convert::port)?;
        let mut chunk: RuntimeArtifactChunk =
            serde_json::from_slice(&bytes.bytes).map_err(|_| {
                ApiError::new(ApiErrorCode::ServiceUnavailable, "artifact block invalid")
            })?;
        chunk.published_chunk_count = metadata.published_chunk_count;
        chunk.published_byte_length = metadata.published_byte_size;
        chunk.complete = metadata.result_completeness == ResultCompleteness::Complete;
        Ok(chunk)
    }
}

struct LiveSubscription {
    history: std::vec::IntoIter<ConnectionEventEnvelope>,
    receiver: std::sync::mpsc::Receiver<ConnectionEventEnvelope>,
    cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
}
impl Iterator for LiveSubscription {
    type Item = ConnectionEventEnvelope;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self
                .cancel
                .as_ref()
                .is_some_and(|v| v.load(std::sync::atomic::Ordering::Acquire))
            {
                return None;
            }
            if let Some(event) = self.history.next() {
                return Some(event);
            }
            match self
                .receiver
                .recv_timeout(std::time::Duration::from_millis(100))
            {
                Ok(event) => return Some(event),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        }
    }
}

fn unknown_source(id: &ExecutionId) -> StatementResultSource {
    StatementResultSource {
        execution_id: id.clone(),
        statement_index: 0,
        context: datazen_platform_api::dto::session::SessionContext {
            namespace: datazen_platform_api::target::NamespaceTarget::default(),
            search_path: None,
            effective_identity: None,
            transaction_state: datazen_platform_api::dto::session::TransactionState::Unknown,
            autocommit: None,
            confidence: datazen_platform_api::dto::session::ContextConfidence::Unknown,
        },
        relation: None,
        writable_mapping: WritableMapping::ReadOnly,
    }
}
