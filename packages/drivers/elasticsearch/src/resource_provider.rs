//! Track O: Elasticsearch's real [`ResourceProvider`].
//!
//! Elasticsearch is reached through the contract's own migration bridge,
//! [`LegacyResourceAdapter`], rather than through a bespoke provider impl. The adapter
//! is what makes the handles real: it opens the connection itself, mints an opaque
//! [`ResourceHandle`] bound to that connection's own id, charges the caller's
//! [`BudgetPort`] *before* any wire work, and releases the charge exactly once on
//! close.
//!
//! Everything Elasticsearch cannot honestly perform is refused by name instead of
//! answered with an empty success:
//!
//! * `request_cancel` — the `_tasks` interrupt path is not wired into this driver, so
//! the adapter answers `CancelDisposition::Unsupported` and never calls the legacy
//! `cancel_query`. That method used to answer `Ok(())`, i.e. a reported cancellation
//! that never happened; Track O turns it into an explicit `DriverError::Unsupported`.
//! * `observe_session` — every field unknown, never back-filled with the acquisition
//! target.
//! * `change_context` — refused, because changing the authenticated principal is a
//! reconnect.
//! * `commit_transaction` / `rollback_transaction` — Elasticsearch has no cross-request
//! transactions, so the port refuses rather than inventing an outcome.
//!
//! # Namespace
//!
//! `ElasticsearchDriver` documents this directly at `elasticsearch.rs:13` — there is no
//! database/schema hierarchy, indices live in one cluster — and `has_schema_level` is
//! the default `false`. The shape therefore declares **no levels at all**, which is the
//! fail-closed reading: an index name travels in the resource `path`, not in the
//! namespace. A caller that supplies `Database`, `Schema` or `Catalog` is refused with
//! `ResourceError::NonexistentNamespaceLevel` rather than having the value silently
//! dropped.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::{
    Availability, CapabilitySet, ContextObservation, DdlAtomicitySupport, NamespaceSwitch,
    PreciseCancelSupport, ResetForReuse, SessionScopedHandleSupport, SnapshotSupport,
    TransactionObservation, TransactionSupport,
};
use datazen_driver_api::namespace::{NamespaceLevel, NamespaceLevelKind, NamespaceShape};
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{BackupSupport, DataSupport, DatabaseDriver};

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

/// The namespace Elasticsearch actually addresses.
///
/// Elasticsearch declares no namespace levels at all.
pub(crate) fn namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![],
        ..NamespaceShape::default()
    }
}

/// The real provider for `elasticsearch`, built once and shared.
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
        "elasticsearch",
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
/// Every row below is a claim about code that already exists in this crate, with the
/// `file:line` that backs it. The blank-looking cells are the interesting ones, so
/// each one says *why it cannot be filled* rather than leaving it to inference. A
/// caller that needs one of them gets `ResourceError::CapabilityNotDeclared` from the
/// registry, never a silent empty success.
///
/// | capability | declared | evidence |
/// | --- | --- | --- |
/// | `stateful_session` | `Unsupported` | `connect` stores `(reqwest::Client, String)` and nothing else (`elasticsearch.rs:19`, inserted at `:174`); every read/write is a fresh `client.get/post(..).send()` on that tuple (`:52-89`, `:464-469`) and `disconnect` only removes the map entry (`:192`). There is no server-side object to bind to. Migration doc §2.2 counts elasticsearch among the drivers that store a `(client, base)` tuple and have 「完全无会话」. |
/// | `namespace_switch` | `Unsupported` | Nothing to switch *to*: `get_databases` answers `vec!["default"]` (`elasticsearch.rs:197-199`) because a cluster has one namespace, and `namespace_shape()` declares zero levels. `change_context` is refused by the adapter. Migration doc §2.1 records `unsupported`. |
/// | `context_observation` | `Unsupported` | There is no read-back path to observe. `resolve_database` is a pure function over its argument (`elasticsearch.rs:43-50`) — the module comments at `:40` record that `use_database` is gone, so no mutable current-context field exists — and `observe_session` returns `SessionObservation::unobservable()` for every field. |
/// | `transaction_observation` | `Unsupported` | The driver overrides none of `begin_transaction`/`commit_transaction`/`rollback_transaction`, so the trait defaults answer the refusal, and the adapter's commit/rollback refuse with `ResourceError::OperationNotSupported`. No transaction state is ever observable. |
/// | `session_scoped_handles` | `Unsupported` | Elasticsearch *does* hand out a server-side object — the `/_sql` cursor, closed by `close_cursor` (`elasticsearch.rs:83`) — but `query_stream` closes it before it returns (`elasticsearch.rs:364-453`, the cursor is closed on stop, on error and at end of stream). It is an implementation detail scoped to one call, never a handle handed to the caller, which is exactly §6.5's 「不表示句柄失效」 case. |
/// | `reset_for_reuse` | `Unsupported` | Only two variants exist and no baseline-restore exists to name. The adapter answers `ResetDisposition::Discard` for `reset_resource`, and the driver has no `discard_connection` override. |
/// | `precise_cancel` | `Unknown` | **Not mine to declare.** `LegacyResourceAdapter` overwrites this field from `supports_query_execution_cancel()` (`resource_adapter.rs:148-152`), which is `false` here, so the provider's registry holds `Unknown` no matter what this function says. Declaring `Unsupported` would make the factory disagree with the provider that owns the registry — the exact drift `factory_capabilities_match_the_provider` exists to catch. The underlying fact is a measured refusal (`cancel_query` returns `DriverError::Unsupported`, `elasticsearch.rs:514`) and is asserted by `the_legacy_cancel_refuses_instead_of_reporting_a_cancellation_that_never_happened`. |
/// | `snapshots` | `Unsupported` | No snapshot endpoint exists anywhere in the crate and `begin_read_snapshot` is not overridden, so the trait default refusal is the driver's own answer rather than a guess. |
/// | `transactions.isolation_levels` | empty | No transaction can be opened at all, so there is no level to honour. Empty is read as "cannot confirm any", which is stronger than listing one. |
/// | `transactions.savepoints` | `Unsupported` | Savepoints require an open transaction; the driver cannot open one (row above). |
/// | `transactions.max_open_transactions` | `None` | Consequence of the same fact: no transaction is openable, so there is no limit to state. |
/// | `ddl_atomicity.by_operation` | empty | `DatabaseDriver::ddl_atomicity` is not overridden, so the driver answers `DdlAtomicity::Unknown` for *any* operation. `atomicity_for()` returns `Unknown` for an unlisted operation, so an empty map reproduces the driver's own answer exactly. It is a measured "I do not know", not a hole. |
/// | `data` | `StreamingReadWrite` | The only affirmative claim, and it is load-bearing. Reads stream incrementally through the real `/_sql` cursor: `query_stream` posts `{"cursor": c}`, pushes each decoded batch through `QueryRowBatcher::new` (`elasticsearch.rs:412`) *before* the next request, and only then asks for more (`elasticsearch.rs:364-453`). Writes are real: `execute` posts the statement and returns the server's own row count (`elasticsearch.rs:464-469`, `:468`). |
/// | `backup` | `Unknown` | Deliberately left unmeasured. Nothing in this crate produces or consumes a backup artifact, but "this driver has no backup code" is not the same as having measured the backup workflow, so this stays `Unknown` rather than claiming a refusal nobody checked. |
///
/// Keep this byte-identical to what `provider()` reports: a factory that looks rosier
/// than its own provider is exactly the drift the contract exists to prevent.
///
/// The per-cell evidence lives here rather than in `CapabilitySnapshot::confirmed`,
/// because the adapter builds that snapshot and has no way to receive a confirmation
/// map — a driver's confirmation claim would have to be added to the adapter first.
pub(crate) fn capabilities() -> CapabilitySet {
    CapabilitySet {
        stateful_session: Availability::Unsupported,
        namespace_switch: NamespaceSwitch::Unsupported,
        context_observation: ContextObservation::Unsupported,
        transaction_observation: TransactionObservation::Unsupported,
        session_scoped_handles: SessionScopedHandleSupport::Unsupported,
        reset_for_reuse: ResetForReuse::Unsupported,
        // See the table: the adapter owns this cell and pins it to `Unknown`.
        precise_cancel: PreciseCancelSupport::Unknown,
        snapshots: SnapshotSupport::Unsupported,
        transactions: TransactionSupport {
            // Empty on purpose, for the same reason postgres leaves it empty: a
            // level listed here would make `begin_transaction` accept an option
            // the driver cannot honour.
            isolation_levels: Vec::new(),
            savepoints: Availability::Unsupported,
            max_open_transactions: None,
        },
        // Empty on purpose: see the `ddl_atomicity` row.
        ddl_atomicity: DdlAtomicitySupport::default(),
        // Measured affirmative, backed by the cursor loop and the row count.
        data: DataSupport::StreamingReadWrite,
        // Never measured against the backup workflow. See the `backup` row.
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
/// Every one of the twelve cells is listed, including the two that stay blank.
/// A blank cell is not an absence of a finding: `preciseCancel`, `transactions` and
/// `ddlAtomicity` are blank because the paths that would fill them are *measured*
/// unreachable or unbacked, and that measurement is exactly what the evidence table
/// exists to preserve. Entries marked `declined:` record a positive decision not to
/// claim, which is the opposite of a silently empty map.
pub(crate) fn capability_evidence() -> Vec<(&'static str, String)> {
    vec![
        (
            "statefulSession",
            "Unsupported: `connect` really does insert a live `reqwest::Client` plus its base \
             URL into the pool under the handle's own pool id \
             (elasticsearch.rs:174-190, the insert at elasticsearch.rs:185), so a session \
             object exists — but it is a stateless HTTP client, and `disconnect` only drops \
             the map entry (elasticsearch.rs:192-195). Nothing lets the host pin engine or \
             index state onto that connection and resume it: `resolve_database` reads the \
             argument on every call and never from session state \
             (elasticsearch.rs:43-50), the adapter refuses `change_context` outright \
             (resource_adapter.rs:407-414), and no session handle is ever minted \
             (resource_adapter.rs:387). So the affirmative half of the claim is absent, \
             which is what `Unsupported` is for. ClickHouse, where the same map shape holds \
             a real HTTP session, gets the same answer for a different reason."
                .to_string(),
        ),
        (
            "namespaceSwitch",
            "Unsupported: Elasticsearch has no database/schema hierarchy — indices live in one \
             cluster-wide namespace (elasticsearch.rs:13) — and `get_databases` is a hardcoded \
             `[\"default\"]` (elasticsearch.rs:197-199), so `has_multi_database` stays at its \
             trait default `false` (traits.rs:133-135). A different index is a different \
             request body, not a context switch, and the adapter refuses `change_context` \
             (resource_adapter.rs:407-414), so an acquired resource provably cannot move."
                .to_string(),
        ),
        (
            "contextObservation",
            "Unsupported: `observe_session` answers `SessionObservation::unobservable()` on \
             every call (resource_adapter.rs:397-402) and `execute_on_resource` pins \
             `context_before`/`context_after` to `unobserved()` (resource_adapter.rs:377-378). \
             No code path in this crate reads Elasticsearch engine state back — the pool entry \
             is only `(client, base)` (elasticsearch.rs:19) — so there is nothing that could \
             replace either default."
                .to_string(),
        ),
        (
            "transactionObservation",
            "Unsupported: `commit_transaction` (resource_adapter.rs:429-438) and \
             `rollback_transaction` (resource_adapter.rs:440-449) both refuse by name because \
             the outcome cannot be read back, and `execute_on_resource` hardcodes \
             `TransactionState::Unknown` with `transaction_id: None` and `effect: None` \
             (resource_adapter.rs:380-382). Elasticsearch has no cross-request transaction \
             for this driver to end, so there is no outcome left to observe."
                .to_string(),
        ),
        (
            "sessionScopedHandles",
            "Unsupported: `execute_on_resource` returns an empty `session_handles` because this \
             path registers none (resource_adapter.rs:387), and the only server-side object \
             that outlives a request — the SQL cursor — is owned by `query_stream` itself, \
             which closes it on the stop path (elasticsearch.rs:431-436) and never hands it to \
             the caller."
                .to_string(),
        ),
        (
            "resetForReuse",
            "Unsupported: `reset_resource` always answers `ResetDisposition::Discard` \
             (resource_adapter.rs:499-506). There is no verified baseline replay, so `Verified` \
             would hand back a resource whose state nobody checked."
                .to_string(),
        ),
        (
            "preciseCancel",
            "Unknown, and the adapter owns this cell. `LegacyResourceAdapter::new` overwrites \
             it from `supports_query_execution_cancel()` (resource_adapter.rs:148-152), which \
             this crate does not override, so the trait default `false` applies \
             (traits.rs:817-819); `cancel_query` agrees by refusing (elasticsearch.rs:514-519) \
             instead of reporting a cancellation that never happened — the `_tasks` interrupt \
             path is not wired in. The value written here is the value the adapter installs, \
             so the two views agree rather than merely not contradicting."
                .to_string(),
        ),
        (
            "snapshots",
            "Unsupported: neither this crate nor the adapter implements `begin_read_snapshot`, \
             so the trait default answers `DriverError::Unsupported` (traits.rs:734-741). An \
             Elasticsearch index snapshot is a repository operation this driver never issues, \
             so there is no point-in-time read to declare."
                .to_string(),
        ),
        (
            "transactions",
            "declined: the declaration is deliberately blank and each blank is a measurement, not \
             an omission. `DatabaseDriver::begin_transaction` is the trait default and errors \
             (traits.rs:602-609), so the adapter's `begin_transaction` \
             (resource_adapter.rs:416-425) can never open one. `isolation_levels: []` therefore \
             reads as 'no level can be honoured' — naming a level would advertise an option the \
             driver silently ignores, since no `SET TRANSACTION` is ever issued. \
             `savepoints: Unsupported` follows from the same unreachable path. \
             `max_open_transactions: None` is 'unmeasured', not 'unbounded' — there is no \
             transaction registry in this crate to count."
                .to_string(),
        ),
        (
            "ddlAtomicity",
            "declined: the map is empty on purpose. `DatabaseDriver::ddl_atomicity` is not \
             overridden in this crate, so it answers `Unknown` (traits.rs:156-158), and \
             `DdlAtomicitySupport::atomicity_for` fails closed to `Unknown` for any absent key \
             (capabilities.rs:240-245). Filing a value per operation would assert a \
             multi-statement DDL atomicity nobody has measured."
                .to_string(),
        ),
        (
            "data",
            "StreamingReadWrite, the one affirmative cell: rows are read (`query`, \
             elasticsearch.rs:330-362) and written (`execute`, elasticsearch.rs:464-469, whose \
             affected-row figure is read out of the same `_sql` response at \
             elasticsearch.rs:468), and `query_stream` walks the response array row by row, \
             decoding each row and pushing it through the batcher (`batcher.push(decoded)`, \
             elasticsearch.rs:424) before following the cursor (elasticsearch.rs:437-442). \
             Rows leave as they arrive rather than after the whole result is buffered, which is \
             what separates this from a driver that only materializes a full result set."
                .to_string(),
        ),
        (
            "backup",
            "declined: `Unknown`, and the reason is narrower than it looks. What this crate \
             registers is a closed list of commands — statement/query/query_stream plus the \
             schema-catalog set (elasticsearch.rs:471-480) — with no artifact producer or \
             consumer in it, and its own UI metadata declares `supportsBackup: false` \
             (ui/meta.ts:15). Both facts are statements about this driver's surface, not \
             measurements of the backup workflow: nothing here ever issues Elasticsearch's \
             `/_snapshot` repository API or a restore, so `BackupSupport::Unsupported` — which \
             is defined as *measured* and genuinely absent — would be the wrong value in the \
             other direction too. `Unknown` claims nothing, which is the only claim the \
             evidence here supports."
                .to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use datazen_driver_api::capabilities::SessionContinuity;
    use datazen_driver_api::capabilities::{
        Availability, CapabilitySet, ContextObservation, NamespaceSwitch, PreciseCancelSupport,
        ResetForReuse, SessionScopedHandleSupport, SnapshotSupport, TransactionObservation,
    };
    use datazen_driver_api::namespace::NamespaceTarget;
    use datazen_driver_api::require_resource_provider;
    use datazen_driver_api::resource::{
        DescribeResourceRequest, IdentityScope, ResourceError, ResourceHandle, ResourcePurpose,
    };
    use datazen_driver_api::resource_adapter::ADAPTER_EVIDENCE_REVISION;
    use datazen_driver_api::{BackupSupport, ConnectionConfig, DataSupport, DatabaseDriverFactory};

    use super::capabilities;
    use crate::{
        ConnectionHandle, DatabaseDriver, DriverError, ElasticsearchDriver, ElasticsearchFactory,
    };

    fn factory() -> ElasticsearchFactory {
        ElasticsearchFactory
    }

    fn config() -> ConnectionConfig {
        serde_json::from_value(serde_json::json!({
            "id": "cfg-1",
            "name": "elasticsearch",
            "databaseType": "elasticsearch",
            "database": "",
        }))
        .expect("a minimal config deserializes")
    }

    #[test]
    fn the_factory_reaches_a_real_provider_instead_of_a_missing_one() {
        let provider = require_resource_provider(&factory())
            .expect("Elasticsearch is migrated, so the provider must be reachable");
        assert_eq!(provider.provider_id(), "elasticsearch");
        assert_eq!(
            provider.namespace_shape().levels.len(),
            0,
            "Elasticsearch declares no namespace levels at all"
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
        assert_ne!(
            factory().resource_capabilities(),
            CapabilitySet::default(),
            "the inert default claims nothing; `capabilities()` documents a measured refusal \
             for every cell except `data`, so it must differ from the default"
        );
        assert!(
            !factory()
                .resource_capabilities()
                .precise_cancel
                .accepts_precise_cancel(),
            "Elasticsearch has no execution-handle cancel protocol"
        );
        assert!(registry.require_precise_cancel().is_err());
        assert_eq!(
            factory().resource_capabilities().stateful_session,
            Availability::Unsupported,
            "an Elasticsearch cluster exposes no fixed session, and `connect` stores nothing \
             but a `(client, base)` tuple to prove it"
        );
    }

    /// Every cell of the declaration is pinned here, so a later edit cannot silently
    /// drift away from the evidence table above it.
    ///
    /// The empty containers are asserted too, not skipped: an empty
    /// `isolation_levels` and an empty `ddl_atomicity` map are deliberate
    /// "cannot confirm" answers, and a future edit that fills one of them should have
    /// to come back here.
    #[test]
    fn every_declared_cell_is_the_value_the_evidence_table_justifies() {
        let caps = capabilities();

        assert_ne!(
            caps,
            CapabilitySet::default(),
            "the declaration must differ from the inert default"
        );
        assert_eq!(caps.stateful_session, Availability::Unsupported);
        assert_eq!(caps.namespace_switch, NamespaceSwitch::Unsupported);
        assert_eq!(caps.context_observation, ContextObservation::Unsupported);
        assert_eq!(
            caps.transaction_observation,
            TransactionObservation::Unsupported
        );
        assert_eq!(
            caps.session_scoped_handles,
            SessionScopedHandleSupport::Unsupported
        );
        assert_eq!(caps.reset_for_reuse, ResetForReuse::Unsupported);
        assert_eq!(caps.precise_cancel, PreciseCancelSupport::Unknown);
        assert_eq!(caps.snapshots, SnapshotSupport::Unsupported);
        assert!(
            caps.transactions.isolation_levels.is_empty(),
            "no level is honoured: the driver cannot open a transaction at all"
        );
        assert_eq!(caps.transactions.savepoints, Availability::Unsupported);
        assert_eq!(caps.transactions.max_open_transactions, None);
        assert!(
            caps.ddl_atomicity.by_operation.is_empty(),
            "`DatabaseDriver::ddl_atomicity` is not overridden, so every operation is \
             `Unknown`; the map must stay empty to reproduce that"
        );
        assert_eq!(caps.data, DataSupport::StreamingReadWrite);
        assert_eq!(caps.backup, BackupSupport::Unknown);
    }

    /// The one affirmative cell has to open the three features it claims, and the
    /// negatives have to stay shut.
    ///
    /// `StreamingReadWrite` is the only variant that satisfies `enables_feature`, so
    /// this is where a wrong answer to the `data` row would show up as a product
    /// behaviour change rather than as a quiet mismatch.
    #[test]
    fn the_data_domain_opens_exactly_what_the_cursor_loop_can_deliver() {
        let caps = capabilities();

        assert!(caps.data.enables_row_read());
        assert!(
            caps.data.enables_row_write(),
            "`execute` posts the statement and returns the server's row count"
        );
        assert!(
            caps.data.enables_streaming_results(),
            "`query_stream` pushes each batch before requesting the next cursor page"
        );
        assert!(caps.data.enables_feature());
    }

    #[test]
    fn the_capability_snapshot_reports_identity_without_inventing_confirmations() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let registry = provider.capabilities();

        assert_eq!(registry.snapshot.driver_id, "elasticsearch");
        assert_eq!(
            registry.snapshot.driver_version,
            env!("CARGO_PKG_VERSION"),
            "the driver version must be the real crate version, not an invented string"
        );
        assert_eq!(
            registry.snapshot.protocol_version,
            datazen_driver_api::PROTOCOL_VERSION
        );
        // Not an empty confirmation set: every one of the twelve cells carries an
        // evidence record, so the snapshot reports a non-zero capability revision
        // and no gaps. This is a stronger claim than "nothing was invented".
        assert!(
            registry.snapshot.capability_revision == ADAPTER_EVIDENCE_REVISION,
            "a populated evidence table must be stamped with the adapter evidence revision; \
             got {}",
            registry.snapshot.capability_revision
        );
        assert!(
            registry.snapshot.capability_revision != 0,
            "the revision must not be left at its zero default"
        );
        assert_eq!(
            registry.evidence_gaps(),
            Vec::<&'static str>::new(),
            "every confirmed cell must carry an evidence record; a gap means a value was \
             claimed without a reason"
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
                target: NamespaceTarget::empty(),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect("describe_resource must succeed without touching the network");

        assert!(!descriptor.is_fixed_session());
        assert_eq!(
            descriptor.session_continuity,
            SessionContinuity::Unknown,
            "Elasticsearch cannot observe a session, so continuity stays unknown"
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
            "Elasticsearch cannot prove a resource is back at its baseline after use"
        );
    }

    #[tokio::test]
    async fn a_namespace_level_elasticsearch_does_not_have_is_refused_by_name() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let err = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty().with_database("logs-2024"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect_err("the namespace shape must reject a level elasticsearch cannot address");

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
        let driver = ElasticsearchDriver::new();
        let err = driver
            .cancel_query(&ConnectionHandle {
                id: "connection-1".into(),
                pool_id: "elasticsearch-pool".into(),
            })
            .await
            .expect_err("Elasticsearch cannot cancel; Ok(()) was the defect Track O removes");

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
            "Elasticsearch has no execution-handle protocol to advertise"
        );
        assert!(!ElasticsearchDriver::new().supports_query_execution_cancel());
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
            .expect("elasticsearch declares a real provider");
        let second = factory()
            .resource_provider()
            .expect("elasticsearch declares a real provider");
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
