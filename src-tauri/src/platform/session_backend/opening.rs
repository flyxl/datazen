use super::*;
impl DesktopSessionBackend {
    pub(super) async fn open_resource(
        &self,
        request: OpenResource,
        command: &str,
    ) -> Result<OpenedResource, rc::ProviderError> {
        // The host's metadata dispatcher is authoritative for every host
        // metadata command, including structure capability reads and pure DDL
        // planning. These operations need a short-lived metadata lease, not
        // a retained interactive backend or stateful-session capability.
        let metadata_only = crate::services::schema_metadata::is_metadata_command(command)
            || matches!(
                command,
                "get_table_schema"
                    | "list_tables"
                    | "list_objects"
                    | "get_object_ddl"
                    | "get_schemas"
                    | "list_databases"
            );
        let (config, metadata) = self
            .store
            .platform_profile(request.connection_id.as_str())
            .await
            .ok_or_else(|| rc::ProviderError::SessionNotFound("profile unavailable".into()))?;
        if !metadata.enabled || metadata.config_revision != request.config_revision.0 {
            return Err(rc::ProviderError::HostRejected(
                "profile disabled or revision changed".into(),
            ));
        }
        // Resolve a persisted tunnel definition before asking the driver to describe the
        // resource. The Runtime session owns the resulting tunnel lease below, so it remains
        // alive with pooled resources and with resources quarantined after uncertain close.
        // The definition is read through the `TunnelRepository` port: the connection side
        // only stores `tunnelId`, and the dereference happens here — at connect time.
        let scope =
            datazen_platform_api::ports::scope::OrganizationScope::new(self.organization.clone());
        let config = crate::tunnel::resolve_saved_tunnel(&*self.tunnels, &scope, config)
            .await
            .map_err(|error| map::error(dr::ResourceError::Driver(error)))?;
        let logical = datazen_runtime::resource::LogicalSessionPermit::reserve(
            self.budget.clone(),
            &self.organization,
            &self.principal,
            &request.connection_id,
            matches!(request.owner, rc::OwnerRef::Editor { .. }),
        )
        .map_err(|reason| {
            use datazen_runtime::budget::{ledger::SessionScope, DenialReason};
            // 额度型拒绝与物理/队列型拒绝走不同的对外码：前者的空位只能靠回收
            // 或用户关闭编辑器腾出来，后者会随并发退去。混成一个 `ResourceBusy`
            // 会让前端把「额度已满」当成可重试的忙，用户只看到一句通用失败。
            let quota = matches!(
                &reason,
                DenialReason::SessionQuotaExceeded { .. }
                    | DenialReason::ConnectedEditorsExhausted { .. }
            );
            let (category, cap, used) = match reason {
                DenialReason::SessionQuotaExceeded { scope, cap, used } => (
                    match scope {
                        SessionScope::PerUser => "logical_user",
                        SessionScope::PerOrganization => "logical_organization",
                    },
                    Some(cap),
                    Some(used),
                ),
                DenialReason::ConnectedEditorsExhausted { cap, .. } => (
                    "connected_editors",
                    Some(cap),
                    Some(self.budget.with_ledger(|ledger| {
                        ledger.user_connected(&request.connection_id, &self.principal)
                    })),
                ),
                _ => ("unknown", None, None),
            };
            tracing::warn!(
                source = "logical_reservation",
                category,
                cap,
                used,
                metadata_only,
                "resource admission refused"
            );
            if quota {
                return rc::ProviderError::QuotaExceeded(
                    "logical session or connected editor quota",
                );
            }
            rc::ProviderError::ResourceBusy("logical session or connected editor quota")
        })?;
        let mut provider = self
            .registry
            .resource_provider(&config.database_type)
            .await
            .map_err(|_| rc::ProviderError::CapabilityUnsupported)?;
        if !metadata_only {
            provider
                .capabilities()
                .require_stateful_session()
                .map_err(|_| rc::ProviderError::CapabilityUnsupported)?;
        }
        // Both editors and explicit-target callers can execute arbitrary SQL.
        // A short logical lifetime does not waive physical-session continuity.
        let purpose = if metadata_only {
            dr::ResourcePurpose::MetadataInspection
        } else {
            dr::ResourcePurpose::InteractiveQuery
        };
        let target = map::namespace(&request.initial_target);
        let identity_scope = dr::IdentityScope::default();
        let descriptor = provider
            .describe_resource(&dr::DescribeResourceRequest {
                connection_config: config.clone(),
                identity_scope: identity_scope.clone(),
                target: target.clone(),
                purpose,
            })
            .await
            .map_err(map::error)?;
        let cost = match descriptor.connection_cost_policy {
            dr::ConnectionCostPolicy::PoolBounded {
                max_physical_connections,
            } if max_physical_connections > 0 => max_physical_connections,
            dr::ConnectionCostPolicy::DeclaredConservative {
                declared_cost,
                hard_cap,
            } if declared_cost > 0 && hard_cap.is_none_or(|cap| cap >= declared_cost) => {
                declared_cost
            }
            _ => {
                tracing::warn!(
                    source = "descriptor_cost",
                    category = "invalid_cost_policy",
                    metadata_only,
                    "resource admission refused"
                );
                return Err(rc::ProviderError::ResourceBusy(
                    "driver connection cost refused under strict budget",
                ));
            }
        };
        let pool_key = datazen_runtime::resource::PoolKeyGeneration {
            fingerprint: rc::PoolKeyFingerprint::derive(&rc::PoolKeyInputs {
                connection_id: request.connection_id.clone(),
                config_revision: request.config_revision,
                driver_id: descriptor.provider_id.clone(),
                namespace: request.initial_target.clone(),
                execution_identity_key: self.principal.as_str().to_owned(),
                policy_isolation_key: self.organization.as_str().to_owned(),
            }),
            credential_revision: metadata.credential_revision,
            // Store config revision includes all network route updates.
            network_route_revision: metadata.config_revision,
        };
        let lease_purpose = if metadata_only {
            datazen_runtime::resource::LeasePurpose::Metadata
        } else {
            datazen_runtime::resource::LeasePurpose::FixedSession
        };
        let pool_policy =
            if metadata_only && descriptor.reuse_policy == dr::ReusePolicy::ReusableAfterReset {
                datazen_runtime::resource::PoolPolicy::stateful(2)
            } else {
                datazen_runtime::resource::PoolPolicy::stateless()
            };
        self.retire_idle(
            self.leases
                .idle_to_retire(Some((&request.connection_id, &pool_key))),
        )
        .await;
        let mut baseline = dr::Baseline::new(descriptor.initialization_requirements);
        let mut budget_scope: Arc<dyn dr::BudgetPort> = Arc::new(DesktopResourceBudget::scoped(
            self.budget.clone(),
            self.organization.clone(),
            self.principal.clone(),
            request.connection_id.clone(),
            if matches!(purpose, dr::ResourcePurpose::MetadataInspection) {
                datazen_runtime::budget::ResourceClass::Metadata
            } else {
                datazen_runtime::budget::ResourceClass::Interactive
            },
            cost,
        ));
        let idle = if metadata_only && pool_policy.parkable() {
            self.leases
                .claim_idle(&pool_key)
                .map_err(|_| rc::ProviderError::ProtocolError("pool lookup failed".into()))?
        } else {
            None
        };
        let (handle, resource_id, reused, tunnel) = if let Some(idle) = idle {
            let held = self.resource(idle.as_str()).await?;
            provider = held.provider.clone();
            baseline = held.baseline.clone();
            budget_scope = held._budget_scope.clone();
            (
                held.handle.clone(),
                idle.as_str().to_owned(),
                true,
                held._tunnel.clone(),
            )
        } else {
            let (effective_config, tunnel) =
                crate::tunnel::start_for_connection(config, self.known_hosts.clone())
                    .await
                    .map_err(|error| map::error(dr::ResourceError::Driver(error)))?;
            let handle = provider
                .acquire_resource(
                    &dr::AcquireResourceRequest {
                        connection_config: effective_config,
                        target,
                        scope: dr::ResourceScope {
                            purpose,
                            max_physical_connections: cost,
                            holds_open_transaction: false,
                            pin_for_streaming: false,
                        },
                        identity_scope,
                        baseline: baseline.clone(),
                    },
                    &budget_scope,
                )
                .await
                .map_err(map::error)?;
            (
                handle,
                uuid::Uuid::new_v4().to_string(),
                false,
                tunnel.map(Arc::new),
            )
        };
        if !reused {
            if self
                .leases
                .opened(
                    rc::ResourceId::new(&resource_id),
                    pool_key.clone(),
                    request.connection_id.clone(),
                    lease_purpose,
                    pool_policy,
                )
                .is_err()
            {
                let confirmed = matches!(
                    provider.close_resource(&handle).await,
                    Ok(ds::CloseDisposition::Closed)
                );
                if !confirmed {
                    self.retain_quarantined(
                        &request,
                        resource_id.clone(),
                        provider.clone(),
                        handle.clone(),
                        baseline.clone(),
                        budget_scope.clone(),
                        tunnel.clone(),
                        lease_purpose,
                    )
                    .await;
                }
                return Err(rc::ProviderError::ProtocolError(
                    "physical lease registration failed".into(),
                ));
            }
        }
        let observation = match provider.observe_session(&handle).await {
            Ok(observation) => observation,
            Err(error) => {
                let _ = self.leases.returning(
                    &rc::ResourceId::new(&resource_id),
                    datazen_runtime::resource::DriverCleanVerdict::reported_unclean(),
                    &Default::default(),
                );
                let confirmed = matches!(
                    provider.close_resource(&handle).await,
                    Ok(ds::CloseDisposition::Closed)
                );
                let _ = self
                    .leases
                    .closed(&rc::ResourceId::new(&resource_id), confirmed);
                if confirmed {
                    self.resources.write().await.remove(&resource_id);
                } else {
                    self.retain_quarantined(
                        &request,
                        resource_id.clone(),
                        provider.clone(),
                        handle.clone(),
                        baseline.clone(),
                        budget_scope.clone(),
                        tunnel.clone(),
                        lease_purpose,
                    )
                    .await;
                }
                return Err(map::error(error));
            }
        };
        let capabilities = provider.capabilities();
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
        self.resources.write().await.insert(
            resource_id.clone(),
            Arc::new(Resource {
                provider,
                handle,
                baseline,
                connection_id: request.connection_id.as_str().to_owned(),
                config_revision: request.config_revision.0,
                epoch: request.runtime_epoch,
                revision: tokio::sync::Mutex::new(0),
                last_context: tokio::sync::Mutex::new(map::context(&observation.context)),
                cancels: tokio::sync::Mutex::new(HashMap::new()),
                transactions: tokio::sync::Mutex::new(Vec::new()),
                finalized_transaction: tokio::sync::Mutex::new(None),
                protocol_drained: std::sync::atomic::AtomicBool::new(observation.protocol_drained),
                _tunnel: tunnel,
                _budget_scope: budget_scope,
                logical: std::sync::Mutex::new(Some(logical)),
                purpose: lease_purpose,
            }),
        );
        // 出向端口的反向索引：没有这一行，借出请求就只能新建连接或凭空拒绝。
        // 先确认资源真的在表里再绑（`insert` 刚刚做完，这一步因此恒真，但留着
        // 是为了让「绑定 ⇒ 资源在册」这条不变式在代码里看得见，而不是靠调用顺序）。
        self.bind_live_resource(request.db_session_id.as_str(), &resource_id)
            .await;
        // 资源开关是额度账的权威流水：额度被占满时，"有没有请求过关闭"与"请求被拒"
        // 在调用方那里是同一个静默 best-effort，只有这条流水能把两者分开。
        tracing::debug!(
            source = "resource_lifecycle",
            resource_id = %resource_id,
            connection_id = %request.connection_id,
            editor = matches!(request.owner, rc::OwnerRef::Editor { .. }),
            metadata_only,
            "resource opened"
        );
        Ok(opened)
    }

    async fn retain_quarantined(
        &self,
        request: &OpenResource,
        resource_id: String,
        provider: Arc<dyn dr::ResourceProvider>,
        handle: dr::ResourceHandle,
        baseline: dr::Baseline,
        budget_scope: Arc<dyn dr::BudgetPort>,
        tunnel: Option<Arc<crate::tunnel::Tunnel>>,
        purpose: datazen_runtime::resource::LeasePurpose,
    ) {
        self.resources.write().await.insert(
            resource_id,
            Arc::new(Resource {
                provider,
                handle,
                baseline,
                connection_id: request.connection_id.as_str().into(),
                config_revision: request.config_revision.0,
                epoch: request.runtime_epoch,
                revision: tokio::sync::Mutex::new(0),
                last_context: tokio::sync::Mutex::new(map::context(
                    &ds::SessionContext::unobserved(),
                )),
                cancels: tokio::sync::Mutex::new(HashMap::new()),
                transactions: tokio::sync::Mutex::new(Vec::new()),
                finalized_transaction: tokio::sync::Mutex::new(None),
                protocol_drained: std::sync::atomic::AtomicBool::new(false),
                _tunnel: tunnel,
                _budget_scope: budget_scope,
                logical: std::sync::Mutex::new(None),
                purpose,
            }),
        );
    }
}
