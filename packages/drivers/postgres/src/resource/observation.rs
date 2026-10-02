//! Reading the session back off the server — the evidence behind every
//! observation this provider returns.
//!
//! Nothing here fills a field with a plausible value. Either the server answered
//! a question or the field stays empty:
//!
//! * **pinned** — a transaction holds a real `PoolConnection`
//!   (`execution.rs:915-947`), the read runs on *that* connection, and the
//!   result is [`ObservationConfidence::Confirmed`].
//! * **leased** — the read runs on one backend of the pool. `current_database`
//!   is a pool constant and is reported, but `search_path` can be changed by a
//!   statement on that same backend, and the next execution may land on a
//!   different backend, so the context is [`ObservationConfidence::Partial`] and
//!   the mutable fields stay empty.
//!
//! The query asks only for facts that have somewhere to go. `backend_pid` and
//! `transaction_isolation` are deliberately **not** read: the contract has no
//! field for either, this provider declares isolation levels unsupported, and a
//! read whose result is thrown away is not evidence.

use std::collections::HashMap;

use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::ResourceError;
use datazen_driver_api::session::{ObservationConfidence, SessionContext, TransactionState};
use datazen_driver_api::DriverError;
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgPool;
use sqlx::{Executor, Postgres, Row};

/// One round trip, three facts, all of which the session contract can carry.
const SESSION_FACTS_SQL: &str = "SELECT current_database() AS database_name, \
     current_user AS effective_identity, \
     current_setting('search_path') AS search_path";

/// What the server said about the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionFacts {
    pub database: String,
    pub effective_identity: String,
    /// The parsed `search_path`. `$user` tokens are dropped — they name the
    /// role's own schema, not a schema the caller can address.
    pub search_path: Vec<String>,
}

impl SessionFacts {
    /// The first addressable entry of `search_path`, i.e. the schema an
    /// unqualified name resolves to.
    pub(crate) fn effective_schema(&self) -> Option<String> {
        self.search_path.first().cloned()
    }
}

/// Read the session on the connection a transaction is pinned to.
///
/// This takes the driver's `transactions` lock for the duration of one round
/// trip. That is deliberate and unavoidable: a `PoolConnection` is not cheaply
/// cloneable, so the pinned connection can only be read while the map that owns
/// it is locked. The hold is a single `SELECT` — no other statement can be
/// issued against that session in the meantime, which is exactly what makes the
/// result [`ObservationConfidence::Confirmed`].
pub(crate) async fn read_pinned_session(
    transactions: &tokio::sync::Mutex<HashMap<String, PoolConnection<Postgres>>>,
    connection_id: &str,
) -> Result<SessionFacts, ResourceError> {
    let mut guard = transactions.lock().await;
    let connection =
        guard
            .get_mut(connection_id)
            .ok_or_else(|| ResourceError::InvalidResourceState {
                resource_key: connection_id.to_string(),
                operation: "observe_session".to_string(),
                state: "no transaction is open on this session, so there is no pinned \
                    connection to read"
                    .to_string(),
            })?;
    fetch_facts(&mut **connection).await
}

/// Read the session on one backend borrowed from the pool.
///
/// The backend goes back to the pool when the read finishes, so the caller
/// learns about *a* PostgreSQL session, not about the one its next execution
/// will use.
pub(crate) async fn read_pooled_session(pool: &PgPool) -> Result<SessionFacts, ResourceError> {
    let mut connection = pool.acquire().await.map_err(|error| {
        ResourceError::Driver(DriverError::ConnectionFailed(format!(
            "could not borrow a backend to observe the session: {error}"
        )))
    })?;
    fetch_facts(&mut *connection).await
}

/// One generic read, serving both the pinned and the leased path.
async fn fetch_facts<'e, E>(executor: E) -> Result<SessionFacts, ResourceError>
where
    E: Executor<'e, Database = Postgres>,
{
    let row = sqlx::query(SESSION_FACTS_SQL)
        .fetch_one(executor)
        .await
        .map_err(|error| {
            ResourceError::Driver(DriverError::QueryFailed(format!(
                "session observation query failed: {error}"
            )))
        })?;

    let database: String = row.try_get("database_name").map_err(column_error)?;
    let effective_identity: String = row.try_get("effective_identity").map_err(column_error)?;
    let raw_search_path: String = row.try_get("search_path").map_err(column_error)?;

    Ok(SessionFacts {
        database,
        effective_identity,
        search_path: parse_search_path(&raw_search_path),
    })
}

fn column_error(error: sqlx::Error) -> ResourceError {
    ResourceError::Driver(DriverError::QueryFailed(format!(
        "session observation column could not be read: {error}"
    )))
}

/// Turn `"$user", public, "MySchema"` into `["public", "MySchema"]`.
///
/// Quoting is stripped: the raw setting quotes entries that need it, and the
/// caller addresses the identifier without the quotes. `$user` is a token, not a
/// schema name, so it is dropped rather than reported as a level.
pub(crate) fn parse_search_path(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|entry| {
            let trimmed = entry.trim();
            let quoted = trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"');
            if quoted {
                trimmed[1..trimmed.len() - 1].to_string()
            } else {
                trimmed.to_string()
            }
        })
        .filter(|entry| !entry.is_empty() && !entry.starts_with('$'))
        .collect()
}

/// The context of a session whose transaction pinned one physical connection.
///
/// Every field was read back on the connection that will run the next
/// statement, so the whole context is [`ObservationConfidence::Confirmed`] and
/// `SessionContext::matches_target` can be used against it.
pub(crate) fn pinned_context(facts: &SessionFacts) -> SessionContext {
    SessionContext {
        namespace: NamespaceTarget {
            database: Some(facts.database.clone()),
            // PostgreSQL has no separate catalog level: `namespace_shape`
            // declares it absent, so a context never invents one.
            catalog: None,
            schema: facts.effective_schema(),
            path: Vec::new(),
        },
        search_path: facts.search_path.clone(),
        effective_identity: Some(facts.effective_identity.clone()),
        // `BEGIN` has been sent on this exact connection and no other
        // statement can run on it until the transaction ends.
        transaction_state: TransactionState::Active,
        autocommit: Some(false),
        confidence: ObservationConfidence::Confirmed,
    }
}

/// The context of a leased session, read from one pooled backend.
///
/// `current_database` is a pool constant and is reported. `search_path` and the
/// transaction state are deliberately left empty: any statement on that backend
/// can change them, and the caller's next execution may not land on that backend
/// at all. The confidence is [`ObservationConfidence::Partial`], which
/// `matches_target` refuses, so a caller cannot mistake this for a confirmed
/// answer.
pub(crate) fn leased_context(facts: &SessionFacts) -> SessionContext {
    SessionContext {
        namespace: NamespaceTarget {
            database: Some(facts.database.clone()),
            catalog: None,
            schema: None,
            path: Vec::new(),
        },
        search_path: Vec::new(),
        effective_identity: Some(facts.effective_identity.clone()),
        transaction_state: TransactionState::Unknown,
        autocommit: None,
        confidence: ObservationConfidence::Partial,
    }
}
