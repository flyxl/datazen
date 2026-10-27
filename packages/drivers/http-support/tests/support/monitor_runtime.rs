//! Test host ports for the shared Monitor policy/Workflow SQL execution slice.
//! Real ResourceProviders execute SQL; this does not model desktop receipts or scheduling.
use async_trait::async_trait;
use datazen_driver_api::{resource as dr, session as ds};
use datazen_feature_core::workflow::{
    resource_execution::{PreparedExecution, ResourceExecutionHost},
    shared::error::WorkflowError,
};
use datazen_modules::execution::runtime::driver_mapping as mapping;
use datazen_modules::execution::runtime::{
    connection as rt,
    registry::{backend::*, SessionBackend},
};
use datazen_modules::execution::target::ExecutionTarget;
use datazen_modules::{
    ports::{profile::*, shared::policy::*},
    shared::error::PortError,
    shared::id::*,
    RequestContext,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
struct Profiles {
    revision: AtomicU64,
}

#[async_trait]
impl ProfileRepository for Profiles {
    async fn list(
        &self,
        c: &RequestContext,
        _: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        Ok(vec![self.get(c, ConnectionId::new("p")).await?.unwrap()])
    }
    async fn get(
        &self,
        c: &RequestContext,
        id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        let mut p = ProfileRecord::new(c.organization_id.clone(), id, "test");
        p.config_revision = ConfigRevision::new(self.revision.load(Ordering::SeqCst));
        Ok(Some(p))
    }
    async fn create(
        &self,
        _: &RequestContext,
        _: datazen_modules::connection::dto::ProfileDraft,
        _: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!()
    }
    async fn compare_and_set(
        &self,
        _: &RequestContext,
        _: ConnectionId,
        _: ConfigRevision,
        _: datazen_modules::connection::dto::ProfilePatch,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!()
    }
    async fn disable(
        &self,
        _: &RequestContext,
        _: ConnectionId,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!()
    }
    async fn advance_credential_revision(
        &self,
        _: &RequestContext,
        _: ConnectionId,
    ) -> Result<CredentialRevision, PortError> {
        unreachable!()
    }
}

pub struct Policy {
    pub revoked: std::sync::atomic::AtomicBool,
    target: ExecutionTarget,
    delegation: datazen_modules::shared::context::DelegationRef,
    config_revision: u64,
}
impl Policy {
    fn current(&self, ctx: &RequestContext) -> Result<(), PortError> {
        if self.revoked.load(Ordering::SeqCst) || ctx != &self.context() {
            return Err(PortError::TokenInvalid);
        }
        Ok(())
    }
    fn context(&self) -> RequestContext {
        RequestContext::new(
            self.delegation.organization_id.clone(),
            self.delegation.principal_id.clone(),
            None,
            self.delegation.client_instance_id.clone(),
            datazen_modules::shared::id::RequestId::new("monitor-live-request"),
            Some(self.delegation.delegation_id.clone()),
        )
    }
}
#[async_trait]
impl PolicyService for Policy {
    async fn authorize(
        &self,
        ctx: &RequestContext,
        subject: AuthorizationSubject,
        action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        self.current(ctx)?;
        Ok(
            if subject == AuthorizationSubject::Delegated(self.delegation.clone())
                && matches!(
                    action,
                    AuthorizationAction::Connect | AuthorizationAction::Query
                )
            {
                AuthorizationDecision::Allow
            } else {
                AuthorizationDecision::Deny {
                    reason_code: "outside_monitor_grant",
                }
            },
        )
    }
    async fn resolve_delegation(&self, ctx: &RequestContext) -> Result<DelegationGrant, PortError> {
        self.current(ctx)?;
        Ok(DelegationGrant {
            delegation: self.delegation.clone(),
            actions: vec![AuthorizationAction::Connect, AuthorizationAction::Query],
        })
    }
    async fn verify_delegation(
        &self,
        ctx: &RequestContext,
        delegation: datazen_modules::shared::context::DelegationRef,
    ) -> Result<DelegationGrant, PortError> {
        self.current(ctx)?;
        let grant = self.resolve_delegation(ctx).await?;
        if grant.delegation != delegation {
            return Err(PortError::TokenInvalid);
        }
        Ok(grant)
    }
    async fn authorize_target(
        &self,
        ctx: &RequestContext,
        subject: AuthorizationSubject,
        action: AuthorizationAction,
        target: &ExecutionTarget,
        command: &str,
        input: &serde_json::Value,
    ) -> Result<AuthorizationDecision, PortError> {
        self.current(ctx)?;
        if target != &self.target
            || action == AuthorizationAction::Query && command != "query"
            || action != AuthorizationAction::Connect
                && !input["sql"].as_str().is_some_and(|sql| {
                    let trimmed = sql.trim_start().to_ascii_uppercase();
                    trimmed.starts_with("SELECT ") || trimmed.starts_with("WITH ")
                })
        {
            return Err(PortError::TokenInvalid);
        }
        self.authorize(ctx, subject, action).await
    }
    fn isolation_key(&self, ctx: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        self.current(ctx)?;
        Ok(PolicyIsolationKey::new(format!(
            "monitor:{}:{}",
            self.delegation.delegation_id, self.config_revision
        )))
    }
    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty())
    }
}

#[derive(Default)]
pub struct Lifecycle {
    pub opened: AtomicU64,
    pub closed: AtomicU64,
    pub executions: AtomicU64,
    pub execution_ids: std::sync::Mutex<Vec<ExecutionId>>,
}
pub struct Host {
    pub runtime: Arc<datazen_modules::execution::application::RuntimeConnectionUseCases>,
    pub policy: Arc<Policy>,
    pub lifecycle: Arc<Lifecycle>,
    config: datazen_driver_api::ConnectionConfig,
    driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
    pub context: RequestContext,
    _artifacts_dir: tempfile::TempDir,
}
pub fn host(
    config: datazen_driver_api::ConnectionConfig,
    driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
    provider: Arc<dyn dr::ResourceProvider>,
    context: RequestContext,
    target: ExecutionTarget,
) -> Host {
    let lifecycle = Arc::new(Lifecycle::default());
    let delegation = datazen_modules::shared::context::DelegationRef {
        delegation_id: context
            .delegation_id
            .clone()
            .expect("monitor fixture context carries delegation"),
        organization_id: context.organization_id.clone(),
        principal_id: context.principal_id.clone(),
        client_instance_id: context.client_instance_id.clone(),
        scope: "monitor-live".into(),
        expires_at: datazen_modules::shared::id::Timestamp::new("2099-01-01T00:00:00Z"),
    };
    let policy = Arc::new(Policy {
        revoked: std::sync::atomic::AtomicBool::new(false),
        target,
        delegation,
        config_revision: 1,
    });
    let publisher = Arc::new(std::sync::Mutex::new(None));
    let runtime = Arc::new(
        datazen_modules::execution::application::RuntimeConnectionUseCases::new(
            Arc::new(Profiles {
                revision: AtomicU64::new(1),
            }),
            policy.clone(),
            Arc::new(Backend {
                config: config.clone(),
                lifecycle: lifecycle.clone(),
                provider,
                resources: tokio::sync::Mutex::new(std::collections::HashMap::new()),
                budget: Arc::new(Budget),
                publisher: publisher.clone(),
            }),
        ),
    );
    let artifacts_dir = tempfile::tempdir().unwrap();
    runtime
        .result_sink()
        .with_artifact_store(Arc::new(
            datazen_modules::execution::runtime::artifact::FileArtifactRepository::open(
                artifacts_dir.path(),
            )
            .unwrap(),
        ))
        .unwrap();
    *publisher.lock().unwrap() = Some(runtime.result_sink());
    Host {
        runtime,
        policy,
        lifecycle,
        config,
        driver,
        context,
        _artifacts_dir: artifacts_dir,
    }
}
#[async_trait]
impl ResourceExecutionHost for Host {
    async fn profile(
        &self,
        id: &str,
    ) -> Result<(datazen_driver_api::ConnectionConfig, u64, bool), WorkflowError> {
        self.policy
            .current(&self.context)
            .map_err(|_| WorkflowError::Validation("grant unavailable".into()))?;
        if id != "p" {
            return Err(WorkflowError::Validation("profile outside scope".into()));
        }
        Ok((self.config.clone(), 1, true))
    }
    async fn driver(
        &self,
        _: &datazen_driver_api::ConnectionConfig,
    ) -> Result<Arc<dyn datazen_driver_api::DatabaseDriver>, WorkflowError> {
        Ok(self.driver.clone())
    }
    async fn safe_mode(&self) -> bool {
        false
    }
    async fn prepare(
        &self,
        id: &str,
        revision: u64,
        command: &str,
        target: ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError> {
        self.policy
            .current(&self.context)
            .map_err(|_| WorkflowError::Validation("grant unavailable".into()))?;
        if id != "p"
            || revision != self.policy.config_revision
            || target != self.policy.target
            || command != "query"
        {
            return Err(WorkflowError::Validation("revision outside grant".into()));
        }
        Ok(PreparedExecution {
            use_cases: self.runtime.clone(),
            artifacts: Arc::new(NoArtifactChunks),
            context: self.context.clone(),
            target,
        })
    }
}
/// 监控类测试不读结果制品；端口存在是为了让派发形状成立。
struct NoArtifactChunks;

#[async_trait]
impl datazen_feature_core::services::ports::ArtifactChunkReader for NoArtifactChunks {
    async fn read_chunk(
        &self,
        _ctx: &RequestContext,
        _artifact: &datazen_modules::shared::id::ArtifactId,
        _index: datazen_modules::shared::id::Counter,
    ) -> Result<datazen_modules::execution::ResultChunk, datazen_modules::shared::error::ApiError>
    {
        Err(datazen_modules::shared::error::ApiError::new(
            datazen_modules::shared::error::ApiErrorCode::CapabilityUnsupported,
            "monitor test does not read result artifacts",
        ))
    }
}

struct Budget;
#[async_trait]
impl dr::BudgetPort for Budget {
    async fn acquire_physical_connections(
        &self,
        n: u32,
    ) -> Result<dr::BudgetPermit, dr::ResourceError> {
        Ok(dr::BudgetPermit {
            permit_id: uuid::Uuid::new_v4().to_string(),
            physical_connections: n,
        })
    }
    async fn release_physical_connections(
        &self,
        _: &dr::BudgetPermit,
    ) -> Result<(), dr::ResourceError> {
        Ok(())
    }
}
#[derive(Default)]
struct Sink(std::sync::Mutex<Vec<serde_json::Value>>);
#[async_trait]
impl dr::ResultSink for Sink {
    async fn write(&self, chunk: dr::ResultChunk) -> Result<(), dr::ResourceError> {
        self.0.lock().unwrap().push(serde_json::json!({"kind":"rows", "statementIndex":chunk.statement_index, "sql":chunk.sql, "rows":chunk.rows}));
        Ok(())
    }
    async fn write_output(&self, output: serde_json::Value) -> Result<(), dr::ResourceError> {
        self.0.lock().unwrap().push(output);
        Ok(())
    }
    async fn complete(&self) -> Result<(), dr::ResourceError> {
        Ok(())
    }
    async fn fail(&self, _: &str) -> Result<(), dr::ResourceError> {
        Ok(())
    }
}
struct Backend {
    lifecycle: Arc<Lifecycle>,
    config: datazen_driver_api::ConnectionConfig,
    provider: Arc<dyn dr::ResourceProvider>,
    publisher:
        Arc<std::sync::Mutex<Option<datazen_modules::execution::application::RuntimeResultSink>>>,
    resources: tokio::sync::Mutex<std::collections::HashMap<String, (dr::ResourceHandle, u64)>>,
    budget: Arc<dyn dr::BudgetPort>,
}
#[async_trait]
impl SessionBackend for Backend {
    async fn open(&self, r: OpenResource) -> Result<OpenedResource, rt::ProviderError> {
        let h = self
            .provider
            .acquire_resource(
                &dr::AcquireResourceRequest {
                    connection_config: self.config.clone(),
                    target: mapping::namespace(&r.initial_target),
                    identity_scope: dr::IdentityScope::default(),
                    baseline: dr::Baseline::default(),
                    scope: dr::ResourceScope {
                        purpose: dr::ResourcePurpose::AgentSession,
                        max_physical_connections: 4,
                        holds_open_transaction: true,
                        pin_for_streaming: false,
                    },
                },
                &self.budget,
            )
            .await
            .map_err(mapping::error)?;
        let observed = self
            .provider
            .observe_session(&h)
            .await
            .map_err(mapping::error)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.resources.lock().await.insert(id.clone(), (h, 1));
        self.lifecycle.opened.fetch_add(1, Ordering::SeqCst);
        Ok(OpenedResource {
            context: mapping::context(&observed.context),
            resource_id: id,
            capabilities:
                datazen_modules::execution::runtime::registry::audit::CapabilityVersions::new(
                    "1", "1",
                ),
            capability_snapshot: CapabilityProvenance::new(self.provider.provider_id(), "test", 1),
            driver_supports_cancel: false,
        })
    }
    async fn execute(&self, r: ExecuteOnResource) -> Result<ResourceExecution, rt::ProviderError> {
        self.lifecycle
            .execution_ids
            .lock()
            .unwrap()
            .push(r.execution_id.clone());
        let mut resources = self.resources.lock().await;
        let (h, revision) = resources
            .get_mut(&r.resource_id)
            .ok_or_else(|| rt::ProviderError::SessionLost("closed fixture".into()))?;
        let sink = Sink::default();
        self.lifecycle.executions.fetch_add(1, Ordering::SeqCst);
        let result = self
            .provider
            .execute_on_resource(
                h,
                &datazen_driver_api::QueryExecutionId::new(r.execution_id.as_str()),
                &dr::CommandCall::new(&r.command.command, r.command.input.clone()),
                &sink,
            )
            .await;
        let publisher = self.publisher.lock().unwrap().clone().unwrap();
        for output in std::mem::take(&mut *sink.0.lock().unwrap()) {
            publisher
                .publish_output(&r.execution_id, output)
                .map_err(|_| {
                    rt::ProviderError::ProtocolError("result publication failed".into())
                })?;
        }
        let context = self
            .provider
            .observe_session(h)
            .await
            .map_err(mapping::error)?;
        *revision += 1;
        Ok(ResourceExecution {
            execution_id: r.execution_id,
            stream_id: rt::StreamId::new(uuid::Uuid::new_v4().to_string()),
            // Match DesktopSessionBackend: a driver SQL error still observes
            // the original resource and publishes Failed/Unknown. It must not
            // skip the observation needed for a subsequent owned rollback.
            state: if result
                .as_ref()
                .is_ok_and(|r| r.completion_status == ds::CompletionStatus::Succeeded)
            {
                rt::ExecutionState::Succeeded
            } else {
                rt::ExecutionState::Failed
            },
            effect_outcome: result
                .as_ref()
                .map(|r| mapping::effect(r.effect_outcome))
                .unwrap_or(rt::EffectOutcome::Unknown),
            context_after: mapping::context(&context.context),
            context_revision: *revision,
            handles: vec![],
            cancel_handle: String::new(),
        })
    }
    async fn cancel(&self, _: CancelOnResource) -> Result<ResourceCancel, rt::ProviderError> {
        Err(rt::ProviderError::CapabilityUnsupported)
    }
    async fn finalize_handles(
        &self,
        r: FinalizeHandles,
    ) -> Result<HandleFinalization, rt::ProviderError> {
        assert!(
            r.handles.is_empty(),
            "fixture does not synthesize transaction handles"
        );
        Ok(HandleFinalization {
            finalized: 0,
            remaining: 0,
            effect_outcome: rt::EffectOutcome::NotStarted,
        })
    }
    async fn close(&self, r: CloseResource) -> Result<CloseResourceOutcome, rt::ProviderError> {
        let (h, _) = self
            .resources
            .lock()
            .await
            .remove(&r.resource_id)
            .ok_or_else(|| rt::ProviderError::SessionLost("closed fixture".into()))?;
        self.provider
            .close_resource(&h)
            .await
            .map_err(mapping::error)?;
        self.lifecycle.closed.fetch_add(1, Ordering::SeqCst);
        Ok(CloseResourceOutcome::Closed)
    }
}
