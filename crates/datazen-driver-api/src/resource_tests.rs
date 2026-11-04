use super::*;
use crate::capabilities::{CapabilityRegistry, SessionContinuity};
use crate::session::ExecutionErrorCode;

fn descriptor_with(continuity: SessionContinuity) -> ResourceDescriptor {
    ResourceDescriptor {
        provider_id: "fake".to_string(),
        resource_key: "fake:main".to_string(),
        session_continuity: continuity,
        reuse_policy: ReusePolicy::SingleUse,
        initialization_requirements: Vec::new(),
        connection_cost_policy: ConnectionCostPolicy::PoolBounded {
            max_physical_connections: 1,
        },
        namespace_shape: NamespaceShape::default(),
    }
}

#[test]
fn handle_rejects_a_foreign_provider() {
    let handle = ResourceHandle::issue("postgres", "conn-1", 7);
    let error = handle
        .check("mysql", 7)
        .expect_err("another provider must not honour this handle");
    assert!(matches!(
        error,
        ResourceError::ResourceOwnershipMismatch { .. }
    ));
}

#[test]
fn handle_rejects_a_stale_runtime_epoch() {
    let handle = ResourceHandle::issue("postgres", "conn-1", 7);
    let error = handle
        .check("postgres", 8)
        .expect_err("a handle from a previous incarnation is refused");
    match error {
        ResourceError::StaleRuntimeEpoch { expected, actual } => {
            assert_eq!(expected, 8);
            assert_eq!(actual, 7);
        }
        other => panic!("expected a stale epoch error, got {other:?}"),
    }
}

#[test]
fn handle_accepts_its_own_provider_and_epoch() {
    let handle = ResourceHandle::issue("postgres", "conn-1", 7);
    assert!(handle.check("postgres", 7).is_ok());
    assert_eq!(handle.provider_id(), "postgres");
    assert_eq!(handle.resource_key(), "conn-1");
    assert_eq!(handle.runtime_epoch(), 7);
}

#[test]
fn handle_serializes_for_a_receipt_but_is_not_reconstructible() {
    let handle = ResourceHandle::issue("postgres", "conn-1", 7);
    let value = serde_json::to_value(&handle).expect("handle serializes");
    assert_eq!(value["providerId"], "postgres");
    assert_eq!(value["resourceKey"], "conn-1");
    assert_eq!(value["runtimeEpoch"], 7);
}

#[test]
fn a_pool_of_one_is_not_a_fixed_session() {
    let descriptor = descriptor_with(SessionContinuity::Leased);
    assert!(!descriptor.is_fixed_session());
}

#[test]
fn a_declared_fixed_session_is_reported_as_one() {
    let descriptor = descriptor_with(SessionContinuity::Fixed);
    assert!(descriptor.is_fixed_session());
}

#[test]
fn an_undeclared_continuity_is_not_treated_as_fixed() {
    // A single-connection pool with an unknown continuity must not pass as a
    // fixed resource (CM-19).
    assert!(!descriptor_with(SessionContinuity::Unknown).is_fixed_session());
}

#[test]
fn reuse_policy_defaults_to_unknown() {
    assert_eq!(ReusePolicy::default(), ReusePolicy::Unknown);
}

#[test]
fn an_empty_namespace_target_carries_nothing() {
    let target = NamespaceTarget::empty();
    assert!(target.database.is_none());
    assert!(target.catalog.is_none());
    assert!(target.schema.is_none());
    assert!(target.path.is_empty());
}

#[test]
fn an_undeclared_capability_is_rejected_rather_than_assumed() {
    let registry = CapabilityRegistry::new(
        "fake",
        crate::capabilities::CapabilitySnapshot::new("fake", "0", crate::PROTOCOL_VERSION, 1),
    );
    let error = registry
        .require_precise_cancel()
        .expect_err("precise cancel was never declared");
    assert!(error.to_string().contains("fake"));
}

#[test]
fn unsupported_construction_reports_the_driver_and_the_operation() {
    let error = ResourceError::unsupported("fake", "resetResource", "no baseline");
    assert!(error.to_string().contains("fake"));
    assert!(error.to_string().contains("resetResource"));
}

#[test]
fn not_started_completion_is_available_as_a_production_value() {
    // Providers reject work before touching the wire by returning this; it is
    // not test-only scaffolding.
    let completion = crate::session::ExecutionCompletion::not_started();
    assert_eq!(
        completion.effect_outcome,
        crate::session::EffectOutcome::NotStarted
    );
    assert!(matches!(
        completion.completion_status,
        crate::session::CompletionStatus::Failed {
            code: ExecutionErrorCode::HostRejected
        }
    ));
}

#[test]
fn baseline_round_trips_its_initialization_requirements() {
    let baseline = Baseline::new(vec![InitializationRequirement {
        id: "search_path".to_string(),
        sql: "SET search_path TO dbo".to_string(),
        mandatory: true,
        idempotent: true,
    }]);
    let value = serde_json::to_value(&baseline).expect("baseline serializes");
    assert_eq!(
        value["initializationRequirements"].as_array().map(Vec::len),
        Some(1)
    );
}

#[test]
fn command_call_carries_the_driver_s_own_command_name_and_payload() {
    let call = CommandCall::new("list_objects", serde_json::json!({"schema": "dbo"}));
    assert_eq!(call.command, "list_objects");
    assert_eq!(call.input["schema"], "dbo");
}

#[test]
fn budget_permit_is_opaque_and_serializes_for_accounting() {
    let permit = BudgetPermit {
        permit_id: "permit-1".to_string(),
        physical_connections: 2,
    };
    let value = serde_json::to_value(&permit).expect("permit serializes");
    assert_eq!(value["permitId"], "permit-1");
    assert_eq!(value["physicalConnections"], 2);
}
