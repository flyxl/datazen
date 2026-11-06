//! MCP Streamable HTTP transport (desktop, opt-in).
//!
//! The listener is deliberately IPv4 loopback only. A bearer token protects
//! every `/mcp` request; only its SHA-256 hash and last four characters are
//! persisted. The plaintext token exists only in the enable/regenerate IPC
//! response and in the running process' request verifier.

use crate::commands::AppState;
use crate::mcp::server::DataZenMcpServer;
use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::session::SessionManager as _;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

pub(crate) const HTTP_HOST: &str = "127.0.0.1";
const TOKEN_BYTES: usize = 32;
const TOKEN_FILE: &str = "mcp-http-token.json";
const TOKEN_FILE_VERSION: u8 = 1;

/// Wire shape returned by the HTTP lifecycle IPC commands. `token` is present
/// only in the response that just generated it; status reads never include it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHttpStatus {
    pub running: bool,
    pub enabled: bool,
    pub transport: String,
    pub url: Option<String>,
    /// Actual bound port (random when configured as 0).
    pub port: Option<u16>,
    /// Persisted configuration; 0 means "random on each start".
    pub configured_port: u16,
    pub token_last4: Option<String>,
    pub token_configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TokenFile {
    version: u8,
    hash: String,
    last4: String,
}

/// The public-for-display part of one token generation. `plaintext` is present
/// only when this start just created the token.
struct IssuedToken {
    last4: String,
    plaintext: Option<String>,
}

/// In-memory verifier. Rotation replaces the hash under the same `Arc`, so a
/// request that arrives after regeneration cannot pass with the old token.
struct HttpAuth {
    hash: RwLock<Option<[u8; 32]>>,
}

impl HttpAuth {
    fn new(hash: Option<[u8; 32]>) -> Self {
        Self {
            hash: RwLock::new(hash),
        }
    }

    async fn verify(&self, token: &str) -> bool {
        let Some(expected) = *self.hash.read().await else {
            return false;
        };
        let presented = Sha256::digest(token.as_bytes());
        expected.ct_eq(presented.as_slice()).into()
    }

    async fn replace(&self, hash: [u8; 32]) {
        *self.hash.write().await = Some(hash);
    }
}

struct HttpHandle {
    auth: Arc<HttpAuth>,
    /// Kept so Stop can explicitly close every session, not merely stop
    /// accepting new connections. Closing a handle drops its server, whose
    /// cleanup guard releases that client's stateful mappings.
    sessions: Arc<LocalSessionManager>,
    port: u16,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

static HTTP_HANDLE: Mutex<Option<HttpHandle>> = Mutex::const_new(None);

#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::const_new(());

/// Start (or restart) the loopback HTTP listener. A token is generated only
/// when none exists yet; the returned status carries that one-time plaintext.
pub(crate) async fn start(
    state: Arc<AppState>,
    configured_port: u16,
) -> Result<McpHttpStatus, String> {
    start_inner(state, configured_port, true).await
}

/// Startup path: start only if an existing token file is present. It must not
/// silently create a token the user has never seen.
pub(crate) async fn start_existing(
    state: Arc<AppState>,
    configured_port: u16,
) -> Result<McpHttpStatus, String> {
    start_inner(state, configured_port, false).await
}

async fn start_inner(
    state: Arc<AppState>,
    configured_port: u16,
    generate_if_missing: bool,
) -> Result<McpHttpStatus, String> {
    let mut slot = HTTP_HANDLE.lock().await;
    if let Some(handle) = slot.take() {
        stop_handle(handle).await;
    }

    // Bind before touching the token file: a port conflict must not rotate or
    // consume a one-time token.
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, configured_port)))
        .await
        .map_err(|error| format!("MCP HTTP could not bind 127.0.0.1:{configured_port}: {error}"))?;
    let actual_port = listener
        .local_addr()
        .map_err(|error| format!("MCP HTTP could not read the bound port: {error}"))?
        .port();

    let token_file = match read_token_file(state.store.data_dir()).await? {
        Some(file) => file,
        None => {
            if !generate_if_missing {
                return Err(
                    "MCP HTTP is enabled but no token is configured; regenerate it in Settings"
                        .to_string(),
                );
            }
            let token = generate_token();
            let file = token_file_for(&token);
            write_token_file(state.store.data_dir(), &file).await?;
            let hash = parse_hash(&file.hash)
                .ok_or_else(|| "MCP HTTP token hash is invalid".to_string())?;
            let auth = Arc::new(HttpAuth::new(Some(hash)));
            return start_listener(
                state,
                listener,
                configured_port,
                actual_port,
                auth,
                IssuedToken {
                    last4: file.last4,
                    plaintext: Some(token),
                },
                slot,
            )
            .await;
        }
    };
    let hash =
        parse_hash(&token_file.hash).ok_or_else(|| "MCP HTTP token hash is invalid".to_string())?;
    let auth = Arc::new(HttpAuth::new(Some(hash)));
    start_listener(
        state,
        listener,
        configured_port,
        actual_port,
        auth,
        IssuedToken {
            last4: token_file.last4,
            plaintext: None,
        },
        slot,
    )
    .await
}

async fn start_listener(
    state: Arc<AppState>,
    listener: TcpListener,
    configured_port: u16,
    actual_port: u16,
    auth: Arc<HttpAuth>,
    issued: IssuedToken,
    mut slot: tokio::sync::MutexGuard<'_, Option<HttpHandle>>,
) -> Result<McpHttpStatus, String> {
    let settings = state.store.get_settings().await;
    let cancel = CancellationToken::new();

    let factory_state = state.clone();
    let mut config = StreamableHttpServerConfig::default();
    config.stateful_mode = true;
    config.allowed_hosts = vec![
        HTTP_HOST.to_string(),
        "localhost".to_string(),
        format!("{HTTP_HOST}:{actual_port}"),
        format!("localhost:{actual_port}"),
    ];
    config.cancellation_token = cancel.child_token();

    let sessions = Arc::new(LocalSessionManager::default());
    let service: StreamableHttpService<DataZenMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || {
                let server = DataZenMcpServer::new(factory_state.clone())
                    .with_disabled_tools(&settings.mcp_disabled_tools)
                    .with_permission_mode(settings.mcp_permission_mode)
                    .with_allowed_connections(&settings.mcp_allowed_connection_ids)
                    .with_allowed_scopes(
                        &settings.mcp_allowed_databases,
                        &settings.mcp_allowed_namespaces,
                    )
                    .with_http_cleanup();
                Ok(server)
            },
            sessions.clone(),
            config,
        );

    let app = Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(auth.clone(), require_bearer));

    let shutdown = cancel.clone();
    let task = tokio::spawn(async move {
        let served = axum::serve(listener, app).with_graceful_shutdown(async move {
            shutdown.cancelled().await;
        });
        if let Err(error) = served.await {
            tracing::error!(error = %error, "MCP HTTP server stopped");
        }
    });

    *slot = Some(HttpHandle {
        auth,
        sessions,
        port: actual_port,
        cancel,
        task,
    });
    drop(slot);

    Ok(McpHttpStatus {
        running: true,
        enabled: true,
        transport: "http".into(),
        url: Some(http_url(actual_port)),
        port: Some(actual_port),
        configured_port,
        token_last4: Some(issued.last4),
        token_configured: true,
        token: issued.plaintext,
    })
}

/// Stop the listener. The HTTP service and every live session are cancelled;
/// each session's cleanup guard releases its stateful mappings as it drops.
pub(crate) async fn stop() {
    // Stop must not leave an HTTP tool call paused forever in the approval UI.
    // This mirrors the stdio Stop path; the broker has no per-transport view.
    crate::mcp::approval_broker().cancel_all();
    let mut slot = HTTP_HANDLE.lock().await;
    if let Some(handle) = slot.take() {
        stop_handle(handle).await;
    }
}

async fn stop_handle(handle: HttpHandle) {
    handle.cancel.cancel();
    let HttpHandle {
        sessions, mut task, ..
    } = handle;
    let ids: Vec<_> = sessions.sessions.read().await.keys().cloned().collect();
    for id in ids {
        // A session paused on an approval must not make Stop hang forever.
        let _ = tokio::time::timeout(Duration::from_secs(2), sessions.close_session(&id)).await;
    }
    if tokio::time::timeout(Duration::from_secs(5), &mut task)
        .await
        .is_err()
    {
        task.abort();
    }
}

/// Persisted status plus the live actual port. No plaintext token is returned.
pub(crate) async fn status(state: &AppState) -> Result<McpHttpStatus, String> {
    let settings = state.store.get_settings().await;
    let file = read_token_file(state.store.data_dir()).await?;
    let (running, actual_port) = {
        let slot = HTTP_HANDLE.lock().await;
        match slot.as_ref() {
            Some(handle) if !handle.task.is_finished() => (true, Some(handle.port)),
            _ => (false, None),
        }
    };
    let token_last4 = file.as_ref().map(|file| file.last4.clone());
    Ok(McpHttpStatus {
        running,
        enabled: running || settings.mcp_http_enabled,
        transport: if running {
            "http".into()
        } else {
            "none".into()
        },
        url: actual_port.map(http_url),
        port: actual_port,
        configured_port: settings.mcp_http_port,
        token_last4,
        token_configured: file.is_some(),
        token: None,
    })
}

/// Generate a new high-entropy token, persist only its hash/last4, and make it
/// effective immediately for a running listener. Old tokens fail on the next
/// request because verification reads the replaced hash.
pub(crate) async fn regenerate_token(state: &AppState) -> Result<McpHttpStatus, String> {
    let token = generate_token();
    let file = token_file_for(&token);
    let hash =
        parse_hash(&file.hash).ok_or_else(|| "MCP HTTP token hash is invalid".to_string())?;
    {
        // Serialize with start/stop: a start that reads the file must see the
        // same generation that a running listener verifies.
        let slot = HTTP_HANDLE.lock().await;
        write_token_file(state.store.data_dir(), &file).await?;
        if let Some(handle) = slot.as_ref() {
            handle.auth.replace(hash).await;
        }
    }
    let mut status = status(state).await?;
    status.token = Some(token);
    Ok(status)
}

fn http_url(port: u16) -> String {
    format!("http://{HTTP_HOST}:{port}/mcp")
}

fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn token_file_for(token: &str) -> TokenFile {
    TokenFile {
        version: TOKEN_FILE_VERSION,
        hash: format!("{:x}", Sha256::digest(token.as_bytes())),
        last4: last4(token),
    }
}

fn last4(token: &str) -> String {
    token
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn parse_hash(value: &str) -> Option<[u8; 32]> {
    let raw = value.strip_prefix("sha256:").unwrap_or(value);
    if raw.len() != 64 {
        return None;
    }
    let mut hash = [0u8; 32];
    for (index, chunk) in raw.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).ok()?;
        hash[index] = u8::from_str_radix(text, 16).ok()?;
    }
    Some(hash)
}

async fn read_token_file(data_dir: &Path) -> Result<Option<TokenFile>, String> {
    let path = data_dir.join(TOKEN_FILE);
    let raw = match tokio::fs::read_to_string(&path).await {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!("MCP HTTP token file could not be read: {error}"));
        }
    };
    let file: TokenFile =
        serde_json::from_str(&raw).map_err(|_| "MCP HTTP token file is invalid".to_string())?;
    if file.version != TOKEN_FILE_VERSION
        || parse_hash(&file.hash).is_none()
        || file.last4.chars().count() != 4
    {
        return Err("MCP HTTP token file is invalid".to_string());
    }
    Ok(Some(file))
}

async fn write_token_file(data_dir: &Path, file: &TokenFile) -> Result<(), String> {
    let path = data_dir.join(TOKEN_FILE);
    let temp = data_dir.join(format!(".{TOKEN_FILE}.{}.tmp", uuid::Uuid::new_v4()));
    let raw = serde_json::to_vec(file)
        .map_err(|_| "MCP HTTP token file could not be encoded".to_string())?;
    tokio::fs::write(&temp, raw)
        .await
        .map_err(|error| format!("MCP HTTP token file could not be written: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|error| format!("MCP HTTP token file permissions failed: {error}"))?;
    }
    #[cfg(windows)]
    {
        // std::fs::rename does not replace an existing destination on Windows.
        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .map_err(|error| format!("MCP HTTP token file could not be replaced: {error}"))?;
        }
    }
    tokio::fs::rename(&temp, &path)
        .await
        .map_err(|error| format!("MCP HTTP token file could not be installed: {error}"))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

async fn require_bearer(
    AxumState(auth): AxumState<Arc<HttpAuth>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(token) = bearer_token(request.headers()) else {
        return unauthorized();
    };
    if auth.verify(token).await {
        next.run(request).await
    } else {
        unauthorized()
    }
}

fn unauthorized() -> Response {
    let mut response = (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    response.headers_mut().insert(
        WWW_AUTHENTICATE,
        HeaderValue::from_static("Bearer realm=\"datazen-mcp\""),
    );
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::app_state_test_support::TestAppState;
    use crate::testing::mock_driver::MockDriverOptions;
    use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
    use serde_json::{json, Value};
    use std::sync::atomic::Ordering;

    fn initialize_body(id: u64) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "datazen-http-test", "version": "1" }
            }
        })
    }

    fn rpc_body(body: &str) -> Value {
        let mut last = None;
        for line in body.lines() {
            if let Some(data) = line.strip_prefix("data:") {
                if let Ok(value) = serde_json::from_str::<Value>(data.trim()) {
                    last = Some(value);
                }
            }
        }
        last.unwrap_or_else(|| serde_json::from_str(body).expect("rpc body"))
    }

    async fn post(
        client: &reqwest::Client,
        url: &str,
        token: Option<&str>,
        session: Option<&str>,
        body: &Value,
    ) -> reqwest::Response {
        let mut request = client
            .post(url)
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .header("MCP-Protocol-Version", "2025-06-18")
            .json(body);
        if let Some(token) = token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        if let Some(session) = session {
            request = request.header("Mcp-Session-Id", session);
        }
        request.send().await.expect("http request")
    }

    async fn initialize(
        client: &reqwest::Client,
        url: &str,
        token: &str,
        id: u64,
    ) -> (String, Value) {
        let response = post(client, url, Some(token), None, &initialize_body(id)).await;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let session = response
            .headers()
            .get("mcp-session-id")
            .expect("session header")
            .to_str()
            .expect("session header text")
            .to_string();
        let text = response.text().await.expect("initialize body");
        (session, rpc_body(&text))
    }

    async fn send_initialized(client: &reqwest::Client, url: &str, token: &str, session: &str) {
        let response = post(
            client,
            url,
            Some(token),
            Some(session),
            &json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }),
        )
        .await;
        assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn token_file_stores_only_hash_and_last4() {
        let _guard = TEST_LOCK.lock().await;
        stop().await;
        let test = TestAppState::new().await;
        let store = test.store.clone();
        let state = Arc::new(test.state);
        let status = start(state.clone(), 0).await.expect("start");
        let token = status.token.expect("first enable returns token once");
        assert_eq!(token.len(), 43);
        assert!(status
            .url
            .as_deref()
            .unwrap_or("")
            .starts_with("http://127.0.0.1:"));
        assert_eq!(
            status.token_last4.as_deref(),
            Some(&token[token.len() - 4..])
        );

        let raw = tokio::fs::read_to_string(store.data_dir().join(TOKEN_FILE))
            .await
            .expect("token file");
        assert!(!raw.contains(&token));
        let parsed: TokenFile = serde_json::from_str(&raw).expect("token json");
        assert_eq!(parsed.last4, token[token.len() - 4..]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(store.data_dir().join(TOKEN_FILE))
                .expect("token metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        assert_eq!(
            parsed.hash,
            format!("{:x}", Sha256::digest(token.as_bytes()))
        );

        // A second start reuses the token and never returns plaintext again.
        stop().await;
        let again = start(state, 0).await.expect("restart");
        assert!(again.token.is_none());
        assert_eq!(again.token_last4, Some(parsed.last4));
        stop().await;
    }

    #[tokio::test]
    async fn disabled_status_does_not_listen() {
        let _guard = TEST_LOCK.lock().await;
        stop().await;
        let test = TestAppState::new().await;
        let status = status(&test.state).await.expect("status");
        assert!(!status.running);
        assert!(!status.enabled);
        assert_eq!(status.transport, "none");
        assert_eq!(status.configured_port, 0);
        assert!(status.url.is_none());
        assert!(!status.token_configured);
    }

    #[tokio::test]
    async fn startup_never_invents_a_token_the_user_has_not_seen() {
        let _guard = TEST_LOCK.lock().await;
        stop().await;
        let test = TestAppState::new().await;
        let error = start_existing(Arc::new(test.state), 0)
            .await
            .expect_err("startup requires an existing token");
        assert!(error.contains("no token is configured"), "{error}");
    }

    #[tokio::test]
    async fn missing_and_wrong_bearer_tokens_are_rejected() {
        let _guard = TEST_LOCK.lock().await;
        stop().await;
        let test = TestAppState::new().await;
        let state = Arc::new(test.state);
        let status = start(state.clone(), 0).await.expect("start");
        let token = status.token.expect("token");
        let url = status.url.expect("url");
        let client = reqwest::Client::new();
        let body = initialize_body(1);

        let missing = post(&client, &url, None, None, &body).await;
        assert_eq!(missing.status(), reqwest::StatusCode::UNAUTHORIZED);
        assert!(missing.headers().contains_key(WWW_AUTHENTICATE));

        let wrong = post(&client, &url, Some("not-the-token"), None, &body).await;
        assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);
        assert!(!wrong.text().await.unwrap_or_default().contains(&token));

        stop().await;
    }

    #[tokio::test]
    async fn authenticated_client_can_initialize_list_and_call_tools() {
        let _guard = TEST_LOCK.lock().await;
        stop().await;
        let test = TestAppState::new().await;
        let state = Arc::new(test.state);
        let status = start(state, 0).await.expect("start");
        let token = status.token.expect("token");
        let url = status.url.expect("url");
        let client = reqwest::Client::new();

        let (session, initialized) = initialize(&client, &url, &token, 1).await;
        assert_eq!(initialized["result"]["protocolVersion"], "2025-06-18");
        send_initialized(&client, &url, &token, &session).await;

        let listed = post(
            &client,
            &url,
            Some(&token),
            Some(&session),
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        )
        .await;
        assert_eq!(listed.status(), reqwest::StatusCode::OK);
        let listed = rpc_body(&listed.text().await.expect("tools body"));
        assert_eq!(
            listed["result"]["tools"].as_array().expect("tools").len(),
            crate::mcp::MCP_ALL_TOOLS.len()
        );

        let called = post(
            &client,
            &url,
            Some(&token),
            Some(&session),
            &json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": { "name": "list_connections", "arguments": {} }
            }),
        )
        .await;
        assert_eq!(called.status(), reqwest::StatusCode::OK);
        let called = rpc_body(&called.text().await.expect("call body"));
        assert_eq!(called["result"]["isError"], false);
        assert!(called["result"]["content"].is_array());

        stop().await;
    }

    #[tokio::test]
    async fn concurrent_sessions_are_isolated_and_delete_closes_them() {
        let _guard = TEST_LOCK.lock().await;
        stop().await;
        let test = TestAppState::new().await;
        let state = Arc::new(test.state);
        let status = start(state, 0).await.expect("start");
        let token = status.token.expect("token");
        let url = status.url.expect("url");
        let client = reqwest::Client::new();

        let (first, second) = tokio::join!(
            initialize(&client, &url, &token, 1),
            initialize(&client, &url, &token, 2)
        );
        let (first_session, _) = first;
        let (second_session, _) = second;
        assert_ne!(first_session, second_session);

        let first_list_body = json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}});
        let second_list_body = json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}});
        let (first_list, second_list) = tokio::join!(
            post(
                &client,
                &url,
                Some(&token),
                Some(&first_session),
                &first_list_body
            ),
            post(
                &client,
                &url,
                Some(&token),
                Some(&second_session),
                &second_list_body
            )
        );
        assert_eq!(first_list.status(), reqwest::StatusCode::OK);
        assert_eq!(second_list.status(), reqwest::StatusCode::OK);

        let deleted = client
            .delete(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header("Mcp-Session-Id", &first_session)
            .send()
            .await
            .expect("delete");
        assert_eq!(deleted.status(), reqwest::StatusCode::ACCEPTED);
        let after = post(
            &client,
            &url,
            Some(&token),
            Some(&first_session),
            &json!({"jsonrpc":"2.0","id":5,"method":"tools/list","params":{}}),
        )
        .await;
        assert!(!after.status().is_success());
        stop().await;
    }

    #[tokio::test]
    async fn deleting_an_http_session_releases_its_stateful_sessions() {
        let _guard = TEST_LOCK.lock().await;
        stop().await;
        let test = TestAppState::with_resource_provider(MockDriverOptions::default()).await;
        test.save_connection("mcp-http-cleanup").await;
        let mut settings = test.store.get_settings().await;
        settings.mcp_allowed_connection_ids = vec!["mcp-http-cleanup".into()];
        test.store
            .save_settings(settings)
            .await
            .expect("save HTTP settings");
        let state = Arc::new(test.state);
        let runtime = state
            .platform_require()
            .expect("platform")
            .runtime()
            .expect("runtime");
        let status = start(state, 0).await.expect("start");
        let token = status.token.expect("token");
        let url = status.url.expect("url");
        let client = reqwest::Client::new();
        let (session, _) = initialize(&client, &url, &token, 1).await;
        send_initialized(&client, &url, &token, &session).await;

        let opened = post(
            &client,
            &url,
            Some(&token),
            Some(&session),
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "open_session",
                    "arguments": {
                        "connection_id": "mcp-http-cleanup",
                        "database": "app",
                        "schema": "public",
                        "ttl_ms": 600000
                    }
                }
            }),
        )
        .await;
        assert_eq!(opened.status(), reqwest::StatusCode::OK);
        let opened = rpc_body(&opened.text().await.expect("open body"));
        assert_eq!(opened["result"]["isError"], false, "{opened}");
        let in_use = runtime
            .observability_snapshot()
            .await
            .expect("snapshot")
            .in_use_total;
        assert!(in_use > 0, "open_session should register a runtime session");

        let deleted = client
            .delete(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header("Mcp-Session-Id", &session)
            .send()
            .await
            .expect("delete");
        assert_eq!(deleted.status(), reqwest::StatusCode::ACCEPTED);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let in_use = runtime
                .observability_snapshot()
                .await
                .expect("snapshot")
                .in_use_total;
            if in_use == 0 {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "HTTP disconnect did not release the session: {in_use} still in use"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        stop().await;
    }

    #[tokio::test]
    async fn stopping_the_http_listener_releases_open_sessions() {
        let _guard = TEST_LOCK.lock().await;
        stop().await;
        let test = TestAppState::with_resource_provider(MockDriverOptions::default()).await;
        test.save_connection("mcp-http-stop-cleanup").await;
        let mut settings = test.store.get_settings().await;
        settings.mcp_allowed_connection_ids = vec!["mcp-http-stop-cleanup".into()];
        test.store
            .save_settings(settings)
            .await
            .expect("save HTTP settings");
        let state = Arc::new(test.state);
        let runtime = state
            .platform_require()
            .expect("platform")
            .runtime()
            .expect("runtime");
        let status = start(state, 0).await.expect("start");
        let token = status.token.expect("token");
        let url = status.url.expect("url");
        let client = reqwest::Client::new();
        let (session, _) = initialize(&client, &url, &token, 1).await;
        send_initialized(&client, &url, &token, &session).await;
        let opened = post(
            &client,
            &url,
            Some(&token),
            Some(&session),
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "open_session",
                    "arguments": {
                        "connection_id": "mcp-http-stop-cleanup",
                        "database": "app",
                        "schema": "public",
                        "ttl_ms": 600000
                    }
                }
            }),
        )
        .await;
        assert_eq!(opened.status(), reqwest::StatusCode::OK);
        assert_eq!(
            rpc_body(&opened.text().await.expect("open body"))["result"]["isError"],
            false
        );

        stop().await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let in_use = runtime
                .observability_snapshot()
                .await
                .expect("snapshot")
                .in_use_total;
            if in_use == 0 {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "HTTP Stop did not release the session: {in_use} still in use"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn a_session_reference_from_one_http_client_is_lost_in_another() {
        let _guard = TEST_LOCK.lock().await;
        stop().await;
        let test = TestAppState::with_resource_provider(MockDriverOptions::default()).await;
        test.save_connection("mcp-http-isolation").await;
        let mut settings = test.store.get_settings().await;
        settings.mcp_allowed_connection_ids = vec!["mcp-http-isolation".into()];
        test.store
            .save_settings(settings)
            .await
            .expect("save HTTP settings");
        let state = Arc::new(test.state);
        let status = start(state, 0).await.expect("start");
        let token = status.token.expect("token");
        let url = status.url.expect("url");
        let client = reqwest::Client::new();
        let (first, _) = initialize(&client, &url, &token, 1).await;
        let (second, _) = initialize(&client, &url, &token, 2).await;
        send_initialized(&client, &url, &token, &first).await;
        send_initialized(&client, &url, &token, &second).await;

        let opened = post(
            &client,
            &url,
            Some(&token),
            Some(&first),
            &json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "open_session",
                    "arguments": {
                        "connection_id": "mcp-http-isolation",
                        "database": "app",
                        "schema": "public",
                        "ttl_ms": 600000
                    }
                }
            }),
        )
        .await;
        let opened = rpc_body(&opened.text().await.expect("open body"));
        assert_eq!(opened["result"]["isError"], false, "{opened}");
        let text = opened["result"]["content"][0]["text"]
            .as_str()
            .expect("open text");
        let reference = serde_json::from_str::<Value>(text).expect("open json")["clientSessionRef"]
            .as_str()
            .expect("clientSessionRef")
            .to_string();

        let used = post(
            &client,
            &url,
            Some(&token),
            Some(&second),
            &json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "use_session",
                    "arguments": {
                        "client_session_ref": reference,
                        "command": "list_tables",
                        "input": {}
                    }
                }
            }),
        )
        .await;
        let used = rpc_body(&used.text().await.expect("use body"));
        assert!(
            used["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("SessionLost"),
            "{used}"
        );
        stop().await;
    }

    #[tokio::test]
    async fn token_rotation_invalidates_old_token_immediately() {
        let _guard = TEST_LOCK.lock().await;
        stop().await;
        let test = TestAppState::new().await;
        let state = Arc::new(test.state);
        let status = start(state.clone(), 0).await.expect("start");
        let old = status.token.expect("old token");
        let url = status.url.expect("url");
        let client = reqwest::Client::new();
        assert_eq!(
            post(&client, &url, Some(&old), None, &initialize_body(1))
                .await
                .status(),
            reqwest::StatusCode::OK
        );

        let rotated = regenerate_token(&state).await.expect("rotate");
        let new = rotated.token.expect("new token");
        assert_ne!(old, new);
        assert_eq!(
            post(&client, &url, Some(&old), None, &initialize_body(2))
                .await
                .status(),
            reqwest::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            post(&client, &url, Some(&new), None, &initialize_body(3))
                .await
                .status(),
            reqwest::StatusCode::OK
        );
        stop().await;
    }

    #[tokio::test]
    async fn port_conflict_fails_without_starting_a_listener() {
        let _guard = TEST_LOCK.lock().await;
        stop().await;
        let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("hold port");
        let port = held.local_addr().expect("port").port();
        let test = TestAppState::new().await;
        let state = Arc::new(test.state);
        let error = start(state.clone(), port).await.expect_err("conflict");
        assert!(error.contains("could not bind"), "{error}");
        assert!(!status(&state).await.expect("status").running);
        assert!(
            read_token_file(state.store.data_dir())
                .await
                .expect("token read")
                .is_none(),
            "a failed bind must not consume a one-time token"
        );
        drop(held);
        stop().await;
    }

    #[tokio::test]
    async fn http_cleanup_guard_marks_the_client_disconnected() {
        let _guard = TEST_LOCK.lock().await;
        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state)).with_http_cleanup();
        let sessions = server.sessions.clone();
        drop(server);
        assert!(sessions.disconnected.load(Ordering::Acquire));
    }
}
