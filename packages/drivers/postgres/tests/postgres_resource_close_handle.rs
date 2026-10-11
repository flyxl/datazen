//! PostgreSQL's per-handle close (§6.5), seen from outside the crate.
//!
//! Split out of `postgres_resource_contract.rs` because that file is already at
//! the size ceiling, and because this method deserves to be judged on its own:
//! it is the one method in the whole tree where a fake `Ok(())` would be
//! *invisible from the outside*. A wrong-handle refusal, an already-closed
//! handle and a genuinely rolled-back transaction all end in "the host no
//! longer lists this ref" — so the only way to tell a real close from a stub is
//! to observe the transaction itself disappear.
//!
//! Everything above the live section is offline: it needs no server, because
//! every case it covers is decided before any socket is touched. The live
//! section at the bottom runs by default but reports itself unverified when no
//! server is configured (and fails outright under
//! `DATAZEN_CONTRACT_REQUIRE_LIVE=1`). It reads credentials from the **process
//! environment only** (`TEST_PG_*`), matching `postgres_resource_contract.rs` —
//! no file fallback, see AGENTS.md 「本地环境变量文件保护」.

use std::sync::{Arc, Mutex};

use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall, IdentityScope,
    ResourceError, ResourceHandle, ResourceProvider, ResourcePurpose, ResourceScope, ResultChunk,
    ResultSink,
};
use datazen_driver_api::session::{
    CloseDisposition, SessionHandleKind, SessionHandleRef, TransactionOptions,
};
use datazen_driver_api::{ConnectionConfig, QueryExecutionId};

use datazen_driver_postgres::{PostgresDriver, PostgresResourceProvider};

// Declared at file scope, not inside `mod live {}`: a `#[path]` inside an inline
// module resolves against a directory rustc synthesises (`tests/live/`) that
// does not exist on disk.
#[path = "../../http-support/tests/support/live_gate.rs"]
mod live_gate;

const POSTGRES_PROVIDER_ID: &str = "postgresql";

/// A real `ConnectionConfig` shaped like a host-built one. No server is
/// contacted while the offline tests run.
fn config() -> ConnectionConfig {
    ConnectionConfig {
        id: "pg-resource-close-handle".into(),
        name: "pg resource close handle".into(),
        database_type: POSTGRES_PROVIDER_ID.into(),
        host: Some("127.0.0.1".into()),
        port: Some(5432),
        database: Some("app_db".into()),
        schema: Some("public".into()),
        username: Some("contract_reader".into()),
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 10,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

fn target() -> NamespaceTarget {
    NamespaceTarget::empty().with_database("app_db")
}

fn interactive_scope(max_physical_connections: u32) -> ResourceScope {
    ResourceScope {
        purpose: ResourcePurpose::InteractiveQuery,
        max_physical_connections,
        holds_open_transaction: false,
        pin_for_streaming: false,
    }
}

fn acquire_request(config: &ConnectionConfig, scope: ResourceScope) -> AcquireResourceRequest {
    AcquireResourceRequest {
        connection_config: config.clone(),
        target: target(),
        scope,
        identity_scope: IdentityScope::default(),
        baseline: Baseline::new(Vec::new()),
    }
}

fn transaction_options() -> TransactionOptions {
    TransactionOptions {
        isolation_level: None,
        read_only: None,
        defers_commit: false,
    }
}

/// A provider bound to a fresh driver. Constructing one is enough: every case
/// below is decided by bookkeeping, never by a round trip.
fn provider() -> PostgresResourceProvider {
    PostgresResourceProvider::new(Arc::new(PostgresDriver::new()))
}

/// A ref of the shape this provider really mints: a transaction, owned by this
/// resource, in this epoch.
fn transaction_ref(handle: &ResourceHandle, epoch: u64, handle_id: &str) -> SessionHandleRef {
    SessionHandleRef::new(
        handle_id.to_string(),
        SessionHandleKind::Transaction,
        handle.resource_key().to_string(),
        epoch,
    )
}

/// A budget port that records charges and releases.
#[derive(Default)]
struct BudgetLedger {
    state: Mutex<LedgerState>,
}

#[derive(Default)]
struct LedgerState {
    acquired: Vec<u32>,
    released: Vec<String>,
}

impl BudgetLedger {
    fn acquired(&self) -> Vec<u32> {
        self.state
            .lock()
            .map(|state| state.acquired.clone())
            .unwrap_or_default()
    }

    fn released(&self) -> Vec<String> {
        self.state
            .lock()
            .map(|state| state.released.clone())
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl BudgetPort for BudgetLedger {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<BudgetPermit, ResourceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.acquired.push(requested);
        Ok(BudgetPermit {
            permit_id: format!("permit-{}", state.acquired.len()),
            physical_connections: requested,
        })
    }

    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.released.push(permit.permit_id.clone());
        Ok(())
    }
}

/// Counts what the live execution actually streamed, so "the transaction was
/// really open" is evidenced by a statement that ran inside it.
#[derive(Default)]
struct RecordingSink {
    chunks: Mutex<u32>,
}

impl RecordingSink {
    fn chunks(&self) -> u32 {
        *self
            .chunks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait::async_trait]
impl ResultSink for RecordingSink {
    async fn write(&self, _chunk: ResultChunk) -> Result<(), ResourceError> {
        *self
            .chunks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) += 1;
        Ok(())
    }

    async fn complete(&self) -> Result<(), ResourceError> {
        Ok(())
    }

    async fn fail(&self, reason: &str) -> Result<(), ResourceError> {
        // Nothing to record: the tests below assert on the error value itself.
        let _ = reason;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Offline: every refusal, proven without a server
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_handle_from_another_provider_is_refused_before_the_handle_is_read() {
    // The ownership check runs first, so a wrong-handle bug is reported as a
    // wrong handle rather than swallowed by any later, vaguer refusal.
    let provider = provider();
    let foreign = ResourceHandle::issue(
        "some-other-provider",
        "not-issued",
        provider.runtime_epoch(),
    );
    let session_handle = transaction_ref(&foreign, provider.runtime_epoch(), "tx-1");

    let error = provider
        .close_handle(&foreign, &session_handle)
        .await
        .expect_err("a handle from another provider cannot be closed through this one");

    assert!(
        matches!(
            error,
            ResourceError::ResourceOwnershipMismatch { ref expected, ref actual }
                if expected == POSTGRES_PROVIDER_ID && actual == "some-other-provider"
        ),
        "expected an ownership mismatch, got {error:?}"
    );
}

#[tokio::test]
async fn a_handle_from_a_retired_epoch_is_refused() {
    // Epoch 0 can never be this provider's: `next_runtime_epoch` starts at 2.
    let provider = provider();
    let stale = ResourceHandle::issue(POSTGRES_PROVIDER_ID, "any-key", 0);
    let session_handle = transaction_ref(&stale, 0, "tx-1");

    let error = provider
        .close_handle(&stale, &session_handle)
        .await
        .expect_err("a retired epoch must not be able to close anything");

    assert!(
        matches!(error, ResourceError::StaleRuntimeEpoch { .. }),
        "expected a stale epoch, got {error:?}"
    );
}

#[tokio::test]
async fn a_ref_that_names_a_different_resource_is_refused() {
    // §6.5: a ref belongs to exactly one resource. Replaying one against
    // another resource's handle is a host bug, and must not roll back whatever
    // happens to be open on the second one.
    let provider = provider();
    let handle = ResourceHandle::issue(POSTGRES_PROVIDER_ID, "some-key", provider.runtime_epoch());
    let borrowed = SessionHandleRef::new(
        "tx-1",
        SessionHandleKind::Transaction,
        "a-different-resource".to_string(),
        provider.runtime_epoch(),
    );

    let error = provider
        .close_handle(&handle, &borrowed)
        .await
        .expect_err("a ref minted for another resource cannot close this one");

    assert!(
        matches!(error, ResourceError::InvalidResourceState { .. }),
        "expected an invalid state, got {error:?}"
    );
}

#[tokio::test]
async fn a_ref_from_a_retired_epoch_is_refused() {
    // The handle is fine; the ref is stale. These are different faults and the
    // host needs to be able to tell them apart to know which retry is safe.
    let provider = provider();
    let handle = ResourceHandle::issue(POSTGRES_PROVIDER_ID, "some-key", provider.runtime_epoch());
    let stale = SessionHandleRef::new(
        "tx-1",
        SessionHandleKind::Transaction,
        handle.resource_key().to_string(),
        0,
    );

    let error = provider
        .close_handle(&handle, &stale)
        .await
        .expect_err("a ref from a retired epoch names nothing live");

    assert!(
        matches!(error, ResourceError::InvalidResourceState { .. }),
        "expected an invalid state, got {error:?}"
    );
}

#[tokio::test]
async fn a_handle_this_provider_never_opened_is_refused() {
    // Ownership and epoch both check out; there is simply no resource behind
    // the key. Saying so is what stops a host from believing it closed one.
    let provider = provider();
    let handle = ResourceHandle::issue(
        POSTGRES_PROVIDER_ID,
        "never-opened",
        provider.runtime_epoch(),
    );
    let session_handle = transaction_ref(&handle, provider.runtime_epoch(), "tx-1");

    let error = provider
        .close_handle(&handle, &session_handle)
        .await
        .expect_err("there is no open resource to close a handle on");

    match error {
        ResourceError::InvalidResourceState {
            ref resource_key,
            ref operation,
            ref state,
            ..
        } => {
            assert_eq!(operation, "close_handle");
            // The key rides in its own field (Display renders it), so an
            // operator reading the error sees *which* resource is missing
            // rather than only "no resource with this key".
            assert_eq!(
                resource_key, "never-opened",
                "the error must name the resource key"
            );
            assert!(
                state.contains("no resource with this key is open"),
                "the state must say what is wrong, got {state}"
            );
        }
        other => panic!("expected an invalid state, got {other:?}"),
    }
}

#[tokio::test]
async fn a_kind_this_provider_never_issues_is_refused_by_name() {
    // PostgreSQL mints exactly one kind: a transaction. A cursor or server-side
    // prepared statement is a different object with different teardown, and
    // this provider has no code that closes either — so it must not pretend to.
    let provider = provider();
    let handle = ResourceHandle::issue(POSTGRES_PROVIDER_ID, "some-key", provider.runtime_epoch());

    for kind in [SessionHandleKind::Cursor, SessionHandleKind::ServerPrepared] {
        let session_handle = SessionHandleRef::new(
            "handle-1",
            kind,
            handle.resource_key().to_string(),
            provider.runtime_epoch(),
        );
        let error = provider
            .close_handle(&handle, &session_handle)
            .await
            .expect_err("a kind this provider never issues cannot be closed");

        assert!(
            matches!(
                error,
                ResourceError::OperationNotSupported { ref operation, ref reason, .. }
                    if operation == "close_handle"
                        && reason.contains("handle-1")
                        && reason.contains(&format!("{kind:?}"))
            ),
            "the refusal must name the handle and its kind, got {error:?}"
        );
    }
}

#[tokio::test]
async fn closing_a_closed_resource_handle_is_a_no_op_success() {
    // `ResourceHandle::is_closed()` is the host's own record that `close_resource`
    // already ran. Replaying a handle close after that is a race the host can
    // legitimately lose, and the contract says it must not be an error.
    let provider = provider();
    let handle = ResourceHandle::issue(POSTGRES_PROVIDER_ID, "some-key", provider.runtime_epoch());
    let session_handle = transaction_ref(&handle, provider.runtime_epoch(), "tx-1");
    handle.mark_closed();

    provider
        .close_handle(&handle, &session_handle)
        .await
        .expect("closing a handle on an already-closed resource is idempotent");
}

#[tokio::test]
async fn every_refusal_leaves_the_resource_handle_open() {
    // Nothing in the failure paths may mark the handle closed: the host keys
    // its "this resource is done" bookkeeping off that flag, so a refusal that
    // flipped it would look exactly like a successful close.
    let provider = provider();
    let handle = ResourceHandle::issue(POSTGRES_PROVIDER_ID, "some-key", provider.runtime_epoch());

    let wrong_kind = SessionHandleRef::new(
        "handle-1",
        SessionHandleKind::Cursor,
        handle.resource_key().to_string(),
        provider.runtime_epoch(),
    );
    assert!(provider.close_handle(&handle, &wrong_kind).await.is_err());
    assert!(!handle.is_closed());

    let borrowed = SessionHandleRef::new(
        "tx-1",
        SessionHandleKind::Transaction,
        "elsewhere".to_string(),
        provider.runtime_epoch(),
    );
    assert!(provider.close_handle(&handle, &borrowed).await.is_err());
    assert!(!handle.is_closed());
}

// ---------------------------------------------------------------------------
// Live: the transaction really goes away
// ---------------------------------------------------------------------------

/// Not `#[ignore]`d. A static `#[ignore]` is neither a pass nor a failure: cargo
/// reports it in the `ignored` count and exits 0, so a suite pinned behind one
/// is indistinguishable from a suite that passed. Instead it runs by default and
/// *reports itself unverified*, failing only under
/// `DATAZEN_CONTRACT_REQUIRE_LIVE=1`.
mod live {
    use super::*;

    const DIMENSION: &str = "resource-close-handle-live";

    fn env_var(key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|value| !value.is_empty())
    }

    fn live_config() -> Option<ConnectionConfig> {
        let host = env_var("TEST_PG_HOST")?;
        let mut config = config();
        config.host = Some(host);
        if let Some(port) = env_var("TEST_PG_PORT") {
            config.port = port.parse().ok();
        }
        if let Some(user) = env_var("TEST_PG_USER") {
            config.username = Some(user);
        }
        if let Some(database) = env_var("TEST_PG_DATABASE") {
            config.database = Some(database);
        }
        if let Some(password) = env_var("TEST_PG_PASSWORD") {
            config.password = Some(password);
        }
        Some(config)
    }

    #[tokio::test]
    async fn closing_a_transaction_handle_rolls_it_back_and_frees_nothing_else() {
        let strict = live_gate::strict_live();
        let Some(config) = live_config() else {
            live_gate::unverified_or_fail(
                "postgresql",
                DIMENSION,
                "TEST_PG_HOST is not in the process environment",
                strict,
            );
            return;
        };
        let provider = provider();
        let ledger = Arc::new(BudgetLedger::default());
        let budget: Arc<dyn BudgetPort> = Arc::clone(&ledger) as Arc<dyn BudgetPort>;

        let handle = match provider
            .acquire_resource(&acquire_request(&config, interactive_scope(32)), &budget)
            .await
        {
            Ok(handle) => handle,
            Err(error) => {
                live_gate::unverified_or_fail(
                    "postgresql",
                    DIMENSION,
                    &format!("the live server did not accept an acquire: {error}"),
                    strict,
                );
                return;
            }
        };

        provider
            .begin_transaction(&handle, &transaction_options())
            .await
            .expect("a live resource must be able to open a transaction");

        // Run a statement *inside* the transaction, so "the transaction existed"
        // is evidenced by something the server actually did rather than by the
        // provider's own bookkeeping agreeing with itself.
        let sink = RecordingSink::default();
        let completion = provider
            .execute_on_resource(
                &handle,
                &QueryExecutionId::new("close-handle-live-1"),
                &CommandCall::new("query", serde_json::json!({ "sql": "SELECT 1" })),
                &sink,
            )
            .await
            .expect("a plain SELECT must run inside the live transaction");
        assert!(
            completion.completion_status.is_success(),
            "the statement inside the transaction must succeed: {:?}",
            completion.completion_status
        );
        assert!(
            sink.chunks() > 0,
            "the statement must stream at least one chunk"
        );

        // The host's §6.5 source: the ref comes from the provider, in the
        // completion, not from anything this test invented.
        let handles = provider
            .observe_session(&handle)
            .await
            .expect("a live transaction must be observable")
            .handles;
        assert_eq!(
            handles.len(),
            1,
            "exactly one transaction handle while one transaction is open, got {handles:?}"
        );
        let session_handle = handles[0].clone();
        assert_eq!(session_handle.kind, SessionHandleKind::Transaction);

        provider
            .close_handle(&handle, &session_handle)
            .await
            .expect("closing a real transaction handle must succeed");

        // The transaction must actually be gone. This is the assertion a stub
        // could not pass: an implementation that returned `Ok(())` without
        // rolling back would leave the commit below working.
        let after = provider
            .commit_transaction(&handle)
            .await
            .expect_err("the transaction was rolled back, so it must not be committable");
        assert!(
            matches!(
                after,
                ResourceError::InvalidResourceState { .. }
                    | ResourceError::TransactionResolutionRequired { .. }
                    | ResourceError::Driver(_)
                    | ResourceError::OperationNotSupported { .. }
            ),
            "committing a rolled-back transaction must fail, got {after:?}"
        );

        // ...and the handle is idempotent: the host may have raced a commit.
        provider
            .close_handle(&handle, &session_handle)
            .await
            .expect_err("a second close of the same ref names no open transaction");

        // The resource is untouched: the permit belongs to the resource, and
        // closing a handle must never be mistaken for closing the resource.
        assert!(!handle.is_closed());
        // Exactly one acquire, charged at the resource's own pool cost: the
        // config's effective pool size plus the single control-pool connection
        // used for cancellation. The `interactive_scope(32)` above is only a
        // ceiling the charge must fit under, never the charge itself.
        let expected_charge = config.effective_max_pool_size() + 1;
        assert_eq!(
            ledger.acquired(),
            vec![expected_charge],
            "the handle close must not have re-acquired a permit either"
        );
        assert!(
            ledger.released().is_empty(),
            "closing a handle must not release the resource's permit"
        );
        assert_eq!(
            provider
                .close_resource(&handle)
                .await
                .expect("close must still work"),
            CloseDisposition::Closed,
            "close must report a confirmed close, not an unconfirmed one"
        );
        assert_eq!(
            ledger.released().len(),
            1,
            "exactly one release per acquire"
        );
    }
}
