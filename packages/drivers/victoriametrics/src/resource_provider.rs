//! Track O: VictoriaMetrics's real [`ResourceProvider`].
//!
//! VictoriaMetrics is reached through the contract's own migration bridge,
//! [`LegacyResourceAdapter`], rather than through a bespoke provider impl. The adapter
//! is what makes the handles real: it opens the connection itself, mints an opaque
//! [`ResourceHandle`] bound to that connection's own id, charges the caller's
//! [`BudgetPort`] *before* any wire work, and releases the charge exactly once on
//! close.
//!
//! Everything VictoriaMetrics cannot honestly perform is refused by name instead of
//! answered with an empty success:
//!
//! * `request_cancel` — MetricsQL/select has no cancel signal on the HTTP path, so the
//! adapter answers `CancelDisposition::Unsupported` and never calls the legacy
//! `cancel_query`. That method used to answer `Ok(())`, i.e. a reported cancellation
//! that never happened; Track O turns it into an explicit `DriverError::Unsupported`.
//! * `observe_session` — every field unknown, never back-filled with the acquisition
//! target.
//! * `change_context` — refused, because switching the VictoriaMetrics account is a
//! reconnect.
//! * `commit_transaction` / `rollback_transaction` — VictoriaMetrics has no
//! transactions, so the port refuses rather than inventing an outcome.
//!
//! # Namespace
//!
//! `VictoriaMetricsDriver` documents this at `victoriametrics.rs:216`: there is no
//! schema level, the server itself is the namespace. `has_schema_level` is the default
//! `false`, and `resolve_database` honours a non-blank configured value while a blank
//! one falls back to `DEFAULT_DATABASE` ("default"). The shape declares an **optional**
//! `Database` level — the tenant — and nothing else, so a `Schema` or `Catalog` is
//! refused by name.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::{
    Availability, CapabilitySet, ContextObservation, DdlAtomicitySupport, NamespaceSwitch,
    PreciseCancelSupport, ResetForReuse, SessionScopedHandleSupport, SnapshotSupport,
    TransactionObservation, TransactionSupport,
};
use datazen_driver_api::capability_domains::{BackupSupport, DataSupport};
use datazen_driver_api::namespace::{NamespaceLevel, NamespaceLevelKind, NamespaceShape};
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::DatabaseDriver;

/// Epoch counter handed to each provider this crate builds.
///
/// The host has no session generation to hand a driver yet
/// (`src-tauri/src/platform/adapter.rs` records `RuntimeEpoch` as 「❌ 无会话世代」), so
/// the provider mints its own instead of inventing a fixed constant.
/// `LegacyResourceAdapter` checks each handle against the epoch it was built with, so
/// a handle minted by one provider instance is rejected by any other.
///
/// The counter is only advanced inside [`PROVIDER`]'s initializer, so the one
/// provider that exists takes one epoch and keeps it: the host calls
/// `require_resource_provider` afresh on every lookup, and a provider rebuilt per
/// call would invalidate every handle the moment it was looked up again.
///
/// The counter restarts at 1 on each process start, so a handle that outlived a
/// restart is *not* caught here — that needs a host-owned epoch, which a driver crate
/// cannot supply.
static PROVIDER_GENERATION: AtomicU64 = AtomicU64::new(1);

/// The one provider this process has. Memoized, so its epoch never changes and the
/// handles it issues stay valid across however many times the host looks it up.
static PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

/// The epoch [`PROVIDER`] was actually built with.
///
/// The counter cannot answer that on its own — it advances for every provider built,
/// memoized or not — so the value the live provider took is recorded next to it.
static PROVIDER_EPOCH: OnceLock<u64> = OnceLock::new();

/// The namespace VictoriaMetrics actually addresses.
///
/// VictoriaMetrics addresses one namespace level, the tenant.
pub(crate) fn namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![NamespaceLevel {
            kind: NamespaceLevelKind::Database,
            exists: true,
            required: false,
        }],
        ..NamespaceShape::default()
    }
}

/// The real provider for `victoriametrics`, built once and shared.
///
/// `driver` is the instance the host also gets from `DatabaseDriverFactory::create`,
/// not a throwaway: the provider and the host must be looking at the same driver, or
/// a handle it issues would address a different one.
pub(crate) fn provider(driver: Arc<dyn DatabaseDriver>) -> Arc<dyn ResourceProvider> {
    PROVIDER
        .get_or_init(|| {
            let runtime_epoch = PROVIDER_GENERATION.fetch_add(1, Ordering::Relaxed);
            // Recorded inside the initializer, so it always describes the provider
            // that won the race and never a discarded attempt.
            let _ = PROVIDER_EPOCH.set(runtime_epoch);
            Arc::new(adapter(driver, runtime_epoch))
        })
        .clone()
}

/// Build the adapter and attach this crate's evidence table.
///
/// Both construction sites go through here on purpose: the memoized provider and
/// the stray one have to describe the *same* declaration, otherwise a test that
/// compares them would be comparing two different capability claims.
fn adapter(driver: Arc<dyn DatabaseDriver>, runtime_epoch: u64) -> LegacyResourceAdapter {
    LegacyResourceAdapter::new(
        driver,
        "victoriametrics",
        env!("CARGO_PKG_VERSION"),
        runtime_epoch,
        namespace_shape(),
        capabilities(),
    )
    .with_evidence(capability_evidence())
}

/// The epoch the live provider stamps into every handle it mints.
///
/// Test-only: production code hands a handle back to the provider that issued it and
/// never has to compare epochs itself.
#[cfg(test)]
pub(crate) fn runtime_epoch() -> u64 {
    *PROVIDER_EPOCH
        .get()
        .expect("the memoized provider is built before any handle is minted")
}

/// Build a provider that is *not* [`PROVIDER`], and the epoch it took.
///
/// Test-only. It reproduces the failure per-call construction caused, so the
/// memoization test can show the epoch check really does refuse a foreign handle.
#[cfg(test)]
pub(crate) fn stray_provider(driver: Arc<dyn DatabaseDriver>) -> Arc<dyn ResourceProvider> {
    let runtime_epoch = PROVIDER_GENERATION.fetch_add(1, Ordering::Relaxed);
    Arc::new(adapter(driver, runtime_epoch))
}

/// What this crate can honestly claim, capability by capability.
///
/// Every entry below is a claim about code that **already exists** in this crate. A
/// capability that is not proven is declared blank (`Unknown` / `Unsupported`) and
/// says here why it could not be proven — a blank cell is a finding, not an
/// omission.
///
/// | Capability | Declared | Why |
/// |---|---|---|
/// | `stateful_session` | `Availability::Unsupported` | the handle owns only a `reqwest::Client` and a base URL in a map keyed by connection id (`victoriametrics.rs:20`) — an HTTP transport, not a server-side session. Every statement independently builds `GET /api/v1/query?query=…` and reads a single body (`victoriametrics.rs:318-335`); no session-establishing call is ever issued and no server-assigned session id is ever stored, so the driver can neither address nor reuse a server-side session across statements. That is the same architecture as mysql, which the corpus already rules `Unsupported` on these grounds (`packages/drivers/mysql/src/resource_capabilities.rs:74` — a client is not a session), so the architecture is *measured*, not unmeasured: `Unknown` would discard evidence the repository already holds. Note this is independent of `SessionContinuity`, which the adapter hard-codes to `Unknown` for every legacy driver (`resource_adapter.rs:271`) — a driver may honestly report one and the other differently, as sqlserver does. |
/// | `namespace_switch` | `Unsupported` | there is no attached context to switch. `resolve_database` is a pure function of the explicit argument (`victoriametrics.rs:44-51`) and the doc states that this driver keeps no mutable session state (`victoriametrics.rs:40-43`, `use_database` is gone). Nothing is switched, so nothing can be switched in place; and there is no replacement mechanism to point at either, which is why this is not `RequiresReplacement`. It is not `PerRequest` either, and the decisive fact is not how many namespaces the server has but *where the argument goes*: in `get_tables` the resolved name reaches only a `tracing::debug!` (`victoriametrics.rs:218`) while the request path is the hardcoded constant `/api/v1/label/__name__/values` (`victoriametrics.rs:222`). The driver accepts the tenant, logs it, and drops it — it never demonstrably addresses another namespace. Measured absent, not merely undeclared. |
/// | `context_observation` | *(blank)* `Unsupported` | `observe_session` returns `SessionObservation::unobservable()` unconditionally in the adapter, so this provider has no code path that can report a session context. `Partial` would promise a half-answered observation that never arrives. |
/// | `transaction_observation` | *(blank)* `Unsupported` | measured absence, not an unmeasured blank: `begin_transaction` is not overridden, so it takes the trait default that errors ("Not supported for this driver type", `traits.rs:602-609`), and `command_definitions()` publishes query, query-stream and schema-catalog commands only — no transaction command exists to invoke (`victoriametrics.rs:395-400`). The adapter refuses commit and rollback too, because VictoriaMetrics has no transactions. |
/// | `session_scoped_handles` | *(blank)* `Unknown` | there is no cursor, prepared statement or transaction handle to mint: the handle is the connection (`victoriametrics.rs:30-36`), and the whole API surface is one HTTP GET per call (`victoriametrics.rs:53-71`). Nothing was measured because nothing was ever opened. |
/// | `reset_for_reuse` | *(blank)* `Unsupported` | there is no verified baseline replay in this crate, and no baseline to replay: the adapter answers `ResetDisposition::Discard`. `Verified` would hand out `Ok(())` and open a feature (`capabilities.rs:534-539`) that nothing here has earned. |
/// | `precise_cancel` | *(blank)* `Unknown` | see the note below — the adapter overwrites this cell. |
/// | `snapshots` | *(blank)* `Unsupported` | there is no snapshot operation in this crate. `command_definitions()` publishes no snapshot command (`victoriametrics.rs:395-400`), and the HTTP API surface used here is the instant-query endpoint (`victoriametrics.rs:326-331`). Measured absence. |
/// | `transactions.isolation_levels` | *empty* | there is no transaction to hand an option to: `begin_transaction` is the trait default error (`traits.rs:602-609`). An empty list reads as "cannot confirm any level", which is the whole truth here. |
/// | `transactions.savepoints` | *(blank)* `Unknown` | nothing was measured, and nothing could be: the driver has no transaction surface at all. `Unsupported` would claim a measurement that was never taken. |
/// | `transactions.max_open_transactions` | `None` | no bound can be proven without a transaction to open. `Some(0)` would claim transactions are refused *by measurement*; `None` is the honest "no measurement exists". |
/// | `ddl_atomicity.by_operation` | *empty* | this driver does not override `DatabaseDriver::ddl_atomicity` (`traits.rs:156`, default `Unknown`), and it cannot: `execute` is a hard read-only refusal — "VictoriaMetrics is read-only via the HTTP API" (`victoriametrics.rs:389-393`) — and no DDL command is published (`victoriametrics.rs:395-400`). Inventing operation keys here would name atomicity guarantees for statements this driver cannot run at all. |
/// | `data` | `BufferedReadOnly` | **measured, not inferred.** Two facts are proven: rows ARE readable (`query` builds the result from the JSON response, `victoriametrics.rs:318-335`), and row write is a *measured* refusal (`execute`, `victoriametrics.rs:389-393`). `query_stream` awaits the whole HTTP round trip first, materialises every row into a `QueryResult` (`victoriametrics.rs:364`), and only then hands them to `stream_decoded_rows` (`victoriametrics.rs:366-375`) — the events are replayed from memory after the last byte arrived, falsifying the streaming axis. Buffered + read-only is now literally nameable (`DataSupport::BufferedReadOnly`, `packages/driver-api/src/capability_domains.rs:55`), so this cell no longer has to round-trip through `Unknown` to avoid lying. A prior revision called the missing variant a driver-api escalation and cited a mirror enum in `packages/application/src/capability/domain.rs`; that module contains no `DataSupport` mirror at all, and the escalation is resolved. |
/// | `backup` | *(blank)* `Unknown` | nothing in this crate produces or consumes a backup artifact, and no backup or restore command is published (`victoriametrics.rs:395-400`). Nothing was measured, so the cell stays blank rather than claiming a measured refusal. |
///
/// # `precise_cancel` is not this driver's cell to fill
///
/// `LegacyResourceAdapter::new` takes the [`CapabilitySet`] and then *overwrites*
/// `precise_cancel` from `driver.supports_query_execution_cancel()`
/// (`resource_adapter.rs:148-152`): `Supported` if the driver says true, `Unknown`
/// otherwise. `VictoriaMetricsDriver` does not override that method, so it is the
/// trait default `false` (`traits.rs:817`), and the effective value is always
/// `Unknown` whatever is written here.
///
/// The value below is therefore the truthful one — and it is also the value the
/// adapter forces, so the factory and the provider cannot drift. The driver has no
/// per-execution cancellation on the HTTP path: `cancel_query` is an explicit
/// `DriverError::Unsupported` whose own message says the legacy session-wide cancel
/// does nothing (`victoriametrics.rs:405-408`) — exactly the pattern a declaration
/// must not contradict.
///
/// A caller that needs a cancellation gets an explicit
/// `ResourceError::CapabilityNotDeclared` instead of a silent empty success.
pub(crate) fn capabilities() -> CapabilitySet {
    CapabilitySet {
        // A handle is a `reqwest::Client` plus a base URL (`victoriametrics.rs:20`),
        // nothing else; every statement independently builds `GET /api/v1/query?query=…`
        // and reads one body (`victoriametrics.rs:318-335`). No session-establishing
        // call is ever issued and no server-assigned session id is ever stored, so the
        // driver can neither address nor reuse a server-side session across statements.
        // The corpus already rules this exact architecture `Unsupported` for mysql.
        stateful_session: Availability::Unsupported,
        namespace_switch: NamespaceSwitch::Unsupported,
        // Blank — `observe_session` is unconditionally unobservable in the adapter.
        context_observation: ContextObservation::Unsupported,
        // Blank — measured absence: `begin_transaction` is the trait default error.
        transaction_observation: TransactionObservation::Unsupported,
        // Blank — no cursor / prepared statement / transaction handle exists.
        session_scoped_handles: SessionScopedHandleSupport::Unknown,
        // Blank — no baseline replay exists, and the adapter answers `Discard`.
        reset_for_reuse: ResetForReuse::Unsupported,
        // Blank, and overwritten by the adapter regardless — see the note above.
        precise_cancel: PreciseCancelSupport::Unknown,
        // Blank — no snapshot operation or command exists in this crate.
        snapshots: SnapshotSupport::Unsupported,
        transactions: TransactionSupport::default(),
        // Empty on purpose: no `ddl_atomicity` override exists and `execute` refuses
        // every statement, so every operation stays `Unknown`.
        ddl_atomicity: DdlAtomicitySupport::default(),
        // Measured, not inferred: `query_stream` awaits the whole round trip and
        // then replays a materialized `Vec` (victoriametrics.rs:363-376), so
        // `streamingResults` does not hold; `execute` refuses every statement
        // (victoriametrics.rs:389-393), so `rowWrite` does not hold either.
        // Both axes are falsified, which is exactly `BufferedReadOnly`.
        data: DataSupport::BufferedReadOnly,
        // Blank — no artifact code and no backup command exist in this crate.
        backup: BackupSupport::Unknown,
    }
}

/// The evidence behind every cell [`capabilities`] fills, as the snapshot records it.
///
/// This is the machine-readable twin of the table above: the table is what a human
/// reads, this is what `CapabilitySnapshot::evidence_gaps` and any future consumer
/// read. Both are derived from the same code citations, and both must agree — a cell
/// that is filled here without a citation above would be an unfalsifiable claim.
///
/// Every one of the twelve cells is listed, including the eleven that stay blank.
/// A blank cell is not an absence of a finding: most of them are blank because the
/// paths that would fill them are *measured* unreachable, and that measurement is
/// exactly what the evidence table exists to preserve. Entries marked `declined:`
/// record a positive decision not to claim, which is the opposite of a silently
/// empty map — it is also the difference between "we measured this and will not
/// claim it" and "nobody ever looked".
///
/// Line numbers are `src/victoriametrics.rs` unless another file is named.
pub(crate) fn capability_evidence() -> Vec<(&'static str, String)> {
    vec![
        (
            "statefulSession",
            "Unsupported: `connect` opens no physical session. It files a \
             `reqwest::Client` and the base URL under a fresh `vm_<uuid>` key minted \
             at victoriametrics.rs:190 and returns a `ConnectionHandle` carrying \
             only that id (victoriametrics.rs:183-199) — the whole of the driver's \
             state is `clients: RwLock<HashMap<String, (reqwest::Client, String)>>` \
             at victoriametrics.rs:20. Every statement independently builds its own \
             `GET /api/v1/query?query=<sql>` and reads one body \
             (victoriametrics.rs:318-335), so no session-establishing call is ever \
             issued and no server-assigned session id is ever stored; the driver can \
             neither address nor reuse a server-side session across statements. A \
             pooled HTTP client is not a server-side session — the corpus already \
             rules this exact architecture `Unsupported` for mysql \
             (packages/drivers/mysql/src/resource_capabilities.rs:74), so this is a \
             measurement, not a blank, and `Unknown` would discard evidence the \
             repository already holds. Independent of `SessionContinuity`, which the \
             adapter hard-codes to `Unknown` for every legacy driver \
             (resource_adapter.rs:271): a driver may honestly report one and the \
             other differently."
                .to_string(),
        ),
        (
            "namespaceSwitch",
            "declined: NamespaceSwitch::Unsupported — measured absence. There is no \
             attached context to switch. `resolve_database` is a pure function of \
             the explicit argument (victoriametrics.rs:44-51) and its own doc states \
             this driver keeps no mutable session state and that `use_database` is \
             gone (victoriametrics.rs:40-41). The resource itself is the base URL \
             fixed at `connect` (victoriametrics.rs:183-199), whose only mutation is \
             `disconnect`'s `remove` (victoriametrics.rs:201-204). Nothing is \
             switched, so nothing can be switched in place. It is NOT \
             `RequiresReplacement` either: the server exposes exactly one \
             namespace — `get_databases` returns `vec![\"default\"]` \
             (victoriametrics.rs:206-208). Critically it is NOT `PerRequest`, and \
             the reason is not the server's namespace count but where the argument \
             *goes*: in `get_tables` the resolved name is consumed only by \
             `tracing::debug!` (victoriametrics.rs:218) while the request path is \
             the hardcoded constant `/api/v1/label/__name__/values` \
             (victoriametrics.rs:222). The driver therefore does not demonstrably \
             reach another namespace per request — it accepts the argument, logs \
             it, and drops it. `PerRequest` would be a claim this code does not \
             support. The adapter refuses `change_context` outright \
             (resource_adapter.rs:407-414)."
                .to_string(),
        ),
        (
            "contextObservation",
            "declined: ContextObservation::Unsupported — measured absence, not an \
             unmeasured blank. This crate contains no context read-back call: \
             `command_definitions` publishes query, query-stream and the schema \
             catalog commands, none of which reports session context \
             (victoriametrics.rs:395-400), and the adapter answers `observe_session` \
             with `SessionObservation::unobservable()` unconditionally \
             (resource_adapter.rs:397-402). `Partial` would promise a half-answered \
             observation that never arrives."
                .to_string(),
        ),
        (
            "transactionObservation",
            "declined: TransactionObservation::Unsupported — measured absence. \
             `VictoriaMetricsDriver` never overrides `begin_transaction`, so it takes \
             the trait default that errors \"Not supported for this driver type\" \
             (traits.rs:602-609), and `command_definitions` publishes no transaction \
             command to invoke instead (victoriametrics.rs:395-400). The adapter \
             refuses commit and rollback outright for the same reason \
             (resource_adapter.rs:429-449). There is no transaction state to observe."
                .to_string(),
        ),
        (
            "sessionScopedHandles",
            "declined: SessionScopedHandleSupport::Unknown. There is no cursor, \
             prepared statement or transaction handle to mint: the handle *is* the \
             connection, looked up by `pool_id` in a plain map \
             (victoriametrics.rs:30-36), and the whole API surface is one HTTP GET \
             per call with no identifier the server hands back \
             (victoriametrics.rs:53-78). Nothing was measured because nothing was ever \
             opened, so `Unknown` is the honest record — `Unsupported` would assert a \
             probe this crate never ran."
                .to_string(),
        ),
        (
            "resetForReuse",
            "declined: ResetForReuse::Unsupported — there is no verified baseline \
             replay in this crate, and no baseline to replay. The only lifecycle \
             call is `disconnect`, which removes the map entry and nothing else \
             (victoriametrics.rs:201-204); the driver keeps no mutable state that \
             could be dirty and re-verified (victoriametrics.rs:40-41), so there is \
             no teardown-then-reverify sequence to measure. The adapter \
             correspondingly answers `ResetDisposition::Discard` \
             (resource_adapter.rs:499-506). `Verified` would open a reuse feature on \
             the strength of a `remove` call."
                .to_string(),
        ),
        (
            "preciseCancel",
            "Unknown, and this driver does not get to fill it: \
             `LegacyResourceAdapter::new` overwrites `precise_cancel` from \
             `driver.supports_query_execution_cancel()` (resource_adapter.rs:148-152) \
             — `Supported` if the driver says true, `Unknown` otherwise. \
             `VictoriaMetricsDriver` never overrides that method, so it is the trait \
             default `false` (traits.rs:817-819) and the effective value is `Unknown` \
             whatever is written here. The written value is therefore both the \
             truthful one and the one the adapter forces, so the factory view and \
             the provider view cannot drift. The absence of the protocol is \
             independently visible: `cancel_query` is an explicit \
             `DriverError::Unsupported` whose own message says the legacy \
             session-wide cancel does nothing (victoriametrics.rs:407-408), under a doc \
             comment recording that it used to answer `Ok(())` and so reported a \
             cancellation that never happened (victoriametrics.rs:402-404)."
                .to_string(),
        ),
        (
            "snapshots",
            "declined: SnapshotSupport::Unsupported — measured absence. There is no \
             snapshot operation in this crate: `command_definitions` publishes no \
             snapshot command (victoriametrics.rs:395-400), and the endpoint used \
             here is the instant-query one — `/api/v1/query?query=<sql>` \
             (victoriametrics.rs:326-331) — which pins nothing server-side for a \
             snapshot to name."
                .to_string(),
        ),
        (
            "transactions",
            "declined: TransactionSupport::default() — empty `isolation_levels`, \
             `savepoints: Unknown`, `max_open_transactions: None`. There is no \
             transaction to hand an isolation option to: `begin_transaction` is the \
             trait default error (traits.rs:602-609) and no transaction command is \
             published (victoriametrics.rs:395-400). An empty list reads as \"cannot \
             confirm any level\", which is the whole truth here; `Some(0)` would claim \
             transactions are refused *by measurement* rather than by absence of the \
             surface, and a named level would name a guarantee no statement in this \
             crate could exercise."
                .to_string(),
        ),
        (
            "ddlAtomicity",
            "declined: DdlAtomicitySupport::default() — `by_operation` is empty. \
             `VictoriaMetricsDriver` does not override \
             `DatabaseDriver::ddl_atomicity` (traits.rs:156-158, default `Unknown`), \
             and it could not honestly: `execute` is a hard read-only refusal, \
             \"VictoriaMetrics is read-only via the HTTP API\" \
             (victoriametrics.rs:389-393), and no DDL command is published \
             (victoriametrics.rs:395-400). So `capabilities::atomicity_for` fails \
             closed to `Unknown` for every operation (capabilities.rs:240-245), and \
             inventing operation keys here would name atomicity guarantees for \
             statements this driver cannot run at all."
                .to_string(),
        ),
        (
            "data",
            "data: DataSupport::BufferedReadOnly. Two facts are proven from this \
             crate: rows ARE readable (`query` builds the result from the JSON \
             response, victoriametrics.rs:318-335), and row write is a *measured* \
             refusal (`execute`, victoriametrics.rs:389-393). `query_stream` awaits \
             the whole HTTP round trip first, materializes every row into a \
             `QueryResult` (victoriametrics.rs:364), and only then hands them to \
             `stream_decoded_rows` (victoriametrics.rs:366-375) — the events are \
             replayed from memory after the last byte arrived, so `streamingResults` \
             is falsified. `rowWrite` is falsified by the refusal above. Buffered + \
             read-only is now literally nameable \
             (packages/driver-api/src/capability_domains.rs:55, `BufferedReadOnly`), \
             so this cell no longer has to round-trip through `Unknown` to avoid \
             lying. A prior revision of this entry escalated the missing variant as \
             driver-api territory and cited a supposed mirror enum in \
             `packages/application/src/capability/domain.rs`; that module holds no \
             `DataSupport` mirror at all, and the escalation is now resolved."
                .to_string(),
        ),
        (
            "backup",
            "declined: BackupSupport::Unknown. Nothing in this crate produces or \
             consumes a backup artifact and no backup or restore command is published \
             — `command_definitions` is a closed list of query, query_stream and the \
             schema catalog commands (victoriametrics.rs:395-400), and the endpoint \
             surface is a single query/series/label GET \
             (victoriametrics.rs:53-78). Nothing was measured, so the cell stays \
             blank rather than claiming a measured refusal."
                .to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use datazen_driver_api::capabilities::SessionContinuity;
    use datazen_driver_api::namespace::NamespaceTarget;
    use datazen_driver_api::require_resource_provider;
    use datazen_driver_api::resource::{
        DescribeResourceRequest, IdentityScope, ResourceError, ResourceHandle, ResourcePurpose,
    };
    use datazen_driver_api::resource_adapter::ADAPTER_EVIDENCE_REVISION;
    use datazen_driver_api::{ConnectionConfig, DatabaseDriverFactory};

    use crate::{
        ConnectionHandle, DatabaseDriver, DriverError, VictoriaMetricsDriver,
        VictoriaMetricsFactory,
    };

    fn factory() -> VictoriaMetricsFactory {
        VictoriaMetricsFactory
    }

    fn config() -> ConnectionConfig {
        serde_json::from_value(serde_json::json!({
            "id": "cfg-1",
            "name": "victoriametrics",
            "databaseType": "victoriametrics",
            "database": "default",
        }))
        .expect("a minimal config deserializes")
    }

    #[test]
    fn the_factory_reaches_a_real_provider_instead_of_a_missing_one() {
        let provider = require_resource_provider(&factory())
            .expect("VictoriaMetrics is migrated, so the provider must be reachable");
        assert_eq!(provider.provider_id(), "victoriametrics");
        assert_eq!(
            provider.namespace_shape().levels.len(),
            1,
            "VictoriaMetrics addresses one namespace level, the tenant"
        );
    }

    #[test]
    fn factory_capabilities_match_the_provider() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let registry = provider.capabilities();

        assert_eq!(
            factory().resource_capabilities(),
            registry.capabilities,
            "the factory's capability view must not be rosier than the provider's own registry"
        );
        // Polarity flipped in place when the set stopped being blank: VictoriaMetrics
        // now declares the two cells it can prove (see `capabilities()`), so the
        // honest assertion is that the declaration is *not* empty. Same test, same
        // assertion, no test removed and no assertion removed.
        assert!(
            factory().resource_capabilities().declares_anything(),
            "VictoriaMetrics declares only the cells its own code proves; the rest stay blank"
        );
        assert!(
            !factory()
                .resource_capabilities()
                .precise_cancel
                .accepts_precise_cancel(),
            "VictoriaMetrics has no execution-handle cancel protocol"
        );
        assert!(registry.require_precise_cancel().is_err());
        assert_eq!(
            factory().resource_capabilities().stateful_session,
            datazen_driver_api::capabilities::Availability::Unsupported,
            "a VictoriaMetrics handle is an HTTP client plus a base URL, never a \
             server-side session"
        );
    }

    #[test]
    fn the_declaration_is_not_the_blank_default_set() {
        let declared = super::capabilities();

        assert_ne!(
            declared,
            datazen_driver_api::capabilities::CapabilitySet::default(),
            "a driver that measurably refuses row writes and measurably has no \
             session to switch must not answer the blank default set"
        );
        // The claims that are backed by code that runs.
        assert_eq!(
            declared.namespace_switch,
            datazen_driver_api::capabilities::NamespaceSwitch::Unsupported,
            "the tenant is a per-request argument; this driver keeps no session to switch"
        );
        assert_eq!(
            declared.transaction_observation,
            datazen_driver_api::capabilities::TransactionObservation::Unsupported,
            "begin_transaction is the trait default error and no transaction command \
             is published"
        );
        assert_eq!(
            declared.snapshots,
            datazen_driver_api::capabilities::SnapshotSupport::Unsupported,
            "no snapshot operation or command exists in this crate"
        );
        // And the claims that stay blank, so a later edit that fills one of them
        // without evidence has to break this test on purpose rather than by drift.
        assert_eq!(
            declared.data,
            datazen_driver_api::capability_domains::DataSupport::BufferedReadOnly,
            "query_stream materializes every row before emitting (victoriametrics.rs:364-375) \
             and execute refuses every statement (victoriametrics.rs:389-393): buffered, \
             readable, not writable"
        );
        // The two axes `BufferedReadOnly` claims are falsified individually, so the
        // variant cannot quietly become a claim that streaming or writes exist.
        assert!(
            !declared.data.enables_streaming_results(),
            "buffered: the stream replays a materialized Vec, it does not deliver incrementally"
        );
        assert!(
            !declared.data.enables_row_write(),
            "read-only: execute refuses every statement rather than reporting a refusal as an axis"
        );
        assert!(
            declared.data.enables_row_read(),
            "read: query builds a real QueryResult from the JSON response (victoriametrics.rs:318-335)"
        );
        assert_eq!(
            declared.stateful_session,
            datazen_driver_api::capabilities::Availability::Unsupported,
            "the handle holds an HTTP client and a base URL only, so no server-side session \
             can be addressed across statements — the same architecture the corpus rules \
             Unsupported for mysql"
        );
        assert!(
            declared.transactions.isolation_levels.is_empty(),
            "there is no transaction to hand an isolation option to"
        );
        assert!(
            declared.ddl_atomicity.by_operation.is_empty(),
            "execute refuses every statement, so no operation has a proven atomicity"
        );
        assert_eq!(
            declared.precise_cancel,
            datazen_driver_api::capabilities::PreciseCancelSupport::Unknown,
            "the adapter overwrites this cell from supports_query_execution_cancel()"
        );
    }

    #[test]
    fn the_capability_snapshot_reports_identity_without_inventing_confirmations() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let registry = provider.capabilities();

        assert_eq!(registry.snapshot.driver_id, "victoriametrics");
        assert_eq!(
            registry.snapshot.driver_version,
            env!("CARGO_PKG_VERSION"),
            "the driver version must be the real crate version, not an invented string"
        );
        assert_eq!(
            registry.snapshot.protocol_version,
            datazen_driver_api::PROTOCOL_VERSION
        );
        assert_eq!(
            registry.snapshot.capability_revision, ADAPTER_EVIDENCE_REVISION,
            "an adapter that carries an evidence table must stamp ADAPTER_EVIDENCE_REVISION; \
             leaving the revision at 0 would claim the snapshot predates any capability module"
        );
        assert_ne!(
            registry.snapshot.capability_revision, 0,
            "revision 0 means 'no capability module existed yet' and must not carry evidence"
        );
        assert_eq!(
            registry.evidence_gaps(),
            Vec::<&'static str>::new(),
            "all twelve capability cells must carry a record — either a real citation, or an \
             explicit 'declined:' record of what was measured and why nothing may be claimed"
        );
    }

    /// The anti-fabrication guard.
    ///
    /// The point of recording evidence is that a cell is no longer a bare assertion in a
    /// struct literal: someone must be able to walk from the claim to the line of code
    /// that justifies it. A record that cites nothing is indistinguishable from a
    /// record somebody made up, so this test requires the citation shape rather than
    /// trusting the surrounding prose.
    #[test]
    fn every_capability_record_cites_the_source_line_it_claims_to_describe() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let registry = provider.capabilities();
        let confirmed = &registry.snapshot.confirmed;

        // Restated here rather than imported, so driver-api changing its own cell list
        // cannot quietly make this test vacuous.
        let cells = [
            "statefulSession",
            "namespaceSwitch",
            "contextObservation",
            "transactionObservation",
            "sessionScopedHandles",
            "resetForReuse",
            "preciseCancel",
            "snapshots",
            "transactions",
            "ddlAtomicity",
            "data",
            "backup",
        ];
        assert_eq!(
            confirmed.len(),
            cells.len(),
            "the evidence table must hold exactly one record per capability cell, no more \
             and no fewer — extra keys would let a cell go unexamined"
        );

        for cell in cells {
            let record = confirmed
                .get(cell)
                .unwrap_or_else(|| panic!("capability cell `{cell}` has no evidence record"));
            assert!(
                cites_a_source_line(record),
                "evidence for `{cell}` cites no `file.rs:NNN`, so it cannot be checked \
                 against the code: {record}"
            );
        }
    }

    /// True when the text points at a concrete source line, i.e. it contains
    /// `something.rs:` immediately followed by a digit.
    fn cites_a_source_line(text: &str) -> bool {
        text.match_indices(".rs:").any(|(at, _)| {
            text[at + 4..]
                .chars()
                .next()
                .is_some_and(|c: char| c.is_ascii_digit())
        })
    }

    #[tokio::test]
    async fn a_described_resource_is_never_mistaken_for_a_fixed_reusable_session() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let descriptor = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty().with_database("default"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect("describe_resource must succeed without touching the network");

        assert!(!descriptor.is_fixed_session());
        assert_eq!(
            descriptor.session_continuity,
            SessionContinuity::Unknown,
            "VictoriaMetrics cannot observe a session, so continuity stays unknown"
        );
        assert!(
            matches!(
                descriptor.connection_cost_policy,
                datazen_driver_api::resource::ConnectionCostPolicy::DeclaredConservative { .. }
            ),
            "a driver that never measured its cost must declare a conservative one, not a free one"
        );
        assert_eq!(
            descriptor.reuse_policy,
            datazen_driver_api::resource::ReusePolicy::Unknown,
            "VictoriaMetrics cannot prove a resource is back at its baseline after use"
        );
    }

    #[tokio::test]
    async fn a_namespace_level_victoriametrics_does_not_have_is_refused_by_name() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let err = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty()
                    .with_database("default")
                    .with_schema("public"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect_err("the namespace shape must reject a level victoriametrics cannot address");

        assert!(
            matches!(
                err,
                datazen_driver_api::resource::ResourceError::NonexistentNamespaceLevel { .. }
            ),
            "a caller must be told the level is not part of the shape, not have it dropped: {err}"
        );
    }

    #[tokio::test]
    async fn the_legacy_cancel_refuses_instead_of_reporting_a_cancellation_that_never_happened() {
        let driver = VictoriaMetricsDriver::new();
        let err = driver
            .cancel_query(&ConnectionHandle {
                id: "connection-1".into(),
                pool_id: "victoriametrics-pool".into(),
            })
            .await
            .expect_err("VictoriaMetrics cannot cancel; Ok(()) was the defect Track O removes");

        assert!(matches!(err, DriverError::Unsupported(_)), "got {err:?}");
    }

    #[test]
    fn no_cancel_predicate_is_claimed_on_either_dialect_of_the_contract() {
        let factory = factory();
        assert!(
            !factory.supports_cancel_query(),
            "the legacy session-wide cancel is refused, so nothing may advertise it"
        );
        assert!(
            !factory.supports_query_execution_cancel(),
            "VictoriaMetrics has no execution-handle protocol to advertise"
        );
        assert!(!VictoriaMetricsDriver::new().supports_query_execution_cancel());
    }

    /// The host must be able to find the same provider twice.
    ///
    /// `require_resource_provider` calls `DatabaseDriverFactory::resource_provider`
    /// afresh on every lookup, and `LegacyResourceAdapter` refuses a handle whose
    /// epoch is not the one it was built with. A provider rebuilt per call therefore
    /// invalidated every handle the instant the host looked it up again: the resource
    /// layer existed, and nothing could use it.
    #[tokio::test]
    async fn the_provider_is_memoized_so_its_own_handles_survive_the_next_lookup() {
        let first = factory()
            .resource_provider()
            .expect("victoriametrics declares a real provider");
        let second = factory()
            .resource_provider()
            .expect("victoriametrics declares a real provider");
        assert!(
            Arc::ptr_eq(&first, &second),
            "two lookups handed back different provider instances, so a handle minted \
             by the first is rejected by the second"
        );

        let handle =
            ResourceHandle::issue(first.provider_id(), "connection-1", super::runtime_epoch());
        // The epoch being checked is the one *this* provider was built with, not one
        // the test hands over: `close_resource` is the cheapest provider call that
        // runs the ownership + epoch check, and it answers `Ok` for a resource it
        // simply is not holding, so a pass is attributable to the check alone.
        second.close_resource(&handle).await.expect(
            "a handle the provider minted must still validate against the provider \
                 the host looks up next time",
        );

        // And the check is not vacuous: a provider built outside the memo took
        // another epoch and still refuses the handle. That is precisely the failure
        // per-call construction produced.
        let stray = super::stray_provider(factory().create());
        assert!(
            !Arc::ptr_eq(&first, &stray),
            "the stray provider must not be the memoized one"
        );
        let rejection = stray
            .close_resource(&handle)
            .await
            .expect_err("a provider built outside the memo must not accept the handle");
        assert!(
            matches!(rejection, ResourceError::StaleRuntimeEpoch { .. }),
            "got {rejection:?}"
        );
    }
}
