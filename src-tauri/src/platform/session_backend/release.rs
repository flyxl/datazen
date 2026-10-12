use super::*;
impl DesktopSessionBackend {
    pub(super) async fn finalize_resource_handles(
        &self,
        request: FinalizeHandles,
    ) -> Result<HandleFinalization, rc::ProviderError> {
        let resource = self.resource(&request.resource_id).await?;
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
        let before = resource
            .provider
            .observe_session(&resource.handle)
            .await
            .map_err(map::error)?;
        let active = request
            .handles
            .iter()
            .filter(|handle| !handle.closed)
            .count();
        let effect = if active > 0 {
            let transaction = match request.disposition {
                HandleDisposition::Commit => {
                    resource.provider.commit_transaction(&resource.handle).await
                }
                HandleDisposition::Rollback => {
                    resource
                        .provider
                        .rollback_transaction(&resource.handle)
                        .await
                }
            }
            .map_err(map::error)?;
            if transaction.state == ds::TransactionState::Idle
                && matches!(
                    transaction.effect,
                    Some(ds::EffectOutcome::Completed | ds::EffectOutcome::RolledBack)
                )
            {
                resource.transactions.lock().await.clear();
                *resource.finalized_transaction.lock().await = Some(transaction.clone());
            }
            resource
                .protocol_drained
                .store(true, std::sync::atomic::Ordering::Release);
            transaction.effect.unwrap_or(ds::EffectOutcome::Unknown)
        } else {
            ds::EffectOutcome::NotStarted
        };
        let after = resource
            .provider
            .observe_session(&resource.handle)
            .await
            .map_err(map::error)?;
        let confirmed = resource.finalized_transaction.lock().await.is_some();
        let remaining = after
            .handles
            .iter()
            .filter(|handle| {
                !handle.closed && !(confirmed && handle.kind == ds::SessionHandleKind::Transaction)
            })
            .count();
        if active == 0 && before.handles.iter().any(|handle| !handle.closed) {
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
    pub(super) async fn close_resource_lease(
        &self,
        request: CloseResource,
    ) -> Result<CloseResourceOutcome, rc::ProviderError> {
        let resource = self.resource(&request.resource_id).await?;
        if request.registered_handles != 0 {
            return Err(rc::ProviderError::CleanupFailed(
                "registered handles remain".into(),
            ));
        }
        let observation = resource
            .provider
            .observe_session(&resource.handle)
            .await
            .unwrap_or_else(|_| ds::SessionObservation::unobservable());
        let confirmed = resource.finalized_transaction.lock().await.is_some();
        if !(observation.protocol_drained
            || resource
                .protocol_drained
                .load(std::sync::atomic::Ordering::Acquire))
            || !resource.cancels.lock().await.is_empty()
            || observation.handles.iter().any(|handle| {
                !handle.closed && !(confirmed && handle.kind == ds::SessionHandleKind::Transaction)
            })
            || matches!(
                observation.transaction.state,
                ds::TransactionState::Active | ds::TransactionState::Aborted
            )
            || (observation.transaction.state == ds::TransactionState::Unknown && !confirmed)
        {
            self.unbind_live_resource_by_resource(&request.resource_id)
                .await;
            resource
                .logical
                .lock()
                .map_err(|_| rc::ProviderError::CleanupFailed("logical quota unavailable".into()))?
                .take();
            self.leases
                .returning(
                    &rc::ResourceId::new(&request.resource_id),
                    datazen_runtime::resource::DriverCleanVerdict::reported_unclean(),
                    &Default::default(),
                )
                .map_err(|_| {
                    rc::ProviderError::CleanupFailed("lease quarantine policy failed".into())
                })?;
            // The actor has awaited the original execution before entering here.
            // Unknown protocol/transaction state forbids reuse; an actual physical
            // close confirmation is still sufficient to settle physical budget.
            let physically_closed = matches!(
                resource.provider.close_resource(&resource.handle).await,
                Ok(ds::CloseDisposition::Closed)
            );
            self.leases
                .closed(
                    &rc::ResourceId::new(&request.resource_id),
                    physically_closed,
                )
                .map_err(|_| {
                    rc::ProviderError::CleanupFailed("closure accounting failed".into())
                })?;
            if physically_closed {
                self.resources.write().await.remove(&request.resource_id);
                return Ok(CloseResourceOutcome::Closed);
            }
            return Ok(CloseResourceOutcome::Undecidable {
                reason: "resource state unresolved and physical close unconfirmed",
            });
        }

        let reset = resource
            .provider
            .reset_resource(&resource.handle, &resource.baseline)
            .await;
        let clean = match reset {
            Ok(ds::ResetDisposition::Clean) => {
                datazen_runtime::resource::DriverCleanVerdict::reported_clean()
            }
            _ => datazen_runtime::resource::DriverCleanVerdict::reported_unclean(),
        };
        let host = datazen_runtime::resource::HostConditionSnapshot {
            fixed_session: resource.purpose
                == datazen_runtime::resource::LeasePurpose::FixedSession,
            ..Default::default()
        };
        if self
            .leases
            .returning(&rc::ResourceId::new(&request.resource_id), clean, &host)
            .map_err(|_| rc::ProviderError::CleanupFailed("lease return policy failed".into()))?
        {
            self.unbind_live_resource_by_resource(&request.resource_id)
                .await;
            resource
                .logical
                .lock()
                .map_err(|_| rc::ProviderError::CleanupFailed("logical quota unavailable".into()))?
                .take();
            return Ok(CloseResourceOutcome::ReturnedToPool);
        }
        // 出向端口绑定与物理资源同帧摘除：`Closed` 与 `Undecidable` 都不该再被借出。
        self.unbind_live_resource_by_resource(&request.resource_id)
            .await;
        match resource.provider.close_resource(&resource.handle).await {
            Ok(ds::CloseDisposition::Closed) => {
                self.leases
                    .closed(&rc::ResourceId::new(&request.resource_id), true)
                    .map_err(|_| {
                        rc::ProviderError::CleanupFailed("lease closure accounting failed".into())
                    })?;
                resource
                    .logical
                    .lock()
                    .map_err(|_| {
                        rc::ProviderError::CleanupFailed("logical quota unavailable".into())
                    })?
                    .take();
                self.resources.write().await.remove(&request.resource_id);
                Ok(CloseResourceOutcome::Closed)
            }
            Ok(ds::CloseDisposition::CloseUnconfirmed) | Err(_) => {
                self.leases
                    .closed(&rc::ResourceId::new(&request.resource_id), false)
                    .map_err(|_| {
                        rc::ProviderError::CleanupFailed("quarantine accounting failed".into())
                    })?;
                resource
                    .logical
                    .lock()
                    .map_err(|_| {
                        rc::ProviderError::CleanupFailed("logical quota unavailable".into())
                    })?
                    .take();
                Ok(CloseResourceOutcome::Undecidable {
                    reason: "driver close unconfirmed",
                })
            }
        }
    }
}
