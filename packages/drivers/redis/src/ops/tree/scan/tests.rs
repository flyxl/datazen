//! Unit tests for [`ops_tree_scan`](crate::ops::tree::scan) and its tree
//! consumers — declared there under `#[cfg(test)] mod tests`.
//!
//! The scripted connection double below is this module's own (the
//! `ScriptedConn` of `ops_workbench/tests.rs` is private to that module). It
//! journals every request with its arguments — and on a cluster every
//! *addressed* request together with the slot it was aimed at — so the tests
//! can assert the cost shape without a live server: `DBSIZE` exactly once per
//! command, two batches per page, one single-key batch per key on a cluster,
//! and slot addressing that a `{hash-tag}` counterfactual turns red if tag
//! handling regresses.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use futures_util::FutureExt;
use redis::{Cmd, Pipeline, RedisFuture};

use crate::ops::key_probe::{key_probe, parse_key_probe, KeyProbe};
use crate::ops::tree::budget::{
    DEFAULT_TREE_BUDGET, HARD_MAX_TREE_BUDGET, MAX_TREE_SCAN_ROUNDS, MAX_TREE_STALLED_ROUNDS,
    MIN_TREE_SCAN_COUNT, TREE_BUDGET_DBSIZE_FACTOR, TREE_SCAN_MIN_ROUND_COUNT,
};
use crate::ops::tree::{list_children_page, ChildEntry};
use crate::ops::workbench::SlotRoutedBatchFuture;

use super::*;

#[test]
fn test_tester_meta_slots_preserve_legacy_root_path() {
    assert_eq!(
        crate::ops::tree::scan::meta_slots::TYPE,
        crate::ops::tree::scan::meta::meta_slots::TYPE
    );
    assert_eq!(
        crate::ops::tree::scan::meta_slots::TTL,
        crate::ops::tree::scan::meta::meta_slots::TTL
    );
    assert_eq!(
        crate::ops::tree::scan::meta_slots::MEMORY,
        crate::ops::tree::scan::meta::meta_slots::MEMORY
    );
}

// ---------------------------------------------------------------------------
// Scripted connection double
// ---------------------------------------------------------------------------

fn args_of(cmd: &Cmd) -> Vec<String> {
    cmd.args_iter()
        .map(|arg| match arg {
            redis::Arg::Simple(bytes) => String::from_utf8_lossy(bytes).into_owned(),
            redis::Arg::Cursor => "@cursor".into(),
        })
        .collect()
}

fn bulk(s: &str) -> RValue {
    RValue::BulkString(s.as_bytes().to_vec())
}

/// The reply shape both `parse_scan_result` implementations accept.
fn scan_reply(cursor: u64, keys: &[String]) -> RValue {
    let items: Vec<RValue> = keys.iter().map(|k| bulk(k)).collect();
    RValue::Array(vec![
        RValue::BulkString(cursor.to_string().into_bytes()),
        RValue::Array(items),
    ])
}

/// Journal form of one request line: `["TYPE", "app:a"]` → `"TYPE app:a"`.
fn joined(requests: &[Vec<String>]) -> Vec<String> {
    requests.iter().map(|args| args.join(" ")).collect()
}

#[derive(Default)]
struct TreeState {
    dbsize: i64,
    /// When set, `DBSIZE` is *refused* by the server — an ACL profile without the
    /// flag, a managed tier that hides the command, or one unreachable master
    /// under the cluster fan-out (redis-tree-backend-BUG-003).
    dbsize_refused: bool,
    /// A `DBSIZE` answer of this exact shape is served instead of an integer,
    /// for the "unusable reply" arm (`None` = the normal integer answer).
    dbsize_reply: Option<RValue>,
    /// Replies handed out by successive `SCAN` rounds; an exhausted script
    /// wraps the cursor (the honest end of a keyspace).
    scan_script: VecDeque<(u64, Vec<String>)>,
    types: HashMap<String, String>,
    /// Keys whose `TYPE` reply arrives as `Nil` — an *unreadable* answer, not
    /// an absent key.
    nil_types: HashSet<String>,
    ttls: HashMap<String, i64>,
    ptls: HashMap<String, i64>,
    mems: HashMap<String, u64>,
    lens: HashMap<String, i64>,
    previews: HashMap<String, RValue>,
    singles: Vec<Vec<String>>,
    batches: Vec<Vec<Vec<String>>>,
    /// `(slot, commands)` of every `command_at_slot` / `pipeline_at_slot`.
    addressed: Vec<(u16, Vec<Vec<String>>)>,
}

impl TreeState {
    /// The answer to a *plain* (unrouted) request. Only `DBSIZE` can fail here:
    /// the tree's own commands all answer from the script above.
    fn ask(&mut self, args: &[String]) -> Result<RValue, redis::RedisError> {
        if args.first().map(String::as_str) == Some("DBSIZE") && self.dbsize_refused {
            return Err(redis::RedisError::from((
                ErrorKind::ResponseError,
                "NOPERM this user has no permissions to run the 'dbsize' command",
            )));
        }
        Ok(self.reply(args))
    }

    fn reply(&mut self, args: &[String]) -> RValue {
        let Some(name) = args.first().map(String::as_str) else {
            return RValue::Nil;
        };
        match name {
            "DBSIZE" => self
                .dbsize_reply
                .clone()
                .unwrap_or(RValue::Int(self.dbsize)),
            "SCAN" => match self.scan_script.pop_front() {
                Some((cursor, keys)) => scan_reply(cursor, &keys),
                None => scan_reply(0, &[]),
            },
            "EXISTS" => {
                let exists = args
                    .get(1)
                    .is_some_and(|key| self.types.get(key).is_some_and(|t| t != "none"));
                RValue::Int(i64::from(exists))
            }
            "TYPE" => {
                let key = args[1].as_str();
                if self.nil_types.contains(key) {
                    RValue::Nil
                } else {
                    bulk(self.types.get(key).map_or("none", String::as_str))
                }
            }
            "TTL" => RValue::Int(self.ttls.get(&args[1]).copied().unwrap_or(-2)),
            "PTTL" => RValue::Int(self.ptls.get(&args[1]).copied().unwrap_or(-2)),
            "MEMORY" => match args.get(2).and_then(|key| self.mems.get(key)) {
                Some(bytes) => RValue::Int(i64::try_from(*bytes).unwrap_or(i64::MAX)),
                None => RValue::Nil,
            },
            "STRLEN" | "LLEN" | "SCARD" | "ZCARD" | "HLEN" | "XLEN" => {
                RValue::Int(self.lens.get(&args[1]).copied().unwrap_or(0))
            }
            "GET" => self
                .previews
                .get(&args[1])
                .cloned()
                .unwrap_or_else(|| bulk("")),
            _ => RValue::Nil,
        }
    }
}

#[derive(Clone, Default)]
struct TreeConn {
    state: Arc<Mutex<TreeState>>,
}

impl TreeConn {
    fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, TreeState> {
        self.state.lock().expect("state lock")
    }

    /// Seed one existing string key with TTL / length / preview answers.
    fn seed_string(&mut self, key: &str, len: i64, preview: &str) {
        let mut st = self.state();
        st.types.insert(key.to_string(), "string".to_string());
        st.ttls.insert(key.to_string(), -1);
        st.lens.insert(key.to_string(), len);
        st.previews.insert(key.to_string(), bulk(preview));
    }
}

impl ConnectionLike for TreeConn {
    /// The scripted double holds no logical database index; `dbIndex` is
    /// resolved before the connection is picked, so `0` is honest here.
    fn get_db(&self) -> i64 {
        0
    }

    fn req_packed_command<'a>(&'a mut self, cmd: &'a Cmd) -> RedisFuture<'a, RValue> {
        let args = args_of(cmd);
        (async move {
            let mut st = self.state();
            st.singles.push(args.clone());
            let reply = st.ask(&args);
            drop(st);
            reply
        })
        .boxed()
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        pipe: &'a Pipeline,
        _offset: usize,
        _count: usize,
    ) -> RedisFuture<'a, Vec<RValue>> {
        let batch: Vec<Vec<String>> = pipe.cmd_iter().map(args_of).collect();
        (async move {
            let mut st = self.state();
            let mut values = Vec::with_capacity(batch.len());
            for args in &batch {
                values.push(st.reply(args));
            }
            st.batches.push(batch);
            drop(st);
            Ok(values)
        })
        .boxed()
    }
}

/// The cluster path must *see* what the ops asked for, so this double
/// implements the routing capability itself and journals the slot.
impl SlotRoutedConnection for TreeConn {
    fn command_at_slot<'a>(&'a mut self, cmd: &'a Cmd, slot: u16) -> RedisFuture<'a, RValue> {
        let args = args_of(cmd);
        (async move {
            let mut st = self.state();
            let reply = st.reply(&args);
            st.addressed.push((slot, vec![args]));
            drop(st);
            Ok(reply)
        })
        .boxed()
    }

    fn pipeline_at_slot<'a>(
        &'a mut self,
        pipe: &'a Pipeline,
        slot: u16,
    ) -> SlotRoutedBatchFuture<'a> {
        let batch: Vec<Vec<String>> = pipe.cmd_iter().map(args_of).collect();
        (async move {
            let mut st = self.state();
            let mut values = Vec::with_capacity(batch.len());
            for args in &batch {
                values.push(st.reply(args));
            }
            st.addressed.push((slot, batch));
            drop(st);
            Ok(values)
        })
        .boxed()
    }
}

// Test surface split by topic. This file keeps only the module docs above and
// the scripted-connection double every child shares through `use super::*;`:
//   * `batch_shapes`       — pure batch shapes + literal-pinned constants.
//   * `page_cost`          — standalone page cost shape, exact-key short
//                            circuit, budget arms.
//   * `count_matching`     — the count reply and its `n+` floor.
//   * `dbsize_degradation` — BUG-003: a refused DBSIZE degrades, never fails.
//   * `list_children`      — budget trio appended, folders never typed.
//   * `cluster`            — one slot per key, SCAN pinned to the anchor.
//   * `fail_soft`          — the replay / short-batch / transport contract.
//   * `page_coverage`      — page-level coverage of the builders' contracts.

// `ErrorKind` used to be imported mid-file by the fail-soft block; the children
// reach it through `use super::*;` now that each one is its own module.
use redis::ErrorKind;

mod batch_shapes;
mod cluster;
mod count_matching;
mod dbsize_degradation;
mod fail_soft;
mod list_children;
mod page_cost;
mod page_coverage;
