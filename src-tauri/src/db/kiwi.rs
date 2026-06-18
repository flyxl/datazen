//! Kiwi driver — proxies SQL queries through the Kiwi HTTP API.
//!
//! Field mapping:
//!   config.host     → Kiwi base URL  (e.g. "https://kiwi.akusre.com")
//!   config.password → Admin-Token JWT
//!   config.username → user_name header
//!   config.database → instance domain (e.g. "pe-xxx.rwlb.ap-southeast-5.rds.aliyuncs.com")
//!   config.port     → source_type    (default 4 = MySQL/PolarDB)

use super::*;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Deserialize as SerdeDeserialize;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

// ── Kiwi API response types ────────────────────────────────────────

#[derive(Debug, SerdeDeserialize)]
struct KiwiResp<T> {
    code: i32,
    #[serde(default)]
    msg: String,
    result: Option<T>,
}

#[derive(Debug, SerdeDeserialize)]
struct KiwiUserInfo {
    #[allow(dead_code)]
    token: Option<String>,
    user_info: Option<KiwiUser>,
}

#[derive(Debug, SerdeDeserialize)]
struct KiwiUser {
    #[allow(dead_code)]
    username: Option<String>,
}

#[derive(Debug, SerdeDeserialize)]
struct KiwiInstance {
    name: String,
    #[serde(default)]
    alias_name: String,
    #[serde(default)]
    short_domain: String,
}

#[derive(Debug, SerdeDeserialize)]
struct KiwiDatabase {
    name: String,
    #[allow(dead_code)]
    #[serde(default)]
    is_expire: bool,
}

#[derive(Debug, SerdeDeserialize)]
struct KiwiTableHeader {
    table_name: String,
    column_name: String,
}

#[derive(Debug, SerdeDeserialize)]
struct KiwiBatchResult {
    #[serde(default)]
    result_id: String,
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    headers: Vec<String>,
    #[serde(default)]
    time: f64,
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
    #[serde(default)]
    total: i64,
    #[serde(default)]
    headers: Vec<String>,
}

#[derive(Debug, SerdeDeserialize)]
struct KiwiInstanceResp {
    #[allow(dead_code)]
    #[serde(default)]
    msg: String,
    code: i32,
    #[serde(default)]
    result: Vec<KiwiInstance>,
}

// ── Session state stored per connection ─────────────────────────────

struct KiwiSession {
    client: reqwest::Client,
    base_url: String,
    token: String,
    username: String,
    domain: String,
    source_type: u32,
    current_database: std::sync::Mutex<String>,
    // Caches to avoid redundant HTTP calls
    cached_databases: std::sync::Mutex<Option<Vec<String>>>,
    cached_tables: std::sync::Mutex<HashMap<String, Vec<String>>>,
    cached_columns: std::sync::Mutex<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>>,
}

impl KiwiSession {
    fn auth_headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Ok(v) = HeaderValue::from_str(&self.token) {
            h.insert("X-Token", v.clone());
            h.insert("authorization", v);
        }
        if !self.username.is_empty() {
            if let Ok(v) = HeaderValue::from_str(&self.username) {
                h.insert("user_name", v);
            }
        }
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h.insert("Accept", HeaderValue::from_static("application/json, text/plain, */*"));
        h.insert("lang", HeaderValue::from_static("en"));
        h
    }

    fn active_db(&self) -> String {
        self.current_database.lock().unwrap().clone()
    }

    fn set_active_db(&self, db: &str) {
        *self.current_database.lock().unwrap() = db.to_string();
    }

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
        serde_json::from_str::<T>(&body)
            .map_err(|e| DriverError::QueryFailed(format!("parse JSON: {e} — body: {}", &body[..body.len().min(200)])))
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, DriverError> {
        let url = format!("{}{}", self.base_url, path);
        tracing::info!("[kiwi] POST {url} body={}", serde_json::to_string(body).unwrap_or_default());
        let resp = self
            .client
            .post(&url)
            .headers(self.auth_headers())
            .json(body)
            .send()
            .await
            .map_err(|e| DriverError::ConnectionFailed(format!("HTTP POST failed: {e}")))?;

        let status = resp.status();
        let body_text = resp
            .text()
            .await
            .map_err(|e| DriverError::QueryFailed(format!("read body: {e}")))?;

        tracing::info!("[kiwi] POST {path} → {status}, len={}", body_text.len());
        if !status.is_success() {
            tracing::error!("[kiwi] POST {path} failed: {body_text}");
            return Err(DriverError::QueryFailed(format!("HTTP {status}: {body_text}")));
        }
        serde_json::from_str::<T>(&body_text)
            .map_err(|e| {
                tracing::error!("[kiwi] POST {path} parse error: {e}, body: {}", &body_text[..body_text.len().min(300)]);
                DriverError::QueryFailed(format!("parse JSON: {e} — body: {}", &body_text[..body_text.len().min(200)]))
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

        let batch_resp: KiwiResp<Vec<KiwiBatchResult>> =
            self.post_json("/gw/v1/dataquery/query/batch", &batch_body).await?;

        if batch_resp.code != 0 {
            return Err(DriverError::QueryFailed(format!("batch error: {}", batch_resp.msg)));
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

    /// Get the active database for a session. Falls back to the first available database.
    async fn resolve_active_db(&self, handle: &ConnectionHandle) -> Result<String, DriverError> {
        {
            let sessions = self.sessions.read().await;
            let s = Self::get_session(&sessions, handle)?;
            let active = s.active_db();
            if !active.is_empty() {
                return Ok(active);
            }
        }
        // No active db set — fall back to first
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

    fn build_session(config: &ConnectionConfig) -> Result<KiwiSession, DriverError> {
        let base_url = config
            .host
            .as_deref()
            .ok_or_else(|| DriverError::InvalidConfig("host (Kiwi URL) is required".into()))?
            .trim_end_matches('/')
            .to_string();

        let token = config
            .password
            .as_deref()
            .ok_or_else(|| DriverError::InvalidConfig("password (Admin-Token) is required".into()))?
            .to_string();

        let username = config.username.clone().unwrap_or_else(|| "unknown".into());
        let domain = config
            .database
            .as_deref()
            .ok_or_else(|| DriverError::InvalidConfig("database (instance domain) is required".into()))?
            .to_string();

        let source_type = config.port.unwrap_or(4) as u32;

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| DriverError::ConnectionFailed(format!("build HTTP client: {e}")))?;

        Ok(KiwiSession {
            client,
            base_url,
            token,
            username,
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

    // ── Connection lifecycle ───────────────────────────────────────

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let session = Self::build_session(config)?;

        // Validate token
        let url = format!(
            "/gw/v1/user/get_user_info_by_token?token={}",
            &session.token
        );
        let resp: KiwiResp<KiwiUserInfo> = session.get(&url).await?;
        if resp.code != 0 {
            return Err(DriverError::AuthenticationFailed(format!(
                "Token validation failed: {}",
                resp.msg
            )));
        }

        let pool_id = format!("kiwi_{}", uuid::Uuid::new_v4());
        let handle = ConnectionHandle {
            id: pool_id.clone(),
            pool_id: pool_id.clone(),
        };

        self.sessions.write().await.insert(pool_id, session);
        Ok(handle)
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let session = Self::build_session(config)?;
        let url = format!(
            "/gw/v1/user/get_user_info_by_token?token={}",
            &session.token
        );
        let resp: KiwiResp<KiwiUserInfo> = session.get(&url).await?;
        if resp.code != 0 {
            return Err(DriverError::AuthenticationFailed(resp.msg));
        }

        let username = resp
            .result
            .and_then(|r| r.user_info)
            .and_then(|u| u.username)
            .unwrap_or_default();

        Ok(ServerInfo {
            server_version: format!("Kiwi (user: {username})"),
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

        // Return cached result if available
        if let Some(cached) = s.cached_databases.lock().unwrap().as_ref() {
            tracing::info!("[kiwi] get_databases: returning {} cached databases", cached.len());
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

        // Return cached if available
        if let Some(cached) = s.cached_tables.lock().unwrap().get(database) {
            tracing::info!("[kiwi] get_tables: returning {} cached tables for {database}", cached.len());
            return Ok(cached.iter().map(|name| TableInfo {
                name: name.clone(),
                schema: None,
                table_type: TableType::Table,
                row_count: None,
            }).collect());
        }

        tracing::info!("[kiwi] get_tables: database={database}, domain={}", s.domain);

        let url = format!(
            "/gw/v1/dataquery/tables?source_type={}&domain={}&database={}",
            s.source_type, s.domain, database
        );
        let resp: KiwiResp<Vec<String>> = s.get(&url).await?;
        if resp.code != 0 {
            return Err(DriverError::QueryFailed(resp.msg));
        }

        let table_names: Vec<String> = resp.result.unwrap_or_default();
        s.cached_tables.lock().unwrap().insert(database.to_string(), table_names.clone());

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

        // Check cache first
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

        // SHOW COLUMNS returns: Field, Type, Null, Key, Default, Extra
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
                if v.is_empty() || v == "NULL" { None } else { Some(v) }
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

        // Cache the result
        let result = (columns.clone(), pks.clone());
        s.cached_columns.lock().unwrap().insert(cache_key, result);

        Ok((columns, pks))
    }

    // ── Query execution ────────────────────────────────────────────

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        let db = self.resolve_active_db(handle).await?;

        // Strip LIMIT/OFFSET — Kiwi's query/result API handles pagination via page_size
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

        // Split on semicolons (simple split — Kiwi handles one statement per batch call)
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
        // Kiwi API doesn't support parameterized queries — execute as-is
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
    // Find the last occurrence of " LIMIT "
    if let Some(limit_pos) = upper.rfind(" LIMIT ") {
        let after_limit = &upper[limit_pos + 7..];
        // Verify it looks like "N OFFSET N" or just "N"
        let trimmed = after_limit.trim();
        let is_limit_clause = trimmed.chars().all(|c| c.is_ascii_digit() || c.is_ascii_whitespace()
            || trimmed.contains("OFFSET"));
        if is_limit_clause {
            return sql[..limit_pos].to_string();
        }
    }
    sql.to_string()
}
