//! Redis Driver Command definitions and dispatch.
//!
//! Redis UI, Workflow, generic IPC, and MCP all execute operations through
//! `execute_command`. The Redis Tauri plugin is setup-only (Pub/Sub events).

use datazen_driver_api::{
    execute_command_definition, query_command_definition, query_stream_command_definition,
    schema_catalog_command_definitions, CommandCategory, DriverCommandDefinition,
    DriverCommandMetadata,
};
use serde_json::Value as JsonValue;

fn redis_command_metadata(id: &str) -> DriverCommandMetadata {
    let category = match id {
        id if id.starts_with("pubsub_") => CommandCategory::PubSub,
        "xrange" | "xadd" | "xgroup_create" | "xgroup_destroy" | "xinfo_groups"
        | "xinfo_consumers" | "xpending" | "xack" | "stream_lag" | "stream_overview" => {
            CommandCategory::Stream
        }
        "dump_keys" | "restore_keys" => CommandCategory::Io,
        "flush_db" | "flush_all" | "slowlog_reset" => CommandCategory::Admin,
        "scan_keys" | "get_key" | "get_key_raw" | "db_sizes" | "list_children" | "info"
        | "memory_sample" | "slowlog_get" | "modules_list" | "cluster_nodes" | "count_matching"
        | "scan_values" | "scan_abort" | "decode_value" | "monitor_start" | "monitor_stop"
        | "monitor_get_buffer" => CommandCategory::Observe,
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
            "Scan keys with type, TTL, and preview (optional TYPE filter and MEMORY USAGE)",
            "redis:allow-info",
            object_schema(
                serde_json::json!({
                    "dbIndex": db,
                    "pattern": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "count": { "type": "integer" },
                    "keyType": { "type": "string", "description": "Optional Redis TYPE filter (string/hash/list/set/zset/stream)" },
                    "withMemory": { "type": "boolean", "description": "When true, size uses MEMORY USAGE (bytes)" },
                    "noTtlOnly": { "type": "boolean", "description": "When true, only include keys without expiry (TTL == -1)" }
                }),
                &[],
            ),
        ),
        cmd(
            "db_sizes",
            "DB Sizes",
            "Fetch key counts for every database (SELECT + DBSIZE)",
            "redis:allow-info",
            serde_json::json!({ "type": "object", "properties": {} }),
        ),
        cmd(
            "list_children",
            "List children",
            "List direct children under a key prefix (leaf keys + virtual folders)",
            "redis:allow-info",
            object_schema(
                serde_json::json!({
                    "dbIndex": db,
                    "prefix": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "count": { "type": "integer" },
                    "sep": { "type": "string", "description": "Separator char (default ':')" },
                    "noTtlOnly": { "type": "boolean" },
                    "keyType": { "type": "string" }
                }),
                &["prefix"],
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
            "scan_values",
            "Scan values",
            "Guarded incremental value/key search across a database (one batch per call; poll with the returned cursor)",
            "redis:allow-info",
            object_schema(
                serde_json::json!({
                    "dbIndex": db,
                    "pattern": { "type": "string", "description": "SCAN glob (default '*')" },
                    "query": { "type": "string", "description": "Substring to match; supports \\xNN byte escapes" },
                    "mode": { "type": "string", "enum": ["key", "value", "all"], "description": "Search scope (v1.0 value/all match string types only)" },
                    "cursor": { "type": "integer", "description": "Resume cursor from the previous batch (0 to start)" },
                    "taskId": { "type": "string", "description": "Active task id to continue; omit or mismatch to start a fresh task" },
                    "maxKeys": { "type": "integer", "description": "Cumulative SCAN key cap (clamped to 200000)" },
                    "byteBudget": { "type": "integer", "description": "Cumulative GETRANGE byte cap (clamped to 268435456)" },
                    "perValuePeek": { "type": "integer", "description": "Max bytes peeked per string value (clamped to 32768)" },
                    "count": { "type": "integer", "description": "Per-batch SCAN COUNT (clamped to 2000)" }
                }),
                &[],
            ),
        ),
        cmd(
            "scan_abort",
            "Abort value scan",
            "Signal the active scan_values task to stop before its next batch (idempotent)",
            "redis:allow-info",
            object_schema(
                serde_json::json!({ "taskId": { "type": "string" } }),
                &[],
            ),
        ),
        cmd(
            "get_key_raw",
            "Get key (binary safe)",
            "Fetch TYPE/TTL/logical length/MEMORY USAGE and raw bytes as base64 for a string key",
            "redis:allow-info",
            object_schema(
                serde_json::json!({
                    "dbIndex": db,
                    "key": key,
                    "withMemory": { "type": "boolean", "description": "Include MEMORY USAGE bytes" }
                }),
                &["key"],
            ),
        ),
        cmd(
            "decode_value",
            "Decode value",
            "Parse-only decode of a base64 payload (msgpack / pickle / php / java) into a JSON tree; never executes host-language objects",
            "redis:allow-info",
            object_schema(
                serde_json::json!({
                    "codec": { "type": "string", "enum": ["msgpack", "pickle", "php", "java"] },
                    "data": { "type": "string", "description": "base64-encoded raw bytes" }
                }),
                &["codec", "data"],
            ),
        ),
        cmd(
            "set_string",
            "Set string",
            "SET a string key (optional KEEPTTL)",
            "redis:allow-set-string",
            object_schema(
                serde_json::json!({
                    "dbIndex": db,
                    "key": key,
                    "value": { "type": "string" },
                    "keepTtl": { "type": "boolean" }
                }),
                &["key", "value"],
            ),
        ),
        cmd(
            "set_string_raw",
            "Set string (binary)",
            "SET a string key from base64 raw bytes (binary-safe; optional KEEPTTL)",
            "redis:allow-set-string",
            object_schema(
                serde_json::json!({
                    "dbIndex": db,
                    "key": key,
                    "dataB64": { "type": "string", "description": "base64-encoded raw bytes" },
                    "keepTtl": { "type": "boolean" }
                }),
                &["key", "dataB64"],
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
            "list_index",
            "List index",
            "LINDEX get element at index",
            "redis:allow-list-index",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "index": { "type": "integer" } }),
                &["key", "index"],
            ),
        ),
        cmd(
            "list_rem",
            "List remove",
            "LREM remove occurrences of value",
            "redis:allow-list-rem",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "count": { "type": "integer" }, "value": { "type": "string" } }),
                &["key", "count", "value"],
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
            "EXPIRE, EXPIREAT, or PERSIST a key",
            "redis:allow-set-ttl",
            object_schema(
                serde_json::json!({
                    "dbIndex": db,
                    "key": key,
                    "ttlSeconds": { "type": "integer" },
                    "expireAt": { "type": "integer", "description": "Unix timestamp seconds for EXPIREAT" }
                }),
                &["key"],
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
            "memory_usage_key",
            "Memory usage (single key)",
            "MEMORY USAGE for a specific key",
            "redis:allow-memory-sample",
            object_schema(serde_json::json!({ "key": { "type": "string" } }), &["key"]),
        ),
        cmd(
            "info_filtered",
            "Info (filtered)",
            "INFO with section and keyword search",
            "redis:allow-info",
            object_schema(
                serde_json::json!({
                    "section": { "type": "string" },
                    "search": { "type": "string" },
                    "nodeAddr": { "type": "string" }
                }),
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
            "monitor_start",
            "Start MONITOR",
            "Start a real-time MONITOR session on a dedicated connection",
            "redis:allow-monitor",
            object_schema(
                serde_json::json!({ "dbSessionId": { "type": "string" }, "bufferSize": { "type": "integer" } }),
                &["dbSessionId"],
            ),
        ),
        cmd(
            "monitor_stop",
            "Stop MONITOR",
            "Stop an active MONITOR session",
            "redis:allow-monitor",
            object_schema(
                serde_json::json!({ "dbSessionId": { "type": "string" }, "monitorId": { "type": "string" } }),
                &["dbSessionId", "monitorId"],
            ),
        ),
        cmd(
            "monitor_get_buffer",
            "MONITOR Buffer",
            "Retrieve buffered MONITOR events",
            "redis:allow-monitor",
            object_schema(
                serde_json::json!({ "dbSessionId": { "type": "string" }, "monitorId": { "type": "string" } }),
                &["dbSessionId", "monitorId"],
            ),
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
            "pubsub_list_subscriptions",
            "List subscriptions",
            "List active Pub/Sub subscriptions for a connection",
            "redis:allow-pubsub-subscribe",
            object_schema(serde_json::json!({}), &[]),
        ),
        cmd(
            "pubsub_stats",
            "Pub/Sub stats",
            "Get aggregated Pub/Sub message statistics",
            "redis:allow-pubsub-subscribe",
            object_schema(serde_json::json!({}), &[]),
        ),
        cmd(
            "json_get",
            "JSON get",
            "JSON.GET",
            "redis:allow-json-get",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "path": { "type": "string" }, "raw": { "type": "boolean" } }),
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
            "xinfo_consumers",
            "XINFO CONSUMERS",
            "List consumers in a group",
            "redis:allow-xinfo-consumers",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "group": { "type": "string" } }),
                &["key", "group"],
            ),
        ),
        cmd(
            "stream_lag",
            "Stream lag",
            "Compute lag for a consumer group",
            "redis:allow-stream-lag",
            object_schema(
                serde_json::json!({ "dbIndex": db, "key": key, "group": { "type": "string" } }),
                &["key", "group"],
            ),
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
