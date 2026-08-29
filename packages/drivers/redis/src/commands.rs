//! Redis Driver Command definitions and dispatch.
//!
//! Redis UI, Workflow, generic IPC, and MCP all execute operations through
//! `execute_command`. The Redis Tauri plugin is setup-only (Pub/Sub events).

use datazen_driver_api::{
    execute_command_definition, execute_standard_sql_command, query_command_definition,
    query_stream_command_definition, schema_catalog_command_definitions,
    try_execute_schema_catalog_command, CommandCategory, CommandResult, ConnectionHandle,
    DriverCommandDefinition, DriverCommandMetadata, DriverError,
};
use serde_json::Value as JsonValue;

use crate::ops::ZsetMember;
use crate::ops_io::RestoreKeyEntry;
use crate::RedisDriver;

fn redis_command_metadata(id: &str) -> DriverCommandMetadata {
    let category = match id {
        id if id.starts_with("pubsub_") => CommandCategory::PubSub,
        "xrange" | "xadd" | "xgroup_create" | "xgroup_destroy" | "xinfo_groups" | "xpending"
        | "xack" | "stream_overview" => CommandCategory::Stream,
        "dump_keys" | "restore_keys" => CommandCategory::Io,
        "flush_db" | "flush_all" | "slowlog_reset" => CommandCategory::Admin,
        "scan_keys" | "get_key" | "info" | "memory_sample" | "slowlog_get" | "modules_list"
        | "cluster_nodes" | "count_matching" => CommandCategory::Observe,
        _ => CommandCategory::Mutate,
    };
    let mut metadata = DriverCommandMetadata {
        category,
        ..DriverCommandMetadata::default()
    };
    if matches!(id, "pubsub_subscribe" | "pubsub_unsubscribe") {
        metadata = metadata.hide_from_workflow();
    }
    metadata
}

fn cmd(
    id: &str,
    name: &str,
    description: &str,
    permission: &str,
    input_schema: JsonValue,
) -> DriverCommandDefinition {
    DriverCommandDefinition {
        id: id.into(),
        name: name.into(),
        description: Some(description.into()),
        input_schema,
        output_schema: None,
        permissions: vec![permission.into()],
        metadata: redis_command_metadata(id),
    }
}

fn object_schema(properties: JsonValue, required: &[&str]) -> JsonValue {
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

/// Commands Redis exposes beyond the standard `query` / `execute` bridge.
pub fn redis_command_definitions() -> Vec<DriverCommandDefinition> {
    let db = serde_json::json!({ "type": "integer", "minimum": 0 });
    let key = serde_json::json!({ "type": "string" });
    let strings = serde_json::json!({ "type": "array", "items": { "type": "string" } });

    let mut cmds = vec![
        query_command_definition(),
        execute_command_definition(),
        query_stream_command_definition(),
        cmd(
            "scan_keys",
            "Scan keys",
            "Scan keys with type, TTL, and preview",
            "redis:allow-info",
            object_schema(
                serde_json::json!({ "dbIndex": db, "pattern": { "type": "string" }, "cursor": { "type": "integer" }, "count": { "type": "integer" } }),
                &[],
            ),
        ),
        cmd(
            "get_key",
            "Get key",
            "Load the full value for a Redis key",
            "redis:allow-info",
            object_schema(serde_json::json!({ "dbIndex": db, "key": key }), &["key"]),
        ),
        cmd(
            "set_string",
            "Set string",
            "SET a string key",
            "redis:allow-set-string",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "value": { "type": "string" } }),
                &["key", "value"],
            ),
        ),
        cmd(
            "hash_set",
            "Hash set",
            "HSET a hash field",
            "redis:allow-hash-set",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "field": { "type": "string" }, "value": { "type": "string" } }),
                &["key", "field", "value"],
            ),
        ),
        cmd(
            "hash_del",
            "Hash delete",
            "HDEL hash fields",
            "redis:allow-hash-del",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "fields": strings }),
                &["key", "fields"],
            ),
        ),
        cmd(
            "list_push",
            "List push",
            "LPUSH or RPUSH values",
            "redis:allow-list-push",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "side": { "type": "string" }, "values": strings }),
                &["key", "side", "values"],
            ),
        ),
        cmd(
            "list_set",
            "List set",
            "LSET a list index",
            "redis:allow-list-set",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "index": { "type": "integer" }, "value": { "type": "string" } }),
                &["key", "index", "value"],
            ),
        ),
        cmd(
            "list_pop",
            "List pop",
            "LPOP or RPOP",
            "redis:allow-list-pop",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "side": { "type": "string" } }),
                &["key", "side"],
            ),
        ),
        cmd(
            "set_add",
            "Set add",
            "SADD members",
            "redis:allow-set-add",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "members": strings }),
                &["key", "members"],
            ),
        ),
        cmd(
            "set_remove",
            "Set remove",
            "SREM members",
            "redis:allow-set-remove",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "members": strings }),
                &["key", "members"],
            ),
        ),
        cmd(
            "zset_add",
            "Zset add",
            "ZADD members",
            "redis:allow-zset-add",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "members": { "type": "array" } }),
                &["key", "members"],
            ),
        ),
        cmd(
            "zset_remove",
            "Zset remove",
            "ZREM members",
            "redis:allow-zset-remove",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "members": strings }),
                &["key", "members"],
            ),
        ),
        cmd(
            "delete_keys",
            "Delete keys",
            "Delete one or more keys",
            "redis:allow-delete-keys",
            object_schema(
                serde_json::json!({ "dbIndex": db, "keys": strings }),
                &["keys"],
            ),
        ),
        cmd(
            "rename",
            "Rename",
            "Rename a key",
            "redis:allow-rename",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "newKey": { "type": "string" } }),
                &["key", "newKey"],
            ),
        ),
        cmd(
            "set_ttl",
            "Set TTL",
            "EXPIRE or PERSIST a key",
            "redis:allow-set-ttl",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "ttlSeconds": { "type": "integer" } }),
                &["key", "ttlSeconds"],
            ),
        ),
        cmd(
            "batch_delete_pattern",
            "Batch delete",
            "Delete keys matching a pattern",
            "redis:allow-batch-delete-pattern",
            object_schema(
                serde_json::json!({ "dbIndex": db, "pattern": { "type": "string" } }),
                &["pattern"],
            ),
        ),
        cmd(
            "batch_set_ttl",
            "Batch set TTL",
            "Set TTL on multiple keys",
            "redis:allow-batch-set-ttl",
            object_schema(
                serde_json::json!({ "dbIndex": db, "keys": strings, "ttlSeconds": { "type": "integer" } }),
                &["keys", "ttlSeconds"],
            ),
        ),
        cmd(
            "batch_rename_prefix",
            "Batch rename prefix",
            "Rename keys by prefix",
            "redis:allow-batch-rename-prefix",
            object_schema(
                serde_json::json!({ "dbIndex": db, "oldPrefix": { "type": "string" }, "newPrefix": { "type": "string" }, "keys": strings }),
                &["oldPrefix", "newPrefix"],
            ),
        ),
        cmd(
            "flush_db",
            "Flush DB",
            "FLUSHDB the selected logical database",
            "redis:allow-flush-db",
            object_schema(
                serde_json::json!({ "dbIndex": db, "allowFlush": { "type": "boolean" } }),
                &["allowFlush"],
            ),
        ),
        cmd(
            "flush_all",
            "Flush all",
            "FLUSHALL",
            "redis:allow-flush-all",
            object_schema(
                serde_json::json!({ "allowFlush": { "type": "boolean" } }),
                &["allowFlush"],
            ),
        ),
        cmd(
            "count_matching",
            "Count matching",
            "Count keys matching a pattern",
            "redis:allow-count-matching",
            object_schema(
                serde_json::json!({ "dbIndex": db, "pattern": { "type": "string" } }),
                &["pattern"],
            ),
        ),
        cmd(
            "cluster_nodes",
            "Cluster nodes",
            "CLUSTER NODES",
            "redis:allow-cluster-nodes",
            object_schema(serde_json::json!({}), &[]),
        ),
        cmd(
            "info",
            "Info",
            "INFO section",
            "redis:allow-info",
            object_schema(
                serde_json::json!({ "section": { "type": "string" }, "nodeAddr": { "type": "string" } }),
                &[],
            ),
        ),
        cmd(
            "memory_sample",
            "Memory sample",
            "Sample large keys",
            "redis:allow-memory-sample",
            object_schema(
                serde_json::json!({ "dbIndex": db, "limit": { "type": "integer" } }),
                &[],
            ),
        ),
        cmd(
            "slowlog_get",
            "Slowlog get",
            "SLOWLOG GET",
            "redis:allow-slowlog-get",
            object_schema(serde_json::json!({ "count": { "type": "integer" } }), &[]),
        ),
        cmd(
            "slowlog_reset",
            "Slowlog reset",
            "SLOWLOG RESET",
            "redis:allow-slowlog-reset",
            object_schema(
                serde_json::json!({ "confirm": { "type": "boolean" } }),
                &["confirm"],
            ),
        ),
        cmd(
            "modules_list",
            "Modules",
            "List loaded Redis modules",
            "redis:allow-modules-list",
            object_schema(serde_json::json!({}), &[]),
        ),
        cmd(
            "exec",
            "Exec",
            "Run raw Redis commands",
            "redis:allow-exec",
            object_schema(
                serde_json::json!({ "dbIndex": db, "commands": { "type": "string" }, "nodeAddr": { "type": "string" } }),
                &["commands"],
            ),
        ),
        cmd(
            "pubsub_publish",
            "Publish",
            "PUBLISH a message",
            "redis:allow-pubsub-publish",
            object_schema(
                serde_json::json!({ "channel": { "type": "string" }, "message": { "type": "string" } }),
                &["channel", "message"],
            ),
        ),
        cmd(
            "pubsub_subscribe",
            "Subscribe",
            "SUBSCRIBE / PSUBSCRIBE and stream messages as events",
            "redis:allow-pubsub-subscribe",
            object_schema(
                serde_json::json!({ "channels": strings, "patterns": strings }),
                &[],
            ),
        ),
        cmd(
            "pubsub_unsubscribe",
            "Unsubscribe",
            "Stop a Pub/Sub subscription",
            "redis:allow-pubsub-unsubscribe",
            object_schema(
                serde_json::json!({ "subscriptionId": { "type": "string" } }),
                &["subscriptionId"],
            ),
        ),
        cmd(
            "json_get",
            "JSON get",
            "JSON.GET",
            "redis:allow-json-get",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "path": { "type": "string" } }),
                &["key"],
            ),
        ),
        cmd(
            "json_set",
            "JSON set",
            "JSON.SET",
            "redis:allow-json-set",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "path": { "type": "string" }, "value": { "type": "string" } }),
                &["key", "path", "value"],
            ),
        ),
        cmd(
            "json_del",
            "JSON delete",
            "JSON.DEL",
            "redis:allow-json-del",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "path": { "type": "string" } }),
                &["key", "path"],
            ),
        ),
        cmd(
            "xrange",
            "XRANGE",
            "Read stream entries",
            "redis:allow-xrange",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "start": { "type": "string" }, "end": { "type": "string" }, "count": { "type": "integer" } }),
                &["key", "start", "end"],
            ),
        ),
        cmd(
            "xadd",
            "XADD",
            "Append a stream entry",
            "redis:allow-xadd",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "fields": { "type": "object" }, "id": { "type": "string" } }),
                &["key", "fields"],
            ),
        ),
        cmd(
            "xgroup_create",
            "XGROUP CREATE",
            "Create a consumer group",
            "redis:allow-xgroup-create",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "group": { "type": "string" }, "startId": { "type": "string" } }),
                &["key", "group"],
            ),
        ),
        cmd(
            "xgroup_destroy",
            "XGROUP DESTROY",
            "Destroy a consumer group",
            "redis:allow-xgroup-destroy",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "group": { "type": "string" } }),
                &["key", "group"],
            ),
        ),
        cmd(
            "xinfo_groups",
            "XINFO GROUPS",
            "List stream consumer groups",
            "redis:allow-xinfo-groups",
            object_schema(serde_json::json!({ "dbIndex": db, "key": key }), &["key"]),
        ),
        cmd(
            "xpending",
            "XPENDING",
            "Pending stream entries",
            "redis:allow-xpending",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "group": { "type": "string" }, "start": { "type": "string" }, "end": { "type": "string" }, "count": { "type": "integer" }, "consumer": { "type": "string" } }),
                &["key", "group"],
            ),
        ),
        cmd(
            "xack",
            "XACK",
            "Acknowledge stream entries",
            "redis:allow-xack",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "group": { "type": "string" }, "ids": strings }),
                &["key", "group", "ids"],
            ),
        ),
        cmd(
            "stream_overview",
            "Stream overview",
            "Summarize streams in a database",
            "redis:allow-stream-overview",
            object_schema(
                serde_json::json!({ "dbIndex": db, "limit": { "type": "integer" } }),
                &[],
            ),
        ),
        cmd(
            "dump_keys",
            "Dump keys",
            "DUMP selected keys",
            "redis:allow-dump-keys",
            object_schema(
                serde_json::json!({ "dbIndex": db, "keys": strings }),
                &["keys"],
            ),
        ),
        cmd(
            "restore_keys",
            "Restore keys",
            "RESTORE dumped keys",
            "redis:allow-restore-keys",
            object_schema(
                serde_json::json!({ "dbIndex": db, "entries": { "type": "array" }, "replace": { "type": "boolean" } }),
                &["entries"],
            ),
        ),
    ];
    cmds.extend(schema_catalog_command_definitions());
    cmds
}

fn req_str<'a>(input: &'a JsonValue, field: &str) -> Result<&'a str, DriverError> {
    input
        .get(field)
        .and_then(JsonValue::as_str)
        .ok_or_else(|| DriverError::InvalidConfig(format!("command input requires '{field}'")))
}

fn opt_str<'a>(input: &'a JsonValue, field: &str) -> Option<&'a str> {
    input
        .get(field)
        .and_then(JsonValue::as_str)
        .filter(|s| !s.is_empty())
}

fn req_i64(input: &JsonValue, field: &str) -> Result<i64, DriverError> {
    input.get(field).and_then(JsonValue::as_i64).ok_or_else(|| {
        DriverError::InvalidConfig(format!("command input requires integer '{field}'"))
    })
}

fn req_bool(input: &JsonValue, field: &str) -> Result<bool, DriverError> {
    input
        .get(field)
        .and_then(JsonValue::as_bool)
        .ok_or_else(|| {
            DriverError::InvalidConfig(format!("command input requires boolean '{field}'"))
        })
}

fn db_index(input: &JsonValue) -> u32 {
    input
        .get("dbIndex")
        .or_else(|| input.get("db_index"))
        .and_then(JsonValue::as_u64)
        .unwrap_or(0) as u32
}

fn opt_string_vec(input: &JsonValue, field: &str) -> Vec<String> {
    input
        .get(field)
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn string_vec(input: &JsonValue, field: &str) -> Result<Vec<String>, DriverError> {
    input
        .get(field)
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_string)
                .collect()
        })
        .ok_or_else(|| {
            DriverError::InvalidConfig(format!("command input requires string array '{field}'"))
        })
}

fn json_ok<T: serde::Serialize>(value: T) -> Result<CommandResult, DriverError> {
    serde_json::to_value(value)
        .map(CommandResult::new)
        .map_err(|e| DriverError::QueryFailed(e.to_string()))
}

fn ok() -> CommandResult {
    CommandResult::new(serde_json::json!({ "ok": true }))
}

pub async fn execute_redis_command(
    driver: &RedisDriver,
    handle: &ConnectionHandle,
    command: &str,
    input: JsonValue,
) -> Result<CommandResult, DriverError> {
    match execute_standard_sql_command(driver, handle, command, input.clone()).await {
        Err(DriverError::Unsupported(_)) => {}
        other => return other,
    }
    if let Some(result) =
        try_execute_schema_catalog_command(driver, handle, command, input.clone()).await?
    {
        return Ok(result);
    }

    let id = handle.pool_id.as_str();
    let db = db_index(&input);

    match command {
        "scan_keys" => {
            let pattern = opt_str(&input, "pattern").unwrap_or("*");
            let cursor = input.get("cursor").and_then(JsonValue::as_u64).unwrap_or(0);
            let count = input
                .get("count")
                .and_then(JsonValue::as_u64)
                .unwrap_or(100) as u32;
            let (next, keys, db_size) = driver
                .scan_keys_with_info(handle, db, pattern, cursor, count)
                .await?;
            json_ok(serde_json::json!({ "cursor": next, "keys": keys, "dbSize": db_size }))
        }
        "get_key" => json_ok(
            driver
                .get_key_detail(handle, db, req_str(&input, "key")?)
                .await?,
        ),
        "set_string" => {
            driver
                .plugin_set_string(id, db, req_str(&input, "key")?, req_str(&input, "value")?)
                .await?;
            Ok(ok())
        }
        "hash_set" => {
            driver
                .plugin_hash_set(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "field")?,
                    req_str(&input, "value")?,
                )
                .await?;
            Ok(ok())
        }
        "hash_del" => {
            let fields = string_vec(&input, "fields")?;
            driver
                .plugin_hash_del(id, db, req_str(&input, "key")?, &fields)
                .await?;
            Ok(ok())
        }
        "list_push" => {
            let values = string_vec(&input, "values")?;
            driver
                .plugin_list_push(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "side")?,
                    &values,
                )
                .await?;
            Ok(ok())
        }
        "list_set" => {
            driver
                .plugin_list_set(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_i64(&input, "index")?,
                    req_str(&input, "value")?,
                )
                .await?;
            Ok(ok())
        }
        "list_pop" => json_ok(
            driver
                .plugin_list_pop(id, db, req_str(&input, "key")?, req_str(&input, "side")?)
                .await?,
        ),
        "set_add" => {
            let members = string_vec(&input, "members")?;
            driver
                .plugin_set_add(id, db, req_str(&input, "key")?, &members)
                .await?;
            Ok(ok())
        }
        "set_remove" => {
            let members = string_vec(&input, "members")?;
            driver
                .plugin_set_remove(id, db, req_str(&input, "key")?, &members)
                .await?;
            Ok(ok())
        }
        "zset_add" => {
            let members: Vec<ZsetMember> =
                serde_json::from_value(input.get("members").cloned().unwrap_or(JsonValue::Null))
                    .map_err(|e| DriverError::InvalidConfig(e.to_string()))?;
            driver
                .plugin_zset_add(id, db, req_str(&input, "key")?, &members)
                .await?;
            Ok(ok())
        }
        "zset_remove" => {
            let members = string_vec(&input, "members")?;
            driver
                .plugin_zset_remove(id, db, req_str(&input, "key")?, &members)
                .await?;
            Ok(ok())
        }
        "delete_keys" => {
            let keys = string_vec(&input, "keys")?;
            json_ok(driver.plugin_delete_keys(id, db, &keys).await?)
        }
        "rename" => {
            let new_key = input
                .get("newKey")
                .or_else(|| input.get("new_key"))
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    DriverError::InvalidConfig("command input requires 'newKey'".into())
                })?;
            driver
                .plugin_rename_key(id, db, req_str(&input, "key")?, new_key)
                .await?;
            Ok(ok())
        }
        "set_ttl" => {
            let ttl = input
                .get("ttlSeconds")
                .or_else(|| input.get("ttl_seconds"))
                .and_then(JsonValue::as_i64)
                .ok_or_else(|| {
                    DriverError::InvalidConfig("command input requires 'ttlSeconds'".into())
                })?;
            driver
                .plugin_set_ttl(id, db, req_str(&input, "key")?, ttl)
                .await?;
            Ok(ok())
        }
        "batch_delete_pattern" => json_ok(
            driver
                .plugin_batch_delete_pattern(id, db, req_str(&input, "pattern")?)
                .await?,
        ),
        "batch_set_ttl" => {
            let keys = string_vec(&input, "keys")?;
            let ttl = input
                .get("ttlSeconds")
                .or_else(|| input.get("ttl_seconds"))
                .and_then(JsonValue::as_i64)
                .ok_or_else(|| {
                    DriverError::InvalidConfig("command input requires 'ttlSeconds'".into())
                })?;
            json_ok(driver.plugin_batch_set_ttl(id, db, &keys, ttl).await?)
        }
        "batch_rename_prefix" => {
            let old_prefix = input
                .get("oldPrefix")
                .or_else(|| input.get("old_prefix"))
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    DriverError::InvalidConfig("command input requires 'oldPrefix'".into())
                })?;
            let new_prefix = input
                .get("newPrefix")
                .or_else(|| input.get("new_prefix"))
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    DriverError::InvalidConfig("command input requires 'newPrefix'".into())
                })?;
            let keys = input
                .get("keys")
                .and_then(JsonValue::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(JsonValue::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                });
            json_ok(
                driver
                    .plugin_batch_rename_prefix(id, db, old_prefix, new_prefix, keys)
                    .await?,
            )
        }
        "flush_db" => {
            crate::ops::ensure_flush_allowed(
                req_bool(&input, "allowFlush").or_else(|_| req_bool(&input, "allow_flush"))?,
            )
            .map_err(DriverError::InvalidConfig)?;
            driver.plugin_flush_db(id, db).await?;
            Ok(ok())
        }
        "flush_all" => {
            crate::ops::ensure_flush_allowed(
                req_bool(&input, "allowFlush").or_else(|_| req_bool(&input, "allow_flush"))?,
            )
            .map_err(DriverError::InvalidConfig)?;
            driver.plugin_flush_all(id).await?;
            Ok(ok())
        }
        "count_matching" => json_ok(
            driver
                .plugin_count_matching(id, db, req_str(&input, "pattern")?)
                .await?,
        ),
        "cluster_nodes" => json_ok(driver.plugin_cluster_nodes(id).await?),
        "info" => json_ok(
            driver
                .plugin_info(
                    id,
                    opt_str(&input, "section").map(str::to_string),
                    opt_str(&input, "nodeAddr")
                        .or_else(|| opt_str(&input, "node_addr"))
                        .map(str::to_string),
                )
                .await?,
        ),
        "memory_sample" => json_ok(
            driver
                .plugin_memory_sample(
                    id,
                    db,
                    input
                        .get("limit")
                        .and_then(JsonValue::as_u64)
                        .map(|v| v as u32),
                )
                .await?,
        ),
        "slowlog_get" => json_ok(
            driver
                .plugin_slowlog_get(
                    id,
                    input.get("count").and_then(JsonValue::as_u64).unwrap_or(16) as u32,
                )
                .await?,
        ),
        "slowlog_reset" => {
            crate::ops_observe::ensure_slowlog_reset_confirmed(req_bool(&input, "confirm")?)
                .map_err(DriverError::InvalidConfig)?;
            driver.plugin_slowlog_reset(id).await?;
            Ok(ok())
        }
        "modules_list" => json_ok(driver.plugin_modules_list(id).await?),
        "exec" => json_ok(
            driver
                .plugin_exec(
                    id,
                    db,
                    req_str(&input, "commands")?,
                    opt_str(&input, "nodeAddr")
                        .or_else(|| opt_str(&input, "node_addr"))
                        .map(str::to_string),
                )
                .await?,
        ),
        "pubsub_publish" => json_ok(
            driver
                .plugin_pubsub_publish(id, req_str(&input, "channel")?, req_str(&input, "message")?)
                .await?,
        ),
        "pubsub_subscribe" => {
            let channels = opt_string_vec(&input, "channels");
            let patterns = opt_string_vec(&input, "patterns");
            json_ok(
                crate::ops_pubsub::start_subscription(
                    driver,
                    handle.pool_id.clone(),
                    channels,
                    patterns,
                )
                .await
                .map_err(DriverError::QueryFailed)?,
            )
        }
        "pubsub_unsubscribe" => {
            let sub_id = input
                .get("subscriptionId")
                .or_else(|| input.get("subscription_id"))
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    DriverError::InvalidConfig("command input requires 'subscriptionId'".into())
                })?;
            crate::ops_pubsub::unsubscribe(sub_id)
                .await
                .map_err(DriverError::QueryFailed)?;
            Ok(ok())
        }
        "json_get" => json_ok(
            driver
                .plugin_json_get(
                    id,
                    db,
                    req_str(&input, "key")?,
                    opt_str(&input, "path").unwrap_or("$"),
                )
                .await?,
        ),
        "json_set" => {
            driver
                .plugin_json_set(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "path")?,
                    req_str(&input, "value")?,
                )
                .await?;
            Ok(ok())
        }
        "json_del" => json_ok(
            driver
                .plugin_json_del(id, db, req_str(&input, "key")?, req_str(&input, "path")?)
                .await?,
        ),
        "xrange" => json_ok(
            driver
                .plugin_xrange(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "start")?,
                    req_str(&input, "end")?,
                    input
                        .get("count")
                        .and_then(JsonValue::as_u64)
                        .map(|v| v as u32),
                )
                .await?,
        ),
        "xadd" => {
            let fields: std::collections::HashMap<String, String> =
                serde_json::from_value(input.get("fields").cloned().unwrap_or(JsonValue::Null))
                    .map_err(|e| DriverError::InvalidConfig(e.to_string()))?;
            json_ok(
                driver
                    .plugin_xadd(
                        id,
                        db,
                        req_str(&input, "key")?,
                        &fields,
                        opt_str(&input, "id").map(str::to_string),
                    )
                    .await?,
            )
        }
        "xgroup_create" => {
            driver
                .plugin_xgroup_create(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "group")?,
                    opt_str(&input, "startId")
                        .or_else(|| opt_str(&input, "start_id"))
                        .map(str::to_string),
                )
                .await?;
            Ok(ok())
        }
        "xgroup_destroy" => {
            driver
                .plugin_xgroup_destroy(id, db, req_str(&input, "key")?, req_str(&input, "group")?)
                .await?;
            Ok(ok())
        }
        "xinfo_groups" => json_ok(
            driver
                .plugin_xinfo_groups(id, db, req_str(&input, "key")?)
                .await?,
        ),
        "xpending" => json_ok(
            driver
                .plugin_xpending(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "group")?,
                    opt_str(&input, "start").map(str::to_string),
                    opt_str(&input, "end").map(str::to_string),
                    input
                        .get("count")
                        .and_then(JsonValue::as_u64)
                        .map(|v| v as u32),
                    opt_str(&input, "consumer").map(str::to_string),
                )
                .await?,
        ),
        "xack" => {
            let ids = string_vec(&input, "ids")?;
            json_ok(
                driver
                    .plugin_xack(
                        id,
                        db,
                        req_str(&input, "key")?,
                        req_str(&input, "group")?,
                        &ids,
                    )
                    .await?,
            )
        }
        "stream_overview" => json_ok(
            driver
                .plugin_stream_overview(
                    id,
                    db,
                    input
                        .get("limit")
                        .and_then(JsonValue::as_u64)
                        .map(|v| v as u32),
                )
                .await?,
        ),
        "dump_keys" => {
            let keys = string_vec(&input, "keys")?;
            json_ok(driver.plugin_dump_keys(id, db, &keys).await?)
        }
        "restore_keys" => {
            let entries: Vec<RestoreKeyEntry> =
                serde_json::from_value(input.get("entries").cloned().unwrap_or(JsonValue::Null))
                    .map_err(|e| DriverError::InvalidConfig(e.to_string()))?;
            json_ok(
                driver
                    .plugin_restore_keys(
                        id,
                        db,
                        entries,
                        input
                            .get("replace")
                            .and_then(JsonValue::as_bool)
                            .unwrap_or(false),
                    )
                    .await?,
            )
        }
        other => Err(DriverError::Unsupported(format!(
            "unsupported driver command: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redis_definitions_include_plugin_and_standard_commands() {
        let ids: Vec<_> = redis_command_definitions()
            .into_iter()
            .map(|d| d.id)
            .collect();
        assert!(ids.contains(&"query".into()));
        assert!(ids.contains(&"set_string".into()));
        assert!(ids.contains(&"flush_db".into()));
        assert!(ids.contains(&"scan_keys".into()));
        assert!(ids.contains(&"get_key".into()));
        assert!(ids.contains(&"pubsub_subscribe".into()));
        assert!(ids.contains(&"pubsub_unsubscribe".into()));
        let subscribe = redis_command_definitions()
            .into_iter()
            .find(|d| d.id == "pubsub_subscribe")
            .unwrap();
        assert!(!subscribe.metadata.workflow);
        assert_eq!(subscribe.metadata.category, CommandCategory::PubSub);
    }

    #[test]
    fn redis_commands_reuse_existing_permission_identifiers() {
        let set = redis_command_definitions()
            .into_iter()
            .find(|d| d.id == "set_string")
            .unwrap();
        assert_eq!(set.permissions, vec!["redis:allow-set-string"]);
        assert!(set.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "key"));
    }

    #[test]
    fn scan_keys_command_accepts_pagination_input() {
        let scan = redis_command_definitions()
            .into_iter()
            .find(|d| d.id == "scan_keys")
            .unwrap();
        let props = &scan.input_schema["properties"];
        assert!(props.get("dbIndex").is_some());
        assert!(props.get("pattern").is_some());
        assert!(props.get("cursor").is_some());
        assert!(props.get("count").is_some());
        assert_eq!(scan.permissions, vec!["redis:allow-info"]);
    }

    #[test]
    fn get_key_command_requires_key_input() {
        let get = redis_command_definitions()
            .into_iter()
            .find(|d| d.id == "get_key")
            .unwrap();
        assert_eq!(
            get.input_schema["required"].as_array().unwrap(),
            &vec![serde_json::json!("key")]
        );
        assert_eq!(get.permissions, vec!["redis:allow-info"]);
    }
}
