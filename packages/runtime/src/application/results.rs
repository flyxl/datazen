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
#[derive(Clone, Debug, serde::Serialize)]
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
    pub chunks: Vec<RuntimeArtifactChunk>,
    pub bytes: u64,
    pub outcome: Option<ResourceExecution>,
    pub created: std::time::Instant,
    pub subscribers: Vec<std::sync::mpsc::Sender<ConnectionEventEnvelope>>,
}
impl ExecutionEntry {
    fn event(&mut self, event: ConnectionEvent) {
        let mut envelope = EventEnvelope::new(
            self.stream.clone(),
            Counter::new(self.events.len() as u64 + 1),
            self.handle.runtime_epoch.clone(),
            event,
        );
        envelope.session_handle = Some(self.handle.clone());
        self.subscribers
            .retain(|tx| tx.send(envelope.clone()).is_ok());
        self.events.push(envelope);
    }
    pub fn visible_to(&self, ctx: &RequestContext) -> bool {
        self.ctx.organization_id == ctx.organization_id
            && self.ctx.principal_id == ctx.principal_id
            && self.ctx.client_instance_id == ctx.client_instance_id
    }
}
/// Host-only publication capability. It accepts only executions already admitted by the use case.
#[derive(Clone, Default)]
pub struct RuntimeResultSink(pub(super) Arc<Mutex<HashMap<ExecutionId, ExecutionEntry>>>);
impl RuntimeResultSink {
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
            None => unknown_source(id),
        };
        let artifact_id = ArtifactId::new(format!("artifact-{}", id.as_str()));
        let size = serde_json::to_vec(
            &serde_json::json!({"columns":&columns,"rows":&rows,"output":&output,"source":&source}),
        )
        .map_err(|_| ApiError::invalid_argument("invalid result block"))?
        .len() as u64;
        if size > 32 * 1024 * 1024
            || total_bytes.saturating_add(size) > 256 * 1024 * 1024
            || entry.chunks.len() >= 65536
        {
            return Err(ApiError::new(
                ApiErrorCode::PayloadTooLarge,
                "result artifact budget exceeded",
            ));
        }
        let byte_offset = Counter::new(entry.bytes);
        entry.bytes += size;
        let index = Counter::new(entry.chunks.len() as u64);
        entry.chunks.push(RuntimeArtifactChunk {
            artifact_id: artifact_id.clone(),
            chunk_index: index,
            byte_offset,
            columns,
            rows,
            output,
            source: source.clone(),
            published_chunk_count: Counter::new(index.get() + 1),
            published_byte_length: Counter::new(entry.bytes),
            complete: false,
        });
        if entry.view.artifact_ids.is_empty() {
            entry.view.artifact_ids.push(artifact_id.clone());
        }
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
        let mut entry = ExecutionEntry {
            ctx,
            handle,
            stream,
            view,
            events: Vec::new(),
            chunks: Vec::new(),
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
    pub(super) fn finish(&self, id: &ExecutionId, success: bool) {
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
                    entry.view.error_code = Some(ExecutionErrorCode::HostRejected);
                    entry.view.runtime_binding = None;
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
        let events = entry
            .events
            .iter()
            .filter(|e| e.sequence.get() > after.map(|v| v.get()).unwrap_or(0))
            .cloned()
            .collect::<Vec<_>>();
        let (tx, rx) = std::sync::mpsc::channel();
        if !entry.view.state.is_terminal() {
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
        let entries = lock(&self.0);
        let entry = entries
            .values()
            .find(|e| e.view.artifact_ids.contains(artifact) && e.visible_to(ctx))
            .ok_or_else(|| ApiError::new(ApiErrorCode::NotFound, "artifact unavailable"))?;
        if entry.created.elapsed() > std::time::Duration::from_secs(86400) {
            return Err(ApiError::new(
                ApiErrorCode::NotFound,
                "result artifact expired",
            ));
        }
        if entry.chunks.is_empty() && index.get() == 0 {
            return Ok(RuntimeArtifactChunk {
                artifact_id: artifact.clone(),
                chunk_index: index,
                byte_offset: Counter::new(0),
                columns: serde_json::json!([]),
                rows: Vec::new(),
                output: None,
                source: unknown_source(&entry.view.execution_id),
                published_chunk_count: Counter::new(0),
                published_byte_length: Counter::new(0),
                complete: entry.view.result_completeness == ResultCompleteness::Complete,
            });
        }
        let mut chunk = entry
            .chunks
            .get(index.get() as usize)
            .cloned()
            .ok_or_else(|| {
                ApiError::new(ApiErrorCode::NotFound, "artifact chunk not yet published")
            })?;
        chunk.published_chunk_count = Counter::new(entry.chunks.len() as u64);
        chunk.published_byte_length = Counter::new(entry.bytes);
        chunk.complete = entry.view.result_completeness == ResultCompleteness::Complete;
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
