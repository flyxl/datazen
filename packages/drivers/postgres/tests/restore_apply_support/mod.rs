//! Test host ports; database effects always come from the real PostgreSQL provider.
use async_trait::async_trait;
use datazen_driver_api::{resource as dr, session as ds};
use datazen_platform_api::{
    error::PortError,
    id::*,
    ports::{policy::*, profile::*},
    RequestContext,
};
use datazen_runtime::{
    connection as rt,
    registry::{backend::*, SessionBackend},
};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
#[path = "../../../../../src-tauri/src/platform/resource_mapping.rs"]
mod mapping;
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
        _: datazen_platform_api::dto::profile::ProfileDraft,
        _: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!()
    }
    async fn compare_and_set(
        &self,
        _: &RequestContext,
        _: ConnectionId,
        _: ConfigRevision,
        _: datazen_platform_api::dto::profile::ProfilePatch,
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
        _: datazen_platform_api::context::DelegationRef,
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

pub fn runtime(
    config: datazen_driver_api::ConnectionConfig,
) -> Arc<datazen_runtime::application::RuntimeConnectionUseCases> {
    Arc::new(
        datazen_runtime::application::RuntimeConnectionUseCases::new(
            Arc::new(Profiles {
                revision: AtomicU64::new(1),
            }),
            Arc::new(Policy {
                allow: AtomicBool::new(true),
            }),
            Arc::new(Backend {
                config,
                provider: datazen_driver_postgres::PostgresResourceProvider::new(Arc::new(
                    datazen_driver_postgres::PostgresDriver::new(),
                )),
                resources: tokio::sync::Mutex::new(std::collections::HashMap::new()),
                budget: Arc::new(Budget),
            }),
        ),
    )
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
struct Sink;
#[async_trait]
impl dr::ResultSink for Sink {
    async fn write(&self, _: dr::ResultChunk) -> Result<(), dr::ResourceError> {
        Ok(())
    }
    async fn write_output(&self, _: serde_json::Value) -> Result<(), dr::ResourceError> {
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
    config: datazen_driver_api::ConnectionConfig,
    provider: datazen_driver_postgres::PostgresResourceProvider,
    resources: tokio::sync::Mutex<std::collections::HashMap<String, (dr::ResourceHandle, u64)>>,
    budget: Arc<dyn dr::BudgetPort>,
}
#[async_trait]
impl SessionBackend for Backend {
    async fn open(&self, r: OpenResource) -> Result<OpenedResource, rt::ProviderError> {
        use dr::ResourceProvider;
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
        Ok(OpenedResource {
            context: mapping::context(&observed.context),
            resource_id: id,
            capabilities: datazen_runtime::registry::audit::CapabilityVersions::new("1", "1"),
            capability_snapshot: CapabilityProvenance::new("postgresql", "test", 1),
            driver_supports_cancel: false,
        })
    }
    async fn execute(&self, r: ExecuteOnResource) -> Result<ResourceExecution, rt::ProviderError> {
        use dr::ResourceProvider;
        let mut resources = self.resources.lock().await;
        let (h, revision) = resources
            .get_mut(&r.resource_id)
            .ok_or_else(|| rt::ProviderError::SessionLost("closed fixture".into()))?;
        let result = self
            .provider
            .execute_on_resource(
                h,
                &datazen_driver_api::QueryExecutionId::new(r.execution_id.as_str()),
                &dr::CommandCall::new(&r.command.command, r.command.input.clone()),
                &Sink,
            )
            .await
            .map_err(mapping::error)?;
        let context = self
            .provider
            .observe_session(h)
            .await
            .map_err(mapping::error)?;
        *revision += 1;
        Ok(ResourceExecution {
            execution_id: r.execution_id,
            stream_id: rt::StreamId::new(uuid::Uuid::new_v4().to_string()),
            state: if result.completion_status == ds::CompletionStatus::Succeeded {
                rt::ExecutionState::Succeeded
            } else {
                rt::ExecutionState::Failed
            },
            effect_outcome: mapping::effect(result.effect_outcome),
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
        use dr::ResourceProvider;
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
        Ok(CloseResourceOutcome::Closed)
    }
}
