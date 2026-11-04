//! Test host ports; database effects always come from the real ResourceProvider.
use async_trait::async_trait;
use datazen::execution::application::policy::{DelegationGrant, PolicyChangeStream};
use datazen::execution::runtime::driver_mapping as mapping;
use datazen::execution::runtime::{
    connection as rt,
    registry::{backend::*, SessionBackend},
};
use datazen::{
    connection::{ProfileRecord, ProfileRepository, ProfileScope},
    execution::application::{
        AuthorizationAction, AuthorizationDecision, AuthorizationSubject, PolicyService,
    },
    shared::error::PortError,
    shared::id::*,
    RequestContext,
};
use datazen_driver_api::{resource as dr, session as ds};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
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
        _: datazen::connection::dto::ProfileDraft,
        _: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!()
    }
    async fn compare_and_set(
        &self,
        _: &RequestContext,
        _: ConnectionId,
        _: ConfigRevision,
        _: datazen::connection::dto::ProfilePatch,
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

struct Policy {
    allow: AtomicBool,
}

#[async_trait]
impl PolicyService for Policy {
    async fn authorize(
        &self,
        _: &RequestContext,
        _: AuthorizationSubject,
        _: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        Ok(if self.allow.load(Ordering::SeqCst) {
            AuthorizationDecision::Allow
        } else {
            AuthorizationDecision::Deny {
                reason_code: "revoked",
            }
        })
    }
    async fn verify_delegation(
        &self,
        _: &RequestContext,
        _: datazen::shared::context::DelegationRef,
    ) -> Result<DelegationGrant, PortError> {
        unreachable!()
    }
    fn isolation_key(&self, _: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        Ok(PolicyIsolationKey::new("test"))
    }
    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty())
    }
}

#[derive(Default)]
pub struct Lifecycle {
    pub opened: AtomicU64,
    pub closed: AtomicU64,
}
pub fn runtime(
    config: datazen_driver_api::ConnectionConfig,
    provider: Arc<dyn dr::ResourceProvider>,
    lifecycle: Arc<Lifecycle>,
) -> Arc<datazen::execution::application::RuntimeConnectionUseCases> {
    let publisher = Arc::new(std::sync::Mutex::new(None));
    let runtime = Arc::new(
        datazen::execution::application::RuntimeConnectionUseCases::new(
            Arc::new(Profiles {
                revision: AtomicU64::new(1),
            }),
            Arc::new(Policy {
                allow: AtomicBool::new(true),
            }),
            Arc::new(Backend {
                config,
                lifecycle,
                provider,
                publisher: publisher.clone(),
                resources: tokio::sync::Mutex::new(std::collections::HashMap::new()),
                budget: Arc::new(Budget),
            }),
        ),
    );
    *publisher.lock().unwrap() = Some(runtime.result_sink());
    runtime
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
struct Sink {
    publisher: datazen::execution::application::RuntimeResultSink,
    execution: ExecutionId,
}
#[async_trait]
impl dr::ResultSink for Sink {
    async fn write(&self, _: dr::ResultChunk) -> Result<(), dr::ResourceError> {
        Ok(())
    }
    async fn write_output(&self, output: serde_json::Value) -> Result<(), dr::ResourceError> {
        self.publisher
            .publish_output(&self.execution, output)
            .map_err(|_| dr::ResourceError::SinkRejected {
                reason: "Runtime output rejected".into(),
            })
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
    publisher: Arc<std::sync::Mutex<Option<datazen::execution::application::RuntimeResultSink>>>,
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
            capabilities: datazen::execution::runtime::registry::audit::CapabilityVersions::new(
                "1", "1",
            ),
            capability_snapshot: CapabilityProvenance::new("postgresql", "test", 1),
            driver_supports_cancel: false,
        })
    }
    async fn execute(&self, r: ExecuteOnResource) -> Result<ResourceExecution, rt::ProviderError> {
        let mut resources = self.resources.lock().await;
        let (h, revision) = resources
            .get_mut(&r.resource_id)
            .ok_or_else(|| rt::ProviderError::SessionLost("closed fixture".into()))?;
        let sink = Sink {
            publisher: self.publisher.lock().unwrap().clone().unwrap(),
            execution: datazen::shared::id::ExecutionId::new(r.execution_id.as_str()),
        };
        let result = self
            .provider
            .execute_on_resource(
                h,
                &datazen_driver_api::QueryExecutionId::new(r.execution_id.as_str()),
                &dr::CommandCall::new(&r.command.command, r.command.input.clone()),
                &sink,
            )
            .await;
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

/// 备份输出端口（测试宿主实现）。
///
/// 桌面宿主的实现（`src-tauri/src/platform/feature_ports.rs` 的
/// `HostBackupBindings`）只是把这件事转发给 runtime；这里照做——测试要覆盖的是
/// **真实**的绑定语义（等完 in-flight 写、拒收迟到输出），不是另写一份。
pub fn backup_binder(
    runtime: Arc<datazen::execution::application::RuntimeConnectionUseCases>,
) -> Arc<dyn datazen::services::ports::BackupOutputBinder> {
    struct Binder(Arc<datazen::execution::application::RuntimeConnectionUseCases>);

    struct Guard(datazen::execution::application::BackupOutputBinding);

    impl datazen::services::ports::BackupOutputGuard for Guard {}

    #[async_trait]
    impl datazen::services::ports::BackupOutputBinder for Binder {
        async fn bind(
            &self,
            ctx: &RequestContext,
            handle: &datazen::execution::SessionHandle,
            target: &datazen::execution::target::ExecutionTarget,
            callback: datazen::execution::BackupOutput,
        ) -> Result<
            Box<dyn datazen::services::ports::BackupOutputGuard>,
            datazen::shared::error::ApiError,
        > {
            let binding = self
                .0
                .bind_backup_output(ctx, handle, target, callback)
                .await?;
            Ok(Box::new(Guard(binding)))
        }
    }

    Arc::new(Binder(runtime))
}
