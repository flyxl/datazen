#[cfg(test)]
mod tests {
    use datazen_driver_api::iter_driver_factories;
    use datazen_runtime::driver_registry::{DriverRegistry, ResourceProviderLookup};
    use std::sync::Arc;

    /// Every factory linked into this build resolves or explains itself, and
    /// every one that resolves hands out the *same* provider on every lookup.
    ///
    /// Runs against real drivers rather than fixtures, so a driver that starts
    /// answering `Ok` with a provider it cannot keep alive — or a driver that
    /// drops a fresh provider mid-build — fails here. Which drivers are linked
    /// depends on the build's driver features, so the census is reported rather
    /// than pinned; only the *properties* are pinned.
    ///
    /// There is no exemption list. A driver that has not been migrated answers
    /// `Missing` and is legitimately absent from the stable set; a driver that
    /// answers `Ok` has claimed the resource contract, and a claim it cannot
    /// honour across two lookups is a defect, not a documented exception.
    #[tokio::test]
    async fn every_linked_factory_resolves_or_reports_why_not() {
        let registry = DriverRegistry::new();
        let mut stable: Vec<&'static str> = Vec::new();
        let mut rebuilt_per_lookup: Vec<&'static str> = Vec::new();
        let mut missing: Vec<&'static str> = Vec::new();

        for factory in iter_driver_factories() {
            let id = factory.driver_id();
            match registry.resource_provider(&id.to_string()).await {
                Ok(provider) => {
                    let again = registry
                        .resource_provider(&id.to_string())
                        .await
                        .expect("a factory that resolved once keeps resolving");
                    if Arc::ptr_eq(&provider, &again) {
                        stable.push(id);
                    } else {
                        rebuilt_per_lookup.push(id);
                    }
                }
                Err(ResourceProviderLookup::Missing(error)) => {
                    assert_eq!(
                        error.driver_id, id,
                        "the error must name the driver asked for"
                    );
                    missing.push(id);
                }
                Err(other) => panic!("driver '{id}' failed for the wrong reason: {other}"),
            }
        }

        // The host links postgres, mysql and sqlite unconditionally
        // (`src-tauri/Cargo.toml`), so an empty stable set would mean the loop
        // never ran — not that there is nothing to check.
        assert!(
            !stable.is_empty(),
            "no linked driver resolved a memoized provider; the census proved nothing"
        );
        assert!(
            rebuilt_per_lookup.is_empty(),
            "driver(s) {rebuilt_per_lookup:?} rebuild their provider on every lookup. \
             The host resolves per call, so the handles one instance issued are \
             rejected by the next — and the provider drives a DatabaseDriver the \
             host never registered. Memoize the driver and its provider together \
             in the driver crate; caching it here would only hide the churn \
             behind one lucky instance."
        );
        println!(
            "{stable:?} keep one provider across lookups; {} rebuilt per call: \
             {rebuilt_per_lookup:?}; {} reported missing {missing:?}",
            stable.len(),
            missing.len(),
        );
    }
}
