//! Typed Redis tool jobs and session-owned stream lifetimes.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
};

use datazen_driver_api::{ConnectionHandle, DatabaseDriver, DriverEvent, DriverEventSink};
use datazen_ui_api::native_kv::KvTool;
use serde_json::{json, Value};

use super::{call, KvSessionState, LiveSession};

#[path = "native_kv_archive.rs"]
mod archive;
#[path = "native_kv_overview.rs"]
mod overview;

#[derive(Clone)]
pub(super) struct Search {
    db: u32,
    query: String,
    pattern: String,
    task: Option<String>,
    cursor: u64,
}

const MAX_EVENTS: usize = 100;

#[derive(Default)]
struct Messages(Mutex<VecDeque<Value>>);

impl DriverEventSink for Messages {
    fn emit(&self, event: &DriverEvent) {
        if event.name != "redis-pubsub-message" {
            return;
        }
        if let Ok(mut messages) = self.0.try_lock() {
            if messages.len() == MAX_EVENTS {
                messages.pop_front();
            }
            let p = &event.payload;
            let limit = |key: &str| {
                p[key]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(1024)
                    .collect::<String>()
            };
            messages.push_back(json!({
                "connectionId": p["connectionId"],
                "subscriptionId": p["subscriptionId"],
                "channel": limit("channel"),
                "payload": limit("payload"),
            }));
        }
    }
}

fn messages() -> &'static Arc<Messages> {
    static SINK: OnceLock<Arc<Messages>> = OnceLock::new();
    SINK.get_or_init(|| {
        let sink = Arc::new(Messages::default());
        for outlet in datazen_driver_api::iter_event_outlets() {
            if outlet.outlet_id() == "redis-pubsub" {
                outlet.install(sink.clone());
            }
        }
        sink
    })
}

fn target(
    live: &Arc<Mutex<LiveSession>>,
) -> Result<std::sync::MutexGuard<'_, LiveSession>, String> {
    live.lock()
        .map_err(|_| "The Redis session is unavailable.".into())
}

fn json_output(value: &Value) -> String {
    let all = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    let mut chars = all.chars();
    let mut preview: String = chars.by_ref().take(8192).collect();
    if chars.next().is_some() {
        preview.push_str("\n… [output truncated]");
    }
    preview
}

fn read_console(command: &str) -> Result<&str, String> {
    let command = command.trim();
    if command.len() > 4096 || command.chars().any(char::is_control) {
        return Err("Enter one Redis command, at most 4096 bytes.".into());
    }
    let verb = command
        .split_ascii_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    // Unknown commands are denied, not assumed safe by a display classifier.
    if !datazen_ui_api::native_kv::REDIS_CONSOLE_READ_COMMANDS.contains(&verb.as_str()) {
        return Err(
            "This console accepts read-only commands such as PING, GET, TYPE, TTL and HGET.".into(),
        );
    }
    Ok(command)
}

fn console_batch(commands: &[String]) -> Result<String, String> {
    if commands.is_empty() || commands.len() > 8 {
        return Err("Queue one to eight read-only commands.".into());
    }
    let commands: Vec<_> = commands
        .iter()
        .map(|cmd| read_console(cmd))
        .collect::<Result<_, _>>()?;
    let batch = commands.join("\n");
    if batch.len() > 8192 {
        return Err("The queued commands exceed 8 KiB.".into());
    }
    Ok(batch)
}

pub(super) async fn stop_owned(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    state: KvSessionState,
) {
    if let Some(task) = state.search.and_then(|s| s.task) {
        let _ = call(driver, handle, "scan_abort", json!({"taskId":task})).await;
    }
    if let Some(id) = state.subscription {
        let _ = call(
            driver,
            handle,
            "pubsub_unsubscribe",
            json!({"subscriptionId": id}),
        )
        .await;
    }
    if let Some(id) = state.monitor {
        let _ = call(driver, handle, "monitor_stop", json!({"monitorId": id})).await;
    }
}

pub(super) async fn run(
    live: &Arc<Mutex<LiveSession>>,
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    db: u32,
    writable: bool,
    connection: &str,
    op: &KvTool,
) -> Result<String, String> {
    let (command, args) = match op {
        KvTool::Console(text) => (
            "exec",
            json!({"dbIndex": db, "commands": read_console(text)?}),
        ),
        KvTool::ConsoleBatch(commands) => (
            "exec",
            json!({"dbIndex":db, "commands":console_batch(commands)?}),
        ),
        KvTool::Overview => return overview::overview(driver, handle).await,
        KvTool::MemorySample => ("memory_sample", json!({"dbIndex":db,"limit":32})),
        KvTool::Info => ("info", json!({})),
        KvTool::SlowLog => ("slowlog_get", json!({"count": 16})),
        KvTool::ClusterNodes => ("cluster_nodes", json!({})),
        KvTool::KeyProperties(key) => ("key_object_info", json!({"dbIndex":db,"key":key})),
        KvTool::DecodeString { key, codec } => {
            let frame = call(
                driver,
                handle,
                "get_key_raw",
                json!({"dbIndex":db,"key":key}),
            )
            .await?;
            if frame["keyType"] != "string" || frame["truncated"] == true {
                return Err("Decode requires a complete Redis String.".into());
            }
            let raw = frame["rawB64"]
                .as_str()
                .filter(|raw| raw.len() <= 5464)
                .ok_or("Native decoding accepts raw values up to 4 KiB.")?;
            ("decode_value", json!({"codec":codec.token(),"data":raw}))
        }
        KvTool::StreamGroups(key) => ("xinfo_groups", json!({"dbIndex":db,"key":key})),
        KvTool::StreamConsumers { key, group } => {
            require_stream_group(key, group)?;
            (
                "xinfo_consumers",
                json!({"dbIndex":db,"key":key,"group":group}),
            )
        }
        KvTool::StreamPending { key, group } => {
            require_stream_group(key, group)?;
            (
                "xpending",
                json!({"dbIndex":db,"key":key,"group":group,"count":32}),
            )
        }
        KvTool::StreamCreateGroup { key, group } => {
            require_writable(writable, key)?;
            require_stream_group(key, group)?;
            let result = call(
                driver,
                handle,
                "xgroup_create",
                json!({"dbIndex":db,"key":key,"group":group,"existingStreamOnly":true}),
            )
            .await?;
            return applied(
                &result,
                "Consumer group created from the first entry.",
                "The group exists or the Stream changed.",
            );
        }
        KvTool::StreamAck { key, group, entry } => {
            require_writable(writable, key)?;
            require_stream_group(key, group)?;
            let (ms, sequence) = entry
                .split_once('-')
                .ok_or("Enter the exact pending ID (milliseconds-sequence).")?;
            if ![ms, sequence].iter().all(|part| {
                !part.is_empty()
                    && part.bytes().all(|b| b.is_ascii_digit())
                    && part.parse::<u64>().is_ok()
            }) {
                return Err("Enter the exact pending ID (milliseconds-sequence).".into());
            }
            let result = call(
                driver,
                handle,
                "xack",
                json!({"dbIndex":db,"key":key,"group":group,"ids":[entry]}),
            )
            .await?;
            return if result.as_u64() == Some(1) {
                Ok("Acknowledged one pending Stream entry.".into())
            } else {
                Err("Redis did not acknowledge a pending entry for that ID.".into())
            };
        }
        KvTool::SearchStart { query, pattern } => {
            if query.trim().is_empty() || query.len() > 256 || pattern.len() > 512 {
                return Err("Enter a value substring of at most 256 bytes.".into());
            }
            stop_search(live, driver, handle).await?;
            return search_batch(
                live,
                driver,
                handle,
                Search {
                    db,
                    query: query.clone(),
                    pattern: pattern.clone(),
                    task: None,
                    cursor: 0,
                },
            )
            .await;
        }
        KvTool::SearchNext => {
            let search = target(live)?
                .kv_state
                .search
                .clone()
                .ok_or("No further value-search results.")?;
            if search.db != db {
                return Err("Switch back to the search's logical database first.".into());
            }
            return search_batch(live, driver, handle, search).await;
        }
        KvTool::SearchStop => {
            stop_search(live, driver, handle).await?;
            return Ok("Value search stopped.".into());
        }

        KvTool::MonitorStart => {
            if target(live)?.kv_state.monitor.is_some() {
                return Err("Monitor is already running.".into());
            }
            let response = call(
                driver,
                handle,
                "monitor_start",
                json!({"bufferSize": MAX_EVENTS}),
            )
            .await?;
            let id = response["monitorId"]
                .as_str()
                .ok_or("Redis did not confirm monitor start.")?;
            target(live)?.kv_state.monitor = Some(id.into());
            return Ok("Monitor started. Use Read monitor to refresh.".into());
        }
        KvTool::MonitorRead => {
            let id = target(live)?
                .kv_state
                .monitor
                .clone()
                .ok_or("Start monitoring first.")?;
            ("monitor_get_buffer", json!({"monitorId": id}))
        }
        KvTool::MonitorStop => {
            let id = target(live)?
                .kv_state
                .monitor
                .clone()
                .ok_or("No monitor is running.")?;
            call(driver, handle, "monitor_stop", json!({"monitorId": id})).await?;
            target(live)?.kv_state.monitor = None;
            return Ok("Monitor stopped.".into());
        }
        KvTool::Subscribe { channels, patterns } => {
            let names = |text: &str| -> Vec<String> {
                text.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            };
            let channels = names(&channels);
            let patterns = names(&patterns);
            let count = channels.len() + patterns.len();
            if count == 0 || count > 8 || channels.iter().chain(&patterns).any(|s| s.len() > 256) {
                return Err(
                    "Enter one to eight comma-separated channels or patterns, each at most 256 bytes."
                        .into(),
                );
            }
            if target(live)?.kv_state.subscription.is_some() {
                return Err("Unsubscribe before changing channels.".into());
            }
            let _ = messages();
            let data = call(
                driver,
                handle,
                "pubsub_subscribe",
                json!({"channels": channels, "patterns": patterns}),
            )
            .await?;
            let id = data
                .as_str()
                .ok_or("Redis did not confirm the subscription.")?;
            target(live)?.kv_state.subscription = Some(id.into());
            return Ok("Subscribed. Use Read messages to refresh.".into());
        }
        KvTool::Messages => {
            let id = target(live)?
                .kv_state
                .subscription
                .clone()
                .ok_or("Subscribe first.")?;
            let messages = messages()
                .0
                .lock()
                .map_err(|_| "Redis messages are unavailable.")?;
            let rows: Vec<Value> = messages
                .iter()
                .filter(|x| x["connectionId"] == connection && x["subscriptionId"] == id)
                .cloned()
                .collect();
            return Ok(json_output(&json!(rows)));
        }
        KvTool::Unsubscribe => {
            let id = target(live)?
                .kv_state
                .subscription
                .clone()
                .ok_or("No subscription is active.")?;
            call(
                driver,
                handle,
                "pubsub_unsubscribe",
                json!({"subscriptionId": id}),
            )
            .await?;
            target(live)?.kv_state.subscription = None;
            return Ok("Unsubscribed.".into());
        }
        KvTool::Export { keys, path } => {
            return archive::export(driver, handle, db, keys, path).await
        }
        KvTool::Import { path } => {
            if !writable {
                return Err("This Redis connection is read only.".into());
            }
            return archive::import(driver, handle, db, path).await;
        }
        KvTool::CreateString { key, value } => {
            require_writable(writable, key)?;
            if value.len() > 4096 {
                return Err("String value is too large.".into());
            }
            let data = call(
                driver,
                handle,
                "set_string",
                json!({"dbIndex":db,"key":key,"value":value,"createOnly":true}),
            )
            .await?;
            return applied(
                &data,
                "Key created.",
                "A key with that name already exists.",
            );
        }
        KvTool::RenameKey {
            key,
            target: new_key,
        } => {
            require_writable(writable, key)?;
            require_writable(writable, new_key)?;
            if key == new_key {
                return Err("Choose a different key name.".into());
            }
            let data = call(
                driver,
                handle,
                "rename",
                json!({"dbIndex":db,"key":key,"newKey":new_key,"noOverwrite":true}),
            )
            .await?;
            return applied(&data, "Key renamed.", "The target key already exists.");
        }
        KvTool::DeleteMarked { keys, confirmation } => {
            let count = keys.len();
            if !(2..=128).contains(&count) || confirmation != &format!("DELETE {count}") {
                return Err(
                    "Type the displayed DELETE followed by the exact number of marked keys.".into(),
                );
            }
            let mut unique = std::collections::HashSet::new();
            for key in keys {
                require_writable(writable, key)?;
                if !unique.insert(key) {
                    return Err("Review the marked list before deleting: duplicate key.".into());
                }
            }
            let (mut removed, mut retained) = (0, 0);
            for key in keys {
                // One exact key per operation also works across Cluster slots.
                match call(
                    driver,
                    handle,
                    "delete_keys",
                    json!({"dbIndex":db,"keys":[key]}),
                )
                .await
                {
                    Ok(data) if data.as_u64() == Some(1) => removed += 1,
                    Ok(data) if data.as_u64() == Some(0) => (),
                    _ => retained += 1,
                }
            }
            return Ok(format!("Deleted {removed} of {count} marked keys; {retained} requests did not confirm deletion."));
        }
        KvTool::DeleteKey { key, confirmation } => {
            require_writable(writable, key)?;
            if key != confirmation {
                return Err("Type the exact selected key name to delete it.".into());
            }
            let data = call(
                driver,
                handle,
                "delete_keys",
                json!({"dbIndex":db,"keys":[key]}),
            )
            .await?;
            return if data.as_u64() == Some(1) {
                Ok("Key deleted.".into())
            } else {
                Err("Redis did not delete the selected key.".into())
            };
        }
        KvTool::Publish { channel, message } => {
            if !writable {
                return Err("This Redis connection is read only.".into());
            }
            let channel = channel.trim();
            if channel.is_empty() || channel.len() > 256 || message.len() > 4096 {
                return Err("Enter a channel and a message no larger than 4096 bytes.".into());
            }
            (
                "pubsub_publish",
                json!({"channel": channel, "message": message}),
            )
        }
    };
    let data = call(driver, handle, command, args).await?;
    Ok(json_output(&data))
}

async fn stop_search(
    live: &Arc<Mutex<LiveSession>>,
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
) -> Result<(), String> {
    let task = target(live)?.kv_state.search.clone().and_then(|s| s.task);
    if let Some(task) = task {
        call(driver, handle, "scan_abort", json!({"taskId":task})).await?;
    }
    target(live)?.kv_state.search = None;
    Ok(())
}

async fn search_batch(
    live: &Arc<Mutex<LiveSession>>,
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    mut search: Search,
) -> Result<String, String> {
    let data = call(driver, handle, "scan_values", json!({
        "dbIndex":search.db,"query":search.query,"pattern":search.pattern,"mode":"value",
        "taskId":search.task,"cursor":search.cursor,"maxKeys":2048,"byteBudget":524288,"perValuePeek":4096,"count":64,
    })).await?;
    search.task = data["taskId"].as_str().map(str::to_owned);
    search.cursor = data["cursor"]
        .as_u64()
        .ok_or("Invalid Redis value-search cursor.")?;
    let more = search.cursor != 0
        && search.task.is_some()
        && data["limitHit"] != true
        && data["cancelled"] != true;
    target(live)?.kv_state.search = more.then_some(search);
    let prefix = if more {
        "More batches are available."
    } else {
        "Value search finished."
    };
    Ok(format!("{prefix}\n{}", json_output(&data)))
}

fn require_stream_group(key: &str, group: &str) -> Result<(), String> {
    if key.is_empty()
        || key.trim() != key
        || group.is_empty()
        || group.trim() != group
        || group.len() > 256
        || group.chars().any(char::is_control)
    {
        return Err("Enter the exact Stream and group without surrounding whitespace (group at most 256 bytes).".into());
    }
    Ok(())
}

fn require_writable(writable: bool, key: &str) -> Result<(), String> {
    if !writable {
        return Err("This Redis connection is read only.".into());
    }
    if key.is_empty() || key.len() > 1024 || key.chars().any(char::is_control) {
        return Err(
            "Enter a key name between 1 and 1024 bytes, without control characters.".into(),
        );
    }
    Ok(())
}

fn applied(data: &Value, success: &str, conflict: &str) -> Result<String, String> {
    match data["applied"].as_bool() {
        Some(true) => Ok(success.into()),
        Some(false) => Err(conflict.into()),
        None => Err("Redis did not confirm the operation.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_batches_are_validated_before_dispatch() {
        assert_eq!(
            console_batch(&["PING".into(), "GET k".into()]).unwrap(),
            "PING\nGET k"
        );
        assert!(console_batch(&["PING".into(), "SET k value".into()]).is_err());
        assert!(console_batch(&[]).is_err());
        assert!(console_batch(&vec!["PING".into(); 9]).is_err());
    }

    #[test]
    fn console_fails_closed_for_unknown_commands_and_multiple_lines() {
        assert!(read_console("GET test").is_ok());
        for text in [
            "EVAL x",
            "UNLINK test",
            "FLUSHALL",
            "GET key\nSET key value",
            "",
        ] {
            assert!(read_console(text).is_err(), "{text}");
        }
    }
}
