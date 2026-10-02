//! The doubles and fixtures every test shares.
//!
//! Two rules for everything in this file:
//!
//! * a double must be able to *fail*, or the assertion it supports proves
//!   nothing — hence [`BudgetLedger::deny`] and [`RecordingSink::reject`];
//! * no double reads `.env`, leaves loopback, or holds a credential. The
//!   Stargate stand-in is a `wiremock` server on `127.0.0.1` whose body is fixed
//!   at compile time.

use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall,
    DescribeResourceRequest, ResourceError, ResourceHandle, ResourcePurpose, ResourceScope,
    ResultChunk, ResultSink,
};
use datazen_driver_api::{ConnectionConfig, SslMode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::hbase::HBaseDriver;
use crate::resource::HBaseResourceProvider;

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
// Wire
// ---------------------------------------------------------------------------

/// Two cells, as the Stargate scanner returns them.
const SCAN_ROWS: &str = r#"{"Row":[{"key":"row-1","Cell":[{"$":"value-1"}]},{"key":"row-2","Cell":[{"$":"value-2"}]}]}"#;

/// The path the scanner is handed back on; the driver follows the `Location`
/// header, so the rows and the delete both have to be mounted here.
const SCANNER_PATH: &str = "/books/scanner/1";

/// A Stargate that answers the liveness probe and reads one table.
///
/// Every mock is mounted for the life of the returned server, and a scan costs
/// three requests (open, read, delete) — all three are answered, so a test that
/// expects rows is exercising the real request sequence and not a shortcut.
pub(super) async fn stargate_answering() -> MockServer {
    let server = MockServer::start().await;
    mount_probe(&server, 200).await;
    mount_scan(&server, "books", 200).await;
    server
}

/// A Stargate that answers the probe but fails every scan.
pub(super) async fn stargate_failing_scans() -> MockServer {
    let server = MockServer::start().await;
    mount_probe(&server, 200).await;
    mount_scan(&server, "books", 500).await;
    server
}

/// A server that answers the probe with a non-success status, which is how a
/// reachable-but-wrong endpoint looks: something is listening and it is not
/// Stargate.
pub(super) async fn stargate_probe_answers_wrongly() -> MockServer {
    let server = MockServer::start().await;
    mount_probe(&server, 404).await;
    server
}

/// The one round trip `observe_session` is allowed to make: `GET {base}/`.
async fn mount_probe(server: &MockServer, status: u16) {
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(status)
                .set_body_json(serde_json::json!({ "status": "ok" })),
        )
        .mount(server)
        .await;
}

/// One scan of `table`: open the scanner, read the rows, delete the scanner.
async fn mount_scan(server: &MockServer, table: &str, status: u16) {
    let open = format!("/{table}/scanner");
    if status != 201 {
        Mock::given(method("POST"))
            .and(path(open.as_str()))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_string(r#"{"error":"the table is not available"}"#),
            )
            .mount(server)
            .await;
        return;
    }

    Mock::given(method("POST"))
        .and(path(open.as_str()))
        .respond_with(ResponseTemplate::new(201).insert_header(
            "Location",
            format!("{}/scanner/1", server.uri()).as_str(),
        ))
        .mount(server)
        .await;

    // No method matcher: the driver reads the scanner once and then deletes it,
    // and both requests have to land on the same answer.
    Mock::given(path(SCANNER_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(SCAN_ROWS))
        .mount(server)
        .await;
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A config that resolves, but points at a port nothing listens on.
///
/// A refused connect is the interesting case for the budget, so the default
/// fixture is the one where acquiring *fails* — and the test that wants a live
/// resource passes [`config_at`] instead.
pub(super) fn config() -> ConnectionConfig {
    config_at(1)
}

pub(super) fn config_at(port: u16) -> ConnectionConfig {
    ConnectionConfig {
        id: "hbase-cfg".into(),
        name: "hbase-cfg".into(),
        database_type: "hbase".into(),
        host: Some("127.0.0.1".into()),
        port: Some(port),
        database: None,
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::Disable,
        connection_timeout: 5,
        max_pool_size: 4,
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

/// A config with no endpoint at all: `base_url` cannot be built from it, so
/// `HBaseDriver::connect` fails before any socket exists.
pub(super) fn config_without_endpoint() -> ConnectionConfig {
    ConnectionConfig {
        host: None,
        port: None,
        ..config()
    }
}

/// A config for the live [`wiremock`] server.
pub(super) fn config_for(server: &MockServer) -> ConnectionConfig {
    config_at(server.address().port())
}

pub(super) fn scope() -> ResourceScope {
    ResourceScope {
        purpose: ResourcePurpose::InteractiveQuery,
        max_physical_connections: 4,
        holds_open_transaction: false,
        pin_for_streaming: false,
    }
}

pub(super) fn baseline() -> Baseline {
    Baseline::default()
}

pub(super) fn acquire_request(config: &ConnectionConfig) -> AcquireResourceRequest {
    AcquireResourceRequest {
        connection_config: config.clone(),
        identity_scope: Default::default(),
        target: NamespaceTarget::empty(),
        baseline: baseline(),
        scope: scope(),
    }
}

pub(super) fn execution_id() -> datazen_driver_api::QueryExecutionId {
    datazen_driver_api::QueryExecutionId::new("exec-hbase")
}

pub(super) fn describe_request(config: &ConnectionConfig) -> DescribeResourceRequest {
    DescribeResourceRequest {
        connection_config: config.clone(),
        identity_scope: Default::default(),
        target: NamespaceTarget::empty(),
        purpose: ResourcePurpose::InteractiveQuery,
    }
}

/// The one read this driver exists to run: `scan <table>`, which
/// `execute_standard_sql_command` hands to `HBaseDriver::query_multi`.
pub(super) fn scan_call() -> CommandCall {
    CommandCall {
        command: "query".into(),
        input: serde_json::json!({ "sql": "scan books" }),
    }
}

/// A catalog command, which answers with a `CommandResult` that is *not* a
/// result set.
pub(super) fn catalog_call(command: &str) -> CommandCall {
    CommandCall {
        command: command.to_string(),
        input: serde_json::json!({}),
    }
}

/// A handle of the right shape for a key this provider never opened.
pub(super) fn unacquired_handle(
    provider: &HBaseResourceProvider,
    resource_key: &str,
) -> ResourceHandle {
    ResourceHandle::issue(
        super::super::capabilities::HBASE_PROVIDER_ID,
        resource_key,
        provider.runtime_epoch(),
    )
}

/// A provider bound to a driver of its own, so tests share no state.
pub(super) fn provider() -> HBaseResourceProvider {
    HBaseResourceProvider::new(Arc::new(HBaseDriver::new()))
}

/// The reason an `OperationNotSupported` carries, or a panic that says what
/// came back instead.
pub(super) fn refusal_reason(error: &ResourceError) -> String {
    match error {
        ResourceError::OperationNotSupported { reason, .. } => reason.clone(),
        other => panic!("expected an OperationNotSupported error, got {other:?}"),
    }
}
