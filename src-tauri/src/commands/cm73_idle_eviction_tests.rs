//! CM-73 idle-sweep invariants that the baseline journey does not pin.
//!
//! Spec: `docs/architecture/platform/connection-management.md` §16.7 的
//! `**CM-73 空闲淘汰与活动事务句柄连续旅程（H）**` (CM-73,
//! H) — the sweep must never act behind a referenced session, must not churn
//! owner bookkeeping, and must not race the `release` path into a double close.
//!
//! `cm73_baseline_tests.rs` records the *single* eviction of one unreferenced
//! session. Everything here is a different question about the same sweep:
//!
//! - does a session that still holds a reference survive repeated sweeps?
//! - does the sweep churn owner bookkeeping across many sessions / many runs?
//! - is `release` authoritative, in both orderings against the sweep?
//! - is the physical resource chain of two consecutive cycles observable?
//! - does a rebuilt session get advertised under the *old* `dbSessionId`?
//! - is a session recovered through the production command path idle-eligible,
//!   i.e. is the sweep's eviction branch reachable outside a hand-built fixture?
//!
//! All assertions are behavioural: they read the driver journal, the command
//! layer, and the public session id. They name no legacy `ConnectionManager`
//! internals, so they must keep holding when §14 retires the old manager — only
//! the fixture (`build_app_state` gives us a `state`) and the command-layer
//! entry points are borrowed, exactly as the spec's journey is.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::query::execute_query_impl;
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

const DB_TYPE: &str = "postgres";

// ── Recording driver ─────────────────────────────────────────────────────────

/// One `connect()` = one physical resource, named in `ConnectionHandle::pool_id`
/// (the runtime never rewrites `pool_id`). `ConnectionHandle::id` is recorded
/// separately because the runtime *does* rewrite it — that is the whole point of
/// the last test in this file.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Connect {
        resource: String,
        connection_id: String,
    },
    Disconnect {
        resource: String,
    },
    Query {
        resource: String,
        connection_id: String,
    },
}

impl Call {
    /// Compact one-line rendering, so a whole journey can be asserted as a list.
    fn tag(&self) -> String {
        match self {
            Call::Connect { resource, .. } => format!("C:{resource}"),
            Call::Disconnect { resource } => format!("D:{resource}"),
            Call::Query { resource, .. } => format!("Q:{resource}"),
        }
    }
}

struct SweepDriver {
    calls: Mutex<Vec<Call>>,
    live: Mutex<HashSet<String>>,
    next: AtomicUsize,
}

impl SweepDriver {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            live: Mutex::new(HashSet::new()),
            next: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("journal").clone()
    }

    fn tags(&self) -> Vec<String> {
        self.calls().iter().map(Call::tag).collect()
    }

    fn connects(&self) -> Vec<String> {
        self.calls()
            .iter()
            .filter_map(|c| match c {
                Call::Connect { resource, .. } => Some(resource.clone()),
                _ => None,
            })
            .collect()
    }

    fn disconnects(&self) -> Vec<String> {
        self.calls()
            .iter()
            .filter_map(|c| match c {
                Call::Disconnect { resource } => Some(resource.clone()),
                _ => None,
            })
            .collect()
    }

    /// The `ConnectionHandle::id` the host presented for each query, in order.
    fn query_connection_ids(&self) -> Vec<String> {
        self.calls()
            .iter()
            .filter_map(|c| match c {
                Call::Query { connection_id, .. } => Some(connection_id.clone()),
                _ => None,
            })
            .collect()
    }
}

#[async_trait]
impl DatabaseDriver for SweepDriver {
    fn driver_type(&self) -> DatabaseType {
        DB_TYPE.into()
    }

    async fn connect(&self, _config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let n = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let resource = format!("res-{n}");
        let handle = ConnectionHandle {
            id: format!("{resource}/conn"),
            pool_id: resource.clone(),
        };
        self.calls.lock().expect("journal").push(Call::Connect {
            resource: resource.clone(),
            connection_id: handle.id.clone(),
        });
        self.live.lock().expect("live").insert(resource);
        Ok(handle)
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        let resource = handle.pool_id.clone();
        self.calls.lock().expect("journal").push(Call::Disconnect {
            resource: resource.clone(),
        });
        self.live.lock().expect("live").remove(&resource);
        Ok(())
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        self.calls.lock().expect("journal").push(Call::Query {
            resource: handle.pool_id.clone(),
            connection_id: handle.id.clone(),
        });
        let _ = sql;
        if !self.live.lock().expect("live").contains(&handle.pool_id) {
            return Err(DriverError::QueryFailed(format!(
                "sweep driver: physical resource {} is closed",
                handle.pool_id
            )));
        }
        Ok(MultiQueryResult {
            results: vec![],
            total_time_ms: 0,
        })
    }

    async fn query(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<QueryResult, DriverError> {
        Err(DriverError::QueryFailed(
            "sweep driver: query is not used here".into(),
        ))
    }

    async fn query_with_params(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        Err(DriverError::QueryFailed(
            "sweep driver: query_with_params is not used here".into(),
        ))
    }

    async fn execute(&self, _handle: &ConnectionHandle, _sql: &str) -> Result<u64, DriverError> {
        Err(DriverError::QueryFailed(
            "sweep driver: execute is not used here".into(),
        ))
    }

    async fn begin_transaction(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Err(DriverError::TransactionError(
            "sweep driver: no transactions here".into(),
        ))
    }

    async fn commit(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError(
            "sweep driver: no transactions here".into(),
        ))
    }

    async fn rollback(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError(
            "sweep driver: no transactions here".into(),
        ))
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
        Err(DriverError::QueryFailed(
            "sweep driver: no schemas here".into(),
        ))
    }

    async fn get_server_info(&self, _handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        Err(DriverError::QueryFailed(
            "sweep driver: no server info here".into(),
        ))
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
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
        Err(DriverError::QueryFailed("sweep driver".into()))
    }
}

// ── Fixture ──────────────────────────────────────────────────────────────────

struct Fixture {
    _keyring: FileKeyringGuard,
    _temp: tempfile::TempDir,
    state: AppState,
    driver: Arc<SweepDriver>,
}

/// Persist one connection config per id, so a single fixture can host several
/// independent sessions.
async fn fixture(connection_ids: &[&str]) -> Fixture {
    let keyring = FileKeyringGuard::set();
    let temp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        Store::init_with_path(temp.path())
            .await
            .expect("store init"),
    );
    let registry = Arc::new(DriverRegistry::new());
    let driver = Arc::new(SweepDriver::new());
    registry.register_test_driver(DB_TYPE, driver.clone()).await;
    let state = build_app_state(store.clone(), registry.clone());
    for id in connection_ids {
        store
            .save_connection(sample_postgres_config(id))
            .await
            .expect("save_connection");
    }
    Fixture {
        _keyring: keyring,
        _temp: temp,
        state,
        driver,
    }
}

impl Fixture {
    /// Push the session past the production idle deadline (30 min) without
    /// touching production code, then run the sweep.
    async fn expire_and_sweep(&self, db_session_id: &str) {
        self.state
            .connection_manager
            .expire_test_session(db_session_id)
            .await;
        self.state
            .connection_manager
            .cleanup_idle_connections()
            .await;
    }

    async fn query(&self, db_session_id: &str, sql: &str) -> Result<(), String> {
        execute_query_impl(
            &self.state,
            db_session_id.to_string(),
            sql.to_string(),
            None,
        )
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// A session that still holds a reference must survive any number of sweeps.
///
/// This is the guard `cleanup_idle_connections` is built on: only ref-count 0
/// sessions are eligible. It is pinned here because the baseline journey only
/// exercises the ref-count 0 side, and a sweep that ignored the guard would
/// close live sessions behind a referenced caller.
#[tokio::test]
async fn idle_sweep_never_closes_a_session_that_still_holds_a_reference() {
    let f = fixture(&["cm73-ref-1"]).await;
    let mgr = &f.state.connection_manager;

    let db_session_id = mgr
        .connect_dedicated("cm73-ref-1", None)
        .await
        .expect("connect_dedicated");
    let resource = f.driver.connects().remove(0);
    assert_eq!(
        mgr.ref_count(&db_session_id).await,
        1,
        "a dedicated session holds one reference"
    );

    for _ in 0..3 {
        f.expire_and_sweep(&db_session_id).await;
    }

    assert_eq!(
        f.driver.disconnects(),
        Vec::<String>::new(),
        "an idle-looking session that still holds a reference must never be closed, journal: {:?}",
        f.driver.tags()
    );
    assert_eq!(
        mgr.ref_count(&db_session_id).await,
        1,
        "the sweep must not touch reference counts"
    );
    assert_eq!(
        mgr.owner_connection_id(&db_session_id).await.as_deref(),
        Some("cm73-ref-1")
    );

    // Still the same physical session, and no silent reconnect behind our back.
    let connects_before = f.driver.connects().len();
    f.query(&db_session_id, "SELECT 1")
        .await
        .expect("a referenced idle session must keep serving");
    assert_eq!(
        f.driver.connects().len(),
        connects_before,
        "the session was rebuilt although it was never evicted"
    );
    assert_eq!(
        f.driver.tags().last().map(String::as_str),
        Some(format!("Q:{resource}").as_str()),
        "the query must land on the original physical resource, journal: {:?}",
        f.driver.tags()
    );
}

/// One sweep, many unreferenced sessions: exactly one close each, exactly one
/// owner entry each, and a second sweep changes nothing.
///
/// The baseline journey evicts a single session, so it cannot see a sweep that
/// leaks owner entries (monotonic growth) or one that re-closes on every run.
#[tokio::test]
async fn idle_sweep_closes_each_unreferenced_session_once_and_keeps_one_owner_entry_each() {
    let f = fixture(&["cm73-many-1", "cm73-many-2", "cm73-many-3"]).await;
    let mgr = &f.state.connection_manager;

    let mut sessions = Vec::new();
    for id in ["cm73-many-1", "cm73-many-2", "cm73-many-3"] {
        sessions.push(mgr.connect(id).await.expect("connect"));
    }
    assert_eq!(
        f.driver.connects().len(),
        3,
        "three distinct physical resources"
    );
    assert_eq!(
        mgr.session_owner_map_len().await,
        3,
        "one owner entry per established session"
    );

    for db_session_id in &sessions {
        f.expire_and_sweep(db_session_id).await;
    }

    assert_eq!(
        f.driver.disconnects().len(),
        3,
        "each unreferenced session must be closed exactly once, journal: {:?}",
        f.driver.tags()
    );
    for (index, db_session_id) in sessions.iter().enumerate() {
        assert_eq!(
            mgr.owner_connection_id(db_session_id).await.as_deref(),
            Some(format!("cm73-many-{}", index + 1).as_str()),
            "the owner entry of an evicted session must survive so recovery is possible"
        );
    }
    assert_eq!(
        mgr.session_owner_map_len().await,
        3,
        "one owner entry per evicted session"
    );

    // A second sweep over the same (now connection-less) sessions must be a
    // no-op: no extra closes, no owner-map growth, no shrink either.
    mgr.cleanup_idle_connections().await;
    mgr.cleanup_idle_connections().await;
    assert_eq!(
        f.driver.disconnects().len(),
        3,
        "repeating the sweep must not re-close or resurrect sessions, journal: {:?}",
        f.driver.tags()
    );
    assert_eq!(
        mgr.session_owner_map_len().await,
        3,
        "the owner map must not drift on a no-op sweep"
    );

    // Owner entries are not immortal: an explicit teardown still reclaims them.
    mgr.disconnect(&sessions[0]).await.expect("disconnect");
    assert_eq!(
        mgr.session_owner_map_len().await,
        2,
        "an explicit disconnect reclaims its owner entry"
    );
    assert!(mgr.owner_connection_id(&sessions[0]).await.is_none());
}

/// Ordering A: the sweep runs first and must hold off, then `release` closes
/// the session exactly once and reclaims its owner entry.
#[tokio::test]
async fn release_after_the_sweep_is_the_only_close_of_a_referenced_session() {
    let f = fixture(&["cm73-order-a"]).await;
    let mgr = &f.state.connection_manager;

    let db_session_id = mgr
        .connect_dedicated("cm73-order-a", None)
        .await
        .expect("connect_dedicated");
    let resource = f.driver.connects().remove(0);

    for _ in 0..2 {
        f.expire_and_sweep(&db_session_id).await;
    }
    assert_eq!(
        f.driver.disconnects(),
        Vec::<String>::new(),
        "the sweep must not close a referenced session"
    );
    f.query(&db_session_id, "SELECT 1")
        .await
        .expect("the session is still up after the sweeps");

    let released = mgr.release(&db_session_id).await.expect("release");
    assert!(released, "release reports that it closed the session");

    assert_eq!(
        f.driver.disconnects(),
        vec![resource.clone()],
        "release must close the physical resource exactly once, journal: {:?}",
        f.driver.tags()
    );
    assert_eq!(mgr.ref_count(&db_session_id).await, 0);
    assert_eq!(
        mgr.session_owner_map_len().await,
        0,
        "release reclaims the owner entry"
    );
    assert!(mgr.owner_connection_id(&db_session_id).await.is_none());

    mgr.cleanup_idle_connections().await;
    assert_eq!(
        f.driver.disconnects(),
        vec![resource],
        "a later sweep must not close an already released session a second time"
    );
}

/// Ordering B: `release` runs first and is authoritative; the sweep that follows
/// must not double-close, resurrect, or leave a stale owner entry behind.
#[tokio::test]
async fn a_sweep_after_release_does_not_close_the_session_twice() {
    let f = fixture(&["cm73-order-b"]).await;
    let mgr = &f.state.connection_manager;

    let db_session_id = mgr
        .connect_dedicated("cm73-order-b", None)
        .await
        .expect("connect_dedicated");
    let resource = f.driver.connects().remove(0);

    // Expired, but still referenced: the reference is the only thing standing
    // between the session and the sweep. Release it, then let the sweep run.
    f.state
        .connection_manager
        .expire_test_session(&db_session_id)
        .await;
    assert!(mgr.release(&db_session_id).await.expect("release"));
    assert_eq!(
        f.driver.disconnects(),
        vec![resource.clone()],
        "release performed the close"
    );

    for _ in 0..2 {
        mgr.cleanup_idle_connections().await;
    }

    assert_eq!(
        f.driver.disconnects(),
        vec![resource],
        "the sweep must not re-close a session release already tore down, journal: {:?}",
        f.driver.tags()
    );
    assert_eq!(
        mgr.session_owner_map_len().await,
        0,
        "no owner entry may be left behind"
    );
    assert_eq!(mgr.ref_count(&db_session_id).await, 0);
    assert_eq!(
        f.driver.connects().len(),
        1,
        "the torn-down session must not be silently rebuilt"
    );

    // A query on the released id must not resurrect anything behind the user's back.
    assert!(
        f.query(&db_session_id, "SELECT 1").await.is_err(),
        "a released session must not come back as a same-id rebuild"
    );
    assert_eq!(
        f.driver.connects().len(),
        1,
        "the failed query must not have connected anything"
    );
}

/// Two consecutive eviction→rebuild cycles of the *same* `dbSessionId` must leave
/// a fully observable resource chain: each cycle closes the previous physical
/// resource before opening the next one, and exactly one resource is live at
/// the end. A chain that skipped a close, or left a zombie live, is what makes
/// CM-73's "记录物理 resourceId" evidence unreadable later.
#[tokio::test]
async fn two_consecutive_eviction_cycles_of_one_session_id_are_fully_observable() {
    let f = fixture(&["cm73-chain"]).await;
    let mgr = &f.state.connection_manager;

    let db_session_id = mgr.connect("cm73-chain").await.expect("connect");

    for cycle in 1..=2u32 {
        let connects_before = f.driver.connects().len();
        f.query(&db_session_id, "SELECT 1")
            .await
            .unwrap_or_else(|e| panic!("cycle {cycle}: a live session must serve queries: {e}"));
        // Cycle 1 runs on the session `connect` just opened, so it must not
        // rebuild. Every later cycle runs on a session the previous sweep
        // closed, so the recovery is exactly one new physical resource.
        let expected_connects = if cycle == 1 {
            connects_before
        } else {
            connects_before + 1
        };
        assert_eq!(
            f.driver.connects().len(),
            expected_connects,
            "cycle {cycle}: exactly one physical resource per cycle is expected, journal: {:?}",
            f.driver.tags()
        );

        f.expire_and_sweep(&db_session_id).await;
        assert_eq!(
            f.driver.disconnects().len(),
            cycle as usize,
            "cycle {cycle}: exactly one physical resource must have been closed, journal: {:?}",
            f.driver.tags()
        );
    }

    // 1 initial connect + exactly 1 rebuild for the second cycle.
    assert_eq!(
        f.driver.connects().len(),
        2,
        "one rebuild per eviction, journal: {:?}",
        f.driver.tags()
    );
    let live: HashSet<String> = f.driver.live.lock().expect("live").clone();
    assert!(
        live.is_empty(),
        "the last sweep must not leave a physical resource live, journal: {:?}",
        f.driver.tags()
    );

    // Ordering: the chain reads C:res-1, D:res-1, C:res-2, D:res-2 — each
    // resource is closed before its successor is opened, so a reader can
    // attribute every call to a resource that was actually alive.
    let actual: Vec<String> = f
        .driver
        .tags()
        .into_iter()
        .filter(|t| !t.starts_with("Q:"))
        .collect();
    assert_eq!(
        actual,
        vec!["C:res-1", "D:res-1", "C:res-2", "D:res-2"],
        "the physical resource chain is not observable in order"
    );
}

/// CM-73 target evidence, known failure until the P3 connection-runtime track
/// (`connection-management.md` §16.7 的 `**CM-73 空闲淘汰与活动事务句柄连续旅程（H）**`)。
///
/// After the physical session is lost, the runtime must not reconnect and then
/// hand the caller back the *same* `dbSessionId` for a brand-new physical
/// resource. Today the rebuilt `ConnectionHandle` is re-labelled with the old
/// session id, so a caller that only knows `dbSessionId` cannot tell a recovered
/// session from the one that was lost — which is precisely the silent same-id
/// rebuild the spec forbids. The assertion reads the id the host *presented to
/// the driver*, so it holds no matter how the recovery is implemented.
#[tokio::test]
#[ignore = "CM-73 known failure: the rebuilt session is advertised under the old dbSessionId, so a caller cannot distinguish it from the session that was lost. connection-management.md CM-73's `- 断言` bullet forbids silently keeping the same id, and its `- 基线说明` bullet allows the known failure and requires green in the P3 connection-runtime track."]
async fn a_rebuilt_session_is_never_advertised_under_the_pre_loss_session_id() {
    let f = fixture(&["cm73-same-id"]).await;
    let mgr = &f.state.connection_manager;

    let db_session_id = mgr.connect("cm73-same-id").await.expect("connect");
    let first_resource = f.driver.connects().remove(0);
    f.query(&db_session_id, "SELECT 1")
        .await
        .expect("the freshly opened session must serve queries");

    f.expire_and_sweep(&db_session_id).await;
    assert_eq!(
        f.driver.disconnects(),
        vec![first_resource.clone()],
        "precondition: the physical resource was closed"
    );

    let rebuilt = f.query(&db_session_id, "SELECT 1").await;
    assert!(
        rebuilt.is_ok(),
        "this test only pins the *identity* of a recovered session; recovering at all is a separate question, got {rebuilt:?}"
    );
    assert_eq!(
        f.driver.connects().len(),
        2,
        "precondition: a rebuild opened a new physical resource, journal: {:?}",
        f.driver.tags()
    );

    let presented = f.driver.query_connection_ids();
    assert_eq!(
        presented.len(),
        2,
        "both the pre-loss and the post-loss query must be journaled, journal: {:?}",
        f.driver.tags()
    );
    assert_ne!(
        presented[0], presented[1],
        "a new physical resource was opened but the host still presented {} for it; a caller holding only the dbSessionId cannot tell the rebuilt session from the lost one",
        presented[0]
    );
}

/// Reachability proof (executable, not reasoning): a session recovered through the
/// **production** command path lands in the sweep's eligible state and is really
/// evicted by it.
///
/// Why this needs its own test: `cleanup_idle_connections` only closes a session
/// whose `ref_counts` entry is 0 *and* whose `last_used` is older than the idle
/// timeout. A fixture built with `ConnectionManager::connect` only exercises that
/// state for the connection a caller opened, which is not the path a real caller
/// takes. `execute_query_impl` resolves its driver through
/// `resolve_command_driver` → `get_session`, and `get_session` falls through to
/// `reconnect()` for a session id the sweep already closed. `reconnect` re-inserts
/// into `connections` **without taking a reference**, so the state it produces is
/// "live physical connection, zero references" — exactly what the sweep exists to
/// collect, and the state the CM-73 journey is about.
///
/// The recovery below is an ordinary query on a `dbSessionId`; the only test-only
/// tool is the `expire_test_session` deadline helper the rest of this file already
/// uses to cross the 30-minute idle timeout without waiting for it.
#[tokio::test]
async fn a_session_recovered_through_the_host_command_path_is_still_swept_when_it_goes_idle() {
    let f = fixture(&["cm73-reachable"]).await;
    let mgr = &f.state.connection_manager;

    let db_session_id = mgr.connect("cm73-reachable").await.expect("connect");
    assert_eq!(
        f.driver.connects(),
        vec!["res-1".to_string()],
        "precondition: opening the session connects exactly one physical resource, journal: {:?}",
        f.driver.tags()
    );

    f.expire_and_sweep(&db_session_id).await;
    assert_eq!(
        f.driver.disconnects(),
        vec!["res-1".to_string()],
        "precondition: the first sweep closes that physical resource, journal: {:?}",
        f.driver.tags()
    );

    // Production recovery path — `execute_query_impl` → `resolve_command_driver` →
    // `get_session` → miss → `reconnect`. No direct `ConnectionManager::connect`,
    // so whatever the query observes is what a real caller observes.
    f.query(&db_session_id, "SELECT 1")
        .await
        .unwrap_or_else(|e| {
            panic!("the query that triggers recovery must succeed on the rebuilt session: {e}")
        });
    assert_eq!(
        f.driver.connects(),
        vec!["res-1".to_string(), "res-2".to_string()],
        "the query must have rebuilt the session on a brand-new physical resource, journal: {:?}",
        f.driver.tags()
    );

    // The state the sweep's predicate keys on, measured rather than assumed.
    assert_eq!(
        mgr.ref_count(&db_session_id).await,
        0,
        "`reconnect` must not take a reference, otherwise a recovered session could never become idle-eligible"
    );

    // Therefore the next sweep must close it.
    f.expire_and_sweep(&db_session_id).await;
    assert_eq!(
        f.driver.disconnects(),
        vec!["res-1".to_string(), "res-2".to_string()],
        "a session recovered at reference count 0 is idle-eligible and must be closed by the sweep; journal: {:?}",
        f.driver.tags()
    );
}
