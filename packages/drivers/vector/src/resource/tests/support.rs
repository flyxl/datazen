//! The doubles and fixtures every test shares.
//!
//! Two rules for everything in this file:
//!
//! * a double must be able to *fail*, or the assertion it supports proves
//!   nothing — hence [`BudgetLedger::deny`] and [`RecordingSink::reject`];
//! * no double reads `.env`, opens a real socket outside loopback, or holds a
//!   credential. The stub answers on `127.0.0.1` with a body fixed at compile
//!   time.

use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall,
    DescribeResourceRequest, ResourceError, ResourceHandle, ResourcePurpose, ResourceScope,
    ResultChunk, ResultSink,
};

use datazen_driver_api::ConnectionConfig;

use crate::resource::VectorResourceProvider;
use crate::vector::VectorDriver;

// ---------------------------------------------------------------------------
// Budget
// ---------------------------------------------------------------------------

/// Records every charge request, grant and release.
///
/// The three are kept apart on purpose: a provider that asks the budget and is
/// refused is not the same as one that never asked, and only the last of them
/// proves a refused caller was not charged for nothing.
#[derive(Default)]
pub(super) struct BudgetLedger {
    inner: Mutex<LedgerState>,
}

#[derive(Default)]
struct LedgerState {
    requested: Vec<u32>,
    granted: Vec<u32>,
    released: Vec<String>,
    deny: bool,
}

impl BudgetLedger {
    pub(super) fn deny(&self, deny: bool) {
        self.state().deny = deny;
    }

    pub(super) fn requested(&self) -> Vec<u32> {
        self.state().requested.clone()
    }

    pub(super) fn granted(&self) -> Vec<u32> {
        self.state().granted.clone()
    }

    /// The permits handed back, in order, as `physical_connections` strings.
    pub(super) fn released(&self) -> Vec<String> {
        self.state().released.clone()
    }

    fn state(&self) -> MutexGuard<'_, LedgerState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn as_port(self: &Arc<Self>) -> Arc<dyn BudgetPort> {
        Arc::new(LedgerPort {
            ledger: self.clone(),
        })
    }
}

struct LedgerPort {
    ledger: Arc<BudgetLedger>,
}

#[async_trait]
impl BudgetPort for LedgerPort {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<BudgetPermit, ResourceError> {
        let mut state = self.ledger.state();
        state.requested.push(requested);
        if state.deny {
            return Err(ResourceError::BudgetDenied {
                requested,
                reason: "the test ledger denies every charge".to_string(),
            });
        }
        state.granted.push(requested);
        Ok(BudgetPermit {
            permit_id: format!("permit-{requested}"),
            physical_connections: requested,
        })
    }

    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        self.ledger
            .state()
            .released
            .push(permit.physical_connections.to_string());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Sink
// ---------------------------------------------------------------------------

/// Keeps everything the provider streamed, so "no rows" and "rows" stay
/// distinguishable in a failed execution.
#[derive(Default)]
pub(super) struct RecordingSink {
    inner: Mutex<SinkState>,
}

#[derive(Default)]
struct SinkState {
    chunks: Vec<ResultChunk>,
    completed: bool,
    failed: Option<String>,
    reject: bool,
}

impl RecordingSink {
    /// Make the next `write` fail, the way a disconnected UI would.
    pub(super) fn reject(&self) {
        self.state().reject = true;
    }

    pub(super) fn chunks(&self) -> Vec<ResultChunk> {
        self.state().chunks.clone()
    }

    pub(super) fn row_count(&self) -> usize {
        self.state()
            .chunks
            .iter()
            .map(|chunk| chunk.rows.len())
            .sum()
    }

    pub(super) fn completed(&self) -> bool {
        self.state().completed
    }

    pub(super) fn failed(&self) -> Option<String> {
        self.state().failed.clone()
    }

    fn state(&self) -> MutexGuard<'_, SinkState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl ResultSink for RecordingSink {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        if self.state().reject {
            return Err(ResourceError::SinkRejected {
                reason: "the test sink refuses writes".to_string(),
            });
        }
        self.state().chunks.push(chunk);
        Ok(())
    }

    async fn complete(&self) -> Result<(), ResourceError> {
        self.state().completed = true;
        Ok(())
    }

    async fn fail(&self, reason: &str) -> Result<(), ResourceError> {
        self.state().failed = Some(reason.to_string());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Wire stub
// ---------------------------------------------------------------------------

/// A loopback HTTP server that answers every request with the same body.
///
/// Hand-rolled rather than pulled in as a dev-dependency: the whole point of
/// this provider is the *one* request per command, so a socket that reads a
/// request head and replies once is enough — and it leaves the crate's
/// dependency list untouched.
pub(super) async fn stub_qdrant(status: u16, body: &'static str) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the loopback stub must bind");
    let port = listener
        .local_addr()
        .expect("the loopback stub must have a local address")
        .port();

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut head = Vec::new();
            let mut byte = [0_u8; 1];
            // The head only: this stub never reads the request body, and the
            // client does not care that its payload was ignored.
            while !head.ends_with(b"\r\n\r\n") {
                match socket.read(&mut byte).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => head.push(byte[0]),
                }
            }
            let response = format!(
                "HTTP/1.1 {status} Stub\r\nContent-Type: application/json\r\nContent-Length: \
                 {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.flush().await;
            let _ = socket.shutdown().await;
        }
    });

    port
}

/// One point returned by the stub, so the row assertions have a real payload
/// behind them.
pub(super) const ONE_POINT: &str =
    r#"{"result":{"points":[{"id":"1","payload":{"title":"dune"},"vector":[0.1,0.2]}]}}"#;

/// The server's own failure shape, answered with a 500.
pub(super) const ERROR_BODY: &str = r#"{"status":{"error":"collection not found"}}"#;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A config that points at a port nothing is listening on.
///
/// `VectorDriver::connect` registers a client without contacting the server, so
/// a refused port is the cheapest honest fixture: anything that needs the wire
/// uses [`config_at`] instead.
pub(super) fn config() -> ConnectionConfig {
    ConnectionConfig {
        id: "vector-cfg".into(),
        name: "vector".into(),
        database_type: "qdrant".into(),
        host: Some("127.0.0.1".into()),
        port: Some(1),
        database: None,
        schema: None,
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 2,
        max_pool_size: 1,
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

pub(super) fn config_at(port: u16) -> ConnectionConfig {
    ConnectionConfig {
        id: "vector-cfg".into(),
        port: Some(port),
        ..config()
    }
}

/// A config with no endpoint, which is the one shape `VectorDriver::connect`
/// actually rejects: the base URL cannot be built, so no client is registered.
pub(super) fn config_without_endpoint() -> ConnectionConfig {
    ConnectionConfig {
        id: "vector-no-endpoint".into(),
        host: None,
        port: None,
        ..config()
    }
}

pub(super) fn scope() -> ResourceScope {
    ResourceScope {
        purpose: ResourcePurpose::InteractiveQuery,
        max_physical_connections: 1,
        holds_open_transaction: false,
        pin_for_streaming: false,
    }
}

pub(super) fn acquire_request(
    config: &ConnectionConfig,
    scope: ResourceScope,
) -> AcquireResourceRequest {
    AcquireResourceRequest {
        connection_config: config.clone(),
        target: NamespaceTarget::empty(),
        scope,
        identity_scope: Default::default(),
        baseline: Baseline::default(),
    }
}

/// One execution id, fixed so a test can compare it against a cancel receipt.
pub(super) fn execution_id() -> datazen_driver_api::QueryExecutionId {
    datazen_driver_api::QueryExecutionId::new("exec-vector")
}

pub(super) fn describe_request(config: &ConnectionConfig) -> DescribeResourceRequest {
    DescribeResourceRequest {
        connection_config: config.clone(),
        identity_scope: Default::default(),
        target: NamespaceTarget::empty(),
        purpose: ResourcePurpose::InteractiveQuery,
    }
}

/// The nearest-neighbour search this driver exists to run: a JSON command in
/// the `sql` field, which is what `execute_standard_sql_command` reads.
pub(super) fn search_call() -> CommandCall {
    CommandCall {
        command: "query".into(),
        input: serde_json::json!({
            "sql": "{\"collection\":\"books\",\"query\":{\"nearest\":{\"vector\":[0.1,0.2],\"limit\":2}}}"
        }),
    }
}

/// A handle of the right shape for a key this provider never opened.
pub(super) fn unacquired_handle(
    provider: &VectorResourceProvider,
    resource_key: &str,
) -> ResourceHandle {
    ResourceHandle::issue(
        super::super::capabilities::VECTOR_PROVIDER_ID,
        resource_key,
        provider.runtime_epoch(),
    )
}

/// A provider bound to a driver of its own, so tests share no state.
pub(super) fn provider() -> VectorResourceProvider {
    VectorResourceProvider::new(Arc::new(VectorDriver::new()))
}

/// The reason an `OperationNotSupported` carries, or a panic that says what
/// came back instead.
pub(super) fn refusal_reason(error: &ResourceError) -> String {
    match error {
        ResourceError::OperationNotSupported { reason, .. } => reason.clone(),
        other => panic!("expected an OperationNotSupported error, got {other:?}"),
    }
}
