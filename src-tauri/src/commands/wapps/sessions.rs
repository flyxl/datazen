//! Instance-owned Wapp sessions. Only the host bridge sees instance IDs/handles.
use super::{AppState, CommandError, State};
use crate::{mcp::session::McpSessionRegistry, platform::PlatformAdapter, wapps::Permission};
use datazen_application::{dto::requests::*, sessions::ConnectionUseCases};
use datazen_platform_api::{
    context::{OwnerRef, RequestContext},
    dto::{idempotency::IdempotentOperation, session::ResourceRelease},
    id::*,
    target::{ExecutionTarget, NamespaceTarget},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, Mutex,
    },
    time::{Duration, Instant},
};

const INSTANCE_TTL: Duration = Duration::from_secs(3600);
struct Instance {
    wapp: String,
    adapter: Arc<PlatformAdapter>,
    context: RequestContext,
    expires: Instant,
    ended: AtomicBool,
    sessions: McpSessionRegistry,
    // Open/use/close admission is serialized per frame, independent of other consumers.
    gate: tokio::sync::Mutex<()>,
    executions: Mutex<HashMap<String, String>>,
    active: Mutex<HashSet<String>>,
}
static INSTANCES: LazyLock<Mutex<HashMap<String, Arc<Instance>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
fn instances() -> std::sync::MutexGuard<'static, HashMap<String, Arc<Instance>>> {
    INSTANCES.lock().unwrap_or_else(|error| error.into_inner())
}
fn lost() -> CommandError {
    CommandError::Validation("SessionLost: Wapp instance/session ended or expired".into())
}
fn internal(error: impl std::fmt::Display) -> CommandError {
    CommandError::Validation(error.to_string())
}
fn enabled(state: &AppState, id: &str) -> Result<(), CommandError> {
    let wapp = state.wapps.get(id).ok_or_else(lost)?;
    if !wapp.enabled
        || !wapp
            .manifest
            .permissions
            .contains(&Permission::SessionStateful)
        || !wapp
            .manifest
            .permissions
            .contains(&Permission::CommandInvoke)
    {
        return Err(CommandError::Validation(
            "Wapp requires command:invoke and session:stateful permissions".into(),
        ));
    }
    Ok(())
}
impl Instance {
    fn check(&self) -> Result<(), CommandError> {
        if self.ended.load(Ordering::Acquire) || Instant::now() >= self.expires {
            Err(lost())
        } else {
            Ok(())
        }
    }
    fn end(&self) {
        self.ended.store(true, Ordering::Release);
        self.sessions
            .close_client(self.context.client_instance_id.as_str());
        self.sessions.wake.notify_one();
    }
}
/// Disable/uninstall revokes exactly this Wapp's instances, retaining cleanup ownership.
pub(crate) fn end_wapp_instances(wapp: &str) {
    let mut all = instances();
    all.retain(|_, instance| {
        if instance.wapp == wapp {
            instance.end();
            false
        } else {
            true
        }
    });
}
fn spawn_cleanup(id: String, instance: Arc<Instance>) {
    tokio::spawn(async move {
        let mut backoff = Duration::from_millis(100);
        loop {
            if Instant::now() >= instance.expires {
                instance.end();
                instances().remove(&id);
            }
            instance.sessions.purge_expired(Instant::now());
            let pending = instance.sessions.pending_cleanup();
            if let Ok(runtime) = instance.adapter.runtime() {
                let executions = instance
                    .executions
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                let active = instance
                    .active
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                for (execution, reference) in executions {
                    if !active.contains(&execution) {
                        continue;
                    }
                    if instance.check().is_err()
                        || instance
                            .sessions
                            .lookup(
                                &reference,
                                instance.context.client_instance_id.as_str(),
                                Instant::now(),
                            )
                            .is_err()
                    {
                        let _ = runtime
                            .cancel_execution(&instance.context, ExecutionId::new(execution))
                            .await;
                    }
                }
                for binding in pending {
                    if matches!(runtime.close_session(&instance.context, CloseSessionRequest::new(binding.handle, CloseMode::RollbackAndClose)).await,
                        Ok(receipt) if receipt.resource_release == ResourceRelease::Confirmed)
                    {
                        instance.sessions.confirm_cleanup(&binding.reference);
                    }
                }
            }
            let pending = !instance.sessions.pending_cleanup().is_empty();
            if instance.ended.load(Ordering::Acquire)
                && !pending
                && instance.sessions.is_empty()
                && instance.gate.try_lock().is_ok()
            {
                return;
            }
            let mut delay = if instance.ended.load(Ordering::Acquire) {
                backoff
            } else if pending {
                backoff
            } else {
                instance.expires.saturating_duration_since(Instant::now())
            };
            if let Some(expiry) = instance.sessions.next_expiry() {
                delay = delay.min(expiry.saturating_duration_since(Instant::now()));
            }
            backoff = if pending {
                (backoff * 2).min(Duration::from_secs(30))
            } else {
                Duration::from_millis(100)
            };
            tokio::select! { _ = tokio::time::sleep(delay) => {}, _ = instance.sessions.wake.notified() => {} }
        }
    });
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum WappSessionAction {
    Register {
        #[serde(rename = "wappId")]
        wapp_id: String,
    },
    End {
        #[serde(rename = "instanceId")]
        instance_id: String,
    },
    Open {
        #[serde(rename = "instanceId")]
        instance_id: String,
        #[serde(rename = "connectionId")]
        connection_id: String,
        database: String,
        schema: Option<String>,
        #[serde(rename = "ttlMs")]
        ttl_ms: Option<u64>,
    },
    Use {
        #[serde(rename = "instanceId")]
        instance_id: String,
        reference: String,
        command: String,
        input: Value,
    },
    Close {
        #[serde(rename = "instanceId")]
        instance_id: String,
        reference: String,
    },
    Receipt {
        #[serde(rename = "instanceId")]
        instance_id: String,
        #[serde(rename = "executionId")]
        execution_id: String,
    },
}
#[tauri::command]
pub async fn wapp_session(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: WappSessionAction,
) -> Result<Value, CommandError> {
    session_action(&app, &state, request).await
}
pub(crate) async fn session_action(
    app: &tauri::AppHandle,
    state: &AppState,
    request: WappSessionAction,
) -> Result<Value, CommandError> {
    if let WappSessionAction::Register { wapp_id } = &request {
        enabled(state, wapp_id)?;
        let adapter = state.platform_require().map_err(internal)?.clone();
        let identity = adapter.identity();
        let client = crate::platform::new_client_instance_id();
        let context = RequestContext::new(
            identity.organization_id().clone(),
            identity.principal_id().clone(),
            None,
            client.clone(),
            crate::platform::identity::new_request_id(),
            None,
        );
        let id = uuid::Uuid::new_v4().to_string();
        let instance = Arc::new(Instance {
            wapp: wapp_id.clone(),
            adapter,
            context,
            expires: Instant::now() + INSTANCE_TTL,
            ended: AtomicBool::new(false),
            sessions: McpSessionRegistry::new(),
            gate: tokio::sync::Mutex::new(()),
            executions: Mutex::new(HashMap::new()),
            active: Mutex::new(HashSet::new()),
        });
        {
            let mut all = instances();
            if all.len() >= 128 {
                return Err(CommandError::Validation("Too many Wapp instances".into()));
            }
            all.insert(id.clone(), instance.clone());
        }
        spawn_cleanup(id.clone(), instance);
        return Ok(json!({"instanceId":id}));
    }
    let id = match &request {
        WappSessionAction::End { instance_id }
        | WappSessionAction::Open { instance_id, .. }
        | WappSessionAction::Use { instance_id, .. }
        | WappSessionAction::Close { instance_id, .. }
        | WappSessionAction::Receipt { instance_id, .. } => instance_id,
        _ => return Err(lost()),
    };
    if matches!(&request, WappSessionAction::End { .. }) {
        if let Some(instance) = instances().remove(id) {
            instance.end();
        }
        return Ok(json!({}));
    }
    let instance = instances().get(id).cloned().ok_or_else(lost)?;
    let _guard = instance.gate.lock().await;
    instance.check()?;
    if let Err(error) = enabled(state, &instance.wapp) {
        instance.end();
        instances().remove(id);
        return Err(error);
    }
    let runtime = instance.adapter.runtime().map_err(internal)?;
    let client = instance.context.client_instance_id.as_str();
    match request {
        WappSessionAction::Open {
            connection_id,
            database,
            schema,
            ttl_ms,
            ..
        } => {
            if database.trim().is_empty()
                || instance.sessions.len() + instance.sessions.pending_cleanup().len() >= 8
            {
                return Err(CommandError::Validation(
                    "Explicit database required; at most eight sessions per instance".into(),
                ));
            }
            let (config, _) = state
                .store
                .platform_profile(&connection_id)
                .await
                .ok_or_else(lost)?;
            if !matches!(config.database_type.as_str(), "postgres" | "mysql") {
                return Err(CommandError::Validation(
                    "CapabilityUnsupported: Wapp sessions support PostgreSQL/MySQL only".into(),
                ));
            }
            if config.database_type == "postgres"
                && schema
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty())
            {
                return Err(CommandError::Validation(
                    "TargetRequired: PostgreSQL Wapp sessions require an explicit schema".into(),
                ));
            }
            if !super::super::dialog::confirm_message(app, "Allow Wapp database session?", &format!("{} requests a dedicated session on {} / {}. It can run SQL and retain session state until closed or expired. Allow this session?", instance.wapp, connection_id, database)).await? { return Err(CommandError::Validation("Wapp session was not approved".into())); }
            instance.check()?;
            enabled(state, &instance.wapp)?;
            let mut namespace = NamespaceTarget::database(database);
            if let Some(schema) = schema {
                namespace.schema = Some(schema);
            }
            let token = runtime
                .issue_submission_token(&instance.context, IdempotentOperation::OpenSession, None)
                .await
                .map_err(internal)?;
            let receipt = runtime
                .open_session(
                    &instance.context,
                    OpenSessionRequest::new(
                        ExecutionTarget::new(ConnectionId::new(connection_id.clone()), namespace),
                        OwnerRef::ClientSession {
                            client_instance_id: instance.context.client_instance_id.clone(),
                            purpose: "wapp-stateful-session".into(),
                        },
                        token.idempotency_key,
                    ),
                )
                .await
                .map_err(internal)?;
            let ttl = crate::mcp::session::sanitize_ttl(ttl_ms.map(Duration::from_millis));
            let binding = instance.sessions.open(
                client,
                &connection_id,
                receipt.session.handle,
                receipt.session.context_revision,
                ttl,
                Instant::now(),
            );
            if instance.check().is_err() {
                instance.sessions.revoke(&binding.reference, client);
                return Err(lost());
            }
            instance.sessions.wake.notify_one();
            Ok(
                json!({"reference":binding.reference,"expiresInMs":ttl.as_millis(),"contextRevision":binding.context_revision}),
            )
        }
        WappSessionAction::Use {
            reference,
            command,
            input,
            ..
        } => {
            // No driver management commands or arbitrary platform commands.
            // Session/transaction state is observed by the original retained provider.
            // Runtime's SQL guard and policy remain authoritative for query/execute.
            if !matches!(command.as_str(), "query" | "execute") || !input.is_object() {
                return Err(CommandError::Validation(
                    "Wapp sessions accept query/execute object input only".into(),
                ));
            }
            if instance
                .executions
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .len()
                >= 1024
            {
                return Err(CommandError::Validation(
                    "Wapp instance receipt limit reached; reload the Wapp".into(),
                ));
            }
            let binding = instance
                .sessions
                .lookup(&reference, client, Instant::now())
                .map_err(|_| lost())?;
            let token = runtime
                .issue_submission_token(
                    &instance.context,
                    IdempotentOperation::ExecuteInSession,
                    Some(&binding.handle),
                )
                .await
                .map_err(internal)?;
            instance.check()?;
            instance
                .sessions
                .lookup(&reference, client, Instant::now())
                .map_err(|_| lost())?;
            let receipt = runtime
                .execute_in_session(
                    &instance.context,
                    ExecuteInSessionRequest::new(
                        binding.handle,
                        CommandCall::new(command, input),
                        token.idempotency_key,
                    )
                    .with_expected_context_revision(binding.context_revision),
                )
                .await
                .map_err(internal)?;
            instance
                .executions
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(receipt.execution_id.as_str().into(), reference);
            instance
                .active
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(receipt.execution_id.as_str().into());
            instance.sessions.wake.notify_one();
            let deadline = Instant::now() + Duration::from_secs(25);
            loop {
                let view = runtime
                    .get_execution(&instance.context, receipt.execution_id.clone())
                    .await
                    .map_err(internal)?;
                if view.state.is_terminal() || Instant::now() >= deadline {
                    // Only opaque receipt fields, no runtime handles/attachment/credentials.
                    return receipt_value(&instance, view.execution_id.as_str()).await;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
        WappSessionAction::Receipt { execution_id, .. } => {
            receipt_value(&instance, &execution_id).await
        }
        WappSessionAction::Close { reference, .. } => {
            instance.sessions.revoke(&reference, client);
            Ok(json!({"resourceRelease":"pending"}))
        }
        _ => Err(lost()),
    }
}

async fn receipt_value(instance: &Instance, execution: &str) -> Result<Value, CommandError> {
    if !instance
        .executions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(execution)
    {
        return Err(lost());
    }
    let runtime = instance.adapter.runtime().map_err(internal)?;
    let view = runtime
        .get_execution(&instance.context, ExecutionId::new(execution))
        .await
        .map_err(internal)?;
    if view.state.is_terminal() {
        instance
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(execution);
    }
    let mut columns = Value::Null;
    let mut rows = Vec::new();
    let mut truncated = false;
    for artifact in &view.artifact_ids {
        let first = runtime
            .read_artifact_chunk(&instance.context, artifact.clone(), Counter::ZERO)
            .await
            .map_err(internal)?;
        if columns.is_null() {
            columns = first.columns.clone();
        }
        for index in 0..first.published_chunk_count.get() {
            let chunk = if index == 0 {
                first.clone()
            } else {
                runtime
                    .read_artifact_chunk(&instance.context, artifact.clone(), Counter::new(index))
                    .await
                    .map_err(internal)?
            };
            for row in chunk.rows {
                if rows.len() >= 1000 {
                    truncated = true;
                    break;
                }
                rows.push(row);
            }
            if truncated {
                break;
            }
        }
        if truncated {
            break;
        }
    }
    Ok(
        json!({"executionId":view.execution_id,"state":view.state,"effectOutcome":view.effect_outcome,"errorCode":view.error_code,"columns":columns,"rows":rows,"rowsTruncated":truncated,"resultCompleteness":view.result_completeness}),
    )
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
