//! Unit tests for [`crate::resource_adapter`].
//! Loaded via `#[path]` from `resource_adapter.rs` under `#[cfg(test)]`.

use super::*;
use crate::capabilities::Availability;
use crate::namespace::{NamespaceLevel, NamespaceLevelKind};
use crate::resource::{IdentityScope, ResourcePurpose, ResourceScope, ResultChunk};
use crate::session::SessionState;
use crate::{
    CommandResult, ConnectionConfig, DatabaseType, MultiQueryResult, QueryResult, ServerInfo,
    TableInfo, TableSchema, Value,
};

/// How the fake driver answers a precise cancel.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CancelBehaviour {
    Unsupported,
    NotRegistered,
    Accepts,
}

/// A legacy-style driver: session-wide `cancel_query` returns `Ok(())` like
/// 13 of the 15 shipped drivers, and `cancel_query_with_execution` is only
/// wired up when the test says so.
struct LegacyFake {
    precise_cancel: Option<CancelBehaviour>,
    legacy_cancel_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    precise_cancel_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    disconnect_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    connect_fails: bool,
    disconnect_fails: bool,
}

impl LegacyFake {
    fn new() -> Self {
        Self {
            precise_cancel: None,
            legacy_cancel_calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            precise_cancel_calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            disconnect_calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            connect_fails: false,
            disconnect_fails: false,
        }
    }
}

#[async_trait]
impl DatabaseDriver for LegacyFake {
    fn driver_type(&self) -> DatabaseType {
        "legacy-fake".to_string()
    }

    async fn connect(&self, _: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        if self.connect_fails {
            return Err(DriverError::ConnectionFailed("refused".into()));
        }
        Ok(ConnectionHandle {
            id: "legacy-1".into(),
            pool_id: "legacy-pool".into(),
        })
    }

    async fn disconnect(&self, _: ConnectionHandle) -> Result<(), DriverError> {
        self.disconnect_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.disconnect_fails {
            return Err(DriverError::ConnectionFailed("stuck".into()));
        }
        Ok(())
    }

    async fn test_connection(&self, _: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: "0".into(),
            server_type: "legacy-fake".into(),
        })
    }

    async fn get_databases(&self, _: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(vec![])
    }

    async fn get_tables(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        Ok(vec![])
    }

    async fn get_table_schema(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: &str,
        _: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        Err(DriverError::NotSupported("get_table_schema".into()))
    }

    async fn query(&self, _: &ConnectionHandle, _: &str) -> Result<QueryResult, DriverError> {
        unreachable!("not used by the adapter tests")
    }

    async fn query_multi(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        unreachable!("not used by the adapter tests")
    }

    async fn query_with_params(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: &[Value],
    ) -> Result<QueryResult, DriverError> {
        unreachable!("not used by the adapter tests")
    }

    async fn execute(&self, _: &ConnectionHandle, _: &str) -> Result<u64, DriverError> {
        unreachable!("not used by the adapter tests")
    }

    /// The session-wide cancel that 13/15 drivers implement as a no-op success.
    async fn cancel_query(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
        self.legacy_cancel_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn supports_query_execution_cancel(&self) -> bool {
        self.precise_cancel.is_some()
    }

    async fn cancel_query_with_execution(
        &self,
        _: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.precise_cancel_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match self.precise_cancel {
            Some(CancelBehaviour::Unsupported) => Err(DriverError::Unsupported("precise".into())),
            Some(CancelBehaviour::NotRegistered) => Err(DriverError::QueryExecutionNotFound(
                execution_id.as_str().to_string(),
            )),
            Some(CancelBehaviour::Accepts) => Ok(()),
            None => Err(DriverError::Unsupported("precise".into())),
        }
    }

    async fn execute_command(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        Ok(CommandResult {
            data: serde_json::json!({ "rows": [] }),
        })
    }
}

/// A budget that records every charge and release.
#[derive(Default)]
struct CountingBudget {
    outstanding: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    acquired: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    released: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    permit_seq: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    refuse: bool,
}

#[async_trait]
impl BudgetPort for CountingBudget {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<BudgetPermit, ResourceError> {
        if self.refuse {
            return Err(ResourceError::BudgetDenied {
                requested,
                reason: "test budget refuses".into(),
            });
        }
        let seq = self
            .permit_seq
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.acquired
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.outstanding
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(BudgetPermit {
            permit_id: format!("permit-{seq}"),
            physical_connections: requested,
        })
    }

    async fn release_physical_connections(&self, _: &BudgetPermit) -> Result<(), ResourceError> {
        self.released
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.outstanding
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

fn config() -> ConnectionConfig {
    serde_json::from_value(serde_json::json!({
        "id": "cfg-1",
        "name": "legacy",
        "databaseType": "legacy-fake",
    }))
    .expect("a minimal config deserializes")
}

/// Builder sugar for the common "one database, no catalog" target.
fn target(database: &str) -> NamespaceTarget {
    NamespaceTarget::empty().with_database(database)
}

/// The trait object the provider actually takes; `Arc<CountingBudget>` has to
/// be widened explicitly so the test exercises the same call an outside caller
/// would.
fn port(budget: &Arc<CountingBudget>) -> Arc<dyn BudgetPort> {
    budget.clone()
}

fn shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![NamespaceLevel {
            kind: NamespaceLevelKind::Database,
            exists: true,
            required: false,
        }],
        ..NamespaceShape::default()
    }
}

fn adapter(driver: LegacyFake) -> LegacyResourceAdapter {
    LegacyResourceAdapter::new(
        Arc::new(driver),
        "legacy-fake",
        "0.0.1",
        7,
        shape(),
        CapabilitySet::default(),
    )
}

fn acquire_request() -> AcquireResourceRequest {
    AcquireResourceRequest {
        connection_config: config(),
        target: target("app"),
        scope: ResourceScope {
            purpose: ResourcePurpose::InteractiveQuery,
            max_physical_connections: 2,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
    }
}

#[tokio::test]
async fn a_pool_of_one_is_not_reported_as_a_fixed_session() {
    let provider = adapter(LegacyFake::new());
    let descriptor = provider
        .describe_resource(&DescribeResourceRequest {
            connection_config: config(),
            identity_scope: IdentityScope::default(),
            target: target("app"),
            purpose: ResourcePurpose::InteractiveQuery,
        })
        .await
        .expect("describe succeeds");

    assert!(
        !descriptor.is_fixed_session(),
        "a legacy pool must never pass for a fixed session"
    );
    assert_eq!(descriptor.session_continuity, SessionContinuity::Unknown);
}

#[tokio::test]
async fn only_the_drivers_own_declaration_lifts_the_precise_cancel_capability() {
    let plain = adapter(LegacyFake::new());
    assert!(!plain
        .capabilities()
        .capabilities
        .precise_cancel
        .accepts_precise_cancel());
    assert!(plain.capabilities().require_precise_cancel().is_err());

    let precise = adapter(LegacyFake {
        precise_cancel: Some(CancelBehaviour::Accepts),
        ..LegacyFake::new()
    });
    assert!(precise.capabilities().require_precise_cancel().is_ok());
}

#[tokio::test]
async fn an_undeclared_capability_is_rejected_by_name_not_silently_succeeded() {
    let provider = adapter(LegacyFake::new());
    let err = provider
        .capabilities()
        .require_session_scoped_handles()
        .expect_err("a legacy driver registers no session handles");
    assert_eq!(err.capability, "sessionScopedHandles");
    assert_eq!(err.provider_id, "legacy-fake");

    let err = provider
        .capabilities()
        .require_reset_for_reuse()
        .expect_err("a legacy driver has no verified reset");
    assert_eq!(err.capability, "resetForReuse");
    assert_eq!(err.provider_id, "legacy-fake");
}

#[tokio::test]
async fn observe_session_reports_unknown_never_the_acquisition_target() {
    let provider = adapter(LegacyFake::new());
    let budget = Arc::new(CountingBudget::default());
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire succeeds");

    let observed = provider
        .observe_session(&handle)
        .await
        .expect("observation is unreadable but not an error");

    assert_eq!(observed.state, SessionState::Unknown);
    assert_eq!(observed.context.namespace, NamespaceTarget::empty());
    assert!(!observed.context.matches_target(&target("app")));
    assert!(observed.protocol_drained == false);
    assert!(observed.context_revision == 0);
}

#[tokio::test]
async fn a_cancel_is_unsupported_and_never_falls_back_to_the_legacy_query() {
    let driver = LegacyFake::new();
    let legacy_calls = Arc::clone(&driver.legacy_cancel_calls);
    let precise_calls = Arc::clone(&driver.precise_cancel_calls);
    let provider = adapter(driver);
    let budget = Arc::new(CountingBudget::default());
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire succeeds");

    let receipt = provider
        .request_cancel(&handle, &QueryExecutionId::new("exec-1"))
        .await
        .expect("a refusal is a receipt, not an error");

    assert_eq!(receipt.disposition, CancelDisposition::Unsupported);
    assert!(!receipt.disposition.is_accepted());
    assert!(receipt.state.is_none(), "no state may be invented");
    assert_eq!(legacy_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(precise_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancel_distinguishes_not_registered_from_unsupported_and_from_accepted() {
    for (behaviour, expected, accepts) in [
        (
            CancelBehaviour::Unsupported,
            CancelDisposition::Unsupported,
            false,
        ),
        (
            CancelBehaviour::NotRegistered,
            CancelDisposition::NotRegistered,
            false,
        ),
        (CancelBehaviour::Accepts, CancelDisposition::Requested, true),
    ] {
        let provider = adapter(LegacyFake {
            precise_cancel: Some(behaviour),
            ..LegacyFake::new()
        });
        let budget = Arc::new(CountingBudget::default());
        let handle = provider
            .acquire_resource(&acquire_request(), &port(&budget))
            .await
            .expect("acquire succeeds");

        let receipt = provider
            .request_cancel(&handle, &QueryExecutionId::new("exec-1"))
            .await
            .expect("a refusal is a receipt, not an error");
        assert_eq!(receipt.disposition, expected);
        assert_eq!(receipt.disposition.is_accepted(), accepts);
        if accepts {
            assert_eq!(receipt.state, Some(ExecutionState::CancelRequested));
        } else {
            assert!(receipt.state.is_none());
        }
    }
}

#[tokio::test]
async fn reset_never_claims_a_clean_resource() {
    let provider = adapter(LegacyFake::new());
    let budget = Arc::new(CountingBudget::default());
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire succeeds");

    let disposition = provider
        .reset_resource(&handle, &Baseline::default())
        .await
        .expect("reset reports a disposition");
    assert_eq!(disposition, ResetDisposition::Discard);
}

#[tokio::test]
async fn context_change_is_refused_rather_than_silently_reconnecting() {
    let provider = adapter(LegacyFake::new());
    let budget = Arc::new(CountingBudget::default());
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire succeeds");

    let disposition = provider
        .change_context(&handle, &target("other"))
        .await
        .expect("a refusal is a disposition, not an error");
    assert_eq!(disposition, ContextChangeDisposition::Unsupported);
}

#[tokio::test]
async fn commit_and_rollback_are_refused_rather_than_reported_as_unknown() {
    let provider = adapter(LegacyFake::new());
    let budget = Arc::new(CountingBudget::default());
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire succeeds");

    let commit = provider
        .commit_transaction(&handle)
        .await
        .expect_err("commit cannot be observed through a legacy driver");
    assert!(matches!(
        commit,
        ResourceError::OperationNotSupported { .. }
    ));

    let rollback = provider
        .rollback_transaction(&handle)
        .await
        .expect_err("rollback cannot be observed through a legacy driver");
    assert!(matches!(
        rollback,
        ResourceError::OperationNotSupported { .. }
    ));
}

#[tokio::test]
async fn the_charge_is_released_exactly_once_even_when_close_repeats() {
    let provider = adapter(LegacyFake::new());
    let budget = Arc::new(CountingBudget::default());
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire succeeds");

    assert_eq!(budget.acquired.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        budget.outstanding.load(std::sync::atomic::Ordering::SeqCst),
        1
    );

    for _ in 0..3 {
        assert_eq!(
            provider
                .close_resource(&handle)
                .await
                .expect("close is idempotent"),
            CloseDisposition::Closed
        );
    }
    assert_eq!(budget.released.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        budget.outstanding.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(provider.open_resource_count(), 0);
}

#[tokio::test]
async fn an_unconfirmed_close_does_not_report_the_charge_as_recovered() {
    let provider = adapter(LegacyFake {
        disconnect_fails: true,
        ..LegacyFake::new()
    });
    let budget = Arc::new(CountingBudget::default());
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire succeeds");

    assert_eq!(
        provider
            .close_resource(&handle)
            .await
            .expect("close reports a disposition"),
        CloseDisposition::CloseUnconfirmed
    );
    assert_eq!(
        budget.released.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "an unconfirmed close must not claim the budget came back"
    );
}

#[tokio::test]
async fn a_failed_connect_returns_the_charge_it_took() {
    let provider = adapter(LegacyFake {
        connect_fails: true,
        ..LegacyFake::new()
    });
    let budget = Arc::new(CountingBudget::default());
    let err = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect_err("connect failed");
    assert!(matches!(err, ResourceError::Driver(_)));
    assert_eq!(budget.acquired.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(budget.released.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        budget.outstanding.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[tokio::test]
async fn a_refused_budget_never_opens_a_connection() {
    let provider = adapter(LegacyFake::new());
    let budget = Arc::new(CountingBudget {
        refuse: true,
        ..CountingBudget::default()
    });
    let err = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect_err("the budget denies the charge");
    assert!(matches!(
        err,
        ResourceError::BudgetDenied { requested: 2, .. }
    ));
    assert_eq!(provider.open_resource_count(), 0);
}

#[tokio::test]
async fn a_handle_from_another_epoch_is_rejected_before_any_driver_call() {
    let provider = adapter(LegacyFake::new());
    let stale = ResourceHandle::issue("legacy-fake".to_string(), "legacy-1".to_string(), 6);
    let budget = Arc::new(CountingBudget::default());
    let err = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("the current epoch works");
    assert!(provider.observe_session(&stale).await.is_err());
    provider
        .close_resource(&err)
        .await
        .expect("the current epoch closes cleanly");
}

#[tokio::test]
async fn a_namespace_level_this_database_lacks_is_rejected_before_connecting() {
    let provider = LegacyResourceAdapter::new(
        Arc::new(LegacyFake::new()),
        "legacy-fake",
        "0.0.1",
        7,
        NamespaceShape::default(),
        CapabilitySet::default(),
    );
    let budget = Arc::new(CountingBudget::default());
    let err = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect_err("the shape has no database level at all");
    assert!(
        matches!(err, ResourceError::NonexistentNamespaceLevel { .. }),
        "{err}"
    );
    assert_eq!(budget.acquired.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn execution_forwards_the_exact_command_pair_and_fabricates_no_observation() {
    let provider = adapter(LegacyFake::new());
    let budget = Arc::new(CountingBudget::default());
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire succeeds");

    let completion = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &CommandCall::new("query", serde_json::json!({ "sql": "select 1" })),
            &DiscardSink,
        )
        .await
        .expect("execute succeeds");

    assert_eq!(completion.completion_status, CompletionStatus::Succeeded);
    assert!(completion.statement_results.is_empty());
    assert!(completion.session_handles.is_empty());
    assert_eq!(completion.context_before, SessionContext::unobserved());
    assert_eq!(completion.context_after, SessionContext::unobserved());
    assert_eq!(
        completion.transaction_observation.state,
        TransactionState::Unknown
    );
}

/// A sink that records nothing: the legacy path produces no chunks.
struct DiscardSink;

#[async_trait]
impl ResultSink for DiscardSink {
    async fn write(&self, _: ResultChunk) -> Result<(), ResourceError> {
        Ok(())
    }

    async fn complete(&self) -> Result<(), ResourceError> {
        Ok(())
    }

    async fn fail(&self, _: &str) -> Result<(), ResourceError> {
        Ok(())
    }
}

#[test]
fn the_caps_the_adapter_claims_are_only_the_ones_it_can_honour() {
    let provider = adapter(LegacyFake::new());
    let capabilities = &provider.capabilities().capabilities;
    assert!(!capabilities.stateful_session.enables_feature());
    assert!(!capabilities.context_observation.confirms_context());
    assert!(!capabilities.namespace_switch.switches_in_place());
    assert!(!capabilities.transactions.savepoints.enables_feature());
    assert!(!capabilities.declares_anything());
    // Every `require_*` except the two the adapter can answer must refuse.
    assert!(provider.capabilities().require_stateful_session().is_err());
    assert!(provider
        .capabilities()
        .require_context_observation()
        .is_err());
    assert!(provider
        .capabilities()
        .require_transaction_observation()
        .is_err());
    assert!(provider
        .capabilities()
        .require_session_scoped_handles()
        .is_err());
    assert!(provider.capabilities().require_reset_for_reuse().is_err());
    assert!(provider.capabilities().require_snapshots().is_err());
    assert!(provider
        .capabilities()
        .require_in_place_namespace_switch()
        .is_err());
}

#[test]
fn the_adapter_refuses_a_context_observation_it_never_performed() {
    // Guards the shape a caller would otherwise be tempted to assume.
    let unobserved = SessionContext::unobserved();
    assert!(!unobserved.matches_target(&target("app")));
    assert!(!unobserved.matches_target(&NamespaceTarget::empty()));
}

/// The twelve declared cells, in declaration order.
///
/// Spelled out here rather than derived from production code: a test that
/// asked the implementation what it expects proves nothing. If a cell is added
/// to `CapabilitySet`, this list has to be updated deliberately — that is the
/// point of writing it out.
const ALL_CELLS: [&str; 12] = [
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

#[test]
fn adapter_without_evidence_reports_every_cell_missing() {
    // The discriminator: a driver on the adapter path that recorded nothing is
    // observable as "all twelve missing" without the driver's cooperation.
    //
    // Note what this test does NOT assert: `gaps.is_empty()`. Asserting that
    // would turn "nobody has filled in the evidence yet" into "the evidence
    // situation is fine", which is the opposite of what the empty table means.
    let provider = adapter(LegacyFake::new());

    assert_eq!(provider.evidence_gaps(), ALL_CELLS);
    // Position taken explicitly: the adapter's own `confirmed` table carries
    // no rationale today, and that is a known gap, not a passing grade.
    assert!(provider.capabilities().snapshot.confirmed.is_empty());
    assert_eq!(provider.capabilities().snapshot.capability_revision, 0);
}

#[test]
fn adapter_evidence_is_recorded_and_the_gap_list_shrinks_to_the_rest() {
    let provider = adapter(LegacyFake::new()).with_evidence([
        (
            "preciseCancel",
            "the fake driver's legacy `cancel_query` returns Ok(()) unconditionally",
        ),
        (
            "statefulSession",
            "SessionContinuity is the non-supporting default: never verified",
        ),
        ("connectionCostPolicy", "not one of the twelve cells"),
    ]);

    // The two cells above dropped off; the third key is not a cell, so it can
    // never appear in a gap list, and it is not an error either.
    assert_eq!(
        provider.evidence_gaps(),
        [
            "namespaceSwitch",
            "contextObservation",
            "transactionObservation",
            "sessionScopedHandles",
            "resetForReuse",
            "snapshots",
            "transactions",
            "ddlAtomicity",
            "data",
            "backup",
        ]
    );

    let snapshot = &provider.capabilities().snapshot;
    assert_eq!(snapshot.capability_revision, ADAPTER_EVIDENCE_REVISION);
    assert_ne!(snapshot.capability_revision, 0);
    assert_eq!(snapshot.confirmed.len(), 3);
    assert_eq!(
        snapshot.confirmed.get("preciseCancel").map(String::as_str),
        Some("the fake driver's legacy `cancel_query` returns Ok(()) unconditionally")
    );
}

#[test]
fn adapter_evidence_over_a_declared_capability_cell_is_visible_to_a_reviewer() {
    // The point of the whole channel: the rationale for a claim that is
    // actually switched on has to be attached to the snapshot, so a reader of
    // the registry sees the claim and its reason in the same place.
    let mut capabilities = CapabilitySet::default();
    capabilities.stateful_session = Availability::Supported;
    let provider = LegacyResourceAdapter::new(
        Arc::new(LegacyFake::new()),
        "legacy-fake",
        "0.0.1",
        7,
        shape(),
        capabilities,
    );

    assert!(provider.capabilities().require_stateful_session().is_ok());
    // `with_evidence` not called: the claim is on, the evidence is missing, and
    // the gap list says which cell that is.
    assert_eq!(provider.evidence_gaps(), ALL_CELLS);
}

#[test]
fn evidence_at_revision_zero_is_refused() {
    // Revision 0 is the "no capability module existed yet" sentinel. Evidence
    // recorded against it would be unattributable, so the constructor refuses.
    let snapshot = CapabilitySnapshot::new("legacy-fake", "0.0.1", crate::PROTOCOL_VERSION, 0);
    let error = snapshot
        .with_evidence([("backup", "restores land on a separate connection")])
        .expect_err("revision 0 must not accept evidence");

    assert_eq!(error.provider_id, "legacy-fake");
    assert_eq!(error.capability_revision, 0);

    // The refusal is about the evidence, not about revision 0 itself: an empty
    // table at revision 0 is still the normal starting point.
    assert!(
        CapabilitySnapshot::new("legacy-fake", "0.0.1", crate::PROTOCOL_VERSION, 0)
            .with_evidence(std::iter::empty::<(String, String)>())
            .is_ok()
    );
}

#[test]
fn evidence_gaps_answers_from_the_confirmed_table_alone() {
    // Proves the gap list is not asking the driver anything: it is a function
    // of `confirmed` minus the cell list, so a driver cannot report itself as
    // complete by declining to answer.
    let snapshot = CapabilitySnapshot::new("legacy-fake", "0.0.1", crate::PROTOCOL_VERSION, 2)
        .with_evidence(ALL_CELLS.map(|cell| (cell, "synthetic rationale")))
        .expect("revision 2 is allowed to carry evidence")
        .with_evidence([("connectionCostPolicy", "extra, not a cell")])
        .expect("still allowed");

    assert_eq!(snapshot.evidence_gaps(), Vec::<&'static str>::new());
    assert_eq!(snapshot.confirmed.len(), 13);
}
