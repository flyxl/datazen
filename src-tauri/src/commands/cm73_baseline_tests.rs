//! CM-73 baseline evidence — 空闲淘汰与活动事务句柄连续旅程（P0 / `p0-cm73-baseline`）。
//!
//! Spec: `docs/architecture/platform/connection-management.md` §16.7 的
//! `**CM-73 空闲淘汰与活动事务句柄连续旅程（H）**` (CM-73, H).
//!
//! The journey driven here is the one the spec asks for, end to end on a single
//! `dbSessionId`:
//! 1. `begin_session_transaction` opens a **real** transaction handle through the
//!    driver and registers it, then writes an uncommitted marker.
//! 2. The session is advanced past the idle-eviction deadline and
//!    `cleanup_idle_connections` runs.
//! 3. A query is issued again with the **same `dbSessionId`**.
//! 4. We record the physical `resourceId`, whether the owner map survived,
//!    whether the old `TransactionHandle` is still readable, and every
//!    rollback/close call the driver actually received.
//!
//! Two tests share one journey:
//! - `cm73_baseline_idle_eviction_reproduces_the_defect` is **green** and pins
//!   the *observed* sequence — that is the `:1284` baseline evidence. When the
//!   P3 connection-runtime track turns CM-73 green, this test is **deleted, not
//!   updated**: the defect it pins no longer exists.
//! - `cm73_registered_live_handle_survives_idle_eviction_and_is_never_reused`
//!   is `#[ignore]`d and asserts the **target behavior only**. Per the `:1285`
//!   retention clause its assertions must survive the removal of the legacy
//!   `ConnectionManager`, so they are written as behavior ("the runtime must not
//!   leave a live handle behind a closed physical resource, and must not rebuild
//!   under the same `dbSessionId`") and never as legacy structure (no assertion
//!   that any particular map is empty, no reliance on legacy function names).
//!
//! Trade-off, stated once and deliberately: observing the current `session_transactions`
//! map is what makes the defect legible (an old handle that still points at a dead
//! physical connection is the harm), so the baseline test records it — but the
//! *target* assertions deliberately abstain from that shape. `session_transactions`
//! is command-layer state that CM-73 will move into the runtime; asserting its
//! emptiness would hard-code a structure that is about to change.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::query::{
    begin_session_transaction_impl, execute_query_impl, rollback_session_transaction_impl,
    session_transaction_status_impl,
};
use super::AppState;
use crate::db::registry::DriverRegistry;
use crate::db::{
    ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType, DriverError,
    MultiQueryResult, QueryResult, ServerInfo, TableInfo, TableSchema, TransactionHandle, Value,
};
use crate::store::Store;
use crate::testing::app_state::{build_app_state, sample_postgres_config};
use crate::testing::FileKeyringGuard;
use datazen_driver_api::{
    execute_standard_sql_command, query_command_definition, CommandResult, DriverCommandDefinition,
};

const CONNECTION_ID: &str = "cm73-conn";

// ── Recording driver ─────────────────────────────────────────────────────────

/// One physical resource = one `connect()`. The id is carried in
/// `ConnectionHandle::pool_id` so every later call can be attributed to the
/// resource that is actually alive.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Connect {
        resource: String,
    },
    Disconnect {
        resource: String,
    },
    Begin {
        resource: String,
        transaction: String,
    },
    Query {
        resource: String,
        sql: String,
    },
    /// `resource` is the physical resource the **host was serving the
    /// transaction's session id on** when the call arrived — not the resource the
    /// transaction was opened on, which is the `Begin` call's `resource`. The two
    /// are the same for every healthy runtime; they diverge the moment a session
    /// id is re-pointed at a rebuilt connection.
    Commit {
        resource: String,
        transaction: String,
    },
    Rollback {
        resource: String,
        transaction: String,
    },
}

struct JournalDriver {
    calls: Mutex<Vec<Call>>,
    live: Mutex<HashSet<String>>,
    /// `ConnectionHandle::id` (as returned by `connect`) → physical resource.
    /// The runtime overwrites `handle.id` with the session id when it adopts a
    /// connection, so this map is only consulted to resolve the `connection_id`
    /// frozen into a `TransactionHandle` — never to route a live call.
    resource_by_connection_id: Mutex<HashMap<String, String>>,
    /// `ConnectionHandle::id` → the physical resource the host most recently
    /// presented **under that id**. `commit`/`rollback` receive only a
    /// `TransactionHandle`, so this is the journal's only honest way to see which
    /// resource a finish call was routed to: the runtime can re-point a session id
    /// at a rebuilt connection without changing the id the caller still holds.
    host_binding: Mutex<HashMap<String, String>>,
    tx_resource: Mutex<HashMap<String, String>>,
    next_resource: AtomicUsize,
    next_transaction: AtomicUsize,
}

impl JournalDriver {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            live: Mutex::new(HashSet::new()),
            resource_by_connection_id: Mutex::new(HashMap::new()),
            host_binding: Mutex::new(HashMap::new()),
            tx_resource: Mutex::new(HashMap::new()),
            next_resource: AtomicUsize::new(0),
            next_transaction: AtomicUsize::new(0),
        }
    }

    fn snapshot(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// The physical resource behind a live call. `pool_id` is the driver's own
    /// identity for the resource and the runtime never rewrites it, so it stays
    /// correct even after the session id is re-pointed at a fresh connection.
    fn resource_of(&self, handle: &ConnectionHandle) -> String {
        handle.pool_id.clone()
    }

    /// The physical resource a (possibly stale) `connection_id` was issued for.
    fn resource_of_connection_id(&self, connection_id: &str) -> Option<String> {
        self.resource_by_connection_id
            .lock()
            .unwrap()
            .get(connection_id)
            .cloned()
    }

    /// Note that the host handed out `handle`, i.e. that `handle.id` is currently
    /// served by `handle.pool_id`. Recorded before the liveness check so that even
    /// a call the driver rejects still counts as "the host pointed here".
    fn bind_host(&self, handle: &ConnectionHandle) {
        self.host_binding
            .lock()
            .unwrap()
            .insert(handle.id.clone(), handle.pool_id.clone());
    }

    /// The newest physical resource the driver has opened — the one the session
    /// id would be served by now.
    fn current_resource(&self, calls: &[Call]) -> Option<String> {
        calls.iter().rev().find_map(|call| match call {
            Call::Connect { resource } => Some(resource.clone()),
            _ => None,
        })
    }

    /// Journal a commit/rollback and decide its result.
    ///
    /// Two different resources matter here and must not be conflated:
    /// - *routed to*: the physical resource the host is currently serving the
    ///   transaction's session id on. A transaction is only safe to finish on the
    ///   resource that opened it, so a divergence is journaled, not hidden.
    /// - *opened on*: where the transaction actually lives. This — and only this —
    ///   decides the result, because a loss of *that* resource is what makes the
    ///   outcome genuinely unknowable.
    fn finish_transaction(&self, commit: bool, tx: TransactionHandle) -> Result<(), DriverError> {
        let opened_on = self
            .tx_resource
            .lock()
            .unwrap()
            .get(&tx.id)
            .cloned()
            .unwrap_or_else(|| "unknown-resource".to_string());
        let routed_to = self
            .host_binding
            .lock()
            .unwrap()
            .get(&tx.connection_id)
            .cloned()
            .unwrap_or_else(|| opened_on.clone());
        self.calls.lock().unwrap().push(if commit {
            Call::Commit {
                resource: routed_to,
                transaction: tx.id.clone(),
            }
        } else {
            Call::Rollback {
                resource: routed_to,
                transaction: tx.id.clone(),
            }
        });
        if !self.live.lock().unwrap().contains(&opened_on) {
            // A real driver must fail closed here: the outcome of the
            // transaction is genuinely unknown once its resource is gone.
            return Err(DriverError::TransactionError(format!(
                "transaction {} was opened on {} which is already closed: outcome unknown",
                tx.id, opened_on
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl DatabaseDriver for JournalDriver {
    fn driver_type(&self) -> DatabaseType {
        "postgres".into()
    }

    async fn connect(&self, _config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let resource = format!(
            "resource-{}",
            self.next_resource.fetch_add(1, Ordering::SeqCst) + 1
        );
        let handle = ConnectionHandle {
            id: format!("{resource}/conn"),
            pool_id: resource.clone(),
        };
        self.calls.lock().unwrap().push(Call::Connect {
            resource: resource.clone(),
        });
        self.live.lock().unwrap().insert(resource.clone());
        self.resource_by_connection_id
            .lock()
            .unwrap()
            .insert(handle.id.clone(), resource);
        Ok(handle)
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        let resource = handle.pool_id.clone();
        self.calls.lock().unwrap().push(Call::Disconnect {
            resource: resource.clone(),
        });
        self.live.lock().unwrap().remove(&resource);
        Ok(())
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.bind_host(handle);
        let resource = self.resource_of(handle);
        if !self.live.lock().unwrap().contains(&resource) {
            return Err(DriverError::TransactionError(format!(
                "cannot open a transaction on closed resource {resource}"
            )));
        }
        let transaction = format!(
            "tx-{}",
            self.next_transaction.fetch_add(1, Ordering::SeqCst) + 1
        );
        self.tx_resource
            .lock()
            .unwrap()
            .insert(transaction.clone(), resource.clone());
        self.calls.lock().unwrap().push(Call::Begin {
            resource,
            transaction: transaction.clone(),
        });
        Ok(TransactionHandle {
            id: transaction,
            connection_id: handle.id.clone(),
        })
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.finish_transaction(true, tx)
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.finish_transaction(false, tx)
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        self.bind_host(handle);
        let resource = self.resource_of(handle);
        if !self.live.lock().unwrap().contains(&resource) {
            return Err(DriverError::QueryFailed(format!(
                "query on closed resource {resource}"
            )));
        }
        self.calls.lock().unwrap().push(Call::Query {
            resource,
            sql: sql.to_string(),
        });
        Ok(MultiQueryResult {
            results: Vec::new(),
            total_time_ms: 0,
        })
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        vec![query_command_definition()]
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        execute_standard_sql_command(self, handle, command, input).await
    }

    async fn test_connection(&self, _config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Err(DriverError::QueryFailed("journal driver".into()))
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(vec![])
    }

    async fn get_tables(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        Ok(vec![])
    }

    async fn get_table_schema(
        &self,
        _handle: &ConnectionHandle,
        _table: &str,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        Err(DriverError::QueryFailed("journal driver".into()))
    }

    async fn query(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<QueryResult, DriverError> {
        Err(DriverError::QueryFailed("journal driver".into()))
    }

    async fn query_with_params(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        Err(DriverError::QueryFailed("journal driver".into()))
    }

    async fn execute(&self, _handle: &ConnectionHandle, _sql: &str) -> Result<u64, DriverError> {
        Err(DriverError::QueryFailed("journal driver".into()))
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

// ── Journey ──────────────────────────────────────────────────────────────────

struct Fixture {
    _keyring: FileKeyringGuard,
    _temp: tempfile::TempDir,
    state: AppState,
    driver: Arc<JournalDriver>,
}

async fn fixture() -> Fixture {
    let keyring = FileKeyringGuard::set();
    let temp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        Store::init_with_path(temp.path())
            .await
            .expect("store init"),
    );
    let registry = Arc::new(DriverRegistry::new());
    let driver = Arc::new(JournalDriver::new());
    registry
        .register_test_driver("postgres", driver.clone())
        .await;
    let state = build_app_state(store.clone(), registry);
    store
        .save_connection(sample_postgres_config(CONNECTION_ID))
        .await
        .expect("save_connection");
    Fixture {
        _keyring: keyring,
        _temp: temp,
        state,
        driver,
    }
}

/// Everything the CM-73 case asks us to record, captured in one pass so both
/// tests read the same journey instead of re-deriving it.
#[derive(Debug)]
struct Observation {
    db_session_id: String,
    /// Physical resource that hosted the transaction (`resource-1`).
    transaction_resource: String,
    /// Did idle cleanup really close the physical resource?
    physical_disconnect_observed: bool,
    /// Is the `dbSessionId → connectionId` owner entry still present after cleanup?
    owner_after_evict: Option<String>,
    /// Does the session still report an open transaction after cleanup?
    status_after_evict: bool,
    /// Did the same `dbSessionId` successfully run a query again, and on what
    /// physical resource?
    query_after_evict_ok: bool,
    resource_after_query: Option<String>,
    /// Is the old `TransactionHandle` still readable, and what does it point at?
    handle_after_query: Option<(String, String)>,
    handle_resource_after_query: Option<String>,
    /// Did the host still route the old transaction's finish call to the driver
    /// **after** the physical resource had already been closed? This is the
    /// journal-level, map-free form of "a committable handle survived the loss of
    /// the thing it commits on": a runtime that terminates the transaction before
    /// closing never produces it.
    old_handle_finished_after_loss: bool,
    /// Result of a rollback attempt against that old handle.
    rollback_result: Result<(), String>,
    /// Was a commit/rollback delivered to a resource other than the one that
    /// opened the transaction?
    transaction_call_misrouted: bool,
    calls: Vec<Call>,
}

async fn observe_journey() -> Observation {
    let fixture = fixture().await;
    let state = &fixture.state;
    let driver = fixture.driver.as_ref();

    // The session must be opened with `ConnectionManager::connect`, which does
    // not acquire a reference: `cleanup_idle_connections` only considers
    // sessions whose ref count is 0, and every host connect path
    // (`get_or_connect_session` / `connect_dedicated`) leaves the count at 1.
    // This is the idle state the eviction sweep is written for.
    let db_session_id = state
        .connection_manager
        .connect(CONNECTION_ID)
        .await
        .expect("connect");
    let after_connect = driver.snapshot();
    let transaction_resource = driver
        .current_resource(&after_connect)
        .expect("physical resource of the first connect");

    // 1. Real transaction handle, registered, plus an uncommitted marker.
    begin_session_transaction_impl(state, db_session_id.clone())
        .await
        .expect("begin_session_transaction");
    let marker = execute_query_impl(
        state,
        db_session_id.clone(),
        "INSERT INTO cm73_marker (note) VALUES ('uncommitted')".to_string(),
        None,
    )
    .await;
    assert!(marker.is_ok(), "uncommitted marker must be writable");

    // 2. Advance to the idle-eviction deadline without touching production code.
    // `expire_test_session` rewinds `last_used` by `idle_timeout + 1s`; it is
    // `#[cfg(test)]`-only and adds nothing to a production build. The deadline
    // itself stays the 30-minute production constant.
    state
        .connection_manager
        .expire_test_session(&db_session_id)
        .await;
    state.connection_manager.cleanup_idle_connections().await;

    let after_evict = driver.snapshot();
    let physical_disconnect_observed = after_evict.contains(&Call::Disconnect {
        resource: transaction_resource.clone(),
    });
    let owner_after_evict = state
        .connection_manager
        .owner_connection_id(&db_session_id)
        .await;
    let status_after_evict = session_transaction_status_impl(state, db_session_id.clone())
        .await
        .expect("session_transaction_status");

    // 3. Same `dbSessionId` again — this is where the silent rebuild happens.
    let query_after_evict_ok =
        execute_query_impl(state, db_session_id.clone(), "SELECT 1".to_string(), None)
            .await
            .is_ok();

    let after_query = driver.snapshot();
    let resource_after_query = driver.current_resource(&after_query);
    let handle_after_query = {
        let txs = state.session_transactions.lock().await;
        txs.get(&db_session_id)
            .map(|tx| (tx.id.clone(), tx.connection_id.clone()))
    };
    let handle_resource_after_query = handle_after_query
        .as_ref()
        .and_then(|(_, connection_id)| driver.resource_of_connection_id(connection_id));

    // 4. Finally try to unwind the transaction that started on the dead resource.
    let rollback_result = rollback_session_transaction_impl(state, db_session_id.clone())
        .await
        .map_err(|err| err.to_string());

    let final_calls = driver.snapshot();

    // Where the transaction was opened, per the driver journal itself.
    let opened: Option<(String, String)> = final_calls.iter().find_map(|call| match call {
        Call::Begin {
            resource,
            transaction,
        } => Some((resource.clone(), transaction.clone())),
        _ => None,
    });

    // "The handle outlived the loss": a finish call for the journey's
    // transaction that reaches the driver *after* the close. A runtime that
    // terminates the transaction on the original resource before closing it
    // produces nothing here; a runtime that keeps the resource up never closes
    // it, so the position lookup below finds no close at all.
    let old_handle_finished_after_loss = match opened.as_ref() {
        None => false,
        Some((_, transaction)) => match final_calls.iter().position(|call| {
            matches!(call, Call::Disconnect { resource } if resource == &transaction_resource)
        }) {
            None => false,
            Some(closed_at) => final_calls[closed_at..].iter().any(|call| {
                matches!(call, Call::Commit { transaction: t, .. } | Call::Rollback { transaction: t, .. } if t == transaction)
            }),
        },
    };

    // Misrouting compares the resource the host delivered the finish call on
    // against the resource the transaction was actually opened on. Both ends
    // come from the journal: `Begin` names the origin, the finish call names the
    // routing. They are only equal when the host never re-pointed the session id
    // at a rebuilt connection underneath the caller's handle.
    let transaction_call_misrouted = match opened.as_ref() {
        None => false,
        Some((origin, transaction)) => final_calls.iter().any(|call| match call {
            Call::Commit {
                resource,
                transaction: t,
            }
            | Call::Rollback {
                resource,
                transaction: t,
            } => t == transaction && resource != origin,
            _ => false,
        }),
    };

    Observation {
        db_session_id,
        transaction_resource,
        physical_disconnect_observed,
        owner_after_evict,
        status_after_evict,
        query_after_evict_ok,
        resource_after_query,
        handle_after_query,
        handle_resource_after_query,
        old_handle_finished_after_loss,
        rollback_result,
        transaction_call_misrouted,
        calls: final_calls,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// CM-73 baseline evidence (`:1284`). Green, and pins the *observed* sequence.
///
/// **Delete this test when CM-73 goes green in the P3 connection-runtime
/// track** — do not relax it. Every assertion below describes the defect.
#[tokio::test]
async fn cm73_baseline_idle_eviction_reproduces_the_defect() {
    let obs = observe_journey().await;
    eprintln!("[CM-73 baseline] db_session_id={}", obs.db_session_id);
    eprintln!("[CM-73 baseline] calls={:#?}", obs.calls);

    // Observed 1: idle cleanup really did close the physical resource that
    // hosted the transaction.
    assert!(
        obs.physical_disconnect_observed,
        "baseline: idle cleanup is expected to close {}",
        obs.transaction_resource
    );

    // Observed 2: the `dbSessionId → connectionId` owner entry survives, which
    // is precisely what lets the old id rebuild a session later.
    assert_eq!(
        obs.owner_after_evict.as_deref(),
        Some(CONNECTION_ID),
        "baseline: the owner entry is expected to survive eviction"
    );

    // Observed 3: the session still reports an open transaction although its
    // physical connection is gone.
    assert!(
        obs.status_after_evict,
        "baseline: an open transaction is expected to still be reported after the physical loss"
    );

    // Observed 4: a query on the same `dbSessionId` succeeds again, on a
    // *different* physical resource that was silently connected for us.
    assert!(
        obs.query_after_evict_ok,
        "baseline: the same dbSessionId is expected to serve queries again"
    );
    assert_ne!(
        obs.resource_after_query.as_deref(),
        Some(obs.transaction_resource.as_str()),
        "baseline: the rebuild is expected to land on a brand-new physical resource"
    );

    // Observed 5: the old `TransactionHandle` is still readable and still points
    // at the dead physical connection.
    let (tx_id, tx_connection_id) = obs
        .handle_after_query
        .as_ref()
        .expect("baseline: the old handle is expected to still be registered");
    assert!(
        !tx_id.is_empty() && !tx_connection_id.is_empty(),
        "baseline: the old handle is expected to remain readable"
    );
    assert_eq!(
        obs.handle_resource_after_query.as_deref(),
        Some(obs.transaction_resource.as_str()),
        "baseline: the old handle is expected to keep pointing at the closed resource {}",
        obs.transaction_resource
    );

    // Observed 6: unwinding it cannot succeed — the resource that owns it is
    // gone, so the outcome is unknown.
    assert!(
        obs.rollback_result.is_err(),
        "baseline: rollback of the orphaned handle is expected to fail, got {:?}",
        obs.rollback_result
    );
}

/// CM-73 target behavior. **Known failure** for the whole of P0 (`:1284` permits
/// it); `:1307` requires it to be green in the P3 connection-runtime track.
///
/// The assertions are behavioral on purpose (`:1285` retention clause): they
/// must keep holding after the legacy `ConnectionManager` is deleted, so they
/// never name a legacy function, a legacy struct field, or a map shape.
/// Deleting or weakening any of them to reach green is forbidden.
#[tokio::test]
#[ignore = "CM-73 known failure: the current runtime evicts a resource that still has a registered live transaction handle and silently rebuilds it under the same dbSessionId. connection-management.md CM-73's `- 基线说明` bullet allows this as a known failure and requires it to turn green in the P3 connection-runtime track."]
async fn cm73_registered_live_handle_survives_idle_eviction_and_is_never_reused() {
    let obs = observe_journey().await;
    eprintln!("[CM-73 target] calls={:#?}", obs.calls);

    // T1 — `:1283` does not forbid the sweep from closing a physical resource; it
    // forbids the *conjunction* of a closed physical resource and a handle that
    // is still offered for finishing. Both compliant shapes make the conjunction
    // false: one keeps the resource up for the life of the transaction (nothing
    // is closed, so the first half is false), the other terminates the
    // transaction on the original resource *before* closing it (so no finish call
    // can still be routed afterwards). Only "close first, keep offering the
    // handle" — today's behavior — makes it true.
    assert!(
        !(obs.physical_disconnect_observed && obs.old_handle_finished_after_loss),
        "idle eviction closed the physical resource {} while dbSessionId {} was still handing its transaction to the driver to be finished there",
        obs.transaction_resource,
        obs.db_session_id
    );

    // T2 — The physical loss must never be papered over by rebuilding the same
    // `dbSessionId`. Either the resource stays up (T1) or recovery requires an
    // explicitly new session with a new `dbSessionId`; a silent same-id
    // rebuild that keeps serving queries is neither.
    assert!(
        !(obs.physical_disconnect_observed
            && obs.query_after_evict_ok
            && obs.resource_after_query.is_some()),
        "after losing the physical resource, dbSessionId {} was silently rebuilt on {:?} and kept answering queries",
        obs.db_session_id,
        obs.resource_after_query
    );

    // T3 — Transaction status must track the physical session's liveness: it may
    // not keep reporting Active once the resource is gone, and it may not stop
    // reporting Active while the resource is still up (`:1283` "事务状态不得在
    // 物理 session 丢失后继续报告 Active"). Exactly one of the two must hold, so
    // status is the *negation* of "the physical resource was closed" — under the
    // two compliant shapes respectively: kept up → Active, torn down first →
    // not Active.
    assert_eq!(
        obs.status_after_evict, !obs.physical_disconnect_observed,
        "transaction status ({}) disagrees with whether the physical resource {} is still alive",
        obs.status_after_evict, obs.transaction_resource
    );

    // T4 — The old handle must never be committed or rolled back on a resource
    // other than the one that opened it.
    assert!(
        !obs.transaction_call_misrouted,
        "a transaction handle was delivered to a resource that did not open it; journal: {:#?}",
        obs.calls
    );

    // T5 — Failing closed is conditional: the outcome is unknowable only once the
    // resource the transaction lives on is actually gone. While it is still up,
    // unwinding must succeed (`:1283` — only an unknown outcome becomes
    // OutcomeUnknown/SessionLost, so a *known* outcome may never be reported as
    // unknown); after the loss it must fail, never report success.
    if obs.physical_disconnect_observed {
        assert!(
            obs.rollback_result.is_err(),
            "unwinding a transaction whose physical session was lost must fail closed, got {:?}",
            obs.rollback_result
        );
    } else {
        assert!(
            obs.rollback_result.is_ok(),
            "the physical session {} is still up, so unwinding its transaction must report the real outcome instead of an unknown one, got {:?}",
            obs.transaction_resource, obs.rollback_result
        );
    }
}
