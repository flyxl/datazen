//! Kiwi driver — proxies SQL queries through the Kiwi HTTP API.
//!
//! Field mapping:
//!   config.host     → Kiwi base URL  (e.g. "https://kiwi.akusre.com")
//!   config.username → SSO username (plaintext)
//!   config.password → SSO password (plaintext)
//!   config.database → instance domain (e.g. "pe-xxx.rwlb.ap-southeast-5.rds.aliyuncs.com")
//!   config.port     → source_type    (default 4 = MySQL/PolarDB)

use super::*;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Deserialize as SerdeDeserialize;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

// ── DES-CBC-PKCS7 encryption (for SSO auth) ────────────────────────

use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use des::Des;

type DesCbcEnc = cbc::Encryptor<Des>;

const DES_KEY: &[u8; 8] = b"ak01$#AK";
const DES_IV: &[u8; 8] = b"ak01$#AK";

fn des_encrypt_hex(plaintext: &str) -> String {
    let enc = DesCbcEnc::new(DES_KEY.into(), DES_IV.into());
    let ct = enc.encrypt_padded_vec_mut::<Pkcs7>(plaintext.as_bytes());
    ct.iter().map(|b| format!("{:02x}", b)).collect()
}

// ── SSO login helpers ──────────────────────────────────────────────

/// Derive SSO URL from Kiwi base URL: https://kiwi.x.com → https://sso.x.com
fn derive_sso_url(base_url: &str) -> Result<String, DriverError> {
    let (scheme, host_path) = if let Some(rest) = base_url.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = base_url.strip_prefix("http://") {
        ("http", rest)
    } else {
        return Err(DriverError::InvalidConfig(
            "URL must start with http(s)://".into(),
        ));
    };
    let host = host_path.split('/').next().unwrap_or(host_path);
    let parts: Vec<&str> = host.splitn(2, '.').collect();
    if parts.len() < 2 {
        return Err(DriverError::InvalidConfig(
            "Cannot derive SSO URL from base URL".into(),
        ));
    }
    Ok(format!("{}://sso.{}", scheme, parts[1]))
}

/// Base64-URL decode (JWT payload uses URL-safe base64 without padding).
fn base64_url_decode(input: &str) -> Result<Vec<u8>, DriverError> {
    let padded = match input.len() % 4 {
        2 => format!("{}==", input),
        3 => format!("{}=", input),
        _ => input.to_string(),
    };
    let standard = padded.replace('-', "+").replace('_', "/");
    BASE64
        .decode(&standard)
        .map_err(|e| DriverError::AuthenticationFailed(format!("Base64 decode: {e}")))
}

/// Extract `ticket` query parameter from the redirect URL returned by SSO auth.
fn extract_ticket_from_url(redirect_url: &str) -> Result<String, DriverError> {
    if let Some(query) = redirect_url.split('?').nth(1) {
        for pair in query.split('&') {
            if let Some(value) = pair.strip_prefix("ticket=") {
                return urlencoding::decode(value)
                    .map(|v| v.to_string())
                    .map_err(|e| {
                        DriverError::AuthenticationFailed(format!("URL decode ticket: {e}"))
                    });
            }
        }
    }
    Err(DriverError::AuthenticationFailed(
        "No ticket in redirect URL".into(),
    ))
}

/// Decode the JWT ticket and extract the `sid` field for the cookie.
fn extract_sid_from_jwt(jwt: &str) -> Result<String, DriverError> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() < 2 {
        return Err(DriverError::AuthenticationFailed(
            "Invalid JWT format".into(),
        ));
    }
    let payload = base64_url_decode(parts[1])?;
    let json: serde_json::Value = serde_json::from_slice(&payload)
        .map_err(|e| DriverError::AuthenticationFailed(format!("JWT payload parse: {e}")))?;
    json.get("sid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| DriverError::AuthenticationFailed("No sid in JWT payload".into()))
}

/// Full SSO login: DES-encrypt credentials → POST sso_auth → extract ticket →
/// GET validate_ticket → return Admin-Token.
pub async fn sso_login(
    client: &reqwest::Client,
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<String, DriverError> {
    let enc_user = des_encrypt_hex(username);
    let enc_pass = des_encrypt_hex(password);
    let sso_url = derive_sso_url(base_url)?;

    tracing::info!("[kiwi] SSO login: sso_url={sso_url}, user={username}");

    // Step 1: SSO auth → get ticket
    #[derive(SerdeDeserialize)]
    struct SsoAuthResp {
        redirect_url: Option<String>,
        err_code: i32,
        #[serde(default)]
        info: String,
    }

    let auth_body = serde_json::json!({
        "username": enc_user,
        "password": enc_pass,
        "service": "",
        "platform": "kiwi",
        "renew": "",
        "extra": ""
    });

    let resp = client
        .post(&format!("{}/api/sso_auth/", sso_url))
        .header("Content-Type", "application/json;charset=UTF-8")
        .header("Accept", "application/json, text/plain, */*")
        .json(&auth_body)
        .send()
        .await
        .map_err(|e| DriverError::AuthenticationFailed(format!("SSO auth request: {e}")))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| DriverError::AuthenticationFailed(format!("SSO auth body: {e}")))?;
    tracing::info!("[kiwi] SSO auth → {status}, len={}", body.len());

    let auth: SsoAuthResp = serde_json::from_str(&body)
        .map_err(|e| DriverError::AuthenticationFailed(format!("SSO auth parse: {e}")))?;

    if auth.err_code != 0 {
        return Err(DriverError::AuthenticationFailed(format!(
            "SSO auth failed (code={}): {}",
            auth.err_code, auth.info
        )));
    }

    let redirect_url = auth.redirect_url.ok_or_else(|| {
        DriverError::AuthenticationFailed("No redirect_url in SSO response".into())
    })?;

    // Step 2: extract ticket from redirect_url
    let ticket = extract_ticket_from_url(&redirect_url)?;

    // Step 3: extract sid from the ticket JWT (needed as cookie for validate_ticket)
    let sid = extract_sid_from_jwt(&ticket)?;

    // Step 4: validate ticket → get Admin-Token
    #[derive(SerdeDeserialize)]
    struct VtResults {
        token: String,
    }
    #[derive(SerdeDeserialize)]
    struct VtResp {
        code: i32,
        #[serde(default)]
        message: String,
        results: Option<VtResults>,
    }

    let service_b64 = BASE64.encode(format!("{}/#/sys-pannel", sso_url).as_bytes());
    let validate_url = format!(
        "{}/gw/v1/auth/validate_ticket?ticket={}&service={}&extra=&platform=kiwi",
        base_url,
        urlencoding::encode(&ticket),
        urlencoding::encode(&service_b64)
    );

    tracing::info!("[kiwi] validate_ticket GET {validate_url}");

    let resp = client
        .get(&validate_url)
        .header("Cookie", format!("sid={}", sid))
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/json")
        .header("lang", "en")
        .send()
        .await
        .map_err(|e| DriverError::AuthenticationFailed(format!("validate_ticket: {e}")))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| DriverError::AuthenticationFailed(format!("validate_ticket body: {e}")))?;
    tracing::info!("[kiwi] validate_ticket → {status}, len={}", body.len());

    let vt: VtResp = serde_json::from_str(&body)
        .map_err(|e| DriverError::AuthenticationFailed(format!("validate_ticket parse: {e}")))?;

    if vt.code != 0 {
        return Err(DriverError::AuthenticationFailed(format!(
            "validate_ticket error (code={}): {}",
            vt.code, vt.message
        )));
    }

    let token = vt
        .results
        .map(|r| r.token)
        .ok_or_else(|| {
            DriverError::AuthenticationFailed("No token in validate_ticket response".into())
        })?;

    tracing::info!(
        "[kiwi] SSO login success, token_len={}",
        token.len()
    );
    Ok(token)
}

// ── Kiwi API response types ────────────────────────────────────────

#[derive(Debug, SerdeDeserialize)]
struct KiwiResp<T> {
    code: i32,
    #[serde(default)]
    msg: String,
    result: Option<T>,
}

#[derive(Debug, SerdeDeserialize)]
struct KiwiDatabase {
    name: String,
    #[allow(dead_code)]
    #[serde(default)]
    is_expire: bool,
}

#[derive(Debug, SerdeDeserialize)]
struct KiwiBatchResult {
    #[serde(default)]
    result_id: String,
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    headers: Vec<String>,
    #[allow(dead_code)]
    #[serde(default)]
    time: f64,
    #[allow(dead_code)]
    #[serde(default)]
    command: String,
    #[serde(default)]
    error: String,
}

fn null_as_empty_vec<'de, D>(d: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Vec<String>>::deserialize(d).map(|o| o.unwrap_or_default())
}

#[derive(Debug, SerdeDeserialize)]
struct KiwiQueryResult {
    #[serde(default)]
    result: Vec<String>,
    #[allow(dead_code)]
    #[serde(default)]
    total: i64,
    #[serde(default)]
    headers: Vec<String>,
}

// ── Session state stored per connection ─────────────────────────────

struct KiwiSession {
    client: reqwest::Client,
    base_url: String,
    token: std::sync::Mutex<String>,
    // Plaintext credentials for auto-refresh
    sso_username: String,
    sso_password: String,
    username: String,
    domain: String,
    source_type: u32,
    current_database: std::sync::Mutex<String>,
    cached_databases: std::sync::Mutex<Option<Vec<String>>>,
    cached_tables: std::sync::Mutex<HashMap<String, Vec<String>>>,
    cached_columns: std::sync::Mutex<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>>,
}

impl KiwiSession {
    fn auth_headers(&self) -> HeaderMap {
        let token = self.token.lock().unwrap().clone();
        let mut h = HeaderMap::new();
        if let Ok(v) = HeaderValue::from_str(&token) {
            h.insert("X-Token", v.clone());
            h.insert("authorization", v);
        }
        if !self.username.is_empty() {
            if let Ok(v) = HeaderValue::from_str(&self.username) {
                h.insert("user_name", v);
            }
        }
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h.insert(
            "Accept",
            HeaderValue::from_static("application/json, text/plain, */*"),
        );
        h.insert("lang", HeaderValue::from_static("en"));
        h
    }

    fn active_db(&self) -> String {
        self.current_database.lock().unwrap().clone()
    }

    fn set_active_db(&self, db: &str) {
        *self.current_database.lock().unwrap() = db.to_string();
    }

    /// Re-login via SSO and replace the token. Clears all metadata caches.
    async fn refresh_token(&self) -> Result<(), DriverError> {
        tracing::info!("[kiwi] Refreshing token for user={}", self.sso_username);
        let new_token =
            sso_login(&self.client, &self.base_url, &self.sso_username, &self.sso_password).await?;
        *self.token.lock().unwrap() = new_token;
        *self.cached_databases.lock().unwrap() = None;
        self.cached_tables.lock().unwrap().clear();
        self.cached_columns.lock().unwrap().clear();
        tracing::info!("[kiwi] Token refreshed successfully");
        Ok(())
    }

    /// HTTP GET with automatic 401 → refresh → retry.
    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, DriverError> {
        let url = format!("{}{}", self.base_url, path);
        tracing::info!("[kiwi] GET {url}");

        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .map_err(|e| DriverError::ConnectionFailed(format!("HTTP GET failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            tracing::info!("[kiwi] GET {path} → 401, refreshing token…");
            self.refresh_token().await?;
            let resp = self
                .client
                .get(&url)
                .headers(self.auth_headers())
                .send()
                .await
                .map_err(|e| DriverError::ConnectionFailed(format!("HTTP GET retry: {e}")))?;
            let status = resp.status();
            let body = resp
                .text()
                .await
                .map_err(|e| DriverError::QueryFailed(format!("read body: {e}")))?;
            tracing::info!("[kiwi] GET {path} retry → {status}, len={}", body.len());
            if !status.is_success() {
                return Err(DriverError::QueryFailed(format!("HTTP {status}: {body}")));
            }
            return serde_json::from_str::<T>(&body).map_err(|e| {
                DriverError::QueryFailed(format!(
                    "parse JSON: {e} — body: {}",
                    &body[..body.len().min(200)]
                ))
            });
        }

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| DriverError::QueryFailed(format!("read body: {e}")))?;
        tracing::info!("[kiwi] GET {path} → {status}, len={}", body.len());
        if !status.is_success() {
            tracing::error!("[kiwi] GET {path} failed: {body}");
            return Err(DriverError::QueryFailed(format!("HTTP {status}: {body}")));
        }
        serde_json::from_str::<T>(&body).map_err(|e| {
            DriverError::QueryFailed(format!(
                "parse JSON: {e} — body: {}",
                &body[..body.len().min(200)]
            ))
        })
    }

    /// HTTP POST (JSON body) with automatic 401 → refresh → retry.
    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, DriverError> {
        let url = format!("{}{}", self.base_url, path);
        tracing::info!(
            "[kiwi] POST {url} body={}",
            serde_json::to_string(body).unwrap_or_default()
        );

        let resp = self
            .client
            .post(&url)
            .headers(self.auth_headers())
            .json(body)
            .send()
            .await
            .map_err(|e| DriverError::ConnectionFailed(format!("HTTP POST failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            tracing::info!("[kiwi] POST {path} → 401, refreshing token…");
            self.refresh_token().await?;
            let resp = self
                .client
                .post(&url)
                .headers(self.auth_headers())
                .json(body)
                .send()
                .await
                .map_err(|e| DriverError::ConnectionFailed(format!("HTTP POST retry: {e}")))?;
            let status = resp.status();
            let body_text = resp
                .text()
                .await
                .map_err(|e| DriverError::QueryFailed(format!("read body: {e}")))?;
            tracing::info!("[kiwi] POST {path} retry → {status}, len={}", body_text.len());
            if !status.is_success() {
                return Err(DriverError::QueryFailed(format!(
                    "HTTP {status}: {body_text}"
                )));
            }
            return serde_json::from_str::<T>(&body_text).map_err(|e| {
                DriverError::QueryFailed(format!(
                    "parse JSON: {e} — body: {}",
                    &body_text[..body_text.len().min(200)]
                ))
            });
        }

        let status = resp.status();
        let body_text = resp
            .text()
            .await
            .map_err(|e| DriverError::QueryFailed(format!("read body: {e}")))?;
        tracing::info!("[kiwi] POST {path} → {status}, len={}", body_text.len());
        if !status.is_success() {
            tracing::error!("[kiwi] POST {path} failed: {body_text}");
            return Err(DriverError::QueryFailed(format!(
                "HTTP {status}: {body_text}"
            )));
        }
        serde_json::from_str::<T>(&body_text).map_err(|e| {
            tracing::error!(
                "[kiwi] POST {path} parse error: {e}, body: {}",
                &body_text[..body_text.len().min(300)]
            );
            DriverError::QueryFailed(format!(
                "parse JSON: {e} — body: {}",
                &body_text[..body_text.len().min(200)]
            ))
        })
    }

    /// Execute SQL through the two-step async API and return (headers, rows, time_ms).
    async fn exec_sql(
        &self,
        sql: &str,
        database: &str,
    ) -> Result<(Vec<String>, Vec<Vec<String>>, u64), DriverError> {
        let start = Instant::now();

        let batch_body = serde_json::json!({
            "command": sql,
            "hash_key": "",
            "domain": self.domain,
            "database": database,
            "source_type": self.source_type,
        });

        let batch_resp: KiwiResp<Vec<KiwiBatchResult>> = self
            .post_json("/gw/v1/dataquery/query/batch", &batch_body)
            .await?;

        if batch_resp.code != 0 {
            return Err(DriverError::QueryFailed(format!(
                "batch error: {}",
                batch_resp.msg
            )));
        }

        let items = batch_resp.result.unwrap_or_default();
        let item = items
            .first()
            .ok_or_else(|| DriverError::QueryFailed("empty batch result".into()))?;

        if !item.error.is_empty() {
            return Err(DriverError::QueryFailed(item.error.clone()));
        }

        let result_url = format!(
            "/gw/v1/dataquery/query/result?result_id={}&page=1&page_size=1000",
            item.result_id
        );
        let qr: KiwiQueryResult = self.get(&result_url).await?;

        let headers = if qr.headers.is_empty() {
            item.headers.clone()
        } else {
            qr.headers
        };

        let rows: Vec<Vec<String>> = qr
            .result
            .iter()
            .filter_map(|row_str| serde_json::from_str::<Vec<String>>(row_str).ok())
            .collect();

        let elapsed = start.elapsed().as_millis() as u64;
        Ok((headers, rows, elapsed))
    }
}

// ── Driver ─────────────────────────────────────────────────────────

pub struct KiwiDriver {
    sessions: RwLock<HashMap<String, KiwiSession>>,
}

impl KiwiDriver {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    async fn resolve_active_db(&self, handle: &ConnectionHandle) -> Result<String, DriverError> {
        {
            let sessions = self.sessions.read().await;
            let s = Self::get_session(&sessions, handle)?;
            let active = s.active_db();
            if !active.is_empty() {
                return Ok(active);
            }
        }
        let dbs = self.get_databases(handle).await?;
        let first = dbs.first().cloned().unwrap_or_default();
        {
            let sessions = self.sessions.read().await;
            let s = Self::get_session(&sessions, handle)?;
            s.set_active_db(&first);
        }
        Ok(first)
    }

    fn get_session<'a>(
        sessions: &'a HashMap<String, KiwiSession>,
        handle: &ConnectionHandle,
    ) -> Result<&'a KiwiSession, DriverError> {
        sessions
            .get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Kiwi session not found".into()))
    }

    async fn build_session(config: &ConnectionConfig) -> Result<KiwiSession, DriverError> {
        let base_url = config
            .host
            .as_deref()
            .ok_or_else(|| DriverError::InvalidConfig("host (Kiwi URL) is required".into()))?
            .trim_end_matches('/')
            .to_string();

        let sso_username = config
            .username
            .clone()
            .ok_or_else(|| DriverError::InvalidConfig("username is required for Kiwi".into()))?;

        let sso_password = config
            .password
            .as_deref()
            .ok_or_else(|| DriverError::InvalidConfig("password is required for Kiwi".into()))?
            .to_string();

        let domain = config
            .database
            .as_deref()
            .ok_or_else(|| {
                DriverError::InvalidConfig("database (instance domain) is required".into())
            })?
            .to_string();

        let source_type = config.port.unwrap_or(4) as u32;

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| DriverError::ConnectionFailed(format!("build HTTP client: {e}")))?;

        let token =
            sso_login(&client, &base_url, &sso_username, &sso_password).await?;

        Ok(KiwiSession {
            client,
            base_url,
            token: std::sync::Mutex::new(token),
            username: sso_username.clone(),
            sso_username,
            sso_password,
            domain,
            source_type,
            current_database: std::sync::Mutex::new(String::new()),
            cached_databases: std::sync::Mutex::new(None),
            cached_tables: std::sync::Mutex::new(HashMap::new()),
            cached_columns: std::sync::Mutex::new(HashMap::new()),
        })
    }
}

#[async_trait]
impl DatabaseDriver for KiwiDriver {
    fn driver_type(&self) -> DatabaseType {
        DatabaseType::Kiwi
    }

    fn driver_category(&self) -> DriverCategory {
        DriverCategory::Sql
    }

    fn quote_char(&self) -> char {
        '`'
    }

    fn skip_count_query(&self) -> bool {
        true
    }

    fn format_sql_literal(&self, value: &Option<super::Value>) -> String {
        match value {
            None | Some(super::Value::Null) => "NULL".to_string(),
            Some(super::Value::Bool(b)) => {
                if *b {
                    "1".to_string()
                } else {
                    "0".to_string()
                }
            }
            Some(super::Value::Integer(i)) => i.to_string(),
            Some(super::Value::Float(f)) => f.to_string(),
            Some(super::Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(super::Value::Bytes(b)) => {
                format!("'{}'", String::from_utf8_lossy(b).replace('\'', "''"))
            }
            Some(super::Value::Timestamp(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(super::Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
        }
    }

    // ── Connection lifecycle ───────────────────────────────────────

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let session = Self::build_session(config).await?;

        let pool_id = format!("kiwi_{}", uuid::Uuid::new_v4());
        let handle = ConnectionHandle {
            id: pool_id.clone(),
            pool_id: pool_id.clone(),
        };

        self.sessions.write().await.insert(pool_id, session);
        Ok(handle)
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let session = Self::build_session(config).await?;
        Ok(ServerInfo {
            server_version: format!("Kiwi (user: {})", session.username),
            server_type: "Kiwi".into(),
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.sessions.write().await.remove(&handle.pool_id);
        Ok(())
    }

    // ── Metadata ───────────────────────────────────────────────────

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let sessions = self.sessions.read().await;
        let s = Self::get_session(&sessions, handle)?;

        if let Some(cached) = s.cached_databases.lock().unwrap().as_ref() {
            tracing::info!(
                "[kiwi] get_databases: returning {} cached databases",
                cached.len()
            );
            return Ok(cached.clone());
        }

        let url = format!(
            "/gw/v1/dataquery/databases?source_type={}&domain={}",
            s.source_type, s.domain
        );
        let resp: KiwiResp<Vec<KiwiDatabase>> = s.get(&url).await?;
        if resp.code != 0 {
            return Err(DriverError::QueryFailed(resp.msg));
        }

        let dbs: Vec<String> = resp
            .result
            .unwrap_or_default()
            .into_iter()
            .map(|d| d.name)
            .filter(|n| !n.is_empty())
            .collect();

        *s.cached_databases.lock().unwrap() = Some(dbs.clone());
        Ok(dbs)
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let sessions = self.sessions.read().await;
        let s = Self::get_session(&sessions, handle)?;

        s.set_active_db(database);

        if let Some(cached) = s.cached_tables.lock().unwrap().get(database) {
            tracing::info!(
                "[kiwi] get_tables: returning {} cached tables for {database}",
                cached.len()
            );
            return Ok(cached
                .iter()
                .map(|name| TableInfo {
                    name: name.clone(),
                    schema: None,
                    table_type: TableType::Table,
                    row_count: None,
                })
                .collect());
        }

        tracing::info!(
            "[kiwi] get_tables: database={database}, domain={}",
            s.domain
        );

        let url = format!(
            "/gw/v1/dataquery/tables?source_type={}&domain={}&database={}",
            s.source_type, s.domain, database
        );
        let resp: KiwiResp<Vec<String>> = s.get(&url).await?;
        if resp.code != 0 {
            return Err(DriverError::QueryFailed(resp.msg));
        }

        let table_names: Vec<String> = resp.result.unwrap_or_default();
        s.cached_tables
            .lock()
            .unwrap()
            .insert(database.to_string(), table_names.clone());

        Ok(table_names
            .into_iter()
            .map(|name| TableInfo {
                name,
                schema: None,
                table_type: TableType::Table,
                row_count: None,
            })
            .collect())
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let (columns, primary_keys) = self.get_columns(handle, table).await?;
        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys,
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
        })
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        let db = self.resolve_active_db(handle).await?;
        let cache_key = format!("{}.{}", db, table);

        let cached = {
            let sessions = self.sessions.read().await;
            let s = Self::get_session(&sessions, handle)?;
            let guard = s.cached_columns.lock().unwrap();
            let val = guard.get(&cache_key).cloned();
            drop(guard);
            val
        };
        if let Some(c) = cached {
            tracing::info!("[kiwi] get_columns: cache hit for {cache_key}");
            return Ok(c);
        }

        tracing::info!("[kiwi] get_columns: table={table}, database={db} via SHOW COLUMNS");
        let sessions = self.sessions.read().await;
        let s = Self::get_session(&sessions, handle)?;

        let sql = format!("SHOW COLUMNS FROM `{}`", table.replace('`', "``"));
        let (headers, rows, _) = s.exec_sql(&sql, &db).await?;

        let field_idx = headers.iter().position(|h| h == "Field").unwrap_or(0);
        let type_idx = headers.iter().position(|h| h == "Type").unwrap_or(1);
        let null_idx = headers.iter().position(|h| h == "Null").unwrap_or(2);
        let key_idx = headers.iter().position(|h| h == "Key").unwrap_or(3);
        let default_idx = headers.iter().position(|h| h == "Default").unwrap_or(4);
        let extra_idx = headers.iter().position(|h| h == "Extra").unwrap_or(5);

        let mut columns = Vec::new();
        let mut pks = Vec::new();

        for row in &rows {
            let get = |i: usize| row.get(i).cloned().unwrap_or_default();
            let name = get(field_idx);
            let data_type = get(type_idx);
            let nullable = get(null_idx) == "YES";
            let key = get(key_idx);
            let default_val = {
                let v = get(default_idx);
                if v.is_empty() || v == "NULL" {
                    None
                } else {
                    Some(v)
                }
            };
            let extra = get(extra_idx);
            let is_pk = key == "PRI";
            let is_auto = extra.contains("auto_increment");

            if is_pk {
                pks.push(name.clone());
            }

            columns.push(ColumnSchema {
                name,
                data_type,
                nullable,
                default_value: default_val,
                comment: None,
                is_primary_key: is_pk,
                is_auto_increment: is_auto,
            });
        }

        let result = (columns.clone(), pks.clone());
        s.cached_columns.lock().unwrap().insert(cache_key, result);

        Ok((columns, pks))
    }

    // ── Query execution ────────────────────────────────────────────

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        let db = self.resolve_active_db(handle).await?;

        let clean_sql = strip_limit_offset(sql);
        tracing::info!("[kiwi] query: database={db}, sql={clean_sql}");

        let sessions = self.sessions.read().await;
        let s = Self::get_session(&sessions, handle)?;
        let (headers, rows, elapsed) = s.exec_sql(&clean_sql, &db).await?;

        let columns: Vec<ColumnInfo> = headers
            .iter()
            .map(|h| ColumnInfo {
                name: h.clone(),
                data_type: "VARCHAR".into(),
                nullable: true,
            })
            .collect();

        let result_rows: Vec<Vec<Option<Value>>> = rows
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|cell| {
                        if cell == "NULL" || cell.is_empty() {
                            None
                        } else {
                            Some(Value::String(cell))
                        }
                    })
                    .collect()
            })
            .collect();

        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: None,
            execution_time_ms: elapsed,
        })
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let start = Instant::now();

        let statements: Vec<&str> = sql
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        let mut results = Vec::new();

        for stmt in &statements {
            match self.query(handle, stmt).await {
                Ok(qr) => results.push(StatementResult {
                    sql: stmt.to_string(),
                    columns: qr.columns,
                    rows: qr.rows,
                    rows_affected: qr.rows_affected,
                    execution_time_ms: qr.execution_time_ms,
                    truncated: false,
                }),
                Err(e) => results.push(StatementResult {
                    sql: stmt.to_string(),
                    columns: vec![ColumnInfo {
                        name: "error".into(),
                        data_type: "TEXT".into(),
                        nullable: true,
                    }],
                    rows: vec![vec![Some(Value::String(e.to_string()))]],
                    rows_affected: None,
                    execution_time_ms: 0,
                    truncated: false,
                }),
            }
        }

        Ok(MultiQueryResult {
            results,
            total_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.query(handle, sql).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        let qr = self.query(handle, sql).await?;
        Ok(qr.rows_affected.unwrap_or(0))
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Err(DriverError::QueryFailed(
            "Cancel is not supported for Kiwi driver".into(),
        ))
    }
}

/// Remove trailing LIMIT/OFFSET clause from SQL (Kiwi API handles pagination via page_size).
fn strip_limit_offset(sql: &str) -> String {
    let upper = sql.to_ascii_uppercase();
    if let Some(limit_pos) = upper.rfind(" LIMIT ") {
        let after_limit = &upper[limit_pos + 7..];
        let trimmed = after_limit.trim();
        let is_limit_clause = trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_whitespace())
            || trimmed.contains("OFFSET");
        if is_limit_clause {
            return sql[..limit_pos].to_string();
        }
    }
    sql.to_string()
}
