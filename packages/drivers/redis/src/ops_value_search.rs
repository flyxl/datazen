//! Guarded value search for Redis (R6).
//!
//! `scan_values` runs a stateful, incremental SCAN task per connection session.
//! Each call processes a single batch (yielding between batches) and returns the
//! resume cursor plus incremental matches. Guard rails — `max_keys`,
//! `byte_budget`, `per_value_peek`, `count` — are clamped to hard caps so a
//! search can never saturate the server or read unbounded bytes. `scan_abort`
//! flips the active task's abort flag, which is checked before the next batch.
//!
//! v1.0 only peeks `string` values; aggregate types are skipped. Standalone and
//! sentinel sessions are supported; cluster is not (see driver meta + UI).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use datazen_driver_api::DriverError;
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Guard rails
// ---------------------------------------------------------------------------

pub(crate) const DEFAULT_MAX_KEYS: u64 = 50_000;
pub(crate) const HARD_MAX_KEYS: u64 = 200_000;
pub(crate) const DEFAULT_BYTE_BUDGET: u64 = 64 * 1024 * 1024;
pub(crate) const HARD_MAX_BYTE_BUDGET: u64 = 256 * 1024 * 1024;
pub(crate) const DEFAULT_PEEK: u64 = 4 * 1024;
pub(crate) const HARD_MAX_PEEK: u64 = 32 * 1024;
pub(crate) const DEFAULT_COUNT: u64 = 500;
pub(crate) const HARD_MAX_COUNT: u64 = 2_000;

/// Cumulative scan limits, already clamped to hard caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScanGuard {
    pub max_keys: u64,
    pub byte_budget: u64,
    pub per_value_peek: u64,
    pub count: u32,
}

fn field_u64(input: &JsonValue, camel: &str, snake: &str) -> Option<u64> {
    input
        .get(camel)
        .or_else(|| input.get(snake))
        .and_then(JsonValue::as_u64)
}

impl ScanGuard {
    /// Build a guard from raw (possibly `None` / out-of-range) inputs, clamping
    /// every field into `[1, HARD_MAX_*]` and defaulting missing values.
    pub(crate) fn clamped(
        max_keys: Option<u64>,
        byte_budget: Option<u64>,
        per_value_peek: Option<u64>,
        count: Option<u64>,
    ) -> ScanGuard {
        ScanGuard {
            max_keys: max_keys.unwrap_or(DEFAULT_MAX_KEYS).clamp(1, HARD_MAX_KEYS),
            byte_budget: byte_budget
                .unwrap_or(DEFAULT_BYTE_BUDGET)
                .clamp(1, HARD_MAX_BYTE_BUDGET),
            per_value_peek: per_value_peek
                .unwrap_or(DEFAULT_PEEK)
                .clamp(1, HARD_MAX_PEEK),
            count: count.unwrap_or(DEFAULT_COUNT).clamp(1, HARD_MAX_COUNT) as u32,
        }
    }

    pub(crate) fn from_input(input: &JsonValue) -> ScanGuard {
        ScanGuard::clamped(
            field_u64(input, "maxKeys", "max_keys"),
            field_u64(input, "byteBudget", "byte_budget"),
            field_u64(input, "perValuePeek", "per_value_peek"),
            field_u64(input, "count", "count"),
        )
    }
}

// ---------------------------------------------------------------------------
// Search mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchMode {
    Key,
    Value,
    All,
}

impl SearchMode {
    pub(crate) fn parse(raw: &str) -> SearchMode {
        match raw.trim().to_ascii_lowercase().as_str() {
            "value" => SearchMode::Value,
            "all" => SearchMode::All,
            _ => SearchMode::Key,
        }
    }
}

// ---------------------------------------------------------------------------
// Byte helpers (pure, unit-tested)
// ---------------------------------------------------------------------------

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Expand `\xNN` escapes into raw bytes; leave everything else verbatim.
pub(crate) fn unescape_needle(raw: &str) -> Vec<u8> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() + 1 && bytes.get(i + 1) == Some(&b'x') {
            if let (Some(h), Some(l)) = (bytes.get(i + 2), bytes.get(i + 3)) {
                if let (Some(hv), Some(lv)) = (hex_val(*h), hex_val(*l)) {
                    out.push(hv << 4 | lv);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// ASCII case-insensitive byte substring search (no UTF-8 validation).
pub(crate) fn contains_ignore_case(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() {
        return false;
    }
    let first = needle[0].to_ascii_lowercase();
    let last_start = hay.len() - needle.len();
    'outer: for i in 0..=last_start {
        if hay[i].to_ascii_lowercase() != first {
            continue;
        }
        for j in 1..needle.len() {
            if hay[i + j].to_ascii_lowercase() != needle[j].to_ascii_lowercase() {
                continue 'outer;
            }
        }
        return true;
    }
    false
}

/// Short, printable preview of the matched region (utf8-lossy, capped).
fn preview_around(data: &[u8], needle: &[u8]) -> String {
    const CAP: usize = 120;
    let start = if needle.is_empty() {
        0
    } else {
        // best-effort find first (case-insensitive) offset for context window
        let lower_needle: Vec<u8> = needle.iter().map(|b| b.to_ascii_lowercase()).collect();
        let lower_hay: Vec<u8> = data.iter().map(|b| b.to_ascii_lowercase()).collect();
        lower_hay
            .windows(lower_needle.len())
            .position(|w| w == &lower_needle[..])
            .unwrap_or(0)
    };
    let window_start = start.saturating_sub(24);
    let slice = &data[window_start..(window_start + CAP).min(data.len())];
    let mut s = String::from_utf8_lossy(slice).to_string();
    if window_start > 0 {
        s = format!("…{s}");
    }
    if window_start + CAP < data.len() {
        s.push('…');
    }
    s
}

fn bulk_bytes(v: &redis::Value) -> Vec<u8> {
    match v {
        redis::Value::BulkString(b) => b.clone(),
        redis::Value::VerbatimString { text, .. } => text.clone().into_bytes(),
        redis::Value::SimpleString(s) => s.clone().into_bytes(),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Match result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MatchHit {
    pub key: String,
    /// "key" | "value"
    pub matched_in: &'static str,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub(crate) struct BatchOutcome {
    pub matched: Vec<MatchHit>,
    pub next_cursor: u64,
    pub scanned: u64,
    pub bytes_used: u64,
}

// ---------------------------------------------------------------------------
// Session task registry (one active task per connection session)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ScanTask {
    task_id: String,
    abort: Arc<AtomicBool>,
    scanned: u64,
    bytes: u64,
}

struct TaskRegistry {
    tasks: Mutex<HashMap<String, ScanTask>>,
}

impl TaskRegistry {
    fn new() -> Self {
        TaskRegistry {
            tasks: Mutex::new(HashMap::new()),
        }
    }

    /// Resolve (or start) the task for a session. A different / missing
    /// `requested` id aborts the previous task and begins a fresh one.
    async fn resolve(&self, session: &str, requested: Option<&str>) -> ScanTask {
        let mut map = self.tasks.lock().await;
        if let Some(req) = requested {
            if let Some(existing) = map.get(session) {
                if existing.task_id == req {
                    return existing.clone();
                }
            }
        }
        if let Some(old) = map.get(session) {
            old.abort.store(true, Ordering::Relaxed);
        }
        let fresh = ScanTask {
            task_id: uuid::Uuid::new_v4().to_string(),
            abort: Arc::new(AtomicBool::new(false)),
            scanned: 0,
            bytes: 0,
        };
        map.insert(session.to_string(), fresh.clone());
        fresh
    }

    /// Fold a finished batch into the task totals; returns cumulative totals.
    async fn commit(
        &self,
        session: &str,
        task_id: &str,
        scanned_add: u64,
        bytes_add: u64,
    ) -> (u64, u64) {
        let mut map = self.tasks.lock().await;
        if let Some(t) = map.get_mut(session) {
            if t.task_id == task_id {
                t.scanned = t.scanned.saturating_add(scanned_add);
                t.bytes = t.bytes.saturating_add(bytes_add);
                return (t.scanned, t.bytes);
            }
        }
        (scanned_add, bytes_add)
    }

    async fn clear(&self, session: &str, task_id: &str) {
        let mut map = self.tasks.lock().await;
        if let Some(t) = map.get(session) {
            if t.task_id == task_id {
                map.remove(session);
            }
        }
    }

    async fn abort(&self, session: &str, requested: Option<&str>) -> bool {
        let map = self.tasks.lock().await;
        if let Some(t) = map.get(session) {
            if requested.map_or(true, |r| r == t.task_id) {
                t.abort.store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    }
}

fn registry() -> &'static TaskRegistry {
    static REGISTRY: OnceLock<TaskRegistry> = OnceLock::new();
    REGISTRY.get_or_init(TaskRegistry::new)
}

// ---------------------------------------------------------------------------
// Batch execution on a connection
// ---------------------------------------------------------------------------

/// Run one SCAN + pipelined peek batch. Does not touch the task registry.
pub(crate) async fn scan_values_on<C>(
    conn: &mut C,
    pattern: &str,
    needle: &[u8],
    mode: SearchMode,
    cursor: u64,
    guard: ScanGuard,
) -> Result<BatchOutcome, DriverError>
where
    C: redis::aio::ConnectionLike + Send,
{
    let (next_cursor, keys) =
        crate::ops::scan_batch(conn, cursor, guard.count, Some(pattern), None)
            .await
            .map_err(DriverError::QueryFailed)?;

    let mut matched: Vec<MatchHit> = Vec::new();
    let mut bytes_used: u64 = 0;

    // Value matching: pipeline TYPE + STRLEN + GETRANGE(peek) per key.
    if mode != SearchMode::Key && !keys.is_empty() {
        let peek_end = guard.per_value_peek.saturating_sub(1).max(0) as i64;
        let mut pipe = redis::pipe();
        for key in &keys {
            pipe.cmd("TYPE").arg(key);
            pipe.cmd("STRLEN").arg(key);
            pipe.cmd("GETRANGE").arg(key).arg(0).arg(peek_end);
        }
        let results: Vec<redis::Value> = pipe
            .query_async(conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        for (i, key) in keys.iter().enumerate() {
            let key_type = crate::redis_value_preview::value_to_string(&results[i * 3]);
            if key_type != "string" {
                continue;
            }
            let data = bulk_bytes(&results[i * 3 + 2]);
            bytes_used = bytes_used.saturating_add(data.len() as u64);
            if contains_ignore_case(&data, needle) {
                matched.push(MatchHit {
                    key: key.clone(),
                    matched_in: "value",
                    preview: preview_around(&data, needle),
                });
            }
        }
    }

    // Key-name matching.
    if mode != SearchMode::Value {
        let mut key_hits: Vec<MatchHit> = Vec::new();
        for key in &keys {
            if contains_ignore_case(key.as_bytes(), needle) {
                key_hits.push(MatchHit {
                    key: key.clone(),
                    matched_in: "key",
                    preview: key.clone(),
                });
            }
        }
        if mode == SearchMode::All {
            // Key hits first, then value hits.
            key_hits.extend(matched);
            matched = key_hits;
        } else {
            matched = key_hits;
        }
    }

    Ok(BatchOutcome {
        matched,
        next_cursor,
        scanned: keys.len() as u64,
        bytes_used,
    })
}

/// Orchestrate one incremental `scan_values` batch: resolve the session task,
/// check abort, run the batch, fold totals, and emit the response JSON.
pub(crate) async fn run_scan_batch<C>(
    conn: &mut C,
    session: &str,
    input: &JsonValue,
) -> Result<JsonValue, DriverError>
where
    C: redis::aio::ConnectionLike + Send,
{
    let pattern = input
        .get("pattern")
        .and_then(JsonValue::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("*");
    let query = input.get("query").and_then(JsonValue::as_str).unwrap_or("");
    let needle = unescape_needle(query);
    let mode = SearchMode::parse(
        input
            .get("mode")
            .and_then(JsonValue::as_str)
            .unwrap_or("value"),
    );
    let cursor = input.get("cursor").and_then(JsonValue::as_u64).unwrap_or(0);
    let guard = ScanGuard::from_input(input);
    let requested = input
        .get("taskId")
        .or_else(|| input.get("task_id"))
        .and_then(JsonValue::as_str);

    let task = registry().resolve(session, requested).await;

    if task.abort.load(Ordering::Relaxed) {
        registry().clear(session, &task.task_id).await;
        return Ok(serde_json::json!({
            "taskId": task.task_id,
            "done": true,
            "cancelled": true,
            "limitHit": false,
            "scannedKeys": task.scanned,
            "matched": [],
            "cursor": 0,
        }));
    }

    let batch = scan_values_on(conn, pattern, &needle, mode, cursor, guard).await?;
    let (scanned_total, bytes_total) = registry()
        .commit(session, &task.task_id, batch.scanned, batch.bytes_used)
        .await;

    let cancelled = task.abort.load(Ordering::Relaxed);
    let done = !cancelled && batch.next_cursor == 0;
    let limit_hit =
        !cancelled && (scanned_total >= guard.max_keys || bytes_total >= guard.byte_budget);
    if done || limit_hit || cancelled {
        registry().clear(session, &task.task_id).await;
    }

    // Yield between batches so a long poll loop never busy-spins.
    if !(done || limit_hit || cancelled) {
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }

    Ok(serde_json::json!({
        "taskId": task.task_id,
        "done": done,
        "cancelled": cancelled,
        "limitHit": limit_hit,
        "scannedKeys": scanned_total,
        "matched": batch.matched,
        "cursor": batch.next_cursor,
    }))
}

/// Set the abort flag for a session's active task (idempotent).
pub(crate) async fn abort_task(session: &str, requested: Option<&str>) -> JsonValue {
    let aborted = registry().abort(session, requested).await;
    serde_json::json!({ "aborted": aborted })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_clamps_to_bounds() {
        let g = ScanGuard::clamped(
            Some(999_999_999),
            Some(u64::MAX),
            Some(1_000_000),
            Some(100_000),
        );
        assert_eq!(g.max_keys, HARD_MAX_KEYS);
        assert_eq!(g.byte_budget, HARD_MAX_BYTE_BUDGET);
        assert_eq!(g.per_value_peek, HARD_MAX_PEEK);
        assert_eq!(g.count as u64, HARD_MAX_COUNT);

        let d = ScanGuard::clamped(None, None, None, None);
        assert_eq!(d.max_keys, DEFAULT_MAX_KEYS);
        assert_eq!(d.byte_budget, DEFAULT_BYTE_BUDGET);
        assert_eq!(d.per_value_peek, DEFAULT_PEEK);
        assert_eq!(d.count as u64, DEFAULT_COUNT);

        // zero clamps up to 1, never 0
        let z = ScanGuard::clamped(Some(0), Some(0), Some(0), Some(0));
        assert_eq!(z.max_keys, 1);
        assert_eq!(z.count, 1);
    }

    #[test]
    fn unescape_expands_hex_bytes() {
        assert_eq!(unescape_needle("abc"), b"abc".to_vec());
        assert_eq!(unescape_needle("\\x00\\xff"), vec![0x00, 0xff]);
        assert_eq!(unescape_needle("a\\x41b"), vec![b'a', 0x41, b'b']);
        // malformed escape stays literal
        assert_eq!(unescape_needle("\\xzz"), b"\\xzz".to_vec());
    }

    #[test]
    fn contains_ignore_case_matches_ascii() {
        assert!(contains_ignore_case(b"Hello WORLD", b"lo wo"));
        assert!(contains_ignore_case(b"ABC", b"abc"));
        assert!(!contains_ignore_case(b"ABC", b"abcd"));
        assert!(contains_ignore_case(b"anything", b""));
        // raw \xNN bytes match too
        assert!(contains_ignore_case(&[0x00, 0xFF, 0x10], &[0xFF]));
    }

    #[tokio::test]
    async fn abort_is_idempotent_and_replaces_old_task() {
        let reg = TaskRegistry::new();
        let t1 = reg.resolve("s", None).await;
        // aborting twice is fine
        assert!(reg.abort("s", Some(&t1.task_id)).await);
        assert!(reg.abort("s", Some(&t1.task_id)).await);
        assert!(t1.abort.load(Ordering::Relaxed));
        // starting a new task aborts the old one and gets a fresh id
        let t2 = reg.resolve("s", None).await;
        assert_ne!(t1.task_id, t2.task_id);
        assert!(!t2.abort.load(Ordering::Relaxed));
        // requesting with the live id continues, not replaces
        let t3 = reg.resolve("s", Some(&t2.task_id)).await;
        assert_eq!(t2.task_id, t3.task_id);
    }

    #[tokio::test]
    async fn commit_accumulates_until_task_replaced() {
        let reg = TaskRegistry::new();
        let t = reg.resolve("s2", None).await;
        let (scanned, bytes) = reg.commit("s2", &t.task_id, 10, 100).await;
        assert_eq!((scanned, bytes), (10, 100));
        let (scanned2, _) = reg.commit("s2", &t.task_id, 5, 0).await;
        assert_eq!(scanned2, 15);
        // a stale task id (already replaced) starts its own count
        let (scanned3, _) = reg.commit("s2", "stale-id", 3, 0).await;
        assert_eq!(scanned3, 3);
    }
}
