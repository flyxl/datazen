//! Live evidence for the Redis resource provider, against a real server.
//!
//! `tests.rs` and `tests_behaviour.rs` prove the contract *shape* offline, and
//! they are honest about their limit: their seeds carry no socket, so every
//! method that must talk to a server fails there. That is deliberate — it is how
//! "returns `Ok` when it could not reach the server" is caught — but it leaves
//! one claim with no evidence behind it. `change_context` returns
//! [`ContextChangeDisposition::Confirmed`], and it says in a comment that the
//! `SELECT` really happened. Offline, all that could be shown was the *failure*
//! path. The success path — the one a real caller depends on — was untested.
//!
//! This file is that missing evidence. Everything below runs against a genuine
//! `redis-server` process, and every assertion is anchored to something the
//! server can contradict.
//!
//! ## Where the server comes from
//!
//! [`crate::live_server`] starts a private `redis-server` child: its own port,
//! its own data directory, persistence off, killed and cleaned up on drop. That
//! module holds the reasoning for why a child process and not a container, a
//! reused 6379, or a fake RESP server, plus the one and only skip condition
//! (`redis-server` not installed). It is crate-level because
//! `connect::tests` uses the same harness, which is how that module's
//! `#[ignore]`'d live test became runnable.
//!
//! ## How a claim gets anchored
//!
//! [`Observer`] is a second, independent `redis` client that the provider knows
//! nothing about. It is the witness: when the provider's own connection writes
//! `live:probe` and the observer then `SELECT`s onto that database by hand and
//! reads the value back, the data provably reached the server. When a
//! `change_context` is followed by the observer reading `nil` in the database
//! the session just left, the `SELECT` provably moved it. Neither fact can be
//! produced by a stub on the provider side.
//!
//! ## Manual reproduction
//!
//! Every test prints the exact port it used, so the run can be repeated by
//! hand:
//!
//! ```sh
//! d=$(mktemp -d)
//! redis-server --port 6390 --dir "$d" --save "" --appendonly no --daemonize no
//! redis-cli -p 6390 -n 3 SET live:probe hello
//! redis-cli -p 6390 -n 3 GET live:probe   # -> "hello"
//! redis-cli -p 6390 -n 7 GET live:probe   # -> (nil)
//! ```

use std::sync::Arc;

use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::{Baseline, ResourceError, ResourceHandle, ResourceProvider};
use datazen_driver_api::session::{
    CloseDisposition, ContextChangeDisposition, ObservationConfidence, ResetDisposition,
};
use datazen_driver_api::{DatabaseDriver, DriverError, QueryResult, Value};

use crate::driver::RedisDriver;
use crate::RedisResourceProvider;

use super::tests::{
    acquire_request, config_on, connection_handle, interactive_scope, ledger_port, BudgetLedger,
};

use crate::live_server::LiveRedis;

// ---------------------------------------------------------------------------
// The witness
// ---------------------------------------------------------------------------

/// An independent Redis client, which the provider shares nothing with.
///
/// Every claim below is checked against this rather than against the provider's
/// own bookkeeping. If the provider and the observer ever agree because both
/// are wrong in the same way, the fact that this one dials the server from
/// scratch is what breaks the tie.
struct Observer {
    conn: redis::aio::MultiplexedConnection,
}

impl Observer {
    async fn connect(server: &LiveRedis) -> Result<Self, String> {
        let client = redis::Client::open(server.url())
            .map_err(|e| format!("observer could not parse {}: {e}", server.url()))?;
        let conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("observer could not connect to {}: {e}", server.url()))?;
        Ok(Self { conn })
    }

    /// Move *this* connection onto `db_index`, so every later read is anchored
    /// to a database chosen by the witness rather than by the code under test.
    async fn select_db(&mut self, db_index: u32) -> Result<(), String> {
        redis::cmd("SELECT")
            .arg(db_index)
            .query_async::<()>(&mut self.conn)
            .await
            .map_err(|e| format!("observer SELECT {db_index} failed: {e}"))
    }

    async fn get(&mut self, key: &str) -> Result<Option<String>, String> {
        redis::cmd("GET")
            .arg(key)
            .query_async::<Option<String>>(&mut self.conn)
            .await
            .map_err(|e| format!("observer GET {key} failed: {e}"))
    }

    async fn dbsize(&mut self) -> Result<i64, String> {
        redis::cmd("DBSIZE")
            .query_async::<i64>(&mut self.conn)
            .await
            .map_err(|e| format!("observer DBSIZE failed: {e}"))
    }
}

// ---------------------------------------------------------------------------
// A real acquired resource
// ---------------------------------------------------------------------------

/// A provider, a real server, and one resource acquired through the real
/// acquisition path — `validate_acquisition`, the budget, `open_session` and
/// the initial `SELECT`, all of it. Not the `seed()` the offline suite uses:
/// that one registers a handle with no connection, which is the whole reason
/// the offline `SELECT` cannot succeed.
struct Session {
    // Dropped last, so the server outlives every connection made through it.
    _server: LiveRedis,
    provider: RedisResourceProvider,
    ledger: Arc<BudgetLedger>,
    handle: ResourceHandle,
}

impl Session {
    /// Start everything, or explain that Redis is not installed.
    async fn start() -> Result<Option<Self>, String> {
        let server = match LiveRedis::start()? {
            Some(server) => server,
            None => return Ok(None),
        };
        server.wait_until_serving().await?;
        let port = server.port();

        let provider = RedisResourceProvider::new(Arc::new(RedisDriver::new()));
        let ledger = Arc::new(BudgetLedger::default());
        let handle = provider
            .acquire_resource(
                &acquire_request(&config_on(port), interactive_scope()),
                &ledger_port(&ledger),
            )
            .await
            .map_err(|e| {
                format!("acquire_resource against the live server on port {port} failed: {e}")
            })?;

        Ok(Some(Self {
            _server: server,
            provider,
            ledger,
            handle,
        }))
    }

    async fn observer(&self) -> Observer {
        Observer::connect(&self._server)
            .await
            .expect("the live server answered PING, so a second connection to it must work")
    }

    /// Issue a command on the provider's **own** connection.
    ///
    /// `DatabaseDriver::query` is the only read/write path in this crate that
    /// reaches the resource's socket with arbitrary arguments. The Driver
    /// Command API is not usable as a probe: `payload::decode_command_result`
    /// accepts only a `MultiQueryResult` or a single-key `{"rowsAffected": n}`
    /// object, and no Redis command in this crate produces either — so
    /// `execute_on_resource` rightly refuses free-form Redis payloads, and the
    /// oracle has to sit beside it rather than inside it.
    async fn on_provider_connection(&self, sql: &str) -> Result<QueryResult, DriverError> {
        let connection = connection_handle(self.handle.resource_key());
        self.provider.driver().query(&connection, sql).await
    }

    /// `SET` through the provider's connection.
    async fn write(&self, key: &str, value: &str) -> Result<(), DriverError> {
        self.on_provider_connection(&format!("SET {key} {value}"))
            .await?;
        Ok(())
    }

    /// `GET` through the provider's connection. `None` is a nil reply, which is
    /// a value in its own right and never an error.
    async fn read(&self, key: &str) -> Result<Option<String>, DriverError> {
        Ok(single_value(
            &self.on_provider_connection(&format!("GET {key}")).await?,
        ))
    }
}

/// The one cell of a `GET` reply, read exactly.
///
/// A nil reply arrives as `Value::Null` and a hit as `Value::String`; anything
/// else means the server answered with a shape this oracle does not model, and
/// that is a failure worth naming rather than a `None` to coerce.
fn single_value(result: &QueryResult) -> Option<String> {
    match result
        .rows
        .first()
        .and_then(|row| row.first())
        .cloned()
        .flatten()
    {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s),
        Some(other) => panic!("expected a bulk string or nil from GET, got {other:?}"),
    }
}

/// Skip a test with its reason printed, rather than passing silently.
macro_rules! session_or_skip {
    () => {
        match Session::start().await {
            Ok(Some(session)) => session,
            Ok(None) => {
                eprintln!(
                    "SKIPPED: no `redis-server` on PATH. Set {} to point at one. \
                     The live suite needs a real Redis to have anything to prove.",
                    crate::live_server::BINARY_ENV
                );
                return;
            }
            Err(reason) => panic!("{reason}"),
        }
    };
}

// ---------------------------------------------------------------------------
// change_context: the claim that had no evidence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn change_context_really_moves_the_connection_and_the_server_agrees() {
    let session = session_or_skip!();
    let mut observer = session.observer().await;

    // The session was acquired onto db3, and the provider's own connection is
    // attached there. Prove the connection works by writing through it.
    session
        .write("live:probe", "db3-value")
        .await
        .expect("the provider's own connection must accept a SET");
    assert_eq!(
        session
            .read("live:probe")
            .await
            .expect("GET must reach the server"),
        Some("db3-value".to_string()),
        "the resource's own connection must read back what it just wrote"
    );

    // Independent confirmation that the write really landed in **db3** and not
    // somewhere the provider merely believes it went.
    observer.select_db(3).await.unwrap();
    assert_eq!(
        observer.get("live:probe").await.unwrap(),
        Some("db3-value".to_string()),
        "a separate client must find the provider's write in db3 on the server"
    );

    // The claim under test.
    let disposition = session
        .provider
        .change_context(
            &session.handle,
            &NamespaceTarget::empty().with_database("db7"),
        )
        .await
        .expect("SELECT 7 must succeed against a server with the default 16 databases");
    assert_eq!(
        disposition,
        ContextChangeDisposition::Confirmed,
        "an in-place SELECT that the server accepted is Confirmed, never RequiresReplacement"
    );

    // Same connection, same handle — and the key is gone, because the session
    // is on a different database.
    assert_eq!(
        session
            .read("live:probe")
            .await
            .expect("GET must reach the server"),
        None,
        "after SELECT 7 the resource's own connection must no longer see a db3 key"
    );

    // The witness says db7 is a genuinely different database, not an empty one.
    observer.select_db(7).await.unwrap();
    assert_eq!(
        observer.get("live:probe").await.unwrap(),
        None,
        "db7 must not contain the db3 key"
    );

    // And the switch is two-way, on the same socket: writing in db7 and coming
    // back must find the original value still there.
    session
        .write("live:probe", "db7-value")
        .await
        .expect("the provider's connection must accept a SET on db7");
    observer.select_db(3).await.unwrap();
    assert_eq!(
        observer.get("live:probe").await.unwrap(),
        Some("db3-value".to_string()),
        "the db7 write must not have disturbed db3"
    );

    assert_eq!(
        session
            .provider
            .change_context(
                &session.handle,
                &NamespaceTarget::empty().with_database("db3")
            )
            .await
            .expect("SELECT 3 must succeed"),
        ContextChangeDisposition::Confirmed
    );
    assert_eq!(
        session
            .read("live:probe")
            .await
            .expect("GET must reach the server"),
        Some("db3-value".to_string()),
        "switching back must find the original value: the connection moved, it was not reissued"
    );
}

#[tokio::test]
async fn change_context_to_a_database_the_server_refuses_keeps_the_session_where_it_was() {
    let session = session_or_skip!();
    let mut observer = session.observer().await;

    session
        .write("live:range", "still-here")
        .await
        .expect("the provider's own connection must accept a SET");
    observer.select_db(3).await.unwrap();
    assert_eq!(
        observer.get("live:range").await.unwrap(),
        Some("still-here".into())
    );

    // `db999` parses as a number, so the provider accepts the target and the
    // *server* is what refuses it: `-ERR DB index is out of range`. This is the
    // case the offline suite cannot reach at all.
    let outcome = session
        .provider
        .change_context(
            &session.handle,
            &NamespaceTarget::empty().with_database("db999"),
        )
        .await;
    assert!(
        matches!(&outcome, Err(ResourceError::Driver(_))),
        "an out-of-range SELECT must surface the server's refusal as an error, got {outcome:?}"
    );

    // A refused switch must not move the session: the value is still where it
    // was, which is the whole reason `change_context` validates before it
    // touches the connection.
    assert_eq!(
        session
            .read("live:range")
            .await
            .expect("GET must reach the server"),
        Some("still-here".to_string()),
        "a rejected context switch must leave the session on its original database"
    );
}

// ---------------------------------------------------------------------------
// reset and close: claims that were only ever comments
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reset_reports_discard_and_does_not_flush_the_database() {
    let session = session_or_skip!();
    let mut observer = session.observer().await;

    session
        .write("live:reset", "keep-me")
        .await
        .expect("the provider's own connection must accept a SET");
    observer.select_db(3).await.unwrap();
    let before = observer.dbsize().await.unwrap();
    assert!(before > 0, "db3 must hold the key before the reset");

    let disposition = session
        .provider
        .reset_resource(&session.handle, &Baseline::new(Vec::new()))
        .await
        .expect("reset_resource must not fail for an empty baseline");
    assert_eq!(
        disposition,
        ResetDisposition::Discard,
        "there is nothing to restore into a fresh connection, so the honest answer is Discard"
    );

    // `FLUSHDB` is destructive and is never run as a reset. Before this test
    // that was a comment; here the server is the witness.
    assert_eq!(
        observer.dbsize().await.unwrap(),
        before,
        "reset_resource must not empty the logical database"
    );
    assert_eq!(
        observer.get("live:reset").await.unwrap(),
        Some("keep-me".to_string()),
        "the key written before the reset must survive it"
    );
}

#[tokio::test]
async fn close_really_drops_the_socket_and_releases_the_budget_once() {
    let session = session_or_skip!();
    let handle = session.handle.clone();
    let key = handle.resource_key().to_string();

    assert_eq!(
        session.ledger.acquired(),
        vec![1],
        "acquiring a Redis resource costs exactly one physical connection"
    );
    assert_eq!(
        session.provider.driver().connections.read().await.len(),
        1,
        "the driver holds the resource's one connection while it is open"
    );
    assert!(
        session.ledger.released().is_empty(),
        "nothing may be released before close"
    );

    let disposition = session
        .provider
        .close_resource(&handle)
        .await
        .expect("closing a live resource must succeed");
    assert_eq!(
        disposition,
        CloseDisposition::Closed,
        "disconnect succeeded, so the close is confirmed"
    );
    assert_eq!(
        session.ledger.released(),
        vec!["permit-1".to_string()],
        "the permit must be released exactly once"
    );
    assert!(
        !session
            .provider
            .driver()
            .connections
            .read()
            .await
            .contains_key(&key),
        "the driver must no longer hold the resource's connection after close"
    );

    // The contract says close is idempotent, and the way it earns that is the
    // `closed` set: the permit went with the first call, so a second close
    // cannot release it a second time. That is the assertion worth making — a
    // second `release_physical_connections` is the bug the set exists for.
    assert_eq!(
        session
            .provider
            .close_resource(&handle)
            .await
            .expect("a second close of an already-closed resource is idempotent, not an error"),
        CloseDisposition::Closed,
        "the resource is closed, and saying so again is honest rather than a second release"
    );
    assert_eq!(
        session.ledger.released(),
        vec!["permit-1".to_string()],
        "the permit must still be released exactly once after a second close"
    );
}

// ---------------------------------------------------------------------------
// Observation: a round trip that must really happen
// ---------------------------------------------------------------------------

#[tokio::test]
async fn observe_session_reports_a_healthy_partial_context_after_a_real_round_trip() {
    let session = session_or_skip!();

    let observation = session
        .provider
        .observe_session(&session.handle)
        .await
        .expect("INFO server on the resource's own connection must answer");

    assert!(
        observation.protocol_drained,
        "the probe completed, so the protocol is drained"
    );
    assert_eq!(
        observation.state,
        datazen_driver_api::session::SessionState::Ready,
        "the session just answered INFO server on its own connection"
    );
    assert_eq!(
        observation.resource_health,
        datazen_driver_api::session::ResourceHealth::Healthy,
        "a probe that succeeded is the evidence for Healthy"
    );
    // The namespace stays empty at Partial on purpose (see `observation.rs`):
    // Redis never tells a client which database it is attached to. This test
    // records that the round trip happened without pretending it read the
    // context back.
    let namespace = &observation.context.namespace;
    assert!(
        namespace.database.is_none()
            && namespace.catalog.is_none()
            && namespace.schema.is_none()
            && namespace.path.is_empty(),
        "Redis cannot report its current database, so the observed namespace must stay empty, \
         got {namespace:?}"
    );
    assert_eq!(
        observation.context.confidence,
        ObservationConfidence::Partial,
        "an empty namespace must not be reported at Confirmed confidence"
    );
    assert!(
        !observation.context.confidence.is_confirmed(),
        "`is_confirmed` is the gate that keeps an unobserved namespace out of the UI as a fact"
    );
}
