use crate::execution::runtime::connection as rc;
use datazen_driver_api::{namespace as dn, resource::ResourceError, session as ds};

pub fn namespace(target: &rc::NamespaceTarget) -> dn::NamespaceTarget {
    fn value(value: &str) -> Option<String> {
        if value.is_empty() || value == rc::types::UNKNOWN_SENTINEL {
            None
        } else {
            Some(value.into())
        }
    }
    dn::NamespaceTarget {
        database: value(&target.database),
        catalog: value(&target.catalog),
        schema: value(&target.schema),
        path: if target.path.is_empty() || target.path == rc::types::UNKNOWN_SENTINEL {
            vec![]
        } else {
            vec![target.path.clone()]
        },
    }
}
pub fn context(context: &ds::SessionContext) -> rc::SessionContext {
    rc::SessionContext {
        namespace: rc::NamespaceTarget {
            database: context.namespace.database.clone().unwrap_or_default(),
            catalog: context.namespace.catalog.clone().unwrap_or_default(),
            schema: context.namespace.schema.clone().unwrap_or_default(),
            path: context.namespace.path.join("/"),
        },
        search_path: context.search_path.clone(),
        effective_identity: context.effective_identity.clone().unwrap_or_default(),
        transaction_state: match context.transaction_state {
            ds::TransactionState::Idle => rc::session::TransactionState::None,
            ds::TransactionState::Active => rc::session::TransactionState::Active,
            ds::TransactionState::Aborted => rc::session::TransactionState::Aborted,
            ds::TransactionState::Unknown => rc::session::TransactionState::Unknown,
            ds::TransactionState::Unsupported => rc::session::TransactionState::Unsupported,
        },
        autocommit: context.autocommit.unwrap_or(false),
        confidence: match context.confidence {
            ds::ObservationConfidence::Confirmed => rc::ContextConfidence::Confirmed,
            ds::ObservationConfidence::Partial => rc::ContextConfidence::Partial,
            ds::ObservationConfidence::Unknown => rc::ContextConfidence::Unknown,
        },
    }
}
/// Project a driver-observed namespace into the platform-api shape.
///
/// Both sides mean the same thing by every field, so this is a straight carry:
/// `path` is a **sequence of path segments** on both sides, and an absent level
/// stays absent. Nothing is joined, defaulted or guessed here.
pub fn platform_namespace(
    target: &dn::NamespaceTarget,
) -> crate::execution::target::NamespaceTarget {
    crate::execution::target::NamespaceTarget {
        database: target.database.clone(),
        catalog: target.catalog.clone(),
        schema: target.schema.clone(),
        path: target.path.clone(),
    }
}

/// Project a driver-observed session context into the platform-api shape.
///
/// Read from the **driver observation** (`ds`), not from the runtime kernel's
/// `rc::SessionContext`: the kernel shape is a scalar projection that joins
/// `path` segments with `/` and turns `None` levels into empty strings, so it
/// loses information this contract can represent. This function is total and
/// field-by-field on purpose — the compiler holds the two shapes together, and
/// no serializer performs the conversion behind its back.
pub fn platform_context(context: &ds::SessionContext) -> crate::execution::SessionContext {
    crate::execution::SessionContext {
        namespace: platform_namespace(&context.namespace),
        // The driver shape has no "not observed" spelling for the search path —
        // it is a plain list — so this boundary cannot tell "observed empty"
        // from "unobserved". `None` would be a claim no driver made, so the
        // list projects as `Some(..)` even when it is empty (a MySQL fixed
        // session reports an empty list; a driver that cannot observe the
        // search path at all says so through `confidence`, not through this
        // field).
        search_path: Some(context.search_path.clone()),
        effective_identity: context.effective_identity.clone(),
        transaction_state: match context.transaction_state {
            ds::TransactionState::Idle => crate::execution::TransactionState::None,
            ds::TransactionState::Active => crate::execution::TransactionState::Active,
            ds::TransactionState::Aborted => crate::execution::TransactionState::Aborted,
            ds::TransactionState::Unknown => crate::execution::TransactionState::Unknown,
            ds::TransactionState::Unsupported => crate::execution::TransactionState::Unsupported,
        },
        autocommit: context.autocommit,
        confidence: match context.confidence {
            ds::ObservationConfidence::Confirmed => crate::execution::ContextConfidence::Confirmed,
            ds::ObservationConfidence::Partial => crate::execution::ContextConfidence::Partial,
            ds::ObservationConfidence::Unknown => crate::execution::ContextConfidence::Unknown,
        },
    }
}
pub fn effect(effect: ds::EffectOutcome) -> rc::EffectOutcome {
    match effect {
        ds::EffectOutcome::NotStarted => rc::EffectOutcome::NotStarted,
        ds::EffectOutcome::Completed => rc::EffectOutcome::Completed,
        ds::EffectOutcome::RolledBack => rc::EffectOutcome::RolledBack,
        ds::EffectOutcome::PartiallyApplied => rc::EffectOutcome::PartiallyApplied,
        ds::EffectOutcome::Unknown => rc::EffectOutcome::Unknown,
    }
}
pub fn state(state: ds::ExecutionState) -> rc::ExecutionState {
    match state {
        ds::ExecutionState::Queued => rc::ExecutionState::Queued,
        ds::ExecutionState::Running => rc::ExecutionState::Running,
        ds::ExecutionState::CancelRequested => rc::ExecutionState::CancelRequested,
        ds::ExecutionState::Succeeded => rc::ExecutionState::Succeeded,
        ds::ExecutionState::Failed => rc::ExecutionState::Failed,
        ds::ExecutionState::Cancelled => rc::ExecutionState::Cancelled,
    }
}
pub fn error(error: ResourceError) -> rc::ProviderError {
    match error {
        ResourceError::OperationNotSupported { .. } => rc::ProviderError::CapabilityUnsupported,
        ResourceError::BudgetDenied { requested, .. } => {
            // Provider reason strings may contain identifiers; never log them.
            tracing::warn!(
                source = "provider_budget",
                category = "unknown",
                requested,
                "resource admission refused"
            );
            rc::ProviderError::ResourceBusy("physical connection budget")
        }
        ResourceError::MissingRequiredNamespaceLevel { .. } => {
            rc::ProviderError::TargetRequired("namespace".into())
        }
        ResourceError::NamespaceTargetRejected { .. }
        | ResourceError::NonexistentNamespaceLevel { .. }
        | ResourceError::ForbiddenNamespaceLevel { .. }
        | ResourceError::AliasConflict { .. } => {
            rc::ProviderError::TargetUnsupported("namespace".into())
        }
        ResourceError::StaleRuntimeEpoch { .. } => {
            rc::ProviderError::RuntimeEpochMismatch("driver resource".into())
        }
        ResourceError::InvalidResourceState { .. } => {
            rc::ProviderError::SessionLost("driver resource unavailable".into())
        }
        ResourceError::TransactionResolutionRequired { .. } => {
            rc::ProviderError::ContextConflict("transaction resolution required".into())
        }
        ResourceError::ResourceOwnershipMismatch { .. } => {
            rc::ProviderError::HostRejected("resource ownership mismatch".into())
        }
        ResourceError::CancelTargetNotRegistered { .. } => {
            rc::ProviderError::HostRejected("cancel target mismatch".into())
        }
        ResourceError::SinkRejected { .. } => {
            rc::ProviderError::ProtocolError("result delivery failed".into())
        }
        ResourceError::Driver(datazen_driver_api::DriverError::SessionLost) => {
            rc::ProviderError::ResourceLost("retained physical session lost".into())
        }
        ResourceError::Driver(_) => rc::ProviderError::SqlError("driver execution failed".into()),
    }
}
pub fn handles(
    handles: &[ds::SessionHandleRef],
    resource_id: &str,
    runtime_epoch: u64,
) -> Vec<rc::SessionHandleRef> {
    handles
        .iter()
        .map(|handle| rc::SessionHandleRef {
            handle_id: rc::HandleId::new(&handle.handle_id),
            kind: match handle.kind {
                ds::SessionHandleKind::Transaction => rc::HandleKind::Transaction,
                ds::SessionHandleKind::Cursor => rc::HandleKind::Cursor,
                ds::SessionHandleKind::ServerPrepared => rc::HandleKind::ServerPrepared,
            },
            resource_id: rc::ResourceId::new(resource_id),
            runtime_epoch: rc::Counter::new(runtime_epoch),
            closed: handle.closed,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution as pa;

    /// A complete driver observation: every level present, two path segments.
    fn observed() -> ds::SessionContext {
        ds::SessionContext {
            namespace: dn::NamespaceTarget {
                database: Some("app".into()),
                catalog: Some("app_catalog".into()),
                schema: Some("public".into()),
                path: vec!["tenant_a".into(), "shard_1".into()],
            },
            search_path: vec!["public".into(), "extra".into()],
            effective_identity: Some("app_user".into()),
            transaction_state: ds::TransactionState::Active,
            autocommit: Some(false),
            confidence: ds::ObservationConfidence::Confirmed,
        }
    }

    /// The kernel projection is known to be lossy; this is the fact that makes
    /// the platform-api projection read the driver observation directly.
    #[test]
    fn the_runtime_kernel_projection_joins_path_segments() {
        assert_eq!(context(&observed()).namespace.path, "tenant_a/shard_1");
    }

    #[test]
    fn platform_context_carries_every_observed_field() {
        let platform = platform_context(&observed());
        assert_eq!(
            platform.namespace,
            crate::execution::target::NamespaceTarget {
                database: Some("app".into()),
                catalog: Some("app_catalog".into()),
                schema: Some("public".into()),
                path: vec!["tenant_a".into(), "shard_1".into()],
            }
        );
        assert_eq!(
            platform.search_path,
            Some(vec!["public".into(), "extra".into()])
        );
        assert_eq!(platform.effective_identity.as_deref(), Some("app_user"));
        assert_eq!(platform.transaction_state, pa::TransactionState::Active);
        assert_eq!(platform.autocommit, Some(false));
        assert_eq!(platform.confidence, pa::ContextConfidence::Confirmed);
    }

    #[test]
    fn platform_context_keeps_absent_levels_absent() {
        let platform = platform_context(&ds::SessionContext::unobserved());
        assert_eq!(platform.namespace.database, None);
        assert_eq!(platform.namespace.catalog, None);
        assert_eq!(platform.namespace.schema, None);
        assert_eq!(platform.namespace.path, Vec::<String>::new());
        assert_eq!(platform.effective_identity, None);
        // "the driver cannot tell" is not the same as `false`.
        assert_eq!(platform.autocommit, None);
        assert_eq!(platform.transaction_state, pa::TransactionState::Unknown);
        assert_eq!(platform.confidence, pa::ContextConfidence::Unknown);
        // A driver always reports a list; empty means observed-empty.
        assert_eq!(platform.search_path, Some(Vec::new()));
    }
}
