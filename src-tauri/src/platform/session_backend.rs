use std::{collections::HashMap, sync::{Arc, RwLock}};
use async_trait::async_trait;
use datazen_driver_api::{resource as dr, session as ds, QueryExecutionId};
use datazen_runtime::{connection as rc, registry::{backend::*, audit::CapabilityVersions}};
use crate::{db::DriverRegistry, store::Store};
use super::{resource_budget::DesktopResourceBudget, resource_mapping as map};

pub type ResultPublisher = Arc<dyn Fn(&rc::ExecutionId, serde_json::Value) -> Result<(), rc::ProviderError> + Send + Sync>;
struct Resource {
    provider: Arc<dyn dr::ResourceProvider>,
    handle: dr::ResourceHandle,
    baseline: dr::Baseline,
    epoch: u64,
    revision: tokio::sync::Mutex<u64>,
    last_context: tokio::sync::Mutex<rc::SessionContext>,
    cancels: tokio::sync::Mutex<HashMap<String,String>>,
    _short_permit: Option<tokio::sync::OwnedSemaphorePermit>,
}
pub struct DesktopSessionBackend {
    registry: Arc<DriverRegistry>, store: Arc<Store>,
    budget: Arc<dyn dr::BudgetPort>, resources: tokio::sync::RwLock<HashMap<String, Arc<Resource>>>,
    publisher: RwLock<Option<ResultPublisher>>,
    short_resources: Arc<tokio::sync::Semaphore>,
}
impl DesktopSessionBackend {
    pub fn new(registry: Arc<DriverRegistry>, store: Arc<Store>, physical_limit: u32) -> Self {
        Self { registry, store, budget: Arc::new(DesktopResourceBudget::new(physical_limit)), resources: tokio::sync::RwLock::new(HashMap::new()), publisher: RwLock::new(None), short_resources: Arc::new(tokio::sync::Semaphore::new(2)) }
    }
    pub fn set_publisher(&self, publisher: ResultPublisher) -> Result<(), rc::ProviderError> {
        *self.publisher.write().map_err(|_| rc::ProviderError::ProtocolError("result publisher unavailable".into()))? = Some(publisher); Ok(())
    }
    async fn resource(&self, id: &str) -> Result<Arc<Resource>, rc::ProviderError> {
        self.resources.read().await.get(id).cloned().ok_or_else(|| rc::ProviderError::SessionLost("physical resource no longer exists".into()))
    }
}
struct Sink { id: rc::ExecutionId, publisher: ResultPublisher }
#[async_trait]
impl dr::ResultSink for Sink {
    async fn write(&self, chunk: dr::ResultChunk) -> Result<(), dr::ResourceError> {
        (self.publisher)(&self.id, serde_json::json!({"kind":"rows","statementIndex":chunk.statement_index,"sql":chunk.sql,"rows":chunk.rows,"rowsAffected":chunk.rows_affected})).map_err(|_| dr::ResourceError::SinkRejected { reason: "result publication failed".into() })
    }
    async fn complete(&self) -> Result<(), dr::ResourceError> { Ok(()) }
    async fn fail(&self, _reason: &str) -> Result<(), dr::ResourceError> { Ok(()) }
}
#[async_trait]
impl SessionBackend for DesktopSessionBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, rc::ProviderError> {
        let (config, metadata) = self.store.platform_profile(request.connection_id.as_str()).await.ok_or_else(|| rc::ProviderError::SessionNotFound("profile unavailable".into()))?;
        if !metadata.enabled || metadata.config_revision != request.config_revision.0 { return Err(rc::ProviderError::HostRejected("profile disabled or revision changed".into())); }
        let provider = self.registry.resource_provider(&config.database_type).await.map_err(|_| rc::ProviderError::CapabilityUnsupported)?;
        if matches!(request.owner, rc::OwnerRef::Editor { .. }) { provider.capabilities().require_stateful_session().map_err(|_| rc::ProviderError::CapabilityUnsupported)?; }
        let purpose = if matches!(request.owner, rc::OwnerRef::ClientSession { .. }) { dr::ResourcePurpose::MetadataInspection } else { dr::ResourcePurpose::InteractiveQuery };
        let target = map::namespace(&request.initial_target);
        let identity_scope = dr::IdentityScope::default();
        let descriptor = provider.describe_resource(&dr::DescribeResourceRequest { connection_config: config.clone(), identity_scope: identity_scope.clone(), target: target.clone(), purpose }).await.map_err(map::error)?;
        let baseline = dr::Baseline::new(descriptor.initialization_requirements);
        let short_permit = if matches!(request.owner, rc::OwnerRef::ClientSession { .. }) { Some(self.short_resources.clone().acquire_owned().await.map_err(|_| rc::ProviderError::ResourceBusy("short resource pool closed"))?) } else { None };
        let handle = provider.acquire_resource(&dr::AcquireResourceRequest { connection_config: config, target, scope: dr::ResourceScope { purpose, max_physical_connections: 1, holds_open_transaction: false, pin_for_streaming: false }, identity_scope, baseline: baseline.clone() }, &self.budget).await.map_err(map::error)?;
        let observation = match provider.observe_session(&handle).await { Ok(observation) => observation, Err(error) => { let _ = provider.close_resource(&handle).await; return Err(map::error(error)); } };
        let resource_id = uuid::Uuid::new_v4().to_string();
        let capabilities = provider.capabilities();
        let opened = OpenedResource { context: map::context(&observation.context), capabilities: CapabilityVersions::new(capabilities.snapshot.protocol_version.to_string(), capabilities.snapshot.driver_version.clone()), resource_id: resource_id.clone(), driver_supports_cancel: capabilities.capabilities.precise_cancel.accepts_precise_cancel() };
        self.resources.write().await.insert(resource_id, Arc::new(Resource { provider, handle, baseline, epoch: request.runtime_epoch, revision: tokio::sync::Mutex::new(0), last_context: tokio::sync::Mutex::new(map::context(&observation.context)), cancels: tokio::sync::Mutex::new(HashMap::new()), _short_permit: short_permit }));
        Ok(opened)
    }
    async fn execute(&self, request: ExecuteOnResource) -> Result<ResourceExecution, rc::ProviderError> {
        let resource = self.resource(&request.resource_id).await?;
        let publisher = self.publisher.read().map_err(|_| rc::ProviderError::ProtocolError("publisher unavailable".into()))?.clone().ok_or_else(|| rc::ProviderError::ProtocolError("publisher not installed".into()))?;
        let execution = QueryExecutionId::new(request.execution_id.as_str());
        let cancel_handle = uuid::Uuid::new_v4().to_string();
        resource.cancels.lock().await.insert(request.execution_id.as_str().into(), cancel_handle.clone());
        request.cancel_handle_sink.publish(&cancel_handle);
        let sink = Sink { id: request.execution_id.clone(), publisher: publisher.clone() };
        let call = dr::CommandCall::new(request.command.command.clone(), request.command.input.clone());
        let result = match call.command.as_str() {
            "platform.beginTransaction" => resource.provider.begin_transaction(&resource.handle, &serde_json::from_value::<ds::TransactionOptions>(call.input.clone()).map_err(|_| rc::ProviderError::HostRejected("invalid transaction options".into()))?).await.map(|transaction| (None, Some(transaction))),
            "platform.commitTransaction" => resource.provider.commit_transaction(&resource.handle).await.map(|transaction| (None, Some(transaction))),
            "platform.rollbackTransaction" => resource.provider.rollback_transaction(&resource.handle).await.map(|transaction| (None, Some(transaction))),
            _ => resource.provider.execute_on_resource(&resource.handle, &execution, &call, &sink).await.map(|completion| (Some(completion), None)),
        };
        resource.cancels.lock().await.remove(request.execution_id.as_str());
        let observation = resource.provider.observe_session(&resource.handle).await;
        let (completion, transaction, driver_failed) = match result {
            Ok((completion, transaction)) => (completion, transaction, false),
            Err(_) => (None, None, true),
        };
        let observation = observation.map_err(map::error)?;
        let context_after = completion.as_ref().map(|c| &c.context_after).unwrap_or(&observation.context);
        let effect_outcome = completion.as_ref().map(|c| c.effect_outcome).or_else(|| transaction.as_ref().and_then(|t| t.effect)).unwrap_or(if driver_failed { ds::EffectOutcome::Unknown } else { ds::EffectOutcome::NotStarted });
        let state = completion.as_ref().map(|c| match c.completion_status { ds::CompletionStatus::Succeeded => rc::ExecutionState::Succeeded, ds::CompletionStatus::Failed { .. } => rc::ExecutionState::Failed, ds::CompletionStatus::Cancelled { .. } => rc::ExecutionState::Cancelled }).unwrap_or(if effect_outcome == ds::EffectOutcome::Unknown { rc::ExecutionState::Failed } else { rc::ExecutionState::Succeeded });
        if let Some(completion) = &completion {
            for (index, statement) in completion.statement_results.iter().enumerate() {
                publisher(&request.execution_id, serde_json::json!({"kind":"statementMetadata","statementIndex":index,"columns":statement.columns,"rowsAffected":statement.rows_affected,"truncated":statement.truncated}))?;
            }
        }
        let driver_handles = completion.as_ref().map(|c| c.session_handles.as_slice()).unwrap_or(observation.handles.as_slice());
        if driver_handles.iter().any(|handle| !handle.belongs_to(resource.handle.resource_key(),resource.handle.runtime_epoch())) { return Err(rc::ProviderError::HostRejected("driver handle bound to another resource".into())); }
        let mut revision = resource.revision.lock().await;
        let mut previous = resource.last_context.lock().await;
        let observed_context = map::context(context_after);
        *revision = (*revision).max(request.expected_context_revision);
        if *previous != observed_context {
            *revision = revision.checked_add(1).ok_or_else(|| rc::ProviderError::ProtocolError("context revision exhausted".into()))?;
            *previous = observed_context.clone();
        }
        Ok(ResourceExecution { execution_id: request.execution_id.clone(), stream_id: rc::StreamId::new(format!("result:{}",request.execution_id.as_str())), state, effect_outcome: map::effect(effect_outcome), context_after: observed_context, context_revision: *revision, handles: map::handles(driver_handles, &request.resource_id, resource.epoch), cancel_handle })
    }
    async fn cancel(&self, request: CancelOnResource) -> Result<ResourceCancel, rc::ProviderError> {
        let resource = self.resource(&request.resource_id).await?;
        if resource.cancels.lock().await.get(request.execution_id.as_str()) != Some(&request.cancel_handle) { return Err(rc::ProviderError::HostRejected("cancel binding mismatch".into())); }
        let receipt = resource.provider.request_cancel(&resource.handle, &QueryExecutionId::new(request.execution_id.as_str())).await.map_err(map::error)?;
        let disposition = match receipt.disposition { ds::CancelDisposition::Requested => rc::port::CancelDisposition::Requested, ds::CancelDisposition::NotRegistered => return Err(rc::ProviderError::HostRejected("cancel target not registered".into())), ds::CancelDisposition::AlreadyFinished => rc::port::CancelDisposition::AlreadyFinished, ds::CancelDisposition::Unsupported => rc::port::CancelDisposition::Unsupported };
        let state = receipt.state.ok_or_else(|| rc::ProviderError::ProtocolError("cancel receipt lacks execution state".into()))?;
        Ok(ResourceCancel { disposition, state: map::state(state) })
    }
    async fn finalize_handles(&self, request: FinalizeHandles) -> Result<HandleFinalization, rc::ProviderError> {
        let resource = self.resource(&request.resource_id).await?;
        if request.handles.iter().any(|handle| handle.resource_id.as_str() != request.resource_id || handle.runtime_epoch.get() != resource.epoch) { return Err(rc::ProviderError::HostRejected("handle resource binding mismatch".into())); }
        if request.handles.iter().any(|handle| !handle.closed && handle.kind != rc::HandleKind::Transaction) { return Err(rc::ProviderError::CapabilityUnsupported); }
        let before = resource.provider.observe_session(&resource.handle).await.map_err(map::error)?;
        let active = request.handles.iter().filter(|handle| !handle.closed).count();
        let effect = if active > 0 { let transaction = match request.disposition { HandleDisposition::Commit => resource.provider.commit_transaction(&resource.handle).await, HandleDisposition::Rollback => resource.provider.rollback_transaction(&resource.handle).await }.map_err(map::error)?; transaction.effect.unwrap_or(ds::EffectOutcome::Unknown) } else { ds::EffectOutcome::NotStarted };
        let after = resource.provider.observe_session(&resource.handle).await.map_err(map::error)?;
        let remaining = after.handles.iter().filter(|handle| !handle.closed).count();
        if active == 0 && before.handles.iter().any(|handle| !handle.closed) { return Err(rc::ProviderError::CleanupFailed("unregistered driver handles".into())); }
        Ok(HandleFinalization { finalized: request.handles.len().saturating_sub(remaining), remaining, effect_outcome: map::effect(effect) })
    }
    async fn close(&self, request: CloseResource) -> Result<CloseResourceOutcome, rc::ProviderError> {
        let resource = self.resource(&request.resource_id).await?;
        if request.registered_handles != 0 { return Err(rc::ProviderError::CleanupFailed("registered handles remain".into())); }
        let observation = resource.provider.observe_session(&resource.handle).await.map_err(map::error)?;
        if observation.handles.iter().any(|handle| !handle.closed) || matches!(observation.transaction.state, ds::TransactionState::Active|ds::TransactionState::Aborted|ds::TransactionState::Unknown) { return Ok(CloseResourceOutcome::Undecidable { reason: "resource state unresolved" }); }
        let _reset = resource.provider.reset_resource(&resource.handle,&resource.baseline).await;
        match resource.provider.close_resource(&resource.handle).await.map_err(map::error)? {
            ds::CloseDisposition::Closed => { self.resources.write().await.remove(&request.resource_id); Ok(CloseResourceOutcome::Closed) },
            ds::CloseDisposition::CloseUnconfirmed => Ok(CloseResourceOutcome::Undecidable { reason: "driver close unconfirmed" }),
        }
    }
}
