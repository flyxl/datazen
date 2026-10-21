//! Reuse aliases (questdb / cloudberry) must reach the resource contract.
//!
//! `driver-capability-migration.md` §6.2 makes "every namespace API call site
//! on the host migrated" the retirement condition for the legacy adapter, and
//! the platform session backend opens every resource through
//! `DriverRegistry::resource_provider(database_type)`. A PostgreSQL alias
//! whose factory returned no provider could never open a platform session —
//! the resource path failed closed with `ResourceProviderMissing` while the
//! legacy path silently kept serving the alias. These tests pin the fixed
//! state: each alias factory resolves a provider, bound to its own memoized
//! driver instance and epoch, with the alias identity stamped on its
//! capability snapshot and on every handle it issues.

use std::sync::Arc;

use datazen_driver_api::resource::ResourceProvider as _;
use datazen_driver_api::{iter_driver_factories, require_resource_provider, DatabaseDriverFactory};

fn factory(driver_id: &str) -> &'static dyn DatabaseDriverFactory {
    iter_driver_factories()
        .into_iter()
        .copied()
        .find(|factory| factory.driver_id() == driver_id)
        .unwrap_or_else(|| panic!("driver id {driver_id} must be registered by this crate"))
}

// The test binary only links datazen-driver-postgres — and thereby runs the
// `inventory::submit!` in `lib.rs` — when something below names a crate symbol.
use datazen_driver_postgres::{PostgresDriver, PostgresResourceProvider};

#[test]
fn postgres_alias_factories_resolve_the_resource_contract() {
    for id in ["questdb", "cloudberry"] {
        let provider = require_resource_provider(factory(id))
            .unwrap_or_else(|error| panic!("{id}: alias must resolve a provider: {error}"));
        assert_eq!(
            provider.provider_id(),
            id,
            "{id}: the alias, not the parent pool, owns the provider identity"
        );
        assert_eq!(
            provider.capabilities().provider_id,
            id,
            "{id}: the capability snapshot is keyed on the alias identity"
        );
    }
}

#[test]
fn alias_providers_are_memoized_and_distinct_from_the_native_pool() {
    let native = require_resource_provider(factory("postgresql")).unwrap();
    for id in ["questdb", "cloudberry"] {
        let factory = factory(id);
        let first = factory.resource_provider().expect("{id} has a provider");
        let second = factory.resource_provider().expect("{id} has a provider");
        assert!(
            Arc::ptr_eq(&first, &second),
            "{id}: a rebuilt provider would reject every handle issued by the previous one"
        );
        assert!(
            !Arc::ptr_eq(&first, &native),
            "{id}: the alias must be its own provider instance, not the native pool's — \
             one runtime epoch per instance is what makes stale handles fail"
        );
    }
}

/// The alias provider is the same machinery family as the native pool, under
/// its own identity: `with_provider_id` is the exact binding the alias
/// factories use, and the native constructor stays on the parent identity.
#[test]
fn postgres_machinery_stays_one_implementation_family() {
    let alias =
        PostgresResourceProvider::with_provider_id(Arc::new(PostgresDriver::new()), "cloudberry");
    assert_eq!(alias.provider_id(), "cloudberry");

    let native = PostgresResourceProvider::new(Arc::new(PostgresDriver::new()));
    assert_eq!(native.provider_id(), "postgresql");
}
