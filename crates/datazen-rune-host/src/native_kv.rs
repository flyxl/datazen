//! Native KV jobs dispatch only fixed read and guarded edit operations on the live handle.
//! Redis metadata/value replies must not pass through the tabular result sink.

use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine};
use datazen_driver_api::{ConnectionConfig, ConnectionHandle, DatabaseDriver};
use datazen_ui_api::native_kv::{
    KvChange, KvCollectionEdit, KvDetail, KvPage, KvRead, KvReadKind, KvReply, KV_EDIT_MAX_BYTES,
    KV_TTL_MAX_SECONDS,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::LiveSession;

#[path = "native_kv_collections.rs"]
mod collections;
#[path = "native_kv_tools.rs"]
mod tools;

#[derive(Default)]
pub(super) struct KvSessionState {
    pub db: u32,
    pub databases: Vec<u32>,
    pub cluster: bool,
    pub monitor: Option<String>,
    pub subscription: Option<String>,
    search: Option<tools::Search>,
}

impl KvSessionState {
    pub fn new(db: u32, databases: Vec<u32>, cluster: bool) -> Self {
        Self {
            db,
            databases,
            cluster,
            ..Self::default()
        }
    }
}

pub(super) fn configured_route(config: &ConnectionConfig) -> Result<(bool, u32), String> {
    let topology = config.options.as_ref().and_then(|o| o.get("topology"));
    let cluster = match topology {
        None => false,
        Some(tag) => match tag.as_str() {
            Some("standalone" | "sentinel") => false,
            Some("cluster") => true,
            _ => return Err("Unknown Redis connection topology.".into()),
        },
    };
    let name = config.database.as_deref().unwrap_or("").trim();
    let db = if name.is_empty() {
        0
    } else {
        name.strip_prefix("db")
            .unwrap_or(name)
            .parse::<u32>()
            .map_err(|_| "Invalid Redis database index.")?
    };
    if cluster && db != 0 {
        return Err("Redis Cluster uses DB 0 only.".into());
    }
    Ok((cluster, db))
}

pub(super) fn apply_connection_options(
    config: &mut ConnectionConfig,
    settings: &datazen_ui_api::native_kv::RedisConnectionOptions,
) -> Result<(), String> {
    use datazen_ui_api::native_kv::RedisTopology;
    if config.database_type != "redis" {
        return Err("Redis options require a Redis connection.".into());
    }
    let mut options = serde_json::Map::new();
    options.insert("topology".into(), json!(settings.topology.label()));
    if settings.topology != RedisTopology::Standalone {
        let nodes = redis_form_nodes(&settings.nodes)?;
        if settings.topology == RedisTopology::Sentinel {
            let master = settings.sentinel_master.trim();
            if master.is_empty() || master.len() > 128 || master.chars().any(char::is_whitespace) {
                return Err("Enter a Sentinel master name without spaces.".into());
            }
            let nodes = if nodes.is_empty() {
                let host = config
                    .host
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .ok_or("Enter the Sentinel address.")?;
                let port = config.port.unwrap_or(26379);
                vec![format!("{host}:{port}")]
            } else {
                nodes
            };
            options.insert("sentinelNodes".into(), json!(nodes));
            options.insert("sentinelMasterName".into(), json!(master));
        } else if !nodes.is_empty() {
            options.insert("clusterNodes".into(), json!(nodes));
        }
    }
    config.read_only = settings.read_only;
    if settings.tls {
        let ca = settings.tls_ca.trim();
        let cert = settings.tls_cert.trim();
        let key = settings.tls_key.trim();
        if cert.is_empty() != key.is_empty() {
            return Err("Provide both the Redis client certificate and its private key.".into());
        }
        let mut tls = serde_json::Map::new();
        tls.insert("enabled".into(), json!(true));
        for (name, path) in [("caPath", ca), ("certPath", cert), ("keyPath", key)] {
            if path.len() > 4096 || path.chars().any(char::is_control) {
                return Err("Enter a valid TLS file path.".into());
            }
            if !path.is_empty() {
                tls.insert(name.into(), json!(path));
            }
        }
        options.insert("tls".into(), json!(tls));
        config.ssl_mode = datazen_driver_api::SslMode::VerifyFull;
    }
    config.options = Some(options);
    configured_route(config)?;
    Ok(())
}

fn redis_form_nodes(input: &str) -> Result<Vec<String>, String> {
    let error =
        || String::from("Enter up to eight node addresses as host:port, separated by commas.");
    let mut nodes = Vec::new();
    for address in input
        .split(',')
        .map(str::trim)
        .filter(|address| !address.is_empty())
    {
        let (host, port) = address.rsplit_once(':').ok_or_else(error)?;
        let domain = !host.is_empty()
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
        let ipv6 = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .is_some_and(|h| h.parse::<std::net::Ipv6Addr>().is_ok());
        if (!domain && !ipv6)
            || host.len() > 253
            || port.parse::<u16>().map_or(true, |port| port == 0)
        {
            return Err(error());
        }
        if !nodes.iter().any(|node| node == address) {
            if nodes.len() == 8 {
                return Err(error());
            }
            nodes.push(address.to_owned());
        }
    }
    Ok(nodes)
}

pub(super) async fn release(previous: LiveSession) {
    if previous.kv.is_some() {
        if let (Some(driver), Some(handle)) = (previous.driver, previous.handle) {
            tools::stop_owned(driver.as_ref(), &handle, previous.kv_state).await;
            let _ = driver.disconnect(handle).await;
        }
    }
}

pub(super) async fn close(live: &Arc<Mutex<LiveSession>>, id: &str, generation: u32) {
    let previous = {
        let Ok(mut session) = live.lock() else {
            return;
        };
        if session.connection.as_deref() != Some(id)
            || session.kv.map(|(_, epoch)| epoch) != Some(generation)
        {
            return;
        }
        std::mem::take(&mut *session)
    };
    release(previous).await;
}

pub(super) async fn read(
    live: &Arc<Mutex<LiveSession>>,
    request: &KvRead,
) -> Result<KvReply, String> {
    let (driver, handle, writable, db, databases, cluster) = {
        let session = live
            .lock()
            .map_err(|_| "The live session is unavailable.")?;
        if session.connection.as_deref() != Some(request.connection_id.as_str())
            || session.kv.map(|(_, generation)| generation) != Some(request.session_generation)
        {
            return Err("This Redis session is no longer active.".into());
        }
        if session.read_only && matches!(&request.kind, KvReadKind::Edit { .. }) {
            return Err("This Redis connection is read only.".into());
        }
        let db = request.db_index.unwrap_or(session.kv_state.db);
        if !session.kv_state.databases.contains(&db) {
            return Err("This Redis logical database is unavailable.".into());
        }
        (
            session
                .driver
                .clone()
                .ok_or("The Redis driver is unavailable.")?,
            session
                .handle
                .clone()
                .ok_or("The Redis session is closed.")?,
            !session.read_only,
            db,
            session.kv_state.databases.clone(),
            session.kv_state.cluster,
        )
    };
    match &request.kind {
        KvReadKind::Scan {
            cursor,
            pattern,
            key_type,
            persistent,
        } => {
            if pattern.len() > 512 {
                return Err("Redis key filter is too long.".into());
            }
            let data = call(
                driver.as_ref(),
                &handle,
                "scan_keys",
                json!({ "dbIndex": db, "cursor": cursor, "pattern": pattern, "count": 32, "budget": 32, "keyType": key_type.token(), "noTtlOnly": persistent }),
            )
            .await?;
            let mut page: KvPage =
                serde_json::from_value(data).map_err(|_| "Redis returned an invalid scan page.")?;
            page.writable = writable;
            page.db_index = db;
            page.databases = databases;
            page.cluster = cluster;
            Ok(KvReply::Page(page))
        }
        KvReadKind::Folder {
            prefix,
            cursor,
            key_type,
        } => {
            if prefix.len() > 512 || prefix.chars().any(char::is_control) {
                return Err(
                    "The Redis folder prefix is too long or contains control characters.".into(),
                );
            }
            let page = call(driver.as_ref(), &handle, "list_children", json!({
                "dbIndex":db,"prefix":prefix,"cursor":cursor,"keyType":key_type.token(),"count":32,"budget":128,"sep":":"
            })).await?;
            serde_json::from_value(page)
                .map(KvReply::Folder)
                .map_err(|_| "Redis returned an invalid folder page.".into())
        }
        KvReadKind::Detail { key } => detail(driver.as_ref(), &handle, db, key)
            .await
            .map(KvReply::Detail),
        KvReadKind::Edit { key, change } => edit(driver.as_ref(), &handle, db, key, change).await,
        KvReadKind::Tool(op) => tools::run(
            live,
            driver.as_ref(),
            &handle,
            db,
            writable,
            &request.connection_id,
            op,
        )
        .await
        .map(KvReply::Tool),
    }
}

// The static read allowlist is the access policy for this slice. Driver errors
// may contain user keys/values, so only the operation is reported to the UI.
async fn call(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    command: &'static str,
    args: Value,
) -> Result<Value, String> {
    driver
        .execute_command(handle, command, args)
        .await
        .map(|result| result.data)
        .map_err(|_| format!("Redis {command} failed. Check the connection and read permissions."))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValueFrame {
    key_type: String,
    ttl: i64,
    raw_b64: Option<String>,
    truncated: bool,
}

async fn detail(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    db: u32,
    key: &str,
) -> Result<KvDetail, String> {
    let data = call(
        driver,
        handle,
        "get_key_raw",
        json!({ "dbIndex": db, "key": key }),
    )
    .await?;
    let frame: ValueFrame =
        serde_json::from_value(data).map_err(|_| "Redis returned an invalid key detail.")?;
    let mut string_edit = None;
    let mut hash_fields = Vec::new();
    let mut collection_edits = Vec::new();
    let preview = match frame.key_type.as_str() {
        _ if frame.ttl == -2 => "This key no longer exists.".to_owned(),
        "string" if frame.truncated => "Value exceeds the driver's preview limit.".to_owned(),
        "string" => {
            let data = frame
                .raw_b64
                .as_deref()
                .ok_or("Redis returned no string bytes.")?;
            let bytes = STANDARD
                .decode(data)
                .map_err(|_| "Redis returned invalid string bytes.")?;
            match std::str::from_utf8(&bytes) {
                Ok(value) => {
                    if editable(value) {
                        string_edit = Some(value.to_owned());
                    }
                    clip(value)
                }
                Err(_) => format!(
                    "Binary string ({} bytes); base64: {}",
                    bytes.len(),
                    clip(data)
                ),
            }
        }
        "hash" => {
            collection_edits.push((
                "Add field as field=value".into(),
                KvCollectionEdit::HashAdd,
                String::new(),
            ));
            let data = call(
                driver,
                handle,
                "hash_scan",
                json!({ "dbIndex": db, "key": key, "cursor": 0, "count": 16 }),
            )
            .await?;
            let rows = data["entries"]
                .as_array()
                .ok_or("Redis returned invalid hash entries.")?;
            hash_fields = rows
                .iter()
                .take(16)
                .filter_map(|row| {
                    let field = row["field"].as_str()?;
                    let value = row["value"].as_str()?;
                    (editable(field) && editable(value))
                        .then(|| (field.to_owned(), value.to_owned()))
                })
                .collect();
            for (field, previous) in &hash_fields {
                collection_edits.push((
                    format!("Remove Hash field {} (type REMOVE)", clip(field)),
                    KvCollectionEdit::HashRemove {
                        field: field.clone(),
                        previous: previous.clone(),
                    },
                    String::new(),
                ));
            }
            let mut lines: Vec<String> = rows
                .iter()
                .take(16)
                .map(|entry| {
                    format!(
                        "{}: {}",
                        clip(&entry["field"].to_string()),
                        clip(&entry["value"].to_string())
                    )
                })
                .collect();
            if rows.len() > 16 || data["cursor"].as_u64().is_some_and(|n| n != 0) {
                lines.push("Partial hash preview.".into());
            }
            clip(&lines.join("\n"))
        }
        "list" | "set" | "zset" | "stream" | "ReJSON-RL" => {
            let (data, edits) =
                collections::preview(driver, handle, db, key, &frame.key_type).await?;
            collection_edits = edits;
            data
        }
        _ => "This Redis key type has no value preview.".into(),
    };
    Ok(KvDetail {
        key_type: frame.key_type,
        ttl: frame.ttl,
        preview,
        string_edit,
        hash_fields,
        collection_edits,
    })
}

fn clip(value: &str) -> String {
    const MAX: usize = 2048;
    let mut chars = value.chars();
    let mut preview: String = chars.by_ref().take(MAX).collect();
    if chars.next().is_some() {
        preview.push_str("… [preview truncated]");
    }
    preview
}

fn editable(value: &str) -> bool {
    value.len() <= KV_EDIT_MAX_BYTES && !value.contains(['\r', '\n', '\0'])
}

async fn edit(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    db: u32,
    key: &str,
    change: &KvChange,
) -> Result<KvReply, String> {
    let (command, args) = match change {
        KvChange::String { expected, value } if editable(expected) && editable(value) => (
            "set_string",
            json!({"dbIndex": db, "key": key, "expectedValue": expected, "value": value}),
        ),
        KvChange::Hash {
            field,
            expected,
            value,
        } if editable(field) && editable(expected) && editable(value) => (
            "hash_set",
            json!({"dbIndex": db, "key": key, "field": field, "expectedValue": expected, "value": value}),
        ),
        KvChange::Collection {
            target,
            expected,
            value,
        } if editable(expected) && editable(value) => {
            collections::edit_args(db, key, target, expected, value)?
        }
        KvChange::Ttl {
            expected_type,
            seconds,
        } if *seconds == -1 || (1..=KV_TTL_MAX_SECONDS).contains(seconds) => (
            "set_ttl",
            json!({"dbIndex": db, "key": key, "expectedType": expected_type, "ttlSeconds": seconds}),
        ),
        _ => return Err("The edit exceeds the supported value or TTL limits.".into()),
    };
    let result = call(driver, handle, command, args).await.map_err(|_| {
        "Redis did not confirm the edit. Check the connection, write permissions, and EVAL support; refresh before retrying.".to_owned()
    })?;
    match result["applied"].as_bool() {
        Some(true) => Ok(KvReply::Edited),
        Some(false) if matches!(change, KvChange::Collection { target:
            KvCollectionEdit::HashRemove { .. } | KvCollectionEdit::ListRemoveHead(_)
            | KvCollectionEdit::SetRemove(_) | KvCollectionEdit::SortedSetRemove { .. }, .. }) =>
            Err("The entry changed or is the last one. Reload it, or explicitly delete the entire key.".into()),
        Some(false) => Err(
            "The key or value changed on Redis. Discard the draft and reload before editing again."
                .into(),
        ),
        None => Err("Redis did not confirm the edit. Refresh before retrying.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_ui_api::native_kv::NativeKvDriver;

    #[test]
    fn redis_form_validates_and_deduplicates_advanced_nodes() {
        assert_eq!(
            redis_form_nodes("a:1, [::1]:6379, a:1").unwrap(),
            ["a:1", "[::1]:6379"]
        );
        for address in ["redis://a:1", "a:0", "a:65536", "::1:6379", "user@a:6379"] {
            assert!(redis_form_nodes(address).is_err(), "{address}");
        }
    }

    #[test]
    fn write_is_rejected_before_contacting_a_read_only_or_stale_session() {
        let live = Arc::new(Mutex::new(LiveSession {
            kv: Some((NativeKvDriver::Redis, 3)),
            connection: Some("profile".into()),
            read_only: true,
            ..LiveSession::default()
        }));
        let mut request = KvRead {
            connection_id: "profile".into(),
            session_generation: 3,
            request_id: 1,
            db_index: None,
            kind: KvReadKind::Edit {
                key: "key".into(),
                change: KvChange::Ttl {
                    expected_type: "string".into(),
                    seconds: -1,
                },
            },
        };
        let error = futures_executor::block_on(read(&live, &request)).expect_err("read only");
        assert!(error.contains("read only"));
        request.session_generation = 2;
        let error = futures_executor::block_on(read(&live, &request)).expect_err("stale epoch");
        assert!(error.contains("no longer active"));
    }
}
