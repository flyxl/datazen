//! Track O: SQL Server's real [`ResourceProvider`].
//!
//! SQL Server is reached through the contract's own migration bridge,
//! [`LegacyResourceAdapter`], rather than through a bespoke provider impl. The adapter
//! is what makes the handles real: it opens the connection itself, mints an opaque
//! [`ResourceHandle`] bound to that connection's own id, charges the caller's
//! [`BudgetPort`] *before* any wire work, and releases the charge exactly once on
//! close.
//!
//! Everything SQL Server cannot honestly perform is refused by name instead of answered
//! with an empty success:
//!
//! * `request_cancel` — `ATTENTION` is per-session and asynchronous, and this driver
//! never opens the second session a cancel would need, so the adapter answers
//! `CancelDisposition::Unsupported` and never calls the legacy `cancel_query`. That
//! method used to answer `Ok(())`, i.e. a reported cancellation that never happened;
//! Track O turns it into an explicit `DriverError::Unsupported`.
//! * `observe_session` — every field unknown, never back-filled with the acquisition
//! target.
//! * `change_context` — refused, because changing the SQL Server login context is a
//! reconnect.
//! * `commit_transaction` / `rollback_transaction` — transaction outcomes are resolved
//! inside the driver's own commands.
//!
//! # Namespace
//!
//! `DatabaseDriver::has_schema_level` is explicitly `true` here, and
//! `SqlServerDriver::qualify_sql_target` routes through `sql_target::qualify_sql(sql,
//! database, schema)`, so both names really take part in addressing. The shape
//! therefore declares an **optional** `Database` level and an **optional** `Schema`
//! level, in that order. `required: false` for both is the fail-closed reading of the
//! evidence: the server resolves an unset database or schema on its own, and a shape
//! that demanded one would refuse connections that work today. `NamespaceTarget` has no
//! catalog slot at all, so a server/catalog cannot even be expressed as a target — the
//! connection itself is the server, and there is nothing for a caller to pass that
//! would be silently dropped.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use crate::capability_evidence::capability_evidence;
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

/// The namespace SQL Server actually addresses.
///
/// SQL Server addresses a database and a schema.
pub(crate) fn namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![
            NamespaceLevel {
                kind: NamespaceLevelKind::Database,
                exists: true,
                required: false,
            },
            NamespaceLevel {
                kind: NamespaceLevelKind::Schema,
                exists: true,
                required: false,
            },
        ],
        ..NamespaceShape::default()
    }
}

/// The real provider for `sqlserver`, built once and shared.
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

/// The one construction of [`LegacyResourceAdapter`] this crate performs.
///
/// Both [`provider`] and the `#[cfg(test)]` stray go through here, and the
/// builder ends in [`with_evidence`] with [`capability_evidence`] — so the live
/// provider and a provider rebuilt outside the memo declare *the same* twelve
/// cells with *the same* provenance. Without this, a test comparing the two
/// would be comparing two different claims instead of two construction sites.
fn adapter(driver: Arc<dyn DatabaseDriver>, runtime_epoch: u64) -> LegacyResourceAdapter {
    LegacyResourceAdapter::new(
        driver,
        "sqlserver",
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
/// | `stateful_session` | `Supported` | this driver reads **session-scoped state back off the server**, which is what the cell asks about. `current_isolation_level` issues `DBCC USEROPTIONS WITH NO_INFOMSGS` and parses the *session's* current isolation level out of the reply (`sqlserver.rs:461-488`) — a per-session diagnostic that cannot be read without a session; and `ensure_no_open_transaction` issues `SELECT @@TRANCOUNT AS [transaction_count]` and treats a nonzero count as an error (`sqlserver.rs:490-509`, called at `:1625`). The transaction handle is registered against that same `SqlClient` and dropped only when the transaction ends (`sqlserver.rs:1631-1637`, `:1717`, `:1752`). Both observations survive between statements on one connection, so the session is measured, not assumed. Independent of `resource_adapter.rs:271`, which reports `SessionContinuity::Unknown` for the *descriptor*: continuity asks whether a reconnection is survivable, which is a different question from whether the session's own state can be read.
///
/// Note that this is *not* in tension with `SessionContinuity::Unknown` in this provider's own descriptor (`resource_adapter.rs:271`, `:215`). The two are independent fields: `SessionContinuity` is what the adapter can prove about a resource without a richer `DatabaseDriver` method, while `stateful_session` is the driver's own claim about its connection. `mysql`, `sqlite` and `mongodb` all report `SessionContinuity::Leased` together with `stateful_session: Unsupported` (`sqlite/src/resource.rs:14`, `mongodb/src/resource.rs:15`), so no adapter descriptor has ever implied a particular `stateful_session`. What a legacy driver may *not* claim is `SessionContinuity::Fixed` — that is what `a_described_resource_is_never_mistaken_for_a_fixed_reusable_session` guards, and this provider never asks for it. |
/// | `namespace_switch` | `Unsupported` | nothing in this crate switches the attached database, schema or server. `sql_target::qualify_sql` rewrites the *SQL text* instead (`sql_target.rs:24-40`), the module states that no session `USE` is ever issued (`sql_target.rs:12`), and a test asserts the generated read SQL contains no `USE [` (`sqlserver.rs:2095`). The database dimension is served by the host session pin (`sql_target.rs:158-165`), which is host session management, not a capability this provider exposes; the adapter refuses `change_context` outright. Measured absent, not merely undeclared. |
/// | `context_observation` | *(blank)* `Unsupported` | `observe_session` returns `SessionObservation::unobservable()` unconditionally in the adapter, so this provider has no code path that can report a session context. `Partial` would promise a half-answered observation that never arrives. |
/// | `transaction_observation` | `Partial` | beginnings are real and server-confirmed: `begin_transaction` refuses a second one (`sqlserver.rs:1616-1620`), checks the server with `SELECT @@TRANCOUNT` (`sqlserver.rs:491`), issues `BEGIN TRANSACTION` (`sqlserver.rs:1626`) and returns a real `sqlserver_tx_<uuid>` handle (`sqlserver.rs:1630`), which the adapter reports as `TransactionObservation::begun(id, 0)`. Outcomes are not observable: the adapter refuses `commit_transaction` / `rollback_transaction` because the outcome cannot be read back (`resource_adapter.rs:429-449`). Beginnings yes, endings no, therefore `Partial` and never `Full`. |
/// | `session_scoped_handles` | `Supported` | the transaction handle is a real session-scoped handle: it is registered in `transactions` under the connection id (`sqlserver.rs:1631-1637`), re-validated by id on commit and rollback (`sqlserver.rs:1693-1705`, `:1728-1740`), removed when it ends (`sqlserver.rs:1717`, `:1752`), and the backing connection is dropped when the session cannot be restored. Its validity *is* the session lifetime. |
/// | `reset_for_reuse` | *(blank)* `Unsupported` | there is no verified baseline replay in this crate, and no baseline to replay: the adapter answers `ResetDisposition::Discard`. `Verified` would hand out `Ok(())` and open a feature (`capabilities.rs:534-539`) that nothing here has earned. |
/// | `precise_cancel` | *(blank)* `Unknown` | see the note below — the adapter overwrites this cell. |
/// | `snapshots` | *(blank)* `Unsupported` | `begin_read_snapshot` exists and does start `SET TRANSACTION ISOLATION LEVEL SNAPSHOT; BEGIN TRANSACTION` (`sqlserver.rs:1663-1666`), but the enum asks for the *scope* of the guarantee (`PerTable` / `PerDatabase` / `Coordinated`) and this code proves none of them. It can even fail outright, because SNAPSHOT must be enabled for the active database (`sqlserver.rs:1669-1675`). |
/// | `transactions.isolation_levels` | *empty* | `begin_transaction` sends a bare `BEGIN TRANSACTION` with no level argument (`sqlserver.rs:1626`); there is no option the driver would honour. `current_isolation_level` only *reads back* a level the session already had and restores it afterwards (`sqlserver.rs:461-488`) — restoring an observation is not accepting a caller choice. Listing one would make `begin_transaction` accept an option it silently ignores. |
/// | `transactions.savepoints` | `Unsupported` | the driver transaction surface is exactly `begin_transaction` / `commit_transaction` / `rollback_transaction` (`sqlserver.rs:1611`, `:1691`, `:1726`); none of them emits `SAVE TRAN` and the adapter exposes no savepoint port. A caller *can* type `SAVE TRAN` into a `query` batch — the splitter routes it to its own batch (`sqlserver.rs:2280`) — but that is raw SQL, not a handle the contract could hand back. |
/// | `transactions.max_open_transactions` | `Some(1)` | the map is keyed by connection id and `begin_transaction` refuses when an entry already exists (`sqlserver.rs:1616-1620`), and `ensure_no_open_transaction` independently refuses when the *server* reports one open (`sqlserver.rs:490-509`, called at `:1625`). One open transaction per resource, proven on both sides of the wire. |
/// | `ddl_atomicity.by_operation` | *empty* | this driver does not override `DatabaseDriver::ddl_atomicity`, so there is no driver-wide answer to expand per operation. `transfer_sql_file_begin_transaction()` returning `BEGIN TRANSACTION;` (`sqlserver.rs:1501-1503`) is the data-transfer *file* renderer wrapping its own generated script, not a per-migration-operation atomicity claim; mapping it onto operation names would invent the keys. `atomicity_for` therefore answers `Unknown` for every operation and the caller asks instead of assuming. |
/// | `data` | `StreamingReadWrite` | all three axes are proven by code that runs. Row write: `execute` runs the statement and returns the server affected-row count (`sqlserver.rs:1604-1608`). Row read: `query_stream` decodes rows off the live tiberius `QueryStream`. Incremental delivery: `stream_one` pulls with `stream.try_next()` (`sqlserver.rs:622-626`) and pushes each row into `QueryRowBatcher` (`sqlserver.rs:639`), which emits a `Rows` event the moment the batch is full (`query_stream.rs:200-224`). Rows leave the driver before the last row arrives; that is the difference between streaming and a buffered replay. |
/// | `backup` | *(blank)* `Unknown` | nothing in this crate produces or consumes a backup artifact: `admin_commands.rs` registers create-database/schema/user commands and no backup or restore command, and the crate's only two mentions of backup are comments about the host dump path feeding schema names (`sqlserver.rs:191`, `:1184`). Nothing was measured, so the cell stays blank rather than claiming a measured refusal. |
///
/// # `precise_cancel` is not this driver's cell to fill
///
/// `LegacyResourceAdapter::new` takes the [`CapabilitySet`] and then *overwrites*
/// `precise_cancel` from `driver.supports_query_execution_cancel()`
/// (`resource_adapter.rs:148-152`): `Supported` if the driver says true, `Unknown`
/// otherwise. `SqlServerDriver` does not override that method, so it is the trait
/// default `false` (`traits.rs:817`), and the effective value is always `Unknown`
/// whatever is written here.
///
/// The value below is therefore the truthful one — and it is also the value the
/// adapter forces, so the factory and the provider cannot drift. SQL Server has no
/// execution-handle cancel protocol: `cancel_query` is an explicit
/// `DriverError::Unsupported` that says the legacy session-wide cancel does nothing
/// (`sqlserver.rs:1765-1767`) — exactly the pattern a declaration must not
/// contradict.
///
/// A caller that needs a cancellation gets an explicit
/// `ResourceError::CapabilityNotDeclared` instead of a silent empty success.
pub(crate) fn capabilities() -> CapabilitySet {
    CapabilitySet {
        // Filled — the driver reads session state off the server: the session's
        // isolation level via `DBCC USEROPTIONS` and its open-transaction count via
        // `SELECT @@TRANCOUNT`. Independent of the adapter's `SessionContinuity`.
        stateful_session: Availability::Supported,
        namespace_switch: NamespaceSwitch::Unsupported,
        // Blank — `observe_session` is unconditionally unobservable in the adapter.
        context_observation: ContextObservation::Unsupported,
        transaction_observation: TransactionObservation::Partial,
        session_scoped_handles: SessionScopedHandleSupport::Supported,
        // Blank — no baseline replay exists, and the adapter answers `Discard`.
        reset_for_reuse: ResetForReuse::Unsupported,
        // Blank, and overwritten by the adapter regardless — see the note above.
        precise_cancel: PreciseCancelSupport::Unknown,
        // Blank — a SNAPSHOT-isolated transaction is not a per-table / per-database /
        // coordinated read guarantee.
        snapshots: SnapshotSupport::Unsupported,
        transactions: TransactionSupport {
            // Empty on purpose: the contract reads this as "cannot confirm any
            // level", which is what a bare `BEGIN TRANSACTION` means.
            isolation_levels: Vec::new(),
            savepoints: Availability::Unsupported,
            max_open_transactions: Some(1),
        },
        // Empty on purpose: no `ddl_atomicity` override exists, so every operation
        // stays `Unknown` and the caller asks rather than assumes.
        ddl_atomicity: DdlAtomicitySupport::default(),
        data: DataSupport::StreamingReadWrite,
        // Blank — no artifact code and no backup command exist in this crate.
        backup: BackupSupport::Unknown,
    }
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

    use crate::{ConnectionHandle, DatabaseDriver, DriverError, SqlServerDriver, SqlServerFactory};

    fn factory() -> SqlServerFactory {
        SqlServerFactory
    }

    fn config() -> ConnectionConfig {
        serde_json::from_value(serde_json::json!({
            "id": "cfg-1",
            "name": "sqlserver",
            "databaseType": "sqlserver",
            "database": "master",
            "schema": "dbo",
        }))
        .expect("a minimal config deserializes")
    }

    #[test]
    fn the_factory_reaches_a_real_provider_instead_of_a_missing_one() {
        let provider = require_resource_provider(&factory())
            .expect("SQL Server is migrated, so the provider must be reachable");
        assert_eq!(provider.provider_id(), "sqlserver");
        assert_eq!(
            provider.namespace_shape().levels.len(),
            2,
            "SQL Server addresses a database and a schema"
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
        // Polarity flipped in place when the set stopped being blank: SQL Server
        // now declares the four cells it can prove (see `capabilities()`), so the
        // honest assertion is that the declaration is *not* empty. Same test, same
        // assertion, no test removed and no assertion removed.
        assert!(
            factory().resource_capabilities().declares_anything(),
            "SQL Server declares only the cells its own code proves; the rest stay blank"
        );
        assert!(
            !factory()
                .resource_capabilities()
                .precise_cancel
                .accepts_precise_cancel(),
            "SQL Server has no execution-handle cancel protocol"
        );
        assert!(registry.require_precise_cancel().is_err());
        assert_eq!(
            factory().resource_capabilities().stateful_session,
            datazen_driver_api::capabilities::Availability::Supported,
            "the driver reads the session's isolation level and open-transaction count \
             back off the server, so the session is measured rather than assumed"
        );
    }

    #[test]
    fn the_declaration_is_not_the_blank_default_set() {
        let declared = super::capabilities();

        assert_ne!(
            declared,
            datazen_driver_api::capabilities::CapabilitySet::default(),
            "a driver that can stream rows off the wire, hold a session-scoped \
             transaction handle and name its concurrency limit must not answer the \
             blank default set"
        );
        // The claims that are backed by code that runs.
        assert_eq!(
            declared.stateful_session,
            datazen_driver_api::capabilities::Availability::Supported,
            "DBCC USEROPTIONS and SELECT @@TRANCOUNT both read session-scoped state \
             back off the server, so this is measured rather than assumed"
        );
        assert_eq!(
            declared.data,
            datazen_driver_api::capability_domains::DataSupport::StreamingReadWrite,
            "stream_one pulls from the live tiberius stream and execute returns the \
             server's affected-row count"
        );
        assert_eq!(
            declared.transactions.max_open_transactions,
            Some(1),
            "begin_transaction refuses a second transaction on the same connection"
        );
        assert_eq!(
            declared.session_scoped_handles,
            datazen_driver_api::capabilities::SessionScopedHandleSupport::Supported,
            "the transaction handle is registered under the connection id and \
             dropped with the session"
        );
        // And the claims that stay blank, so a later edit that fills one of them
        // without evidence has to break this test on purpose rather than by drift.
        assert_eq!(
            declared.namespace_switch,
            datazen_driver_api::capabilities::NamespaceSwitch::Unsupported,
            "no session USE is ever issued; the driver rewrites SQL text instead"
        );
        assert_eq!(
            declared.snapshots,
            datazen_driver_api::capabilities::SnapshotSupport::Unsupported,
            "a SNAPSHOT-isolated transaction is not a per-table / per-database / \
             coordinated read guarantee"
        );
        assert!(
            declared.transactions.isolation_levels.is_empty(),
            "a bare BEGIN TRANSACTION honours no named level, so listing one would \
             accept an option the driver silently ignores"
        );
        assert!(
            declared.ddl_atomicity.by_operation.is_empty(),
            "no ddl_atomicity override exists, so every operation stays Unknown"
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

        assert_eq!(registry.snapshot.driver_id, "sqlserver");
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
            "the adapter stamps this revision when it merges evidence; it must not \
             still read 0"
        );
        assert_ne!(
            registry.snapshot.capability_revision, 0,
            "a revision of 0 means \"no capability module yet\" — the provider was \
             reached, but not one that can say anything about capabilities"
        );
        assert_eq!(
            registry.evidence_gaps(),
            Vec::<&'static str>::new(),
            "all twelve cells must carry evidence, including the ones this driver \
             declines: a declined cell is answered, not left blank"
        );
    }

    /// Every evidence record must name the source line it claims to describe.
    ///
    /// A capability claim whose explanation cannot be traced back to a file and
    /// line is indistinguishable, to a reader, from one that was invented. This
    /// fails when a record loses its citation, and it keeps the list of cells
    /// honest about which twelve names this crate owes records for.
    #[test]
    fn every_capability_record_cites_the_source_line_it_claims_to_describe() {
        const CELLS: [&str; 12] = [
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
        let records = super::capability_evidence();
        assert_eq!(
            records.len(),
            CELLS.len(),
            "there must be exactly one evidence record per capability cell"
        );
        for cell in CELLS {
            let record = records
                .iter()
                .find(|(key, _)| *key == cell)
                .unwrap_or_else(|| panic!("no evidence record for capability cell `{cell}`"));
            assert!(
                cites_a_source_line(&record.1),
                "the `{cell}` evidence record must cite the `file.rs:NNN` it rests on: {}",
                record.1
            );
        }
    }

    /// True when `text` names a file and a line number, e.g. `sqlserver.rs:1626`.
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
                target: NamespaceTarget::empty().with_database("master"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect("describe_resource must succeed without touching the network");

        assert!(!descriptor.is_fixed_session());
        assert_eq!(
            descriptor.session_continuity,
            SessionContinuity::Unknown,
            "SQL Server cannot observe a session, so continuity stays unknown"
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
            "SQL Server cannot prove a resource is back at its baseline after use"
        );
    }

    use datazen_driver_api::namespace::{NamespaceLevel, NamespaceLevelKind};

    #[tokio::test]
    async fn a_schema_qualified_target_is_accepted_because_sqlserver_really_has_one() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let descriptor = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: serde_json::from_value(serde_json::json!({
                    "id": "cfg-1",
                    "name": "sqlserver",
                    "databaseType": "sqlserver",
                    "database": "sales",
                    "schema": "dbo",
                }))
                .expect("a qualified config deserializes"),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty()
                    .with_database("sales")
                    .with_schema("dbo"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect("Database and Schema are both declared, so both must be accepted");

        assert_eq!(
            descriptor.namespace_shape.levels,
            vec![
                NamespaceLevel {
                    kind: NamespaceLevelKind::Database,
                    exists: true,
                    required: false,
                },
                NamespaceLevel {
                    kind: NamespaceLevelKind::Schema,
                    exists: true,
                    required: false,
                },
            ],
            "the descriptor must hand back the declared levels in order"
        );
    }

    #[tokio::test]
    async fn a_blank_target_is_still_accepted_because_both_levels_are_optional() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty(),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect("declaring required: false means the server resolves the blanks itself");
    }

    #[tokio::test]
    async fn the_legacy_cancel_refuses_instead_of_reporting_a_cancellation_that_never_happened() {
        let driver = SqlServerDriver::new();
        let err = driver
            .cancel_query(&ConnectionHandle {
                id: "connection-1".into(),
                pool_id: "sqlserver-pool".into(),
            })
            .await
            .expect_err("SQL Server cannot cancel; Ok(()) was the defect Track O removes");

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
            "SQL Server has no execution-handle protocol to advertise"
        );
        assert!(!SqlServerDriver::new().supports_query_execution_cancel());
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
            .expect("sqlserver declares a real provider");
        let second = factory()
            .resource_provider()
            .expect("sqlserver declares a real provider");
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
