use datazen_driver_api::{namespace as dn, session as ds, resource::ResourceError};
use datazen_runtime::connection as rc;

pub fn namespace(target: &rc::NamespaceTarget) -> dn::NamespaceTarget {
    fn value(value: &str) -> Option<String> { if value.is_empty() || value == rc::types::UNKNOWN_SENTINEL { None } else { Some(value.into()) } }
    dn::NamespaceTarget { database: value(&target.database), catalog: value(&target.catalog), schema: value(&target.schema), path: if target.path.is_empty() || target.path == rc::types::UNKNOWN_SENTINEL { vec![] } else { vec![target.path.clone()] } }
}
pub fn context(context: &ds::SessionContext) -> rc::SessionContext {
    rc::SessionContext { namespace: rc::NamespaceTarget { database: context.namespace.database.clone().unwrap_or_default(), catalog: context.namespace.catalog.clone().unwrap_or_default(), schema: context.namespace.schema.clone().unwrap_or_default(), path: context.namespace.path.join("/") }, search_path: context.search_path.clone(), effective_identity: context.effective_identity.clone().unwrap_or_default(), transaction_state: match context.transaction_state { ds::TransactionState::Idle => rc::session::TransactionState::None, ds::TransactionState::Active => rc::session::TransactionState::Active, ds::TransactionState::Aborted => rc::session::TransactionState::Aborted, ds::TransactionState::Unknown => rc::session::TransactionState::Unknown, ds::TransactionState::Unsupported => rc::session::TransactionState::Unsupported }, autocommit: context.autocommit.unwrap_or(false), confidence: match context.confidence { ds::ObservationConfidence::Confirmed => rc::ContextConfidence::Confirmed, ds::ObservationConfidence::Partial => rc::ContextConfidence::Partial, ds::ObservationConfidence::Unknown => rc::ContextConfidence::Unknown } }
}
pub fn effect(effect: ds::EffectOutcome) -> rc::EffectOutcome {
    match effect { ds::EffectOutcome::NotStarted => rc::EffectOutcome::NotStarted, ds::EffectOutcome::Completed => rc::EffectOutcome::Completed, ds::EffectOutcome::RolledBack => rc::EffectOutcome::RolledBack, ds::EffectOutcome::PartiallyApplied => rc::EffectOutcome::PartiallyApplied, ds::EffectOutcome::Unknown => rc::EffectOutcome::Unknown }
}
pub fn state(state: ds::ExecutionState) -> rc::ExecutionState {
    match state { ds::ExecutionState::Queued => rc::ExecutionState::Queued, ds::ExecutionState::Running => rc::ExecutionState::Running, ds::ExecutionState::CancelRequested => rc::ExecutionState::CancelRequested, ds::ExecutionState::Succeeded => rc::ExecutionState::Succeeded, ds::ExecutionState::Failed => rc::ExecutionState::Failed, ds::ExecutionState::Cancelled => rc::ExecutionState::Cancelled }
}
pub fn error(error: ResourceError) -> rc::ProviderError {
    match error {
        ResourceError::OperationNotSupported { .. } => rc::ProviderError::CapabilityUnsupported,
        ResourceError::BudgetDenied { .. } => rc::ProviderError::ResourceBusy("physical connection budget"),
        ResourceError::MissingRequiredNamespaceLevel { .. } => rc::ProviderError::TargetRequired("namespace".into()),
        ResourceError::NamespaceTargetRejected { .. } | ResourceError::NonexistentNamespaceLevel { .. } | ResourceError::ForbiddenNamespaceLevel { .. } | ResourceError::AliasConflict { .. } => rc::ProviderError::TargetUnsupported("namespace".into()),
        ResourceError::StaleRuntimeEpoch { .. } => rc::ProviderError::RuntimeEpochMismatch("driver resource".into()),
        ResourceError::InvalidResourceState { .. } => rc::ProviderError::SessionLost("driver resource unavailable".into()),
        ResourceError::TransactionResolutionRequired { .. } => rc::ProviderError::ContextConflict("transaction resolution required".into()),
        ResourceError::ResourceOwnershipMismatch { .. } => rc::ProviderError::HostRejected("resource ownership mismatch".into()),
        ResourceError::CancelTargetNotRegistered { .. } => rc::ProviderError::HostRejected("cancel target mismatch".into()),
        ResourceError::SinkRejected { .. } => rc::ProviderError::ProtocolError("result delivery failed".into()),
        ResourceError::Driver(_) => rc::ProviderError::SqlError("driver execution failed".into()),
    }
}
pub fn handles(handles: &[ds::SessionHandleRef], resource_id: &str, runtime_epoch: u64) -> Vec<rc::SessionHandleRef> {
    handles.iter().map(|handle| rc::SessionHandleRef { handle_id: rc::HandleId::new(&handle.handle_id), kind: match handle.kind { ds::SessionHandleKind::Transaction => rc::HandleKind::Transaction, ds::SessionHandleKind::Cursor => rc::HandleKind::Cursor, ds::SessionHandleKind::ServerPrepared => rc::HandleKind::ServerPrepared }, resource_id: rc::ResourceId::new(resource_id), runtime_epoch: rc::Counter::new(runtime_epoch), closed: handle.closed }).collect()
}
