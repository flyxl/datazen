// Unit tests for `ops::workbench` — declared by `ops/workbench/mod.rs`, which
// holds `#[cfg(test)] mod tests;`. Kept in its own file so the implementation
// module stays readable; `ScriptedConn` is a scripted `ConnectionLike` that
// journals every request, which is what lets these tests assert the round-trip
// shape (one pipeline per key, one TYPE pipeline per 500 keys, no KEYS) without
// a live Redis.

use super::*;

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use futures_util::FutureExt;
use redis::RedisFuture;

/// A RESP error line parsed into a `Value` — `ServerError` itself is not
/// nameable in redis 0.27, and the parser is how a real reply arrives.
fn err_reply(detail: &str) -> RValue {
    let line = format!("-ERR {detail}\r\n");
    match redis::parse_redis_value(line.as_bytes()) {
        Ok(v) => v,
        Err(e) => panic!("test fixture produced an invalid error reply: {e}"),
    }
}

/// A bulk-string reply — the shape scalar answers actually arrive in.
fn bulk(s: &str) -> RValue {
    RValue::BulkString(s.as_bytes().to_vec())
}

/// Replay journal: what the connection was asked to do.
#[derive(Clone, Debug, Default, PartialEq)]
struct Journal {
    /// Commands sent one at a time (`req_packed_command`).
    singles: Vec<Vec<String>>,
    /// Pipelines sent (`req_packed_commands`), one vector per batch.
    batches: Vec<Vec<Vec<String>>>,
}

impl Journal {
    fn total(&self) -> usize {
        self.singles.len() + self.batches.len()
    }
    fn single_names(&self) -> Vec<String> {
        self.singles
            .iter()
            .map(|c| c.first().cloned().unwrap_or_default())
            .collect()
    }
    /// How many one-at-a-time commands were issued with this name — used to
    /// prove e.g. that DBSIZE is read once and SCAN stops at the window.
    fn count_single(&self, name: &str) -> usize {
        self.single_names()
            .iter()
            .filter(|n| n.as_str() == name)
            .count()
    }
    fn flat(&self) -> Vec<Vec<String>> {
        self.singles
            .iter()
            .cloned()
            .chain(self.batches.iter().flatten().cloned())
            .collect()
    }
    /// How many commands were issued for one logical probe, i.e. round trips.
    fn round_trips(&self) -> usize {
        self.total()
    }
}

/// Scripted in-memory stand-in for a `redis::aio::ConnectionLike`.
///
/// Replies are queued per command name (FIFO, case-insensitive), so a
/// pipeline of six `OBJECT`/`MEMORY`/`PTTL`/`TYPE` commands reads them back
/// in the order they were queued. Every request is journaled so tests can
/// assert the round-trip shape.
#[derive(Clone)]
struct ScriptedConn {
    queues: Arc<Mutex<HashMap<String, VecDeque<RValue>>>>,
    journal: Arc<Mutex<Journal>>,
    db: i64,
}

fn args_of(cmd: &redis::Cmd) -> Vec<String> {
    cmd.args_iter()
        .map(|arg| match arg {
            redis::Arg::Simple(bytes) => String::from_utf8_lossy(bytes).into_owned(),
            redis::Arg::Cursor => "@cursor".into(),
        })
        .collect()
}

/// Journal form of a request: servers treat the command name
/// case-insensitively, so only that token is normalized. Arguments are kept
/// verbatim — a journaled `SCAN 0 COUNT 500` must still read as one.
fn normalized(args: &[String]) -> Vec<String> {
    let mut entry = args.to_vec();
    if let Some(name) = entry.first_mut() {
        *name = name.to_ascii_uppercase();
    }
    entry
}

impl ScriptedConn {
    fn new() -> Self {
        Self {
            queues: Arc::new(Mutex::new(HashMap::new())),
            journal: Arc::new(Mutex::new(Journal::default())),
            db: 0,
        }
    }

    /// Queue one reply for the next command named `name`.
    fn push(&mut self, name: &str, value: RValue) -> &mut Self {
        let key = name.to_ascii_uppercase();
        self.queues
            .lock()
            .expect("reply queue lock")
            .entry(key)
            .or_default()
            .push_back(value);
        self
    }

    /// Queue one `SCAN` reply: `[cursor, [keys]]`.
    fn push_scan(&mut self, cursor: u64, keys: &[&str]) -> &mut Self {
        let items: Vec<RValue> = keys
            .iter()
            .map(|k| RValue::BulkString(k.as_bytes().to_vec()))
            .collect();
        self.push(
            "SCAN",
            RValue::Array(vec![
                RValue::BulkString(cursor.to_string().into_bytes()),
                RValue::Array(items),
            ]),
        )
    }

    /// Queue a batch of SCAN replies from owned key lists.
    fn push_scan_batches(&mut self, batches: &[(u64, Vec<String>)]) -> &mut Self {
        for (cursor, keys) in batches {
            let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
            self.push_scan(*cursor, &refs);
        }
        self
    }

    fn push_type(&mut self, ty: &str) -> &mut Self {
        self.push("TYPE", RValue::BulkString(ty.as_bytes().to_vec()))
    }

    fn push_types(&mut self, types: &[&str]) -> &mut Self {
        for ty in types {
            self.push_type(ty);
        }
        self
    }

    fn push_int(&mut self, name: &str, n: i64) -> &mut Self {
        self.push(name, RValue::Int(n))
    }

    fn push_str(&mut self, name: &str, s: &str) -> &mut Self {
        self.push(name, RValue::BulkString(s.as_bytes().to_vec()))
    }

    fn take_reply(&self, name: &str) -> RValue {
        self.queues
            .lock()
            .expect("reply queue lock")
            .get_mut(&name.to_ascii_uppercase())
            .and_then(VecDeque::pop_front)
            .unwrap_or(RValue::Nil)
    }

    fn journal(&self) -> Journal {
        self.journal.lock().expect("journal lock").clone()
    }
}

impl ConnectionLike for ScriptedConn {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a redis::Cmd) -> RedisFuture<'a, RValue> {
        let args = args_of(cmd);
        let name = args.first().cloned().unwrap_or_default();
        (async move {
            self.journal
                .lock()
                .expect("journal lock")
                .singles
                .push(normalized(&args));
            Ok(self.take_reply(&name))
        })
        .boxed()
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        pipe: &'a redis::Pipeline,
        _offset: usize,
        _count: usize,
    ) -> RedisFuture<'a, Vec<RValue>> {
        let batch: Vec<Vec<String>> = pipe.cmd_iter().map(args_of).collect();
        (async move {
            let mut values = Vec::with_capacity(batch.len());
            let mut journaled = Vec::with_capacity(batch.len());
            for args in &batch {
                let name = args.first().cloned().unwrap_or_default();
                values.push(self.take_reply(&name));
                journaled.push(normalized(args));
            }
            self.journal
                .lock()
                .expect("journal lock")
                .batches
                .push(journaled);
            Ok(values)
        })
        .boxed()
    }

    fn get_db(&self) -> i64 {
        self.db
    }
}

/// A single-node connection has exactly one node, so the address is vacuous and
/// the default implementation (send it) is the honest one. Every journal
/// assertion below keeps working unchanged; the cluster double overrides this.
impl SlotRoutedConnection for ScriptedConn {}

// 跨主题共用的 reply fixture：`fix_round1` 经 `use super::*;` 取用，故与
// `ShortReplyConn` 一起留在本文件。
fn full_key_info_replies() -> Vec<RValue> {
    vec![
        RValue::Int(104),                       // MEMORY USAGE
        RValue::BulkString(b"embstr".to_vec()), // OBJECT ENCODING
        RValue::Int(37),                        // OBJECT IDLETIME
        RValue::Int(9),                         // OBJECT FREQ
        RValue::Int(12_000),                    // PTTL
        RValue::BulkString(b"string".to_vec()), // TYPE
    ]
}

/// A pipeline that answers with fewer replies than it was given commands,
/// i.e. a desynced / truncated transport read. `ScriptedConn` always answers
/// one reply per command, so it cannot reach that branch.
struct ShortReplyConn {
    replies: usize,
}

impl ConnectionLike for ShortReplyConn {
    fn req_packed_command<'a>(&'a mut self, _cmd: &'a redis::Cmd) -> RedisFuture<'a, RValue> {
        async { Ok(RValue::Nil) }.boxed()
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        _pipe: &'a redis::Pipeline,
        _offset: usize,
        _count: usize,
    ) -> RedisFuture<'a, Vec<RValue>> {
        let values = vec![RValue::BulkString(b"string".to_vec()); self.replies];
        async move { Ok(values) }.boxed()
    }

    fn get_db(&self) -> i64 {
        0
    }
}

impl SlotRoutedConnection for ShortReplyConn {}

// ==========================================================================
// [coder] 修复回合（Bug 第 1 轮）— 回归面按主题拆成两个子模块，避免本文件
// 继续超出单文件规模：
//   * `fix_round1`       — redis-workbench-BUG-003 / redis-workbench-BUG-004 / redis-workbench-BUG-005（拓扑无关）
//   * `cluster_topology` — BUG-001 / BUG-002，直接仿真 redis 的 cluster 分发层
//                          （逐项错误折叠 + 单 slot 路由），而不是把它 mock 掉。
// ==========================================================================

// [tester] 第 1 轮修复后的复测新增面：错误分类表逐类覆盖 + redis 真实 cluster
// 路由的事实钉板（BUG-007 证据）+ `sample_window_for` 的 Sentinel 臂。

// 主题拆分（本文件只保留共享脚手架）：
//   * `contract_helpers`  — 纯 helper：夹取、truncated 语义、解析器。
//   * `key_object_info`   — 一次往返、逐字段独立降级。
//   * `type_distribution` — SCAN + 分块 TYPE，从不 KEYS。
//   * `memory_sample`     — BUG-003：大 key 的 type/TTL/size 同批读取。
//   * `cluster_batch`     — 每 key 一个寻址批 + 逐条重放降级。
//   * `tester_coverage`   — [tester] 覆盖率补齐：JSON 形状与分支补测。

mod cluster_batch;
mod cluster_topology;
mod contract_helpers;
mod fix_round1;
mod fix_round1_retest;
mod key_object_info;
mod memory_sample;
mod tester_coverage;
mod type_distribution;
