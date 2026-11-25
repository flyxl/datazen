//! Redis MONITOR realtime capture — dedicated connection, circular buffer, start/stop lifecycle.

use std::collections::VecDeque;
use std::sync::{Arc, OnceLock};

use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::connect::ConnectionPlan;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single MONITOR event parsed from the Redis streaming response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorEvent {
    /// Unix timestamp in seconds with fractional milliseconds.
    pub timestamp: f64,
    /// Database index selected by the client.
    pub db: u32,
    /// Client address (ip:port).
    pub client_addr: String,
    /// Client name.
    pub client_name: String,
    /// Command executed.
    pub command: String,
    /// Command arguments.
    pub args: Vec<String>,
}

/// Handle returned by `start_monitor` — caller stores this to poll or stop.
pub struct MonitorHandle {
    #[allow(dead_code)]
    pub monitor_id: String,
    buffer: Arc<Mutex<VecDeque<MonitorEvent>>>,
    handle: JoinHandle<()>,
    stop_tx: tokio::sync::oneshot::Sender<()>,
}

// ---------------------------------------------------------------------------
// Global registry
// ---------------------------------------------------------------------------

struct MonitorRegistry {
    active: std::collections::HashMap<String, MonitorHandle>,
    next_id: u64,
}

static REGISTRY: OnceLock<Mutex<MonitorRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<MonitorRegistry> {
    REGISTRY.get_or_init(|| {
        Mutex::new(MonitorRegistry {
            active: std::collections::HashMap::new(),
            next_id: 1,
        })
    })
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Start a MONITOR session on a dedicated connection.
///
/// `plan` provides the connection parameters; a fresh connection is opened
/// specifically for MONITOR so it doesn't interfere with normal operations.
/// `max_buffer_size` caps the circular event buffer (default 1000).
pub async fn start_monitor(
    plan: &ConnectionPlan,
    max_buffer_size: Option<usize>,
) -> Result<String, String> {
    let buf_size = max_buffer_size.unwrap_or(1000).clamp(10, 10_000);

    let mut monitor = crate::connect::live::open_monitor_connection(plan)
        .await
        .map_err(|error| format!("failed to open MONITOR connection: {error}"))?;
    monitor
        .monitor()
        .await
        .map_err(|error| format!("MONITOR command failed: {error}"))?;
    let events = monitor.into_on_message::<String>();

    let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(buf_size)));
    let buffer_clone = buffer.clone();
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn the reader task that reads MONITOR streaming responses.
    let handle = tokio::spawn(async move {
        tokio::pin!(events);
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                line = events.next() => {
                    let Some(line) = line else { break; };
                    if let Some(mut event) = parse_monitor_line(&line) {
                        event.args.truncate(16);
                        for argument in &mut event.args {
                            if let Some((end, _)) = argument.char_indices().nth(1024) {
                                argument.truncate(end);
                                argument.push('…');
                            }
                        }
                        let mut guard = buffer_clone.lock().await;
                        if guard.len() >= buf_size { guard.pop_front(); }
                        guard.push_back(event);
                    }
                }
            }
        }
    });

    let mut reg = registry().lock().await;
    let monitor_id = format!("monitor_{}", reg.next_id);
    reg.next_id += 1;
    reg.active.insert(
        monitor_id.clone(),
        MonitorHandle {
            monitor_id: monitor_id.clone(),
            buffer,
            handle,
            stop_tx,
        },
    );

    Ok(monitor_id)
}

/// Stop an active MONITOR session.
pub async fn stop_monitor(monitor_id: &str) -> Result<(), String> {
    let mut reg = registry().lock().await;
    let entry = reg
        .active
        .remove(monitor_id)
        .ok_or_else(|| format!("monitor not found: {monitor_id}"))?;
    let _ = entry.stop_tx.send(());
    entry.handle.abort();
    Ok(())
}

/// Retrieve buffered MONITOR events.
pub async fn get_monitor_buffer(monitor_id: &str) -> Result<Vec<MonitorEvent>, String> {
    let reg = registry().lock().await;
    let entry = reg
        .active
        .get(monitor_id)
        .ok_or_else(|| format!("monitor not found: {monitor_id}"))?;
    let guard = entry.buffer.lock().await;
    Ok(guard.iter().cloned().collect())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Parse a MONITOR output line into a MonitorEvent.
///
/// Format: `<timestamp> [<db> <addr>] "cmd" [args...]`
pub fn parse_monitor_line(line: &str) -> Option<MonitorEvent> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // Extract timestamp
    let (ts_str, rest) = line.split_once(' ')?;
    let timestamp: f64 = ts_str.parse().ok()?;

    // Extract db and client from `[<db> <addr>]`
    let rest = rest.trim_start();
    let (db_addr, cmd_part) = if rest.starts_with('[') {
        let close = rest.find(']')?;
        let inside = &rest[1..close];
        let cmd_part = rest[close + 1..].trim();
        (inside, cmd_part)
    } else {
        ("0 0.0.0.0:0", rest)
    };

    let mut db_addr_parts = db_addr.splitn(2, ' ');
    let db: u32 = db_addr_parts.next()?.parse().unwrap_or(0);
    let client_addr = db_addr_parts.next().unwrap_or("0.0.0.0:0").to_string();

    // Parse the command and args from quoted strings
    let parts = split_quoted_args(cmd_part);
    if parts.is_empty() {
        return None;
    }

    let command = parts[0].clone();
    let args = parts[1..].to_vec();

    Some(MonitorEvent {
        timestamp,
        db,
        client_addr,
        client_name: String::new(),
        command,
        args,
    })
}

/// Split a quoted Redis command string into arguments.
fn split_quoted_args(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;

    for ch in input.chars() {
        match ch {
            '"' if in_quote => {
                result.push(current.clone());
                current.clear();
                in_quote = false;
            }
            '"' => {
                in_quote = true;
            }
            ' ' if !in_quote => {
                if !current.is_empty() {
                    result.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_monitor_line_basic() {
        let line = r#"1234567890.123 [0 127.0.0.1:12345] "GET" "foo""#;
        let event = parse_monitor_line(line).unwrap();
        assert_eq!(event.timestamp, 1234567890.123);
        assert_eq!(event.db, 0);
        assert_eq!(event.client_addr, "127.0.0.1:12345");
        assert_eq!(event.command, "GET");
        assert_eq!(event.args, vec!["foo"]);
    }

    #[test]
    fn parse_monitor_line_no_args() {
        let line = r#"1234567890.123 [0 127.0.0.1:12345] "PING""#;
        let event = parse_monitor_line(line).unwrap();
        assert_eq!(event.command, "PING");
        assert!(event.args.is_empty());
    }

    #[test]
    fn parse_monitor_line_multiple_args() {
        let line = r#"1234567890.123 [0 127.0.0.1:12345] "HSET" "user:1" "name" "Alice""#;
        let event = parse_monitor_line(line).unwrap();
        assert_eq!(event.command, "HSET");
        assert_eq!(event.args, vec!["user:1", "name", "Alice"]);
    }

    #[test]
    fn parse_monitor_line_empty() {
        assert!(parse_monitor_line("").is_none());
        assert!(parse_monitor_line("   ").is_none());
    }

    #[test]
    fn split_quoted_args_basic() {
        assert_eq!(
            split_quoted_args(r#""GET" "foo" "bar""#),
            vec!["GET", "foo", "bar"]
        );
    }

    #[test]
    fn split_quoted_args_empty() {
        assert_eq!(split_quoted_args(""), Vec::<String>::new());
    }

    #[test]
    fn split_quoted_args_single() {
        assert_eq!(split_quoted_args(r#""PING""#), vec!["PING"]);
    }
}
