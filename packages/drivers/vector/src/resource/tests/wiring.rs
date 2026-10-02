//! How the provider is reached, and what it claims about itself.
//!
//! Two questions are answered here, and both have a failure mode this
//! migration produces regularly:
//!
//! * does the driver really resolve to a *resource provider* through the
//!   fail-closed accessor, or does it fall back to "no provider"?
//! * does it declare a capability shape, or does it ship the inert default
//!   that reads exactly like an un-migrated driver?

use std::sync::Arc;

use datazen_driver_api::capabilities::{
    Availability, CapabilitySet, ContextObservation, NamespaceSwitch, PreciseCancelSupport,
    ResetForReuse, SessionScopedHandleSupport, SnapshotSupport, TransactionObservation,
};
use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::DatabaseDriverFactory;

use crate::resource::capabilities::{
    vector_capability_evidence, vector_capability_registry, vector_capability_set,
    vector_namespace_shape, VECTOR_PROVIDER_ID,
};
use crate::vector::VectorDriver;
use crate::VectorFactory;

use super::support::provider;

#[test]
fn the_factory_resolves_a_real_provider_through_the_fail_closed_accessor() {
    // `resource_provider().ok_or(..)` would let a driver that quietly returns
    // `None` pass this test; `require_resource_provider` cannot.
    let provider = datazen_driver_api::require_resource_provider(&VectorFactory)
        .expect("the vector factory must resolve a resource provider");
    assert_eq!(provider.provider_id(), VECTOR_PROVIDER_ID);

    // Memoized, and bound to the one driver the host also gets — so a handle
    // minted through the provider is reachable by the host's driver.
    let again = datazen_driver_api::require_resource_provider(&VectorFactory)
        .expect("a second resolution must find the same provider");
    assert!(Arc::ptr_eq(&provider, &again));
    assert!(Arc::ptr_eq(
        &VectorFactory.driver(),
        &VectorFactory.driver()
    ));
}

#[test]
fn the_declared_driver_id_is_the_one_the_factory_reports() {
    let factory = VectorFactory;
    assert_eq!(DatabaseDriverFactory::driver_id(&factory), "vector");
    assert_eq!(factory.resource_capabilities(), vector_capability_set());
}

#[test]
fn the_provider_shares_the_driver_instance_it_was_given() {
    let driver = Arc::new(VectorDriver::new());
    let provider = crate::resource::VectorResourceProvider::new(driver.clone());
    assert!(
        Arc::ptr_eq(provider.driver(), &driver),
        "the provider must use the driver's own HTTP pool, not a clone of it"
    );
}

#[test]
fn each_provider_instance_stamps_its_own_runtime_epoch() {
    let first = provider();
    let second = provider();
    assert_ne!(
        first.runtime_epoch(),
        second.runtime_epoch(),
        "two instances must not accept each other's handles"
    );
}

#[test]
fn the_provider_exposes_the_capabilities_and_shape_it_validates_against() {
    let provider = provider();
    assert_eq!(
        provider.namespace_shape(),
        &vector_namespace_shape(),
        "the exposed shape must be the one acquisition validation uses"
    );
    assert_eq!(
        provider.capabilities().capabilities,
        vector_capability_set(),
        "the registry must carry the declared set, not the default one"
    );
    assert_eq!(provider.capabilities().provider_id, VECTOR_PROVIDER_ID);
}

#[test]
fn the_capability_set_declares_a_shape_and_is_not_the_inert_default() {
    let declared = vector_capability_set();
    assert_ne!(
        declared,
        CapabilitySet::default(),
        "the default is indistinguishable from an un-migrated driver"
    );
    assert!(declared.declares_anything());

    // Eight cells declined outright, each for a reason the capability table
    // spells out against the code that proves it.
    assert!(matches!(
        declared.stateful_session,
        Availability::Unsupported
    ));
    assert!(matches!(
        declared.namespace_switch,
        NamespaceSwitch::Unsupported
    ));
    assert!(matches!(
        declared.context_observation,
        ContextObservation::Unsupported
    ));
    assert!(matches!(
        declared.transaction_observation,
        TransactionObservation::Unsupported
    ));
    assert!(matches!(
        declared.session_scoped_handles,
        SessionScopedHandleSupport::Unsupported
    ));
    assert!(matches!(
        declared.reset_for_reuse,
        ResetForReuse::Unsupported
    ));
    assert!(matches!(
        declared.precise_cancel,
        PreciseCancelSupport::Unsupported
    ));
    assert!(matches!(declared.snapshots, SnapshotSupport::Unsupported));

    // A transaction claim with nothing behind it is worse than none at all.
    assert!(declared.transactions.isolation_levels.is_empty());
    assert!(matches!(
        declared.transactions.savepoints,
        Availability::Unsupported
    ));
    assert_eq!(declared.transactions.max_open_transactions, None);

    // The three cells with no honest answer are left at the fail-closed default
    // rather than guessed at.
    let default = CapabilitySet::default();
    assert_eq!(declared.ddl_atomicity, default.ddl_atomicity);
    assert_eq!(declared.data, default.data);
    assert_eq!(declared.backup, default.backup);
}

#[test]
fn every_declared_capability_carries_a_written_reason() {
    let evidence = vector_capability_evidence();
    assert!(
        evidence.len() >= 12,
        "expected an entry per capability cell, got {}",
        evidence.len()
    );
    for (name, reason) in &evidence {
        assert!(
            reason.trim().len() > 20,
            "capability {name} is asserted without a reason a reviewer could check"
        );
        assert!(
            reason.contains(':') && reason.trim_start().starts_with("declined:")
                || reason.trim_start().starts_with("unknown:")
                || reason.trim_start().starts_with("confirmed:"),
            "capability {name} must say which kind of claim it is"
        );
    }
    for cell in [
        "stateful_session",
        "namespace_switch",
        "context_observation",
        "transaction_observation",
        "session_scoped_handles",
        "reset_for_reuse",
        "precise_cancel",
        "snapshots",
        "transactions",
        "ddl_atomicity",
        "data",
        "backup",
    ] {
        assert!(
            evidence.iter().any(|(name, _)| *name == cell),
            "no evidence recorded for {cell}"
        );
    }
}

#[test]
fn the_capability_registry_carries_the_snapshot_that_backed_it() {
    let snapshot = vector_capability_registry().snapshot();
    assert_eq!(snapshot.driver_id, VECTOR_PROVIDER_ID);
    assert!(
        !snapshot.confirmed.is_empty(),
        "a snapshot with nothing confirmed cannot justify the declared set"
    );
}

#[test]
fn the_namespace_shape_declares_no_levels_so_any_supplied_target_is_refused() {
    let shape = vector_namespace_shape();
    assert!(shape.levels.is_empty());
    assert!(shape.path_segments.is_empty());

    // Nothing to name, so naming something is an error — not a silently
    // discarded target.
    for target in [
        NamespaceTarget {
            database: Some("books".into()),
            ..NamespaceTarget::empty()
        },
        NamespaceTarget {
            schema: Some("public".into()),
            ..NamespaceTarget::empty()
        },
        NamespaceTarget {
            catalog: Some("prod".into()),
            ..NamespaceTarget::empty()
        },
    ] {
        assert!(
            shape.canonicalize(&target).is_err(),
            "{target:?} names a level this shape does not declare"
        );
    }
    assert!(shape.canonicalize(&NamespaceTarget::empty()).is_ok());
}
