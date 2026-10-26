use super::error::{CmdExt, CommandError};
use super::AppState;
use datazen_driver_api::{ConnectionConfig, DriverCategory, ServerInfo};
use datazen_modules::connection::ConnectionError;
use datazen_platform_api::error::ApiErrorCode;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Self-describing result of `connect` / `connect_dedicated`.
///
/// The two ids are different concepts ([docs/architecture/naming.md]):
/// `connectionId` is the persistent connection-config id (survives restart),
/// `dbSessionId` is the runtime database session id (in-memory only, never
/// persisted). Returning both in a typed payload instead of a bare `String`
/// makes that boundary explicit and keeps callers from guessing which id the
/// value is.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSessionRef {
    pub connection_id: String,
    pub db_session_id: String,
}

/// 宿主组织作用域 → 模块连接作用域。
fn scope_modules(
    scope: &datazen_platform_api::ports::scope::OrganizationScope,
) -> datazen_modules::connection::ConnectionScope {
    datazen_modules::connection::ConnectionScope::new(scope.organization_id().as_str())
}

/// `ConnectionError` → `CommandError`（与 `port_cmd` 同形：`NotFound` 保留，
/// 其余降为内部错误；映射前记一条脱敏日志）。
pub(crate) fn conn_cmd<T>(
    result: Result<T, ConnectionError>,
    cmd: &'static str,
) -> Result<T, CommandError> {
    result.map_err(|error| {
        let err = match error {
            ConnectionError::NotFound(entity) => CommandError::NotFound(entity),
            other => CommandError::Internal(other.to_string()),
        };
        let redacted = datazen_infra::redaction::redact_secrets_for_log(&err.to_string());
        tracing::error!(cmd, error = %redacted);
        err
    })
}

/// 概要 §6.2 第 8 步：把本次 IPC 调用组装出的身份记进结构化日志。
///
/// 刻意**不**接收也不返回任何身份参数、也不改变控制流 —— 既有 IPC 外观原样保留
/// （开发计划 :77），身份由 [`crate::platform`] 组装。身份组装不出来时这里什么都不写，
/// 原因已在启动期打过一次 warn；这些命令走的是旧用例实现，不存在可被伪映射的
/// owner 语义。详见 [`crate::platform::ipc`]。
fn log_ipc_identity(state: &AppState, command: &'static str) {
    if let Some(scope) = crate::platform::ipc::observe_ipc_call(state) {
        tracing::debug!(
            command,
            request_id = scope.request_id(),
            principal = scope.principal(),
            client_instance_id = scope.client_instance(),
            owner = ?scope.owner(),
            "ipc identity assembled"
        );
    }
}

pub(crate) async fn get_connections_impl(
    state: &AppState,
) -> Result<Vec<ConnectionConfig>, CommandError> {
    let scope = state.connection_scope()?;
    let service = crate::platform::ipc::require_connection_service(state)?;
    let list = conn_cmd(service.list_redacted(&scope).await, "get_connections")?;
    tracing::debug!(count = list.len(), "get_connections");
    Ok(list)
}

pub(crate) async fn save_connection_impl(
    state: &AppState,
    config: ConnectionConfig,
    clear_missing_credentials: bool,
) -> Result<(), CommandError> {
    tracing::info!(id = %config.id, name = %config.name, "save_connection");
    let scope = state.connection_scope()?;
    let service = crate::platform::ipc::require_connection_service(state)?;
    let connection_id = config.id.clone();
    // 表单回显把密文/密钥置 None：合并语义（不覆盖已存密钥）由模块用例承担。
    conn_cmd(
        service
            .save_merged(&scope, config, clear_missing_credentials)
            .await,
        "save_connection",
    )?;
    // 保存新配置后让该 profile 的旧会话退休（PR #116 的语义）：端口只负责落盘，
    // 「旧连接串已经作废」这件事必须由命令层显式触发，否则用户改完密码仍复用旧会话。
    crate::services::migration_endpoint::retire_sessions_for_profile(state, &connection_id).await;
    Ok(())
}

pub(crate) async fn duplicate_connection_impl(
    state: &AppState,
    source_id: String,
    new_id: String,
    new_name: String,
) -> Result<(), CommandError> {
    let scope = state.connection_scope()?;
    let service = crate::platform::ipc::require_connection_service(state)?;
    conn_cmd(
        service
            .duplicate(&scope, &source_id, &new_id, &new_name)
            .await,
        "duplicate_connection",
    )
}

pub(crate) async fn delete_connection_impl(
    state: &AppState,
    id: String,
) -> Result<(), CommandError> {
    tracing::info!(%id, "delete_connection");
    let scope = state.connection_scope()?;
    let service = crate::platform::ipc::require_connection_service(state)?;
    conn_cmd(service.delete(&scope, &id).await, "delete_connection")
}

pub(crate) async fn test_connection_impl(
    state: &AppState,
    config: ConnectionConfig,
) -> Result<ServerInfo, CommandError> {
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(state)?;
    let connection_service = crate::platform::ipc::require_connection_service(state)?;
    let mut config = config;
    if let Some(saved) = conn_cmd(
        connection_service
            .get(&scope_modules(&scope), &config.id)
            .await,
        "test_connection",
    )? {
        datazen_modules::connection::merge_missing_secrets(&mut config, saved);
    }
    tracing::info!(
        name = %config.name,
        host = ?config.host,
        port = ?config.port,
        db_type = ?config.database_type,
        ssh = ?config.ssh_tunnel.as_ref().map(|s| format!("{}@{}:{}", s.username, s.host, s.port)),
        "test_connection"
    );
    let result = crate::tunnel::test_connection(
        &*services.tunnels,
        &scope,
        &services.known_hosts,
        &state.driver_registry,
        config,
    )
    .await
    .cmd_err("test_connection")?;
    tracing::info!(version = %result.server_version, "test_connection OK");
    Ok(result)
}

pub(crate) async fn connect_impl(
    state: &AppState,
    connection_id: String,
) -> Result<ConnectionSessionRef, CommandError> {
    log_ipc_identity(state, "connect");
    tracing::info!(%connection_id, "connect");
    let scope = state.connection_scope()?;
    let connection_service = crate::platform::ipc::require_connection_service(state)?;
    let config = conn_cmd(
        connection_service.get(&scope, &connection_id).await,
        "connect",
    )?
    .ok_or_else(|| CommandError::NotFound(format!("connection '{connection_id}' not found")))?;
    if let Err(e) = connection_service
        .touch_last_connected_at(&scope, &connection_id)
        .await
    {
        tracing::warn!(%connection_id, error = %e, "failed to persist last_connected_at");
    }
    // No database is a valid profile: the session opens with an empty
    // namespace and the driver resolves its own default (PostgreSQL → the
    // `postgres` database, MySQL → server level), then reports the observed
    // database back. Requiring one here would contradict the driver-declared
    // namespace shape and broke profiles saved without a database.
    let db_session_id = crate::services::migration_endpoint::open_dedicated_session(
        state,
        &connection_id,
        config.database.as_deref(),
        "client",
    )
    .await?;

    tracing::info!(db_session_id = %db_session_id, "connect OK");
    Ok(ConnectionSessionRef {
        connection_id: connection_id.clone(),
        db_session_id,
    })
}

pub(crate) async fn connect_dedicated_impl(
    state: &AppState,
    connection_id: String,
    database: Option<String>,
) -> Result<ConnectionSessionRef, CommandError> {
    log_ipc_identity(state, "connect_dedicated");
    tracing::info!(%connection_id, database = ?database, "connect_dedicated");
    // An explicit `database` pins the session; otherwise fall back to the
    // profile. Both may be unset — the driver then resolves its own default,
    // exactly as on the shared `connect` path.
    let scope = state.connection_scope()?;
    let connection_service = crate::platform::ipc::require_connection_service(state)?;
    let database =
        match crate::services::migration_endpoint::normalize_database(database.as_deref()) {
            Some(database) => Some(database),
            // 取不到配置（越组织 / 已被删除）时保持 `None`：驱动会解析自己的默认库，
            // 与「配置里没写 database」是同一档语义。
            None => conn_cmd(
                connection_service.get(&scope, &connection_id).await,
                "connect_dedicated",
            )?
            .and_then(|config| config.database),
        };
    if let Err(e) = connection_service
        .touch_last_connected_at(&scope, &connection_id)
        .await
    {
        tracing::warn!(%connection_id, error = %e, "failed to persist last_connected_at");
    }
    let db_session_id = crate::services::migration_endpoint::open_dedicated_session(
        state,
        &connection_id,
        database.as_deref(),
        "clientDedicated",
    )
    .await?;

    tracing::info!(db_session_id = %db_session_id, "connect_dedicated OK");
    Ok(ConnectionSessionRef {
        connection_id: connection_id.clone(),
        db_session_id,
    })
}

pub(crate) async fn ping_connection_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<bool, CommandError> {
    log_ipc_identity(state, "ping_connection");
    Ok(
        crate::services::migration_endpoint::runtime_session_context(
            state,
            &db_session_id,
            datazen_runtime::application::LivePurpose::Inspection,
        )
        .await
        .is_ok(),
    )
}

pub(crate) async fn release_connection_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<bool, CommandError> {
    log_ipc_identity(state, "release_connection");
    tracing::info!(%db_session_id, "release_connection");
    match crate::services::migration_endpoint::close_dedicated_session(state, &db_session_id).await
    {
        Ok(()) => {
            state.schema_cache.clear_connection(&db_session_id).await;
            tracing::info!(%db_session_id, "release_connection: Runtime session closed");
            Ok(true)
        }
        Err(CommandError::NotFound(_)) => Ok(true),
        Err(error) => Err(error),
    }
}

/// Close the driver-side connection pool that serves one database of a session.
///
/// This is the *only* way a database's backend resources are released without
/// tearing down the whole session. It is deliberately not a `USE`-style switch:
/// the shared session stays connected to whatever it was connected to, and any
/// later metadata read simply re-opens a pool for the database it asks for.
///
/// The host also drops every cached schema scope of that database, so the next
/// read re-fetches instead of serving structure from a pool that just went away.
pub(crate) async fn close_database_impl(
    state: &AppState,
    db_session_id: String,
    database: String,
) -> Result<bool, CommandError> {
    log_ipc_identity(state, "close_database");
    let database = database.trim().to_string();
    if database.is_empty() {
        return Err(CommandError::Validation(
            "close_database requires a non-empty database name".into(),
        ));
    }
    tracing::info!(%db_session_id, %database, "close_database");

    // Per-database pool maintenance is a runtime session demand: the runtime
    // raises the use case (lease + authorize + driver call) and the host only
    // carries the verdict into its cache policy.
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let call = adapter.begin_call();
    let runtime = adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?;

    // Driver-level failure is best-effort (`ServiceUnavailable`): a close that
    // cannot be performed must not leave stale metadata behind, and a driver
    // without per-database resources reports `false` anyway. Session-level
    // rejections (unknown/dead session, permission) are honest failures the
    // caller must see.
    let closed = match runtime
        .close_database(call.context(), &db_session_id, &database)
        .await
    {
        Ok(closed) => Some(closed),
        Err(e) if e.code == ApiErrorCode::ServiceUnavailable => {
            tracing::warn!(%db_session_id, %database, error = %e, "close_database failed");
            None
        }
        Err(e) => return Err(crate::platform::into_command_error(e)),
    };
    let closed = closed.unwrap_or(false);
    tracing::info!(%db_session_id, %database, closed, "close_database OK");
    state
        .schema_cache
        .invalidate(&db_session_id, &database, None)
        .await;
    Ok(closed)
}

/// Databases this session currently holds an open resource for.
///
/// Drives the navigator's "open" marker so a user can see which database nodes
/// a right-click "close database connection" would actually release. A driver
/// with no per-database resource returns an empty list, which the UI renders as
/// "no marker" rather than as an error.
pub(crate) async fn get_open_databases_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<Vec<String>, CommandError> {
    log_ipc_identity(state, "get_open_databases");
    let start = std::time::Instant::now();
    // Runtime session demand; the host never sees a driver or a handle here.
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let call = adapter.begin_call();
    let runtime = adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?;
    let mut open = runtime
        .open_databases(call.context(), &db_session_id)
        .await
        .map_err(crate::platform::into_command_error)?;
    open.sort();
    open.dedup();
    tracing::info!(
        %db_session_id,
        count = open.len(),
        ms = start.elapsed().as_millis() as u64,
        "get_open_databases OK"
    );
    Ok(open)
}

pub(crate) async fn disconnect_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<(), CommandError> {
    log_ipc_identity(state, "disconnect");
    tracing::info!(%db_session_id, "disconnect (force)");
    crate::services::migration_endpoint::close_dedicated_session(state, &db_session_id).await?;
    state.schema_cache.clear_connection(&db_session_id).await;
    tracing::info!(%db_session_id, "disconnect OK");
    Ok(())
}

pub(crate) async fn get_connection_info_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<serde_json::Value, CommandError> {
    log_ipc_identity(state, "get_connection_info");
    let config = crate::services::migration_endpoint::runtime_session_context(
        state,
        &db_session_id,
        datazen_runtime::application::LivePurpose::Inspection,
    )
    .await?
    .config;

    let db_type = &config.database_type;

    let driver = state.driver_registry.get(db_type).await;
    let driver_category = match driver.as_ref().map(|d| d.driver_category()) {
        Some(DriverCategory::KeyValue) => "keyvalue",
        Some(DriverCategory::Document) => "document",
        _ => "sql",
    };
    let capabilities = state.driver_registry.get_capabilities(db_type).await;

    Ok(serde_json::json!({
        "databaseType": db_type,
        "driverCategory": driver_category,
        "name": config.name,
        "host": config.host,
        "port": config.port,
        "database": config.database,
        "schema": config.schema,
        "serverVersion": config.server_version,
        "capabilities": capabilities,
    }))
}

pub(crate) async fn get_available_drivers_impl(
    state: &AppState,
) -> Result<Vec<String>, CommandError> {
    Ok(state.driver_registry.available_types())
}

#[tauri::command]
pub async fn get_connections(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionConfig>, CommandError> {
    get_connections_impl(&state).await
}

#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
    clear_missing_credentials: Option<bool>,
) -> Result<(), CommandError> {
    save_connection_impl(&state, config, clear_missing_credentials.unwrap_or(false)).await
}

#[tauri::command]
pub async fn duplicate_connection(
    state: State<'_, AppState>,
    source_id: String,
    new_id: String,
    new_name: String,
) -> Result<(), CommandError> {
    duplicate_connection_impl(&state, source_id, new_id, new_name).await
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    delete_connection_impl(&state, id).await
}

#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<ServerInfo, CommandError> {
    test_connection_impl(&state, config).await
}

#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<ConnectionSessionRef, CommandError> {
    // connection_id = 持久化配置连接 id；返回值携带 connectionId + 运行时 dbSessionId。
    connect_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn connect_dedicated(
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
) -> Result<ConnectionSessionRef, CommandError> {
    connect_dedicated_impl(&state, connection_id, database).await
}

#[tauri::command]
pub async fn ping_connection(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<bool, CommandError> {
    ping_connection_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn release_connection(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<bool, CommandError> {
    release_connection_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<(), CommandError> {
    disconnect_impl(&state, db_session_id).await
}

/// F5: release one database's backend resources without closing the session.
#[tauri::command]
pub async fn close_database(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
) -> Result<bool, CommandError> {
    close_database_impl(&state, db_session_id, database).await
}

#[tauri::command]
pub async fn get_open_databases(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<Vec<String>, CommandError> {
    get_open_databases_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn get_connection_info(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<serde_json::Value, CommandError> {
    get_connection_info_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn reorder_connections(
    state: State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> Result<(), CommandError> {
    tracing::info!(count = ordered_ids.len(), "reorder_connections");
    let scope = state.connection_scope()?;
    let service = crate::platform::ipc::require_connection_service(&state)?;
    conn_cmd(
        service.reorder(&scope, ordered_ids).await,
        "reorder_connections",
    )
}

#[tauri::command]
pub async fn get_available_drivers(
    state: State<'_, AppState>,
) -> Result<Vec<String>, CommandError> {
    get_available_drivers_impl(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use datazen_driver_api::{HttpProxyTunnelConfig, SshTunnelConfig, WebSocketTunnelConfig};

    #[tokio::test]
    async fn get_connections_empty_then_saved() {
        let test = TestAppState::new().await;
        assert!(get_connections_impl(&test.state).await.unwrap().is_empty());
        test.save_connection("c1").await;
        let list = get_connections_impl(&test.state).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "c1");
    }

    #[tokio::test]
    async fn get_connections_redacts_credentials_and_save_preserves_or_clears_them() {
        let test = TestAppState::new().await;
        let mut config = sample_postgres_config("secret-profile");
        config.password = Some("db-password-marker".into());
        config.ssh_tunnel = Some(SshTunnelConfig {
            enabled: true,
            host: "ssh.example.com".into(),
            port: 22,
            username: "ssh-user".into(),
            auth_method: "password".into(),
            password: Some("ssh-password-marker".into()),
            private_key_path: None,
            passphrase: Some("ssh-passphrase-marker".into()),
            jump: Some(Box::new(SshTunnelConfig {
                enabled: true,
                host: "jump.example.com".into(),
                port: 22,
                username: "jump-user".into(),
                auth_method: "password".into(),
                password: Some("jump-password-marker".into()),
                private_key_path: None,
                passphrase: None,
                jump: None,
            })),
        });
        config.http_proxy_tunnel = Some(HttpProxyTunnelConfig {
            enabled: true,
            host: "proxy.example.com".into(),
            port: 8080,
            scheme: "https".into(),
            username: Some("proxy-user".into()),
            password: Some("proxy-password-marker".into()),
            headers: Some([("Authorization".into(), "proxy-header-marker".into())].into()),
            connect_timeout_secs: 30,
        });
        config.websocket_tunnel = Some(WebSocketTunnelConfig {
            enabled: true,
            url: "wss://relay.example.com/tunnel".into(),
            auth_token: Some("ws-token-marker".into()),
            headers: Some([("Authorization".into(), "ws-header-marker".into())].into()),
            connect_timeout_secs: 30,
            ping_interval_secs: 30,
            mode: "datazen_v1".into(),
        });
        save_connection_impl(&test.state, config, false)
            .await
            .unwrap();

        let projected = get_connections_impl(&test.state).await.unwrap();
        let wire = serde_json::to_string(&projected).unwrap();
        for secret in [
            "db-password-marker",
            "ssh-password-marker",
            "ssh-passphrase-marker",
            "jump-password-marker",
            "proxy-password-marker",
            "proxy-header-marker",
            "ws-token-marker",
            "ws-header-marker",
        ] {
            assert!(!wire.contains(secret), "IPC leaked {secret}");
        }
        assert!(projected[0].password.is_none());

        let mut edit = projected[0].clone();
        edit.name = "Edited profile".into();
        save_connection_impl(&test.state, edit.clone(), false)
            .await
            .unwrap();
        let saved = test.store.get_connection(&edit.id).await.unwrap();
        assert_eq!(saved.password.as_deref(), Some("db-password-marker"));
        assert_eq!(
            saved
                .ssh_tunnel
                .as_ref()
                .and_then(|ssh| ssh.password.as_deref()),
            Some("ssh-password-marker")
        );
        assert_eq!(
            saved
                .ssh_tunnel
                .as_ref()
                .and_then(|ssh| ssh.passphrase.as_deref()),
            Some("ssh-passphrase-marker")
        );
        assert_eq!(
            saved
                .ssh_tunnel
                .as_ref()
                .and_then(|ssh| ssh.jump.as_ref())
                .and_then(|jump| jump.password.as_deref()),
            Some("jump-password-marker")
        );
        assert_eq!(
            saved
                .http_proxy_tunnel
                .as_ref()
                .and_then(|proxy| proxy.password.as_deref()),
            Some("proxy-password-marker")
        );
        assert_eq!(
            saved
                .http_proxy_tunnel
                .as_ref()
                .and_then(|proxy| proxy.headers.as_ref())
                .and_then(|headers| headers.get("Authorization"))
                .map(String::as_str),
            Some("proxy-header-marker")
        );
        assert_eq!(
            saved
                .websocket_tunnel
                .as_ref()
                .and_then(|ws| ws.auth_token.as_deref()),
            Some("ws-token-marker")
        );
        assert_eq!(
            saved
                .websocket_tunnel
                .as_ref()
                .and_then(|ws| ws.headers.as_ref())
                .and_then(|headers| headers.get("Authorization"))
                .map(String::as_str),
            Some("ws-header-marker")
        );

        save_connection_impl(&test.state, edit, true).await.unwrap();
        let cleared = test.store.get_connection("secret-profile").await.unwrap();
        assert!(cleared.password.is_none());
        assert!(cleared.ssh_tunnel.as_ref().unwrap().password.is_none());
        assert!(cleared.ssh_tunnel.as_ref().unwrap().passphrase.is_none());
        assert!(cleared
            .ssh_tunnel
            .as_ref()
            .unwrap()
            .jump
            .as_ref()
            .unwrap()
            .password
            .is_none());
        assert!(cleared
            .http_proxy_tunnel
            .as_ref()
            .unwrap()
            .password
            .is_none());
        assert!(cleared
            .http_proxy_tunnel
            .as_ref()
            .unwrap()
            .headers
            .is_none());
        assert!(cleared
            .websocket_tunnel
            .as_ref()
            .unwrap()
            .auth_token
            .is_none());
        assert!(cleared.websocket_tunnel.as_ref().unwrap().headers.is_none());
    }

    #[tokio::test]
    async fn duplicate_connection_copies_credentials_without_returning_them() {
        let test = TestAppState::new().await;
        let mut source = sample_postgres_config("source-profile");
        source.password = Some("duplicate-secret-marker".into());
        test.store.save_connection(source).await.unwrap();

        duplicate_connection_impl(
            &test.state,
            "source-profile".into(),
            "copied-profile".into(),
            "Copied profile".into(),
        )
        .await
        .unwrap();

        let copied = test.store.get_connection("copied-profile").await.unwrap();
        assert_eq!(copied.name, "Copied profile");
        assert_eq!(copied.password.as_deref(), Some("duplicate-secret-marker"));
        let wire =
            serde_json::to_string(&get_connections_impl(&test.state).await.unwrap()).unwrap();
        assert!(!wire.contains("duplicate-secret-marker"));
    }

    #[tokio::test]
    async fn save_and_delete_connection() {
        let test = TestAppState::new().await;
        let cfg = sample_postgres_config("del-me");
        save_connection_impl(&test.state, cfg, false).await.unwrap();
        delete_connection_impl(&test.state, "del-me".into())
            .await
            .unwrap();
        assert!(get_connections_impl(&test.state).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_connection_success_and_unknown_driver() {
        let test = TestAppState::with_tables().await;
        let info = test_connection_impl(&test.state, sample_postgres_config("t1"))
            .await
            .unwrap();
        assert!(info.server_version.contains("PostgreSQL"));

        let mut bad = sample_postgres_config("bad");
        bad.database_type = "nonexistent_driver".into();
        assert!(test_connection_impl(&test.state, bad).await.is_err());
    }

    #[tokio::test]
    async fn connect_updates_last_connected_and_ping_disconnect() {
        let test = TestAppState::with_tables().await;
        test.save_connection("cfg-1").await;
        let conn_id = connect_impl(&test.state, "cfg-1".into())
            .await
            .unwrap()
            .db_session_id;
        let stored = test.store.get_connection("cfg-1").await.unwrap();
        assert!(stored.last_connected_at.is_some());

        assert!(ping_connection_impl(&test.state, conn_id.clone())
            .await
            .unwrap());
        disconnect_impl(&test.state, conn_id).await.unwrap();
        assert!(!ping_connection_impl(&test.state, "missing".into())
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn get_connection_info_and_available_drivers() {
        let test = TestAppState::with_tables().await;
        let (_, db_session_id) = test.save_and_connect("cfg-2").await;
        let info = get_connection_info_impl(&test.state, db_session_id.clone())
            .await
            .unwrap();
        assert_eq!(info["databaseType"], "postgres");
        assert_eq!(info["driverCategory"], "sql");
        assert!(info["capabilities"].is_null());

        test.state
            .driver_registry
            .register_test_driver_with_capabilities(
                "postgres",
                test.mock.clone(),
                datazen_runtime::driver_registry::DriverCapabilities {
                    has_multi_database: false,
                    supports_cancel_query: true,
                    supports_query_execution_cancel: true,
                    supports_explain: true,
                    supports_streaming_results: false,
                    supports_offset: true,
                    has_schema_level: true,
                },
            )
            .await;
        let info = get_connection_info_impl(&test.state, db_session_id)
            .await
            .unwrap();
        assert_eq!(info["capabilities"]["supportsCancelQuery"], true);
        assert_eq!(info["capabilities"]["supportsExplain"], true);
        assert_eq!(info["capabilities"]["supportsStreamingResults"], false);

        let drivers = get_available_drivers_impl(&test.state).await.unwrap();
        // Test drivers are registered via register_test_driver, not inventory catalog.
        let _ = drivers;
        assert!(test
            .state
            .driver_registry
            .get(&"postgres".to_string())
            .await
            .is_some());
    }

    #[tokio::test]
    async fn connect_without_configured_database_opens_a_session() {
        // The connection form does not require a database for server-level
        // drivers, so "open this profile" must work with the field left blank.
        // The driver resolves its own default; the host must not reject the
        // profile with a global "database required" rule.
        let test = TestAppState::with_tables().await;
        let mut config = sample_postgres_config("no-db");
        config.database = None;
        test.store.save_connection(config).await.unwrap();

        let session = connect_impl(&test.state, "no-db".into())
            .await
            .expect("a profile without a database must still open a session");
        assert_eq!(session.connection_id, "no-db");
        assert!(!session.db_session_id.is_empty());

        let info = get_connection_info_impl(&test.state, session.db_session_id)
            .await
            .unwrap();
        assert!(info["database"].is_null());
    }

    #[tokio::test]
    async fn connect_treats_a_blank_database_as_unset() {
        let test = TestAppState::with_tables().await;
        let mut config = sample_postgres_config("blank-db");
        config.database = Some("   ".into());
        test.store.save_connection(config).await.unwrap();

        connect_impl(&test.state, "blank-db".into())
            .await
            .expect("a whitespace-only database is the same as no database");
    }

    #[tokio::test]
    async fn connect_dedicated_without_any_database_opens_a_session() {
        let test = TestAppState::with_tables().await;
        let mut config = sample_postgres_config("no-db-dedicated");
        config.database = None;
        test.store.save_connection(config).await.unwrap();

        let session = connect_dedicated_impl(&test.state, "no-db-dedicated".into(), None)
            .await
            .expect("a dedicated session may open before a database is chosen");
        assert!(!session.db_session_id.is_empty());
    }

    #[tokio::test]
    async fn connect_dedicated_blank_database_falls_back_to_the_profile() {
        let test = TestAppState::with_tables().await;
        test.save_connection("blank-dedicated").await;

        let session =
            connect_dedicated_impl(&test.state, "blank-dedicated".into(), Some("  ".into()))
                .await
                .expect("blank means unpinned, so the profile's database applies");
        assert!(!session.db_session_id.is_empty());
    }

    #[test]
    fn normalize_database_collapses_missing_and_blank_values() {
        use crate::services::migration_endpoint::normalize_database;

        assert_eq!(normalize_database(None), None);
        assert_eq!(normalize_database(Some("")), None);
        assert_eq!(normalize_database(Some("   ")), None);
        assert_eq!(
            normalize_database(Some("  app_db  ")),
            Some("app_db".to_owned())
        );
    }

    #[tokio::test]
    async fn connect_unknown_config_errors() {
        let test = TestAppState::new().await;
        assert!(connect_impl(&test.state, "missing".into()).await.is_err());
    }

    #[tokio::test]
    async fn get_connection_info_unknown_connection_errors() {
        let test = TestAppState::new().await;
        assert!(get_connection_info_impl(&test.state, "nope".into())
            .await
            .is_err());
    }

    #[test]
    fn sample_config_has_expected_database_type() {
        let cfg = sample_postgres_config("x");
        assert_eq!(cfg.database_type, "postgres");
    }
}

#[cfg(test)]
mod coverage_tests {
    //! BUG-002 anchors: exercises the remaining `connection.rs` branches —
    //! release ref-counting, disconnect's pending-transaction rollback,
    //! driver-category reporting, and the reorder wrapper.

    use super::*;
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};

    #[tokio::test]
    async fn connect_dedicated_opens_separate_session_from_reuse() {
        let test = TestAppState::with_tables().await;
        test.save_connection("d1").await;
        let revision_before = test
            .store
            .platform_profile("d1")
            .await
            .expect("the saved profile exists")
            .1
            .config_revision;

        let shared = connect_impl(&test.state, "d1".into())
            .await
            .unwrap()
            .db_session_id;
        let dedicated = connect_dedicated_impl(&test.state, "d1".into(), Some("app".into()))
            .await
            .unwrap()
            .db_session_id;
        assert_ne!(shared, dedicated);
        // Connecting is metadata-only: `store::save_connection` would bump the
        // config revision and invalidate every live session on the profile with
        // "connection configuration changed". `touch_last_connected_at` must not.
        // Release semantics are covered by
        // `release_connection_decrements_then_tears_down` below.
        let revision_after = test
            .store
            .platform_profile("d1")
            .await
            .expect("the saved profile exists")
            .1
            .config_revision;
        assert_eq!(
            revision_before, revision_after,
            "opening a session must not re-save the profile"
        );
        assert!(
            ping_connection_impl(&test.state, shared).await.unwrap(),
            "the shared session must stay usable after a dedicated session opens"
        );
    }

    #[tokio::test]
    async fn close_database_releases_one_database_and_keeps_the_session() {
        let test = TestAppState::with_options(MockDriverOptions {
            has_schema_level: true,
            default_schema: Some("public"),
            ..Default::default()
        })
        .await;
        test.save_connection("cd1").await;
        let session = connect_impl(&test.state, "cd1".into())
            .await
            .unwrap()
            .db_session_id;

        let closed = close_database_impl(&test.state, session.clone(), "analytics".into())
            .await
            .unwrap();
        assert!(closed, "a schema-aware driver reports a released pool");
        assert_eq!(
            test.mock.close_database_calls(),
            vec!["analytics".to_string()]
        );
        // The session must survive: only the named database's resources went away.
        assert!(ping_connection_impl(&test.state, session).await.unwrap());
    }

    #[tokio::test]
    async fn open_databases_reports_what_the_driver_still_holds() {
        let test = TestAppState::with_options(MockDriverOptions {
            has_schema_level: true,
            default_schema: Some("public"),
            open_databases: vec!["analytics".into(), "app".into(), "analytics".into()],
            ..Default::default()
        })
        .await;
        test.save_connection("od1").await;
        let session = connect_impl(&test.state, "od1".into())
            .await
            .unwrap()
            .db_session_id;

        let open = get_open_databases_impl(&test.state, session).await.unwrap();
        // Sorted and de-duplicated, so the UI marker is stable across calls.
        assert_eq!(open, vec!["analytics".to_string(), "app".to_string()]);
    }

    #[tokio::test]
    async fn open_databases_is_empty_for_drivers_without_database_pools() {
        let test = TestAppState::with_tables().await;
        test.save_connection("od2").await;
        let session = connect_impl(&test.state, "od2".into())
            .await
            .unwrap()
            .db_session_id;
        assert!(get_open_databases_impl(&test.state, session)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn open_databases_rejects_an_unknown_session() {
        let test = TestAppState::with_tables().await;
        let err = get_open_databases_impl(&test.state, "no-such-session".into())
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("not connected")
                || err.to_string().contains("session unavailable")
        );
    }

    #[tokio::test]
    async fn close_database_is_a_noop_for_drivers_without_database_pools() {
        let test = TestAppState::with_tables().await;
        test.save_connection("cd2").await;
        let session = connect_impl(&test.state, "cd2".into())
            .await
            .unwrap()
            .db_session_id;
        let closed = close_database_impl(&test.state, session, "app".into())
            .await
            .unwrap();
        assert!(!closed);
    }

    #[tokio::test]
    async fn close_database_rejects_blank_names_and_unknown_sessions() {
        let test = TestAppState::with_tables().await;
        test.save_connection("cd3").await;
        let session = connect_impl(&test.state, "cd3".into())
            .await
            .unwrap()
            .db_session_id;

        let blank = close_database_impl(&test.state, session.clone(), "   ".into())
            .await
            .unwrap_err();
        assert!(blank.to_string().contains("non-empty database name"));

        let unknown = close_database_impl(&test.state, "no-such-session".into(), "app".into())
            .await
            .unwrap_err();
        assert!(
            unknown.to_string().contains("not connected")
                || unknown.to_string().contains("session unavailable")
        );
        assert!(
            test.mock.close_database_calls().is_empty(),
            "neither rejection may reach the driver"
        );
    }

    #[tokio::test]
    async fn release_connection_decrements_then_tears_down() {
        let test = TestAppState::with_tables().await;
        test.save_connection("r1").await;
        // Runtime opens one owned session per connect call.
        let s1 = connect_impl(&test.state, "r1".into())
            .await
            .unwrap()
            .db_session_id;
        let s2 = connect_impl(&test.state, "r1".into())
            .await
            .unwrap()
            .db_session_id;
        assert_ne!(s1, s2, "Runtime session ownership is explicit per connect");

        // Each Runtime release closes exactly the owned session.
        let torn = release_connection_impl(&test.state, s1.clone())
            .await
            .unwrap();
        assert!(torn);
        assert!(!ping_connection_impl(&test.state, s1.clone()).await.unwrap());

        // The second owned session is independent.
        let torn = release_connection_impl(&test.state, s2.clone())
            .await
            .unwrap();
        assert!(torn);
        assert!(
            !ping_connection_impl(&test.state, s1).await.unwrap(),
            "session must be gone after the final release"
        );

        // Releasing an unknown session is a benign no-op (reports torn down).
        let torn = release_connection_impl(&test.state, "missing".into())
            .await
            .unwrap();
        assert!(torn);
    }

    #[tokio::test]
    async fn disconnect_rolls_back_runtime_transaction_and_clears_status() {
        let test = TestAppState::with_tables().await;
        let (_, id) = test.save_and_connect("tx-close").await;
        crate::commands::query::begin_session_transaction_impl(&test.state, id.clone())
            .await
            .unwrap();
        assert_eq!(test.mock.open_transaction_count(), 1);
        disconnect_impl(&test.state, id.clone()).await.unwrap();
        assert!(
            !crate::commands::query::session_transaction_status_impl(&test.state, id.clone())
                .await
                .unwrap()
        );
        assert_eq!(test.mock.open_transaction_count(), 0);
        assert!(!ping_connection_impl(&test.state, id).await.unwrap());
    }

    #[tokio::test]
    async fn get_connection_info_reports_driver_category() {
        let harness = TestAppState::new().await;
        for (driver_type, expected_category, category) in [
            (
                "kv-mock",
                "keyvalue",
                datazen_driver_api::DriverCategory::KeyValue,
            ),
            (
                "doc-mock",
                "document",
                datazen_driver_api::DriverCategory::Document,
            ),
            ("postgres", "sql", datazen_driver_api::DriverCategory::Sql),
        ] {
            let opts = MockDriverOptions {
                category,
                ..Default::default()
            };
            let mock = MockDriver::new(driver_type, opts);
            harness
                .state
                .driver_registry
                .register_test_driver(driver_type, mock.clone())
                .await;
            harness.register_runtime_driver(driver_type, mock).await;
            let mut cfg = sample_postgres_config(driver_type);
            cfg.database_type = driver_type.into();
            save_connection_impl(&harness.state, cfg, false)
                .await
                .unwrap();

            let conn_id = connect_impl(&harness.state, driver_type.into())
                .await
                .unwrap()
                .db_session_id;
            let info = get_connection_info_impl(&harness.state, conn_id)
                .await
                .unwrap();
            assert_eq!(info["databaseType"], driver_type);
            assert_eq!(
                info["driverCategory"], expected_category,
                "driver {driver_type} must report category {expected_category}"
            );
        }
    }

    #[tokio::test]
    async fn reorder_connections_persists_new_order() {
        // `reorder_connections` is a thin IPC wrapper over the store call;
        // tauri::State cannot be built outside the tauri runtime, so exercise
        // the store path the wrapper delegates to.
        let test = TestAppState::new().await;
        for id in ["a", "b", "c"] {
            save_connection_impl(&test.state, sample_postgres_config(id), false)
                .await
                .unwrap();
        }
        test.state
            .store
            .reorder_connections(vec!["c".into(), "a".into(), "b".into()])
            .await
            .unwrap();
        let order: Vec<String> = get_connections_impl(&test.state)
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(order, vec!["c", "a", "b"]);
    }

    /// 连接目录族（§5.1.2 第六步第三条命令族）的生产半边不直读宿主 store。
    ///
    /// 计数而不是「有没有」：这 8 条命令原本各有 1 处 `state.store`，只要有人把
    /// 某条换回宿主 store，这里会先红。`duplicate_connection_impl` 与
    /// `connect_dedicated_impl` 里的 `.store` 曾经跨行书写，所以断言不能用「逐行 grep」
    /// 的口径来写。
    #[test]
    fn the_command_shell_reads_the_module_service_not_the_host_store() {
        let source = include_str!("connection.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert_eq!(
            production.matches("state.store").count(),
            0,
            "连接目录命令必须走模块连接服务，不得直读宿主 store"
        );
        // D2 后：连接用例归 datazen-modules::connection，经
        // require_connection_service 取宿主装配的同一个服务实例。
        // 聚合（require_application_services）只服务隧道测试等仍住在
        // application 层的端口：测试连接 1、连接 1、指定库连接 1。
        assert_eq!(
            production.matches("require_connection_service").count(),
            8,
            "列表 / 保存 / 复制 / 删除 / 测试连接 / 连接 / 指定库连接 / 重排 各取一次模块服务"
        );
        assert_eq!(
            production.matches("merge_missing_secrets").count()
                + production.matches("redact_secrets").count(),
            2,
            "合并缺失密钥与脱敏投影必须复用模块的纯函数，不得在命令层另写一份"
        );
    }
}
