//! Port contract tests for the shared metadata source.
//!
//! The source is exercised against a **fake Runtime** — a real
//! `RuntimeConnectionUseCases` wired to fake profile, policy and backend
//! ports — and a **fake execution host**. The fakes stay thin on purpose: the
//! contract under test is the source's own mapping (session to connection,
//! profile to driver, command to parsed metadata, refusal to leak a physical
//! handle), not the Runtime's own behaviour.

use super::*;
use crate::services::ports::{
    ArtifactChunkReader, LogicalSchemaContextSource, PhysicalSchemaContextSource,
    SessionConnectionLookup,
};
use crate::workflow::error::WorkflowError;
use crate::workflow::resource_execution::{PreparedExecution, ResourceExecutionHost};
use async_trait::async_trait;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{ConnectionConfig, DatabaseDriver};
use datazen_modules::execution::application::requests::OpenSessionRequest;
use datazen_modules::execution::application::use_cases::ConnectionUseCases;
use datazen_modules::execution::application::{RuntimeConnectionUseCases, RuntimeResultSink};
use datazen_modules::execution::runtime::{
    connection as rt,
    registry::{audit::CapabilityVersions, backend::*, SessionBackend},
};
use datazen_modules::{
    connection::domain::*,
    connection::dto::{ProfileDraft, ProfilePatch},
    execution::application::idempotency::IdempotentOperation,
    execution::application::policy::*,
    execution::target::{ExecutionTarget, NamespaceTarget},
    shared::context::{DelegationRef, OwnerRef, RequestContext},
    shared::error::PortError,
    shared::id::*,
};
/// The test fake answers the consumer-defined lookup with the real facade.
///
/// D6 item 4 收尾：`SessionConnectionLookup` 的唯一定义随实现迁入
/// modules::ai::application，外部 trait 与外部类型不能再直接 impl（孤儿
/// 规则），所以这里用本地包装转发——与下方 `RuntimeChunks` 同一模式，
/// 生产半边（宿主装配）同样只依赖端口，不点名门面类型。
struct RuntimeLookup(Arc<RuntimeConnectionUseCases>);
impl SessionConnectionLookup for RuntimeLookup {
    fn connection_id_for_session(&self, session_id: &str) -> Option<String> {
        RuntimeConnectionUseCases::connection_id_for_session(
            &self.0,
            &rt::DbSessionId::new(session_id),
        )
        .map(|connection| connection.as_str().to_owned())
    }
}

use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 测试用结果块读取器：把消费者定义端口转发给真实 Runtime 门面。
///
/// 生产实现住在宿主（`src-tauri`），测试实现住在这里——两者都只依赖
/// `ArtifactChunkReader` 这个本 crate 定义的 trait，所以共享层不必点名门面类型。
struct RuntimeChunks(Arc<RuntimeConnectionUseCases>);
#[async_trait::async_trait]
impl ArtifactChunkReader for RuntimeChunks {
    async fn read_chunk(
        &self,
        ctx: &datazen_modules::shared::context::RequestContext,
        artifact: &datazen_modules::shared::id::ArtifactId,
        index: datazen_modules::shared::id::Counter,
    ) -> Result<datazen_modules::execution::ResultChunk, datazen_modules::shared::error::ApiError>
    {
        self.0
            .read_artifact_chunk(ctx, artifact.clone(), index)
            .await
    }
}

const CONNECTION_ID: &str = "conn-1";
const REVISION: u64 = 1;

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `expect_err` needs `Debug` on the success type; a driver handle has none.
fn expect_error<T>(result: Result<T, String>, message: &str) -> String {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

fn context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("principal-1"),
        None,
        ClientInstanceId::new("client-1"),
        RequestId::new("request-1"),
        None,
    )
}

fn target() -> ExecutionTarget {
    ExecutionTarget::new(
        ConnectionId::new(CONNECTION_ID),
        NamespaceTarget::database("main"),
    )
}

// ---------------------------------------------------------------------------
// 假 Runtime 的三个端口
// ---------------------------------------------------------------------------

struct FakeProfiles;

#[async_trait]
impl ProfileRepository for FakeProfiles {
    async fn list(
        &self,
        ctx: &RequestContext,
        _scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        Ok(vec![self
            .get(ctx, ConnectionId::new(CONNECTION_ID))
            .await?
            .expect("the fixture profile exists")])
    }

    async fn get(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        if connection_id.as_str() != CONNECTION_ID {
            return Ok(None);
        }
        let mut record = ProfileRecord::new(ctx.organization_id.clone(), connection_id, "sqlite");
        record.config_revision = ConfigRevision::new(REVISION);
        Ok(Some(record))
    }

    async fn create(
        &self,
        _ctx: &RequestContext,
        _draft: ProfileDraft,
        _idem: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!("the fixture never writes a profile")
    }

    async fn compare_and_set(
        &self,
        _ctx: &RequestContext,
        _connection_id: ConnectionId,
        _expected: ConfigRevision,
        _patch: ProfilePatch,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!("the fixture never writes a profile")
    }

    async fn disable(
        &self,
        _ctx: &RequestContext,
        _connection_id: ConnectionId,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!("the fixture never writes a profile")
    }

    async fn advance_credential_revision(
        &self,
        _ctx: &RequestContext,
        _connection_id: ConnectionId,
    ) -> Result<CredentialRevision, PortError> {
        unreachable!("the fixture never rotates a credential")
    }
}

struct FakePolicy;

#[async_trait]
impl PolicyService for FakePolicy {
    async fn authorize(
        &self,
        _ctx: &RequestContext,
        _subject: AuthorizationSubject,
        _action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        Ok(AuthorizationDecision::Allow)
    }

    async fn verify_delegation(
        &self,
        _ctx: &RequestContext,
        _delegation: DelegationRef,
    ) -> Result<DelegationGrant, PortError> {
        Err(PortError::TokenInvalid)
    }

    fn isolation_key(&self, _ctx: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        Ok(PolicyIsolationKey::new("schema-context-fixture"))
    }

    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty())
    }
}

/// 假物理后端：打开一个资源，执行时把一条命令结果发布进结果槽。
struct FakeBackend {
    sink: Mutex<Option<RuntimeResultSink>>,
    contexts: Mutex<HashMap<String, rt::SessionContext>>,
    payload: Mutex<serde_json::Value>,
}

impl FakeBackend {
    fn new(payload: serde_json::Value) -> Self {
        Self {
            sink: Mutex::new(None),
            contexts: Mutex::new(HashMap::new()),
            payload: Mutex::new(payload),
        }
    }
}

#[async_trait]
impl SessionBackend for FakeBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, rt::ProviderError> {
        let resource_id = format!("resource-{}", request.db_session_id.as_str());
        let context = rt::SessionContext::new(request.initial_target, "fixture-identity");
        lock(&self.contexts).insert(resource_id.clone(), context.clone());
        Ok(OpenedResource {
            context,
            capabilities: CapabilityVersions::new("fixture", "fixture"),
            capability_snapshot: CapabilityProvenance::new("fixture-driver", "1.0.0", 1),
            resource_id,
            driver_supports_cancel: true,
        })
    }

    async fn execute(
        &self,
        request: ExecuteOnResource,
    ) -> Result<ResourceExecution, rt::ProviderError> {
        request.cancel_handle_sink.publish("fixture-cancel");
        let payload = lock(&self.payload).clone();
        if let Some(sink) = lock(&self.sink).clone() {
            sink.publish_output(
                &request.execution_id,
                json!({ "kind": "commandResult", "data": payload }),
            )
            .map_err(|error| rt::ProviderError::ProtocolError(error.message))?;
        }
        let context = lock(&self.contexts)
            .get(&request.resource_id)
            .cloned()
            .ok_or_else(|| rt::ProviderError::SessionLost("unknown resource".into()))?;
        Ok(ResourceExecution {
            execution_id: request.execution_id.clone(),
            stream_id: rt::StreamId::new("fixture-stream"),
            state: rt::ExecutionState::Succeeded,
            effect_outcome: rt::EffectOutcome::Completed,
            context_after: context,
            context_revision: request.expected_context_revision,
            handles: Vec::new(),
            cancel_handle: "fixture-cancel".into(),
        })
    }

    async fn cancel(
        &self,
        _request: CancelOnResource,
    ) -> Result<ResourceCancel, rt::ProviderError> {
        Ok(ResourceCancel {
            disposition: rt::port::CancelDisposition::Requested,
            state: rt::ExecutionState::CancelRequested,
        })
    }

    async fn finalize_handles(
        &self,
        request: FinalizeHandles,
    ) -> Result<HandleFinalization, rt::ProviderError> {
        Ok(HandleFinalization {
            finalized: request.handles.len(),
            remaining: 0,
            effect_outcome: rt::EffectOutcome::RolledBack,
        })
    }

    async fn close(
        &self,
        request: CloseResource,
    ) -> Result<CloseResourceOutcome, rt::ProviderError> {
        lock(&self.contexts).remove(&request.resource_id);
        Ok(CloseResourceOutcome::Closed)
    }
}

// ---------------------------------------------------------------------------
// 假执行宿主
// ---------------------------------------------------------------------------

/// 假 `ResourceExecutionHost`：回答 profile / driver / prepare 三件事。
struct FakeHost {
    runtime: Arc<RuntimeConnectionUseCases>,
    config: ConnectionConfig,
    enabled: bool,
    profile_error: Option<String>,
    driver_error: Option<String>,
    seen: Mutex<Vec<String>>,
}

impl FakeHost {
    fn new(runtime: Arc<RuntimeConnectionUseCases>) -> Self {
        let mut config = ConnectionConfig {
            id: CONNECTION_ID.to_owned(),
            name: "fixture".to_owned(),
            database_type: "sqlite".into(),
            host: None,
            port: None,
            database: Some("main".to_owned()),
            schema: Some("public".to_owned()),
            username: None,
            password: None,
            ssl_mode: Default::default(),
            connection_timeout: 30,
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
        };
        config.schema = Some("public".to_owned());
        Self {
            runtime,
            config,
            enabled: true,
            profile_error: None,
            driver_error: None,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn with_schema(mut self, schema: Option<&str>) -> Self {
        self.config.schema = schema.map(str::to_owned);
        self
    }

    fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    fn without_profile(mut self) -> Self {
        self.profile_error = Some("Connection profile unavailable".to_owned());
        self
    }

    fn without_driver(mut self) -> Self {
        self.driver_error = Some("Driver unavailable".to_owned());
        self
    }
}

#[async_trait]
impl ResourceExecutionHost for FakeHost {
    async fn profile(&self, id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        lock(&self.seen).push(id.to_owned());
        if let Some(message) = &self.profile_error {
            return Err(WorkflowError::ConnectionFailed {
                connection_id: id.to_owned(),
                message: message.clone(),
            });
        }
        Ok((self.config.clone(), REVISION, self.enabled))
    }

    async fn driver(
        &self,
        _config: &ConnectionConfig,
    ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
        if let Some(message) = &self.driver_error {
            return Err(WorkflowError::Driver(message.clone()));
        }
        Ok(MockDriver::new("sqlite", MockDriverOptions::default()))
    }

    async fn safe_mode(&self) -> bool {
        false
    }

    async fn prepare(
        &self,
        _id: &str,
        _revision: u64,
        _command: &str,
        target: ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError> {
        Ok(PreparedExecution {
            use_cases: self.runtime.clone(),
            artifacts: std::sync::Arc::new(RuntimeChunks(self.runtime.clone())),
            context: context(),
            target,
        })
    }
}

// ---------------------------------------------------------------------------
// 装配
// ---------------------------------------------------------------------------

struct Fixture {
    runtime: Arc<RuntimeConnectionUseCases>,
}

fn runtime_with(payload: serde_json::Value) -> Fixture {
    let backend = Arc::new(FakeBackend::new(payload));
    let runtime = Arc::new(RuntimeConnectionUseCases::new(
        Arc::new(FakeProfiles),
        Arc::new(FakePolicy),
        backend.clone(),
    ));
    *lock(&backend.sink) = Some(runtime.result_sink());
    Fixture { runtime }
}

/// 开一个会话并返回它的公开 id。
async fn open_session(runtime: &RuntimeConnectionUseCases) -> String {
    let ctx = context();
    let token = runtime
        .issue_submission_token(&ctx, IdempotentOperation::OpenSession, None)
        .await
        .expect("the fixture policy admits the token");
    let receipt = runtime
        .open_session(
            &ctx,
            OpenSessionRequest::new(
                target(),
                OwnerRef::Editor {
                    client_instance_id: ctx.client_instance_id.clone(),
                    editor_session_id: EditorSessionId::new("editor-1"),
                },
                token.idempotency_key,
            ),
        )
        .await
        .expect("the fixture backend opens the session");
    receipt.session.handle.db_session_id.as_str().to_owned()
}

fn tables_payload() -> serde_json::Value {
    json!({
        "tables": [{
            "name": "users",
            "schema": "public",
            "tableType": "table",
            "rowCount": 42
        }]
    })
}

fn schema_payload() -> serde_json::Value {
    json!({
        "schema": {
            "tableName": "users",
            "columns": [{
                "name": "id",
                "dataType": "integer",
                "nullable": false,
                "defaultValue": null,
                "comment": null,
                "isPrimaryKey": true,
                "isAutoIncrement": false
            }],
            "primaryKeys": ["id"],
            "indexes": [],
            "foreignKeys": []
        }
    })
}

// ---------------------------------------------------------------------------
// 契约
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_session_refuses_to_hand_out_a_physical_handle() {
    let fixture = runtime_with(tables_payload());
    let source = RuntimeSchemaContextSource::new(
        Arc::new(RuntimeLookup(fixture.runtime.clone())),
        Arc::new(FakeHost::new(fixture.runtime.clone())),
    );
    let error = expect_error(
        source.get_session("dbs-1").await,
        "the Runtime path never exposes a physical handle",
    );
    assert_eq!(
        error,
        "The Runtime metadata source does not expose physical session handles"
    );
}

#[tokio::test]
async fn an_unknown_session_has_no_connection_owner() {
    let fixture = runtime_with(tables_payload());
    let source = RuntimeSchemaContextSource::new(
        Arc::new(RuntimeLookup(fixture.runtime.clone())),
        Arc::new(FakeHost::new(fixture.runtime.clone())),
    );
    let error = expect_error(
        source.driver_for_session("dbs-unknown").await,
        "an unknown session resolves no connection",
    );
    assert_eq!(error, "Runtime session is unavailable");
    assert_eq!(source.session_schema("dbs-unknown").await, None);
}

#[tokio::test]
async fn the_session_is_mapped_to_its_owning_connection() {
    let fixture = runtime_with(tables_payload());
    let host = Arc::new(FakeHost::new(fixture.runtime.clone()));
    let source = RuntimeSchemaContextSource::new(
        Arc::new(RuntimeLookup(fixture.runtime.clone())),
        host.clone(),
    );
    let session = open_session(&fixture.runtime).await;

    let driver = source
        .driver_for_session(&session)
        .await
        .expect("the fixture profile and driver exist");
    assert_eq!(driver.driver_type(), "sqlite");
    assert_eq!(
        lock(&host.seen).clone(),
        vec![CONNECTION_ID.to_owned()],
        "the host is asked for the connection that owns the session"
    );
}

#[tokio::test]
async fn the_session_schema_comes_from_the_connection_profile() {
    let fixture = runtime_with(tables_payload());
    let session = open_session(&fixture.runtime).await;

    let with_schema = RuntimeSchemaContextSource::new(
        Arc::new(RuntimeLookup(fixture.runtime.clone())),
        Arc::new(FakeHost::new(fixture.runtime.clone()).with_schema(Some("public"))),
    );
    assert_eq!(
        with_schema.session_schema(&session).await,
        Some("public".to_owned())
    );

    let without_schema = RuntimeSchemaContextSource::new(
        Arc::new(RuntimeLookup(fixture.runtime.clone())),
        Arc::new(FakeHost::new(fixture.runtime.clone()).with_schema(None)),
    );
    assert_eq!(without_schema.session_schema(&session).await, None);
}

#[tokio::test]
async fn a_missing_or_disabled_profile_is_reported_as_unavailable() {
    let fixture = runtime_with(tables_payload());
    let session = open_session(&fixture.runtime).await;

    let missing = RuntimeSchemaContextSource::new(
        Arc::new(RuntimeLookup(fixture.runtime.clone())),
        Arc::new(FakeHost::new(fixture.runtime.clone()).without_profile()),
    );
    assert_eq!(
        expect_error(
            missing.driver_for_session(&session).await,
            "a missing profile cannot be read",
        ),
        "Connection profile unavailable"
    );

    let disabled = RuntimeSchemaContextSource::new(
        Arc::new(RuntimeLookup(fixture.runtime.clone())),
        Arc::new(FakeHost::new(fixture.runtime.clone()).disabled()),
    );
    assert_eq!(
        expect_error(
            disabled.driver_for_session(&session).await,
            "a disabled profile cannot be read",
        ),
        "Connection profile unavailable"
    );
    assert_eq!(disabled.session_schema(&session).await, None);
}

#[tokio::test]
async fn an_unavailable_driver_keeps_the_host_wording() {
    let fixture = runtime_with(tables_payload());
    let session = open_session(&fixture.runtime).await;
    let source = RuntimeSchemaContextSource::new(
        Arc::new(RuntimeLookup(fixture.runtime.clone())),
        Arc::new(FakeHost::new(fixture.runtime.clone()).without_driver()),
    );
    assert_eq!(
        expect_error(
            source.driver_for_session(&session).await,
            "the fixture driver is missing",
        ),
        "Driver unavailable"
    );
}

#[tokio::test]
async fn tables_are_read_through_the_shared_execution_gateway() {
    let fixture = runtime_with(tables_payload());
    let session = open_session(&fixture.runtime).await;
    let source = RuntimeSchemaContextSource::new(
        Arc::new(RuntimeLookup(fixture.runtime.clone())),
        Arc::new(FakeHost::new(fixture.runtime.clone())),
    );

    let tables = source
        .tables(&session, "main", Some("public"))
        .await
        .expect("the fixture backend answers list_tables");
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].name, "users");
    assert_eq!(tables[0].schema.as_deref(), Some("public"));
    assert_eq!(tables[0].row_count, Some(42));
}

#[tokio::test]
async fn a_table_schema_is_read_through_the_shared_execution_gateway() {
    let fixture = runtime_with(schema_payload());
    let session = open_session(&fixture.runtime).await;
    let source = RuntimeSchemaContextSource::new(
        Arc::new(RuntimeLookup(fixture.runtime.clone())),
        Arc::new(FakeHost::new(fixture.runtime.clone())),
    );

    let schema = source
        .table_schema(&session, "main", Some("public"), "users")
        .await
        .expect("the fixture backend answers get_table_schema");
    assert_eq!(schema.table_name, "users");
    assert_eq!(schema.columns.len(), 1);
    assert_eq!(schema.columns[0].name, "id");
    assert!(schema.columns[0].is_primary_key);
}

#[tokio::test]
async fn a_malformed_table_schema_fails_with_the_payload_error() {
    let fixture = runtime_with(json!({ "schema": "not-a-schema" }));
    let session = open_session(&fixture.runtime).await;
    let source = RuntimeSchemaContextSource::new(
        Arc::new(RuntimeLookup(fixture.runtime.clone())),
        Arc::new(FakeHost::new(fixture.runtime.clone())),
    );

    let error = source
        .table_schema(&session, "main", None, "users")
        .await
        .expect_err("a malformed payload is not a table schema");
    assert!(
        error.contains("invalid type"),
        "the payload error must reach the caller: {error}"
    );
}

#[tokio::test]
async fn an_unwired_runtime_fails_every_read_with_the_assembly_reason() {
    let source = UnavailableSchemaContextSource::new("ServiceUnavailable: runtime not injected");
    assert_eq!(
        expect_error(
            source.driver_for_session("dbs-1").await,
            "no runtime means no driver",
        ),
        "ServiceUnavailable: runtime not injected"
    );
    assert_eq!(source.session_schema("dbs-1").await, None);
    assert_eq!(
        expect_error(
            source.tables("dbs-1", "main", None).await,
            "no runtime means no tables",
        ),
        "ServiceUnavailable: runtime not injected"
    );
    assert_eq!(
        expect_error(
            source.get_session("dbs-1").await,
            "the Runtime path never exposes a physical handle",
        ),
        "The Runtime metadata source does not expose physical session handles"
    );
}
