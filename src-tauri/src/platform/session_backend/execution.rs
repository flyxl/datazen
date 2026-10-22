use super::*;
use datazen_driver_api::CommandCatalog;
impl DesktopSessionBackend {
    pub(super) async fn execute_resource(
        &self,
        request: ExecuteOnResource,
    ) -> Result<ResourceExecution, rc::ProviderError> {
        let resource = self.resource(&request.resource_id).await?;
        self.leases
            .executing(&rc::ResourceId::new(&request.resource_id))
            .map_err(|_| rc::ProviderError::SessionLost("resource lease cannot execute".into()))?;
        let mut input = request.command.input.clone();
        if matches!(
            request.command.command.as_str(),
            "query" | "execute" | "query_stream" | "explain"
        ) {
            if let Some(params) = input.get("params").cloned() {
                let sql = input
                    .get("sql")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        rc::ProviderError::HostRejected("SQL input unavailable".into())
                    })?;
                let bound_sql = datazen_runtime::sql_guard::apply_params(sql, &params)
                    .map_err(rc::ProviderError::HostRejected)?;
                if let Some(object) = input.as_object_mut() {
                    object.insert("sql".into(), serde_json::Value::String(bound_sql));
                    object.remove("params");
                }
            }
        }
        let export_output = self
            .export_output
            .read()
            .map_err(|_| rc::ProviderError::ProtocolError("export authority unavailable".into()))?
            .as_ref()
            .is_some_and(|authority| authority(&request.execution_id));
        if !export_output && matches!(request.command.command.as_str(), "query" | "query_stream") {
            self.store
                .execution_settings()
                .await
                .apply_result_limit(&mut input);
        }
        let (config, metadata) = self
            .store
            .platform_profile(&resource.connection_id)
            .await
            .ok_or_else(|| rc::ProviderError::HostRejected("profile unavailable".into()))?;
        if !metadata.enabled || metadata.config_revision != resource.config_revision {
            return Err(rc::ProviderError::HostRejected(
                "profile disabled or revision changed".into(),
            ));
        }
        if request.command.command == "explain" {
            let driver = self
                .registry
                .get(&config.database_type)
                .await
                .ok_or(rc::ProviderError::CapabilityUnsupported)?;
            let catalog = datazen_driver_api::LegacyCommandCatalog::new(driver.as_ref());
            if !catalog
                .command_definitions()
                .iter()
                .any(|definition| definition.id == "explain")
            {
                return Err(rc::ProviderError::CapabilityUnsupported);
            }
        }
        if matches!(
            request.command.command.as_str(),
            "query" | "execute" | "query_stream" | "explain"
        ) {
            let sql = input
                .get("sql")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| rc::ProviderError::HostRejected("SQL input unavailable".into()))?;
            datazen_runtime::sql_guard::check_sql(
                sql,
                config.read_only,
                self.store.execution_settings().await.safe_mode,
            )
            .map_err(rc::ProviderError::HostRejected)?;
        }
        let publisher = self
            .publisher
            .read()
            .map_err(|_| rc::ProviderError::ProtocolError("publisher unavailable".into()))?
            .clone()
            .ok_or_else(|| rc::ProviderError::ProtocolError("publisher not installed".into()))?;
        let started = std::time::Instant::now();
        let record_history = request
            .command
            .input
            .get("recordHistory")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let history_sql = request
            .command
            .input
            .get("sql")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let execution = QueryExecutionId::new(request.execution_id.as_str());
        let cancel_handle = uuid::Uuid::new_v4().to_string();
        resource
            .cancels
            .lock()
            .await
            .insert(request.execution_id.as_str().into(), cancel_handle.clone());
        request.cancel_handle_sink.publish(&cancel_handle);
        let sink = Sink {
            id: request.execution_id.clone(),
            publisher: publisher.clone(),
        };
        if let Some(object) = input.as_object_mut() {
            object.remove("recordHistory");
        }
        let call = dr::CommandCall::new(request.command.command.clone(), input);
        let mapping_probe = if matches!(call.command.as_str(), "query" | "query_stream") {
            if let Some(candidate) = call
                .input
                .get("sql")
                .and_then(serde_json::Value::as_str)
                .and_then(crate::commands::result_relation_candidate)
            {
                if let Some(driver) = self.registry.get(&config.database_type).await {
                    match resource.driver_handle() {
                        Ok(handle) => {
                            let before = resource
                                .provider
                                .observe_session(&resource.handle)
                                .await
                                .ok();
                            match driver
                                .execute_command(
                                    &handle,
                                    "get_table_schema",
                                    serde_json::json!({"relationSql":candidate.relation_sql}),
                                )
                                .await
                            {
                                Ok(metadata) => {
                                    before.map(|before| (before.context, metadata.data))
                                }
                                Err(_) => None,
                            }
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        *resource.finalized_transaction.lock().await = None;
        resource
            .protocol_drained
            .store(false, std::sync::atomic::Ordering::Release);
        let result = if crate::services::schema_metadata::is_metadata_command(&call.command) {
            let driver = self
                .registry
                .get(&config.database_type)
                .await
                .ok_or(rc::ProviderError::CapabilityUnsupported)?;
            let handle = resource
                .provider
                .connection_handle(&resource.handle)
                .map_err(map::error)?;
            let before = resource
                .provider
                .observe_session(&resource.handle)
                .await
                .map_err(map::error)?;
            match crate::services::schema_metadata::execute_on_runtime_resource(
                driver,
                &handle,
                &call.command,
                call.input.clone(),
            )
            .await
            {
                Ok(result) => {
                    dr::ResultSink::write_output(
                        &sink,
                        serde_json::json!({"kind":"commandResult","data":result.data}),
                    )
                    .await
                    .map_err(map::error)?;
                    let after = resource
                        .provider
                        .observe_session(&resource.handle)
                        .await
                        .map_err(map::error)?;
                    Ok((
                        Some(ds::ExecutionCompletion {
                            completion_status: ds::CompletionStatus::Succeeded,
                            effect_outcome: ds::EffectOutcome::NotStarted,
                            statement_results: Vec::new(),
                            context_before: before.context,
                            context_after: after.context,
                            transaction_observation: after.transaction,
                            session_handles: after.handles,
                            protocol_drained: after.protocol_drained,
                            resource_health: after.resource_health,
                        }),
                        None,
                    ))
                }
                Err(error) => Err(dr::ResourceError::SinkRejected {
                    reason: crate::platform::diagnostics::safe_message(&error.to_string()),
                }),
            }
        } else {
            match call.command.as_str() {
                "platform.beginTransaction" => resource
                    .provider
                    .begin_transaction(
                        &resource.handle,
                        &serde_json::from_value::<ds::TransactionOptions>(call.input.clone())
                            .map_err(|_| {
                                rc::ProviderError::HostRejected(
                                    "invalid transaction options".into(),
                                )
                            })?,
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
                _ => {
                    #[cfg(feature = "webdriver")]
                    super::webdriver_dispatch::dispatch(&resource.connection_id);
                    resource
                        .provider
                        .execute_on_resource(&resource.handle, &execution, &call, &sink)
                        .await
                        .map(|completion| (Some(completion), None))
                }
            }
        };
        resource
            .cancels
            .lock()
            .await
            .remove(request.execution_id.as_str());
        let observation = resource.provider.observe_session(&resource.handle).await;
        let (completion, transaction, driver_failed) = match result {
            Ok((completion, transaction)) => (completion, transaction, false),
            Err(error) => {
                // A diagnostic artifact is best-effort. Disk/quota failure must not
                // hide the independently readable failed execution/errorCode.
                let _ = publisher(
                    request.execution_id.clone(),
                    serde_json::json!({"kind":"executionError", "message":crate::platform::diagnostics::safe_message(&error.to_string())}),
                ).await;
                (None, None, true)
            }
        };
        let drained = completion.as_ref().is_some_and(|c| c.protocol_drained)
            || transaction.is_some()
            || observation.as_ref().is_ok_and(|o| o.protocol_drained);
        resource
            .protocol_drained
            .store(drained, std::sync::atomic::Ordering::Release);
        let observation = observation.map_err(map::error)?;
        let mut context_after = completion
            .as_ref()
            .map(|c| &c.context_after)
            .unwrap_or(&observation.context)
            .clone();
        if let Some(transaction) = &transaction {
            context_after.transaction_state = transaction.state;
        }
        let effect_outcome = completion
            .as_ref()
            .map(|c| c.effect_outcome)
            .or_else(|| transaction.as_ref().and_then(|t| t.effect))
            .unwrap_or(if driver_failed {
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
                let mut output = serde_json::json!({"kind":"statementMetadata","statementIndex":index,"columns":statement.columns,"rowsAffected":statement.rows_affected,"truncated":statement.truncated});
                // Attach the writable source only when the probe answers. A
                // `None` here is not an error: the grid just stays read-only.
                if let Some(source) = verified_source(
                    &request.execution_id,
                    index as u32,
                    &statement.columns,
                    completion,
                    mapping_probe.as_ref(),
                    &resource.connection_id,
                ) {
                    match serde_json::to_value(source) {
                        Ok(source) => output["source"] = source,
                        // Unencodable provenance is *no* provenance; it still
                        // must not cost the user the rows.
                        Err(_) => tracing::debug!(
                            source = "result_mapping",
                            "verified source could not be encoded; result stays read-only"
                        ),
                    }
                }
                publisher(request.execution_id.clone(), output).await?;
            }
        }
        let mut driver_handles = completion
            .as_ref()
            .map(|c| c.session_handles.as_slice())
            .unwrap_or(observation.handles.as_slice())
            .to_vec();
        if let Some(transaction) = &transaction {
            let mut tracked = resource.transactions.lock().await;
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
            } else if transaction.state == ds::TransactionState::Idle
                && matches!(
                    transaction.effect,
                    Some(ds::EffectOutcome::Completed | ds::EffectOutcome::RolledBack)
                )
            {
                for held in tracked.iter() {
                    let mut closed = held.clone();
                    closed.closed = true;
                    driver_handles.retain(|other| other.handle_id != held.handle_id);
                    driver_handles.push(closed);
                }
            }
            *tracked = driver_handles
                .iter()
                .filter(|held| held.kind == ds::SessionHandleKind::Transaction && !held.closed)
                .cloned()
                .collect();
        } else {
            let mut tracked = resource.transactions.lock().await;
            for handle in driver_handles
                .iter()
                .filter(|held| held.kind == ds::SessionHandleKind::Transaction)
            {
                tracked.retain(|held| held.handle_id != handle.handle_id);
                if !handle.closed {
                    tracked.push(handle.clone());
                }
            }
        }
        if driver_handles.iter().any(|handle| {
            !handle.belongs_to(
                resource.handle.resource_key(),
                resource.handle.runtime_epoch(),
            )
        }) {
            return Err(rc::ProviderError::HostRejected(
                "driver handle bound to another resource".into(),
            ));
        }
        let mut revision = resource.revision.lock().await;
        let mut previous = resource.last_context.lock().await;
        let observed_context = map::context(&context_after);
        *revision = (*revision).max(request.expected_context_revision);
        if *previous != observed_context {
            *revision = revision.checked_add(1).ok_or_else(|| {
                rc::ProviderError::ProtocolError("context revision exhausted".into())
            })?;
            *previous = observed_context.clone();
        }
        let context_revision = *revision;
        drop(previous);
        drop(revision);
        if record_history {
            if let Some(sql) = history_sql {
                let rows_affected = completion
                    .as_ref()
                    .map(|completion| {
                        completion
                            .statement_results
                            .iter()
                            .filter_map(|statement| statement.rows_affected)
                            .reduce(u64::saturating_add)
                    })
                    .flatten();
                let entry = crate::store::QueryHistoryEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    connection_id: resource.connection_id.clone(),
                    database: observed_context.namespace.database.clone(),
                    schema: (!observed_context.namespace.schema.is_empty())
                        .then(|| observed_context.namespace.schema.clone()),
                    sql,
                    executed_at: chrono::Utc::now(),
                    execution_time_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                    rows_affected,
                    success: state == rc::ExecutionState::Succeeded,
                    error_message: if driver_failed {
                        Some("executionFailed".into())
                    } else {
                        None
                    },
                };
                if self.store.add_query_history(entry).await.is_err() {
                    tracing::warn!("query history persistence failed");
                }
            }
        }
        Ok(ResourceExecution {
            execution_id: request.execution_id.clone(),
            stream_id: rc::StreamId::new(format!("result:{}", request.execution_id.as_str())),
            state,
            effect_outcome: map::effect(effect_outcome),
            context_after: observed_context,
            context_revision,
            handles: map::handles(&driver_handles, &request.resource_id, resource.epoch),
            cancel_handle,
        })
    }
}

/// Can this single statement's result be written back?
///
/// This is a **capability probe, not a precondition.** Every way it can fail
/// means "we do not know that these rows came from one real relation", and the
/// caller renders that as *no source* — the grid stays read-only. None of these
/// paths may abort the execution: a probe that cannot answer has no business
/// taking the user's rows away, so everything here degrades to `None` (and a
/// debug line) instead of bubbling an error.
fn verified_source(
    execution_id: &rc::ExecutionId,
    statement_index: u32,
    columns: &[datazen_driver_api::ColumnInfo],
    completion: &ds::ExecutionCompletion,
    mapping_probe: Option<&(ds::SessionContext, serde_json::Value)>,
    connection_id: &str,
) -> Option<datazen_platform_api::dto::execution::StatementResultSource> {
    // One statement, and a context the driver actually confirmed — a batch or
    // an unobserved context can never name a single writable relation.
    if completion.statement_results.len() != 1
        || completion.context_before.confidence != ds::ObservationConfidence::Confirmed
    {
        return None;
    }
    let Some((observed, metadata)) = mapping_probe else {
        return None;
    };
    // The probe must describe the very context this statement ran in; if the
    // session moved underneath it, the probe says nothing about these rows.
    if *observed != completion.context_before {
        tracing::debug!(
            source = "result_mapping",
            "session context changed between the probe and the execution"
        );
        return None;
    }
    let (Some(schema), Some(resolved)) = (metadata.get("schema"), metadata.get("resolvedTarget"))
    else {
        tracing::debug!(
            source = "result_mapping",
            "driver did not answer the relation probe completely"
        );
        return None;
    };
    let Ok(schema) = serde_json::from_value::<datazen_driver_api::TableSchema>(schema.clone())
    else {
        tracing::debug!(
            source = "result_mapping",
            "driver relation metadata was not a readable table schema"
        );
        return None;
    };
    let relation = crate::commands::verify_result_mapping(
        rc::ConnectionId::new(connection_id),
        columns,
        &schema,
        resolved,
    )?;
    // The context is projected from the driver observation *field by field*; it
    // must never be rebuilt by serializing one shape and deserializing another.
    Some(
        datazen_platform_api::dto::execution::StatementResultSource {
            execution_id: execution_id.clone(),
            statement_index,
            context: map::platform_context(observed),
            relation: Some(relation),
            writable_mapping: datazen_platform_api::dto::execution::WritableMapping::Verified,
        },
    )
}
