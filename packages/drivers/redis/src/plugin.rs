//! Tauri plugin commands for Redis deep ops (`plugin:redis|*`).

use crate::ops::{self, ZsetMember};
use crate::ops_exec::ExecResponse;
use crate::ops_observe::{self, MemorySampleResult, SlowlogEntry};
use crate::ops_json::{JsonDelResult, JsonGetResult};
use crate::ops_pubsub;
use crate::ops_stream;
use crate::shared_driver;
use datazen_driver_api::DriverError;

fn map_err(e: DriverError) -> String {
    e.to_string()
}

#[tauri::command]
async fn set_string(
    connection_id: String,
    db_index: u32,
    key: String,
    value: String,
) -> Result<(), String> {
    shared_driver()
        .plugin_set_string(&connection_id, db_index, &key, &value)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn hash_set(
    connection_id: String,
    db_index: u32,
    key: String,
    field: String,
    value: String,
) -> Result<(), String> {
    shared_driver()
        .plugin_hash_set(&connection_id, db_index, &key, &field, &value)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn hash_del(
    connection_id: String,
    db_index: u32,
    key: String,
    fields: Vec<String>,
) -> Result<(), String> {
    shared_driver()
        .plugin_hash_del(&connection_id, db_index, &key, &fields)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn list_push(
    connection_id: String,
    db_index: u32,
    key: String,
    side: String,
    values: Vec<String>,
) -> Result<(), String> {
    shared_driver()
        .plugin_list_push(&connection_id, db_index, &key, &side, &values)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn list_set(
    connection_id: String,
    db_index: u32,
    key: String,
    index: i64,
    value: String,
) -> Result<(), String> {
    shared_driver()
        .plugin_list_set(&connection_id, db_index, &key, index, &value)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn list_pop(
    connection_id: String,
    db_index: u32,
    key: String,
    side: String,
) -> Result<Option<String>, String> {
    shared_driver()
        .plugin_list_pop(&connection_id, db_index, &key, &side)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn set_add(
    connection_id: String,
    db_index: u32,
    key: String,
    members: Vec<String>,
) -> Result<(), String> {
    shared_driver()
        .plugin_set_add(&connection_id, db_index, &key, &members)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn set_remove(
    connection_id: String,
    db_index: u32,
    key: String,
    members: Vec<String>,
) -> Result<(), String> {
    shared_driver()
        .plugin_set_remove(&connection_id, db_index, &key, &members)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn zset_add(
    connection_id: String,
    db_index: u32,
    key: String,
    members: Vec<ZsetMember>,
) -> Result<(), String> {
    shared_driver()
        .plugin_zset_add(&connection_id, db_index, &key, &members)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn zset_remove(
    connection_id: String,
    db_index: u32,
    key: String,
    members: Vec<String>,
) -> Result<(), String> {
    shared_driver()
        .plugin_zset_remove(&connection_id, db_index, &key, &members)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn delete_keys(
    connection_id: String,
    db_index: u32,
    keys: Vec<String>,
) -> Result<u64, String> {
    shared_driver()
        .plugin_delete_keys(&connection_id, db_index, &keys)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn rename(
    connection_id: String,
    db_index: u32,
    key: String,
    new_key: String,
) -> Result<(), String> {
    shared_driver()
        .plugin_rename_key(&connection_id, db_index, &key, &new_key)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn set_ttl(
    connection_id: String,
    db_index: u32,
    key: String,
    ttl_seconds: i64,
) -> Result<(), String> {
    shared_driver()
        .plugin_set_ttl(&connection_id, db_index, &key, ttl_seconds)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn batch_delete_pattern(
    connection_id: String,
    db_index: u32,
    pattern: String,
) -> Result<ops::BatchDeleteResult, String> {
    shared_driver()
        .plugin_batch_delete_pattern(&connection_id, db_index, &pattern)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn batch_set_ttl(
    connection_id: String,
    db_index: u32,
    keys: Vec<String>,
    ttl_seconds: i64,
) -> Result<ops::BatchSetTtlResult, String> {
    shared_driver()
        .plugin_batch_set_ttl(&connection_id, db_index, &keys, ttl_seconds)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn batch_rename_prefix(
    connection_id: String,
    db_index: u32,
    old_prefix: String,
    new_prefix: String,
    keys: Option<Vec<String>>,
) -> Result<ops::BatchRenameResult, String> {
    shared_driver()
        .plugin_batch_rename_prefix(&connection_id, db_index, &old_prefix, &new_prefix, keys)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn flush_db(
    connection_id: String,
    db_index: u32,
    allow_flush: bool,
) -> Result<(), String> {
    ops::ensure_flush_allowed(allow_flush)?;
    shared_driver()
        .plugin_flush_db(&connection_id, db_index)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn flush_all(connection_id: String, allow_flush: bool) -> Result<(), String> {
    ops::ensure_flush_allowed(allow_flush)?;
    shared_driver()
        .plugin_flush_all(&connection_id)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn count_matching(
    connection_id: String,
    db_index: u32,
    pattern: String,
) -> Result<u64, String> {
    shared_driver()
        .plugin_count_matching(&connection_id, db_index, &pattern)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn info(
    connection_id: String,
    section: Option<String>,
    node_addr: Option<String>,
) -> Result<String, String> {
    shared_driver()
        .plugin_info(&connection_id, section, node_addr)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn memory_sample(
    connection_id: String,
    db_index: u32,
    limit: Option<u32>,
) -> Result<MemorySampleResult, String> {
    shared_driver()
        .plugin_memory_sample(&connection_id, db_index, limit)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn slowlog_get(connection_id: String, count: u32) -> Result<Vec<SlowlogEntry>, String> {
    shared_driver()
        .plugin_slowlog_get(&connection_id, count)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn slowlog_reset(connection_id: String, confirm: bool) -> Result<(), String> {
    ops_observe::ensure_slowlog_reset_confirmed(confirm)?;
    shared_driver()
        .plugin_slowlog_reset(&connection_id)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn modules_list(connection_id: String) -> Result<Vec<String>, String> {
    shared_driver()
        .plugin_modules_list(&connection_id)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn exec(
    connection_id: String,
    db_index: u32,
    commands: String,
) -> Result<ExecResponse, String> {
    shared_driver()
        .plugin_exec(&connection_id, db_index, &commands)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn pubsub_subscribe<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    connection_id: String,
    channels: Vec<String>,
    patterns: Vec<String>,
) -> Result<String, String> {
    ops_pubsub::start_subscription(app, shared_driver(), connection_id, channels, patterns).await
}

#[tauri::command]
async fn pubsub_unsubscribe(subscription_id: String) -> Result<(), String> {
    ops_pubsub::unsubscribe(&subscription_id).await
}

#[tauri::command]
async fn pubsub_publish(
    connection_id: String,
    channel: String,
    message: String,
) -> Result<u64, String> {
    shared_driver()
        .plugin_pubsub_publish(&connection_id, &channel, &message)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn json_get(
    connection_id: String,
    db_index: u32,
    key: String,
    path: Option<String>,
) -> Result<JsonGetResult, String> {
    let path = path.unwrap_or_else(|| "$".into());
    shared_driver()
        .plugin_json_get(&connection_id, db_index, &key, &path)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn json_set(
    connection_id: String,
    db_index: u32,
    key: String,
    path: String,
    value: String,
) -> Result<(), String> {
    shared_driver()
        .plugin_json_set(&connection_id, db_index, &key, &path, &value)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn json_del(
    connection_id: String,
    db_index: u32,
    key: String,
    path: String,
) -> Result<JsonDelResult, String> {
    shared_driver()
        .plugin_json_del(&connection_id, db_index, &key, &path)
        .await
        .map_err(map_err)
}

/// Register Redis IPC commands as a Tauri plugin (`plugin:redis|*`).
pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("redis")
        .invoke_handler(tauri::generate_handler![
            set_string,
            hash_set,
            hash_del,
            list_push,
            list_set,
            list_pop,
            set_add,
            set_remove,
            zset_add,
            zset_remove,
            delete_keys,
            rename,
            set_ttl,
            batch_delete_pattern,
            batch_set_ttl,
            batch_rename_prefix,
            flush_db,
            flush_all,
            count_matching,
            info,
            memory_sample,
            slowlog_get,
            slowlog_reset,
            modules_list,
            exec,
            pubsub_subscribe,
            pubsub_unsubscribe,
            pubsub_publish,
            json_get,
            json_set,
            json_del,
        ])
        .build()
}
