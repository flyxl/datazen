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

use crate::hbase::HBaseDriver;
use crate::resource::capabilities::{
    hbase_capability_evidence, hbase_capability_registry, hbase_capability_set,
    hbase_namespace_shape, HBASE_PROVIDER_ID,
};
use crate::HBaseFactory;

use super::support::provider;

#[test]
fn the_factory_resolves_a_real_provider_through_the_fail_closed_accessor() {
    // `resource_provider().ok_or(..)` would let a driver that quietly returns
    // `None` pass this test; `require_resource_provider` cannot.
    let provider = datazen_driver_api::require_resource_provider(&HBaseFactory)
        .expect("the hbase factory must resolve a resource provider");
    assert_eq!(provider.provider_id(), HBASE_PROVIDER_ID);

    // Memoized, and bound to the one driver the host also gets — so a handle
    // minted through the provider is reachable by the host's driver.
    let again = datazen_driver_api::require_resource_provider(&HBaseFactory)
        .expect("a second resolution must find the same provider");
    assert!(Arc::ptr_eq(&provider, &again));
    assert!(Arc::ptr_eq(
        &HBaseFactory.driver(),
        &HBaseFactory.driver()
    ));
}

#[test]
fn the_declared_driver_id_is_the_one_the_factory_reports() {
    let factory = HBaseFactory;
    assert_eq!(DatabaseDriverFactory::driver_id(&factory), "hbase");
    assert_eq!(factory.resource_capabilities(), hbase_capability_set());
}

#[test]
fn the_provider_shares_the_driver_instance_it_was_given() {
    let driver = Arc::new(HBaseDriver::new());
    let provider = crate::resource::HBaseResourceProvider::new(driver.clone());
    assert!(
        Arc::ptr_eq(provider.driver(), &driver),
        "the provider must use the driver's own client pool, not a clone of it"
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
        &hbase_namespace_shape(),
        "the exposed shape must be the one acquisition validation uses"
    );
    assert_eq!(
        provider.capabilities().capabilities,
        hbase_capability_set(),
        "the registry must carry the declared set, not the default one"
    );
    assert_eq!(provider.capabilities().provider_id, HBASE_PROVIDER_ID);
}

#[test]
fn the_capability_set_declares_a_shape_and_is_not_the_inert_default() {
    let declared = hbase_capability_set();
    assert_ne!(
        declared,
        CapabilitySet::default(),
        "an all-default set is the signature of a driver that was never migrated"
    );
    assert!(
        declared.declares_anything(),
        "a driver that declares nothing must not be presented as migrated"
    );

    // Stargate exposes a REST address, nothing like a session: no namespace
    // level, no transaction, no server-side handle a caller could hand back.
    let flat = declared.clone();
    assert_eq!(flat.stateful_session, Availability::Unsupported);
    assert_eq!(flat.namespace_switch, NamespaceSwitch::Unsupported);
    assert_eq!(flat.context_observation, ContextObservation::Unsupported);
    assert_eq!(flat.snapshots, SnapshotSupport::Unsupported);
    assert_eq!(
        flat.session_scoped_handles,
        SessionScopedHandleSupport::Unsupported
    );
    assert_eq!(flat.reset_for_reuse, ResetForReuse::Unsupported);
    assert_eq!(flat.precise_cancel, PreciseCancelSupport::Unsupported);
    assert_eq!(flat.transaction_observation, TransactionObservation::Unsupported);

    // `transactions` is the one cell that is a shape rather than a flag, so it
    // gets compared whole.
    assert!(flat.transactions.isolation_levels.is_empty());
    assert_eq!(flat.transactions.savepoints, Availability::Unsupported);
    assert_eq!(flat.transactions.max_open_transactions, None);
}

/// Cells this driver *cannot* answer from its own code stay at the fail-closed
/// default — and the reason is written down rather than guessed at.
#[test]
fn the_cells_without_evidence_stay_closed_and_say_why() {
    let declared = hbase_capability_set();
    assert_eq!(
        declared.ddl_atomicity,
        CapabilitySet::default().ddl_atomicity,
        "Stargate has no DDL to make atomic, and the driver refuses every write \
         (HBaseDriver::execute) — this must not be softened to Supported"
    );
    assert_eq!(
        declared.data,
        CapabilitySet::default().data,
        "there is no data-import entry point in this driver at all"
    );
    assert_eq!(
        declared.backup,
        CapabilitySet::default().backup,
        "ResourceProvider carries no backup surface to declare"
    );

    let evidence = hbase_capability_evidence();
    assert!(
        evidence.len() >= 12,
        "every declared cell needs a cited reason, got {}",
        evidence.len()
    );
    for (cell, reason) in &evidence {
        assert!(
            !reason.trim().is_empty(),
            "the evidence for {cell} must say something"
        );
    }
    for required in ["declined:", "unknown:", "confirmed:"] {
        assert!(
            evidence.iter().any(|(_, reason)| reason.contains(required)),
            "the evidence must classify its cells with `{required}`"
        );
    }
}

/// The registry is what the host reads; `CapabilityRegistry::new` starts from
/// the default, so the declared set has to overwrite it explicitly.
#[test]
fn the_registry_is_overwritten_rather_than_inherited() {
    let registry = hbase_capability_registry();
    assert_eq!(registry.provider_id, HBASE_PROVIDER_ID);
    assert_ne!(registry.capabilities, CapabilitySet::default());
    assert_eq!(registry.capabilities, hbase_capability_set());
}

/// Stargate is addressed by one cluster URL: there is no level to name.
#[test]
fn the_namespace_shape_declares_no_level_and_naming_one_is_refused() {
    let shape = hbase_namespace_shape();
    assert!(shape.levels.is_empty());
    assert!(shape.path_segments.is_empty());

    // Nothing to name, so naming something is an error — not a silently
    // discarded target.
    for target in [
        NamespaceTarget {
            database: Some("default".into()),
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
