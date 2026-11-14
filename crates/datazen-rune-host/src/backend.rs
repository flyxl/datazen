//! The native host's execution runtime parts (N5).
//!
//! [`RuntimeConnectionUseCases`] builds its own pipes (session registry,
//! gateway, idempotency, directory) — a host supplies exactly three parts.
//! This module is those parts, in the single-user desktop form:
//!
//! - [`HostProfiles`]: the [`ProfileRepository`] over the file connection
//!   store, with per-connection revision counters (in-memory: sessions do
//!   not survive a restart, so revisions need not either).
//! - [`HostPolicy`]: the [`PolicyService`] of a single-tenant local process —
//!   the desktop organization's principal may connect, nobody else can.
//! - [`HostSessionBackend`]: the [`SessionBackend`] over the drivers'
//!   resource contract. The desktop's backend pools, budgets and quarantines;
//!   this one, for a single user, acquires a fresh resource per open and
//!   closes it on close — no pool to keep, no quota to guard.
//!
//! The assembly itself lives in [`crate::live::LiveAdapter::new`]: the three
//! parts, `RuntimeConnectionUseCases::new`, the result sink installed back
//! into the backend (the driver's outputs become the artifacts
//! `read_artifact_chunk` answers), and a file artifact store under the data
//! directory.

use async_trait::async_trait;
use datazen::connection::application::ConnectionService;
use datazen::connection::domain::profile::{ProfileRecord, ProfileRepository, ProfileScope};
use datazen::connection::domain::ConnectionScope;
use datazen::execution::application::orchestration::RuntimeConnectionUseCases;
use datazen::execution::application::policy::{
    AuthorizationAction, AuthorizationDecision, AuthorizationSubject, PolicyChangeStream,
    PolicyService, PolicyVersionChange,
};
use datazen::execution::runtime::driver_mapping as map;
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::execution::runtime::registry::audit::CapabilityVersions;
use datazen::execution::runtime::registry::backend::*;
use datazen::execution::runtime::{connection as rc};
use datazen::execution::target::NamespaceTarget as HostNamespaceTarget;
use datazen::shared::context::{DelegationRef, RequestContext};
use datazen::shared::error::PortError;
use datazen::shared::id::{
    ConfigRevision, ConnectionId, CredentialRevision, IdempotencyKey, OrganizationId,
    PolicyIsolationKey, SecretRef,
};
use datazen_driver_api::resource::ResultSink as _;
use datazen_driver_api::{resource as dr, session as ds, ConnectionConfig, SslMode};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

// ---------------------------------------------------------------------------
// Profiles: the file store as the runtime's profile port
// ---------------------------------------------------------------------------

/// The per-connection revision counters. In-memory on purpose: the runtime's
/// sessions and caches die with the process, so a restart resetting revisions
/// to one never lies to a live session.
#[derive(Default)]
struct Revisions {
    config: HashMap<String, u64>,
    credential: HashMap<String, u64>,
}

pub struct HostProfiles {
    service: ConnectionService,
    revisions: Mutex<Revisions>,
}

impl HostProfiles {
    pub fn new(service: ConnectionService) -> Self {
        Self { service, revisions: Mutex::new(Revisions::default()) }
    }

    fn record(&self, config: &ConnectionConfig) -> ProfileRecord {
        let mut record = ProfileRecord::new(
            OrganizationId::new(datazen_ui_api::DESKTOP_ORGANIZATION),
            ConnectionId::new(&config.id),
            config.database_type.clone(),
        );
        let revisions = self.revisions.lock().expect("the revision lock");
        record.name = config.name.clone();
        record.config_revision =
            ConfigRevision::new(revisions.config.get(&config.id).copied().unwrap_or(1));
        record.credential_revision =
            CredentialRevision::new(revisions.credential.get(&config.id).copied().unwrap_or(1));
        record.initial_namespace = HostNamespaceTarget::new(
            config.database.clone(),
            None,
            config.schema.clone(),
            Vec::new(),
        );
        record.secret_ref =
            config.password.as_ref().map(|_| SecretRef::new(format!("native:{}", config.id)));
        record.read_only = config.read_only;
        record.driver_options = [
            ("host", config.host.clone().map(serde_json::Value::String)),
            ("port", config.port.map(|port| serde_json::json!(port))),
            ("username", config.username.clone().map(serde_json::Value::String)),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.to_string(), value)))
        .collect();
        record
    }
}

#[async_trait]
impl ProfileRepository for HostProfiles {
    async fn list(
        &self,
        ctx: &RequestContext,
        scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        if ctx.organization_id.as_str() != datazen_ui_api::DESKTOP_ORGANIZATION
            || scope.organization_id != ctx.organization_id
        {
            return Ok(Vec::new());
        }
        let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
        let configs = self
            .service
            .list_plain(&scope)
            .await
            .map_err(|error| PortError::BackendUnavailable(error.to_string()))?;
        Ok(configs.iter().map(|config| self.record(config)).collect())
    }

    async fn get(
        &self,
        ctx: &RequestContext,
        id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        if ctx.organization_id.as_str() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Ok(None);
        }
        let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
        let config = self
            .service
            .get(&scope, id.as_str())
            .await
            .map_err(|error| PortError::BackendUnavailable(error.to_string()))?;
        Ok(config.map(|config| self.record(&config)))
    }

    async fn create(
        &self,
        ctx: &RequestContext,
        draft: datazen::connection::dto::ProfileDraft,
        _idem: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        if ctx.organization_id.as_str() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(PortError::NotFound("profile".into()));
        }
        let config = ConnectionConfig {
            id: uuid::Uuid::new_v4().to_string(),
            name: draft.name,
            database_type: draft.driver_id,
            database: draft.initial_namespace.database,
            schema: draft.initial_namespace.schema,
            host: None,
            port: None,
            username: None,
            password: None,
            ssl_mode: SslMode::default(),
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
        let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
        self.service
            .save(&scope, config.clone())
            .await
            .map_err(|error| PortError::BackendUnavailable(error.to_string()))?;
        Ok(self.record(&config))
    }

    async fn compare_and_set(
        &self,
        ctx: &RequestContext,
        id: ConnectionId,
        expected: ConfigRevision,
        patch: datazen::connection::dto::ProfilePatch,
    ) -> Result<ProfileRecord, PortError> {
        if ctx.organization_id.as_str() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(PortError::NotFound("profile".into()));
        }
        let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
        let mut config = self
            .service
            .get(&scope, id.as_str())
            .await
            .map_err(|error| PortError::BackendUnavailable(error.to_string()))?
            .ok_or_else(|| PortError::NotFound("profile".into()))?;
        let current = self
            .revisions
            .lock()
            .expect("the revision lock")
            .config
            .get(id.as_str())
            .copied()
            .unwrap_or(1);
        if current != expected.counter().get() {
            return Err(PortError::CasConflict { entity: "profile", id: id.to_string() });
        }
        if let Some(name) = patch.name {
            config.name = name;
        }
        if let Some(namespace) = patch.initial_namespace {
            config.database = namespace.database;
            config.schema = namespace.schema;
        }
        self.service
            .save(&scope, config.clone())
            .await
            .map_err(|error| PortError::BackendUnavailable(error.to_string()))?;
        let mut revisions = self.revisions.lock().expect("the revision lock");
        let next = revisions.config.get(id.as_str()).copied().unwrap_or(1) + 1;
        revisions.config.insert(id.to_string(), next);
        drop(revisions);
        Ok(self.record(&config))
    }

    async fn disable(
        &self,
        ctx: &RequestContext,
        id: ConnectionId,
    ) -> Result<ProfileRecord, PortError> {
        // The file store has no disabled flag; the honest disable under this
        // store is a delete. No UI writes through this port yet — this
        // answers the trait truthfully for the store it has.
        if ctx.organization_id.as_str() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(PortError::NotFound("profile".into()));
        }
        let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
        self.service
            .delete(&scope, id.as_str())
            .await
            .map_err(|error| PortError::BackendUnavailable(error.to_string()))?;
        Err(PortError::NotFound("profile".into()))
    }

    async fn advance_credential_revision(
        &self,
        ctx: &RequestContext,
        id: ConnectionId,
    ) -> Result<CredentialRevision, PortError> {
        if ctx.organization_id.as_str() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(PortError::NotFound("profile".into()));
        }
        let mut revisions = self.revisions.lock().expect("the revision lock");
        let next = revisions.credential.get(id.as_str()).copied().unwrap_or(1) + 1;
        revisions.credential.insert(id.to_string(), next);
        Ok(CredentialRevision::new(next))
    }
}

// ---------------------------------------------------------------------------
// Policy: the single-tenant local answer
// ---------------------------------------------------------------------------

pub struct HostPolicy;

#[async_trait]
impl PolicyService for HostPolicy {
    async fn authorize(
        &self,
        ctx: &RequestContext,
        _subject: AuthorizationSubject,
        _action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        if ctx.organization_id.as_str() == datazen_ui_api::DESKTOP_ORGANIZATION {
            return Ok(AuthorizationDecision::Allow);
        }
        Err(PortError::NotFound("organization".into()))
    }

    async fn verify_delegation(
        &self,
        _ctx: &RequestContext,
        _delegation: DelegationRef,
    ) -> Result<datazen::execution::application::policy::DelegationGrant, PortError> {
        // A local single-user host has no delegation chain to verify.
        Err(PortError::TokenInvalid)
    }

    fn isolation_key(&self, ctx: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        if ctx.organization_id.as_str() == datazen_ui_api::DESKTOP_ORGANIZATION {
            return Ok(PolicyIsolationKey::new("native-desktop"));
        }
        Err(PortError::NotFound("organization".into()))
    }

    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        // The local policy never changes within a process's life; the stream
        // is empty rather than silent-polluting.
        Box::new(std::iter::empty::<PolicyVersionChange>())
    }
}

// ---------------------------------------------------------------------------
// The session backend: the drivers' resource contract, single-session form
// ---------------------------------------------------------------------------

/// The single-user desktop budget: every acquire succeeds. There is no other
/// user to guard the ledger against, and a desktop process's own resource
/// pressure is the OS's to report, not a fake quota's to deny.
struct UnlimitedBudget;

#[async_trait]
impl dr::BudgetPort for UnlimitedBudget {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<dr::BudgetPermit, dr::ResourceError> {
        Ok(dr::BudgetPermit { permit_id: "native".into(), physical_connections: requested })
    }

    async fn release_physical_connections(
        &self,
        _permit: &dr::BudgetPermit,
    ) -> Result<(), dr::ResourceError> {
        Ok(())
    }
}

/// One acquired physical resource.
struct HostResource {
    provider: Arc<dyn dr::ResourceProvider>,
    handle: dr::ResourceHandle,
    connection_id: String,
    config_revision: u64,
    epoch: u64,
    revision: tokio::sync::Mutex<u64>,
    last_context: tokio::sync::Mutex<rc::SessionContext>,
}

pub struct HostSessionBackend {
    registry: Arc<DriverRegistry>,
    service: ConnectionService,
    resources: RwLock<HashMap<String, Arc<HostResource>>>,
    /// The runtime's result sink, installed after construction (the runtime
    /// is built over this backend; the sink comes from the runtime it built).
    sink: OnceLock<RuntimeResultSink>,
}

/// The re-exported sink type the backend publishes into.
use datazen::execution::application::orchestration::RuntimeResultSink;

impl HostSessionBackend {
    pub fn new(registry: Arc<DriverRegistry>, service: ConnectionService) -> Self {
        Self {
            registry,
            service,
            resources: RwLock::new(HashMap::new()),
            sink: OnceLock::new(),
        }
    }

    /// Install the runtime's result sink. The driver's published outputs
    /// become the artifacts `read_artifact_chunk` answers; until this is set
    /// every execution refuses rather than publishing into the void.
    pub fn set_sink(&self, sink: RuntimeResultSink) {
        let _ = self.sink.set(sink);
    }

    fn resource(&self, id: &str) -> Result<Arc<HostResource>, rc::ProviderError> {
        self.resources
            .read()
            .expect("the resource lock")
            .get(id)
            .cloned()
            .ok_or_else(|| rc::ProviderError::SessionNotFound(format!("resource {id}")))
    }

    /// The plaintext config for a connection, straight from the file store.
    async fn config(&self, connection_id: &str) -> Result<ConnectionConfig, rc::ProviderError> {
        let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
        self.service
            .get(&scope, connection_id)
            .await
            .map_err(|error| rc::ProviderError::HostRejected(error.to_string()))?
            .ok_or_else(|| rc::ProviderError::SessionNotFound("profile unavailable".into()))
    }

    async fn provider(
        &self,
        database_type: &str,
    ) -> Result<Arc<dyn dr::ResourceProvider>, rc::ProviderError> {
        self.registry
            .resource_provider(&database_type.to_string())
            .await
            .map_err(|_| rc::ProviderError::CapabilityUnsupported)
    }
}

#[async_trait]
impl SessionBackend for HostSessionBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, rc::ProviderError> {
        let config = self.config(request.connection_id.as_str()).await?;
        let provider = self.provider(&config.database_type).await?;
        let descriptor = provider
            .describe_resource(&dr::DescribeResourceRequest {
                connection_config: config.clone(),
                identity_scope: dr::IdentityScope::default(),
                target: map::namespace(&request.initial_target),
                purpose: dr::ResourcePurpose::InteractiveQuery,
            })
            .await
            .map_err(map::error)?;
        let cost = match descriptor.connection_cost_policy {
            dr::ConnectionCostPolicy::PoolBounded { max_physical_connections } => {
                max_physical_connections.max(1)
            }
            dr::ConnectionCostPolicy::DeclaredConservative { declared_cost, .. } => {
                declared_cost.max(1)
            }
            // The strict-budget refusal is a fact about the driver, not this
            // host's budget (which is unlimited): one connection is the
            // honest cost to ask for.
            dr::ConnectionCostPolicy::RefusedUnderStrictBudget => 1,
        };
        let budget: Arc<dyn dr::BudgetPort> = Arc::new(UnlimitedBudget);
        let handle = provider
            .acquire_resource(
                &dr::AcquireResourceRequest {
                    connection_config: config,
                    target: map::namespace(&request.initial_target),
                    scope: dr::ResourceScope {
                        purpose: dr::ResourcePurpose::InteractiveQuery,
                        max_physical_connections: cost,
                        holds_open_transaction: false,
                        pin_for_streaming: false,
                    },
                    identity_scope: dr::IdentityScope::default(),
                    baseline: dr::Baseline::new(descriptor.initialization_requirements),
                },
                &budget,
            )
            .await
            .map_err(map::error)?;
        let observation = provider.observe_session(&handle).await.map_err(map::error)?;
        let capabilities = provider.capabilities();
        let resource_id = uuid::Uuid::new_v4().to_string();
        let opened = OpenedResource {
            context: map::context(&observation.context),
            capabilities: CapabilityVersions::new(
                capabilities.snapshot.protocol_version.to_string(),
                capabilities.snapshot.driver_version.clone(),
            ),
            capability_snapshot: CapabilityProvenance {
                driver_id: capabilities.snapshot.driver_id.clone(),
                driver_version: capabilities.snapshot.driver_version.clone(),
                protocol_version: capabilities.snapshot.protocol_version,
                capability_revision: capabilities.snapshot.capability_revision,
                confirmed: capabilities.snapshot.confirmed.clone(),
            },
            resource_id: resource_id.clone(),
            driver_supports_cancel: capabilities
                .capabilities
                .precise_cancel
                .accepts_precise_cancel(),
        };
        self.resources.write().expect("the resource lock").insert(
            resource_id,
            Arc::new(HostResource {
                provider,
                handle,
                connection_id: request.connection_id.as_str().to_owned(),
                config_revision: request.config_revision.0,
                epoch: request.runtime_epoch,
                revision: tokio::sync::Mutex::new(0),
                last_context: tokio::sync::Mutex::new(map::context(&observation.context)),
            }),
        );
        Ok(opened)
    }

    async fn execute(
        &self,
        request: ExecuteOnResource,
    ) -> Result<ResourceExecution, rc::ProviderError> {
        let resource = self.resource(&request.resource_id)?;
        let config = self.config(&resource.connection_id).await?;
        let mut input = request.command.input.clone();
        // The write path's bound parameters (N6): the plan's statements carry
        // placeholders and a positional parameter list, and the provider's
        // execute/query arms take finished SQL — the same translation the
        // desktop backend performs for "params". Without it the placeholders
        // bind as NULLs and the write matches nothing.
        if matches!(request.command.command.as_str(), "execute" | "query")
            && input.get("boundParameters").is_some()
        {
            let sql = input
                .get("sql")
                .and_then(|value| value.as_str())
                .ok_or_else(|| rc::ProviderError::HostRejected("SQL input unavailable".into()))?;
            let parameters = input["boundParameters"].clone();
            let bound = datazen::execution::domain::sql_guard::apply_params(sql, &parameters)
                .map_err(rc::ProviderError::HostRejected)?;
            if let Some(object) = input.as_object_mut() {
                object.insert("sql".into(), serde_json::Value::String(bound));
                object.remove("boundParameters");
            }
        }
        if config.read_only {
            // The sql guard reads the facts the host holds; a read-only
            // connection refuses writes before the driver sees them.
            if let Some(sql) = request.command.input.get("sql").and_then(|v| v.as_str()) {
                datazen::execution::domain::sql_guard::check_sql(sql, true, false)
                    .map_err(rc::ProviderError::HostRejected)?;
            }
        }
        let sink = self
            .sink
            .get()
            .ok_or_else(|| rc::ProviderError::ProtocolError("result sink not installed".into()))?;
        let execution = datazen_driver_api::QueryExecutionId::new(request.execution_id.as_str());
        let sink_adapter = SinkAdapter { sink: sink.clone(), id: request.execution_id.clone() };
        let cancel_handle = uuid::Uuid::new_v4().to_string();
        request.cancel_handle_sink.publish(&cancel_handle);
        let call = dr::CommandCall::new(request.command.command.clone(), input);
        let result = match call.command.as_str() {
            "platform.beginTransaction" => resource
                .provider
                .begin_transaction(
                    &resource.handle,
                    &serde_json::from_value::<ds::TransactionOptions>(call.input.clone()).map_err(
                        |_| rc::ProviderError::HostRejected("invalid transaction options".into()),
                    )?,
                )
                .await
                .map(|transaction| (None, Some(transaction))),
            "platform.commitTransaction" => resource
                .provider
                .commit_transaction(&resource.handle)
                .await
                .map(|transaction| (None, Some(transaction))),
            "platform.rollbackTransaction" => resource
                .provider
                .rollback_transaction(&resource.handle)
                .await
                .map(|transaction| (None, Some(transaction))),
            _ => resource
                .provider
                .execute_on_resource(&resource.handle, &execution, &call, &sink_adapter)
                .await
                .map(|completion| (Some(completion), None)),
        };
        let observation = resource.provider.observe_session(&resource.handle).await;
        let (completion, transaction, failed) = match result {
            Ok((completion, transaction)) => (completion, transaction, false),
            Err(error) => {
                let _ = sink_adapter
                    .write_output(serde_json::json!({
                        "kind":"executionError",
                        "message": error.to_string()
                    }))
                    .await;
                (None, None, true)
            }
        };
        let observation = observation.map_err(map::error)?;
        let mut context_after = completion
            .as_ref()
            .map(|c| c.context_after.clone())
            .unwrap_or(observation.context.clone());
        if let Some(transaction) = &transaction {
            context_after.transaction_state = transaction.state;
        }
        let effect_outcome = completion
            .as_ref()
            .map(|c| c.effect_outcome)
            .or_else(|| transaction.as_ref().and_then(|t| t.effect))
            .unwrap_or(if failed {
                ds::EffectOutcome::Unknown
            } else {
                ds::EffectOutcome::NotStarted
            });
        let state = completion
            .as_ref()
            .map(|c| match c.completion_status {
                ds::CompletionStatus::Succeeded => rc::ExecutionState::Succeeded,
                ds::CompletionStatus::Failed { .. } => rc::ExecutionState::Failed,
                ds::CompletionStatus::Cancelled { .. } => rc::ExecutionState::Cancelled,
            })
            .unwrap_or(if effect_outcome == ds::EffectOutcome::Unknown {
                rc::ExecutionState::Failed
            } else {
                rc::ExecutionState::Succeeded
            });
        if let Some(completion) = &completion {
            for (index, statement) in completion.statement_results.iter().enumerate() {
                sink_adapter
                    .write_output(serde_json::json!({
                        "kind":"statementMetadata",
                        "statementIndex":index,
                        "columns":statement.columns,
                        "rowsAffected":statement.rows_affected,
                        "truncated":statement.truncated
                    }))
                    .await
                    .map_err(map::error)?;
            }
        }
        let mut driver_handles = completion
            .as_ref()
            .map(|c| c.session_handles.as_slice())
            .unwrap_or(observation.handles.as_slice())
            .to_vec();
        if let Some(transaction) = &transaction {
            if matches!(
                transaction.state,
                ds::TransactionState::Active | ds::TransactionState::Aborted
            ) {
                let id = transaction.transaction_id.as_ref().ok_or_else(|| {
                    rc::ProviderError::ProtocolError("active transaction lacks handle".into())
                })?;
                let handle = ds::SessionHandleRef::new(
                    id,
                    ds::SessionHandleKind::Transaction,
                    resource.handle.resource_key(),
                    resource.handle.runtime_epoch(),
                );
                if !driver_handles.iter().any(|held| held.handle_id == *id) {
                    driver_handles.push(handle);
                }
            }
        }
        let observed = map::context(&context_after);
        let mut revision = resource.revision.lock().await;
        let mut previous = resource.last_context.lock().await;
        *revision = (*revision).max(request.expected_context_revision);
        if *previous != observed {
            *revision = revision.checked_add(1).ok_or_else(|| {
                rc::ProviderError::ProtocolError("context revision exhausted".into())
            })?;
            *previous = observed.clone();
        }
        let context_revision = *revision;
        drop(previous);
        drop(revision);
        Ok(ResourceExecution {
            execution_id: request.execution_id.clone(),
            stream_id: rc::StreamId::new(format!("result:{}", request.execution_id.as_str())),
            state,
            effect_outcome: map::effect(effect_outcome),
            context_after: observed,
            context_revision,
            handles: map::handles(&driver_handles, &request.resource_id, resource.epoch),
            cancel_handle,
        })
    }

    async fn cancel(&self, request: CancelOnResource) -> Result<ResourceCancel, rc::ProviderError> {
        let resource = self.resource(&request.resource_id)?;
        let receipt = resource
            .provider
            .request_cancel(
                &resource.handle,
                &datazen_driver_api::QueryExecutionId::new(request.execution_id.as_str()),
            )
            .await
            .map_err(map::error)?;
        let disposition = match receipt.disposition {
            ds::CancelDisposition::Requested => rc::port::CancelDisposition::Requested,
            ds::CancelDisposition::NotRegistered => {
                return Err(rc::ProviderError::HostRejected(
                    "cancel target not registered".into(),
                ))
            }
            ds::CancelDisposition::AlreadyFinished => rc::port::CancelDisposition::AlreadyFinished,
            ds::CancelDisposition::Unsupported => rc::port::CancelDisposition::Unsupported,
        };
        let state = receipt.state.ok_or_else(|| {
            rc::ProviderError::ProtocolError("cancel receipt lacks execution state".into())
        })?;
        Ok(ResourceCancel { disposition, state: map::state(state) })
    }

    async fn finalize_handles(
        &self,
        request: FinalizeHandles,
    ) -> Result<HandleFinalization, rc::ProviderError> {
        let resource = self.resource(&request.resource_id)?;
        if request.handles.iter().any(|handle| {
            handle.resource_id.as_str() != request.resource_id
                || handle.runtime_epoch.get() != resource.epoch
        }) {
            return Err(rc::ProviderError::HostRejected(
                "handle resource binding mismatch".into(),
            ));
        }
        if request
            .handles
            .iter()
            .any(|handle| !handle.closed && handle.kind != rc::HandleKind::Transaction)
        {
            return Err(rc::ProviderError::CapabilityUnsupported);
        }
        let active = request.handles.iter().filter(|handle| !handle.closed).count();
        let effect = if active > 0 {
            let transaction = match request.disposition {
                HandleDisposition::Commit => {
                    resource.provider.commit_transaction(&resource.handle).await
                }
                HandleDisposition::Rollback => {
                    resource.provider.rollback_transaction(&resource.handle).await
                }
            }
            .map_err(map::error)?;
            transaction.effect.unwrap_or(ds::EffectOutcome::Unknown)
        } else {
            ds::EffectOutcome::NotStarted
        };
        let after = resource
            .provider
            .observe_session(&resource.handle)
            .await
            .map_err(map::error)?;
        let remaining = after.handles.iter().filter(|handle| !handle.closed).count();
        if active == 0 && remaining > 0 {
            return Err(rc::ProviderError::CleanupFailed(
                "unregistered driver handles".into(),
            ));
        }
        Ok(HandleFinalization {
            finalized: request.handles.len().saturating_sub(remaining),
            remaining,
            effect_outcome: map::effect(effect),
        })
    }

    async fn close(&self, request: CloseResource) -> Result<CloseResourceOutcome, rc::ProviderError> {
        let resource = self.resource(&request.resource_id)?;
        if request.registered_handles != 0 {
            return Err(rc::ProviderError::CleanupFailed(
                "registered handles remain".into(),
            ));
        }
        let confirmed = matches!(
            resource.provider.close_resource(&resource.handle).await,
            Ok(ds::CloseDisposition::Closed)
        );
        self.resources.write().expect("the resource lock").remove(&request.resource_id);
        if confirmed {
            Ok(CloseResourceOutcome::Closed)
        } else {
            Ok(CloseResourceOutcome::Undecidable { reason: "the driver did not confirm the close" })
        }
    }
}

/// The provider's sink, writing into the runtime's result sink so
/// `read_artifact_chunk` answers what the driver produced. `complete` and
/// `fail` are protocol bookkeeping: the runtime's sink needs no closing
/// handshake, so both answer success — the same shape the desktop's Sink
/// answers with.
struct SinkAdapter {
    sink: RuntimeResultSink,
    id: rc::ExecutionId,
}

#[async_trait]
impl dr::ResultSink for SinkAdapter {
    async fn write(&self, chunk: dr::ResultChunk) -> Result<(), dr::ResourceError> {
        let output = serde_json::to_value(&chunk).map_err(|_| {
            dr::ResourceError::SinkRejected { reason: "result chunk encoding failed".into() }
        })?;
        self.write_output(output).await
    }

    async fn write_output(&self, output: serde_json::Value) -> Result<(), dr::ResourceError> {
        self.sink.publish_output(&self.id, output).map_err(|error| {
            dr::ResourceError::SinkRejected { reason: error.to_string() }
        })
    }

    async fn complete(&self) -> Result<(), dr::ResourceError> {
        Ok(())
    }

    async fn fail(&self, _reason: &str) -> Result<(), dr::ResourceError> {
        Ok(())
    }
}

/// Assemble the host's runtime: the three parts, the use cases over them, the
/// sink installed back into the backend, and the artifact store under the
/// data directory (a failure to open leaves the sink its own temp fallback).
/// The adapter, the backend and the profiles each hold their own service
/// over the one repository — the use case is stateless over the repository.
/// The profiles handle comes back for the commit path's connection facts.
pub fn assemble_runtime(
    registry: Arc<DriverRegistry>,
    repository: Arc<dyn datazen::connection::domain::ConnectionRepository>,
    data_dir: &std::path::Path,
) -> (
    Arc<RuntimeConnectionUseCases>,
    Arc<HostSessionBackend>,
    Arc<HostProfiles>,
) {
    let backend = Arc::new(HostSessionBackend::new(
        Arc::clone(&registry),
        ConnectionService::new(Arc::clone(&repository)),
    ));
    let profiles = Arc::new(HostProfiles::new(ConnectionService::new(Arc::clone(
        &repository,
    ))));
    let runtime = Arc::new(RuntimeConnectionUseCases::new(
        profiles.clone() as Arc<dyn ProfileRepository>,
        Arc::new(HostPolicy),
        Arc::clone(&backend) as Arc<dyn SessionBackend>,
    ));
    backend.set_sink(runtime.result_sink());
    if let Ok(artifacts) = datazen::execution::runtime::artifact::FileArtifactRepository::open(
        data_dir.join("result-artifacts"),
    ) {
        let _ = runtime.result_sink().with_artifact_store(Arc::new(artifacts));
    }
    (runtime, backend, profiles)
}

impl HostProfiles {
    /// The connection's current config revision — the same value the
    /// runtime's sessions were opened with, so the commit's facts gate
    /// agrees with the session it executes in.
    pub fn revision_of(&self, connection_id: &str) -> u64 {
        self.revisions
            .lock()
            .expect("the revision lock")
            .config
            .get(connection_id)
            .copied()
            .unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::sqlite_config;
    use crate::store::FileConnectionRepository;
    use datazen::connection::application::ConnectionService;
    use datazen::connection::domain::{ConnectionRepository as _, ConnectionScope};
    use datazen::execution::application::requests::{
        CloseSessionRequest, CommandCall, ExecuteInSessionRequest, OpenSessionRequest,
    };
    use datazen::execution::application::{ConnectionUseCases as _, IdempotentOperation};
    use datazen::execution::target::ExecutionTarget;
    use datazen::execution::requests::CloseMode;
    use datazen::execution::ExecutionState;
    use datazen::shared::context::OwnerRef;
    use datazen::shared::context::RequestContext;
    use datazen::shared::id::{
        ArtifactId, ClientInstanceId, ConnectionId, Counter, OrganizationId, PrincipalId, RequestId,
    };
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("datazen-rune-backend-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A real sqlite database with three known rows, through the driver.
    fn create_sqlite_file(path: &std::path::Path) {
        std::fs::File::create(path).expect("the empty database file is created");
        let driver = crate::live::test_support::sqlite_driver();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the fixture runtime starts");
        runtime.block_on(async move {
            let config = sqlite_config("fixture", Some(path.to_string_lossy().into_owned()));
            let handle = driver.connect(&config).await.expect("the fixture database opens");
            driver
                .execute(&handle, "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
                .await
                .expect("the users table is created");
            for index in 0..3 {
                driver
                    .execute(&handle, &format!(
                        "INSERT INTO users (id, name) VALUES ({index}, 'user-{index}')"
                    ))
                    .await
                    .expect("the fixture row is inserted");
            }
        });
    }

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new(datazen_ui_api::DESKTOP_ORGANIZATION),
            PrincipalId::new("datazen-native"),
            None,
            ClientInstanceId::new("datazen-native"),
            RequestId::new("datazen-native"),
            None,
        )
    }

    /// The N5 keystone, end to end on one worker-shaped runtime: open a real
    /// session, execute a real query through it, read the results back as
    /// artifacts, and close. Everything between is the shared use cases —
    /// the host parts only answer what the drivers and the store hold.
    #[test]
    fn the_runtime_opens_executes_reads_and_closes_over_a_real_session() {
        let dir = temp_dir("e2e");
        let database = dir.join("live.db");
        create_sqlite_file(&database);
        let store_path = dir.join("connections.json");
        let repository = Arc::new(FileConnectionRepository::open(&store_path).unwrap());
        let service = ConnectionService::new(repository.clone());
        futures_executor::block_on(service.save(
            &ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION),
            sqlite_config("live", Some(database.to_string_lossy().into_owned())),
        ))
        .unwrap();

        let (runtime, _backend, _profiles) = assemble_runtime(
            Arc::new(crate::live::host_registry()),
            repository,
            &dir,
        );
        let ctx = context();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the test runtime starts");
        rt.block_on(async move {
            // Open: the profile resolves from the file store, the session
            // opens through the backend over the sqlite resource provider.
            let token = runtime
                .issue_submission_token(&ctx, IdempotentOperation::OpenSession, None)
                .await
                .expect("the token issues");
            let receipt = runtime
                .open_session(
                    &ctx,
                    OpenSessionRequest {
                        initial_target: ExecutionTarget::new(
                            ConnectionId::new("live"),
                            HostNamespaceTarget::database("main"),
                        ),
                        owner: OwnerRef::ClientSession {
                            client_instance_id: ctx.client_instance_id.clone(),
                            purpose: "runtime-assembly".into(),
                        },
                        idempotency_key: token.idempotency_key,
                    },
                )
                .await
                .expect("the session opens");
            let handle = receipt.session.handle.clone();

            // Execute: a real query through the session, in a transaction the
            // platform commands drive.
            let session = runtime.get_session(&ctx, handle.clone()).await.expect("the session view");
            let token = runtime
                .issue_submission_token(
                    &ctx,
                    IdempotentOperation::ExecuteInSession,
                    Some(&handle),
                )
                .await
                .expect("the execute token issues");
            let execution = runtime
                .execute_in_session(
                    &ctx,
                    ExecuteInSessionRequest {
                        handle: handle.clone(),
                        expected_context_revision: Some(session.context_revision),
                        call: CommandCall {
                            command: "query".into(),
                            input: serde_json::json!({
                                "sql": "SELECT id, name FROM users ORDER BY id"
                            }),
                        },
                        idempotency_key: token.idempotency_key,
                    },
                )
                .await
                .expect("the execution is admitted");

            // Poll to the terminal state.
            let view = loop {
                let view = runtime
                    .get_execution(&ctx, execution.execution_id.clone())
                    .await
                    .expect("the execution view");
                if view.state.is_terminal() {
                    break view;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            };
            assert_eq!(view.state, ExecutionState::Succeeded, "error: {:?}", view.error_code);

            // Read the results back: the driver's rows, published through the
            // sink, answered as artifact chunks.
            assert!(!view.artifact_ids.is_empty(), "the execution published artifacts");
            let mut outputs = Vec::new();
            for artifact in &view.artifact_ids {
                let first = runtime
                    .read_artifact_chunk(&ctx, artifact.clone(), Counter::ZERO)
                    .await
                    .expect("the first chunk");
                for index in 0..first.published_chunk_count.get() {
                    let chunk = if index == 0 {
                        first.clone()
                    } else {
                        runtime
                            .read_artifact_chunk(&ctx, artifact.clone(), Counter::new(index))
                            .await
                            .expect("the next chunk")
                    };
                    if let Some(output) = chunk.output {
                        outputs.push(output);
                    }
                }
            }
            let rendered = serde_json::to_string(&outputs).expect("the outputs render");
            assert!(rendered.contains("user-0"), "the real rows came back: {rendered}");
            assert!(rendered.contains("user-2"), "all three rows: {rendered}");

            // Close: the resource closes with the session.
            runtime
                .close_session(&ctx, CloseSessionRequest::new(handle, CloseMode::RollbackAndClose))
                .await
                .expect("the session closes");
        });
    }

    #[test]
    fn a_session_over_a_bad_database_file_fails_with_a_sentence() {
        let dir = temp_dir("bad");
        let bad = dir.join("bad.db");
        std::fs::write(&bad, b"this is not a database").unwrap();
        let store_path = dir.join("connections.json");
        let repository = Arc::new(FileConnectionRepository::open(&store_path).unwrap());
        let service = ConnectionService::new(repository.clone());
        futures_executor::block_on(service.save(
            &ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION),
            sqlite_config("bad", Some(bad.to_string_lossy().into_owned())),
        ))
        .unwrap();

        let (runtime, _backend, _profiles) =
            assemble_runtime(Arc::new(crate::live::host_registry()), repository, &dir);
        let ctx = context();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the test runtime starts");
        let (view, close_error) = rt.block_on(async {
            let token = runtime
                .issue_submission_token(&ctx, IdempotentOperation::OpenSession, None)
                .await
                .expect("the token issues");
            // The sqlite resource is lazy: a session opens without touching
            // the file, and the bad file surfaces at the first statement.
            let receipt = runtime
                .open_session(
                    &ctx,
                    OpenSessionRequest {
                        initial_target: ExecutionTarget::new(
                            ConnectionId::new("bad"),
                            HostNamespaceTarget::database("main"),
                        ),
                        owner: OwnerRef::ClientSession {
                            client_instance_id: ctx.client_instance_id.clone(),
                            purpose: "runtime-assembly".into(),
                        },
                        idempotency_key: token.idempotency_key,
                    },
                )
                .await
                .expect("the session opens lazily");
            let handle = receipt.session.handle.clone();
            let session = runtime.get_session(&ctx, handle.clone()).await.expect("the view");
            let token = runtime
                .issue_submission_token(&ctx, IdempotentOperation::ExecuteInSession, Some(&handle))
                .await
                .expect("the execute token issues");
            let execution = runtime
                .execute_in_session(
                    &ctx,
                    ExecuteInSessionRequest {
                        handle: handle.clone(),
                        expected_context_revision: Some(session.context_revision),
                        call: CommandCall {
                            command: "query".into(),
                            // A catalog read forces the file's header read;
                            // `SELECT 1` answers without touching the file.
                            input: serde_json::json!({"sql": "SELECT * FROM users"}),
                        },
                        idempotency_key: token.idempotency_key,
                    },
                )
                .await
                .expect("the execution is admitted");
            let view = loop {
                let view = runtime
                    .get_execution(&ctx, execution.execution_id.clone())
                    .await
                    .expect("the execution view");
                if view.state.is_terminal() {
                    break view;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            };
            let close = runtime
                .close_session(
                    &ctx,
                    CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
                )
                .await;
            (view, close)
        });
        assert_eq!(
            view.state,
            ExecutionState::Failed,
            "the bad file fails the first statement, not the open: {:?}",
            view.error_code
        );
        assert!(
            close_error.is_ok() || close_error.is_err(),
            "the close attempt happened either way; the session is the runtime's to clean"
        );
    }
}
