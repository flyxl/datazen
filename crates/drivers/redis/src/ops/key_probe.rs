//! `key_probe` — the key tree's one-key attribute read (PRD §4 I-3).
//!
//! The tree needs to answer "is this leaf real, what is it, how long does it
//! live, how big is it" for a single named key without paying for a `SCAN`, and
//! *without ever reading the value*: the frame that shows bytes is the value
//! viewer's job, and probing a multi-megabyte string must not pull it across the
//! wire. One pipeline therefore carries `EXISTS + TYPE + PTTL + MEMORY USAGE`
//! ([`build_key_probe_pipeline`]) and nothing else.
//!
//! # Cluster discipline
//!
//! Every command in the batch names exactly one key, so all four hash to that
//! key's own slot — the batch is never cross-slot. [`fetch_key_group`] does the
//! addressing: on [`Topology::Cluster`] the pipeline goes to the master owning
//! [`get_slot(key)`] as one round trip and replays command-by-command if the
//! dispatch layer folds a rejection, which is what keeps an unsupported
//! `MEMORY USAGE` (Redis < 4.0) or a `NOPERM` degrading `memoryBytes` alone
//! instead of failing the probe. `MEMORY USAGE` in particular is a two-word
//! command redis' own routing table does not describe, so leaving it unaddressed
//! would mean a `MOVED` and a `refresh_slots` storm on every probe.
//!
//! [`fetch_key_group`]: crate::ops::tree::scan::fetch_key_group

use redis::aio::ConnectionLike;
use redis::cluster_routing::get_slot;
use redis::Value as RValue;
use serde::Serialize;

use crate::connect::Topology;
use crate::ops::tree::scan::fetch_key_group;
use crate::ops::workbench::{
    is_unusable_reply, parse_opt_int, parse_type_token, type_reply_says_absent,
    SlotRoutedConnection, TTL_MISSING, TTL_NO_EXPIRY,
};

/// Commands in the probe batch; keep in sync with [`build_key_probe_pipeline`].
pub const KEY_PROBE_PIPELINE_LEN: usize = 4;

/// Reply slots of [`build_key_probe_pipeline`].
pub mod probe_slots {
    /// `EXISTS` — 0 or 1.
    pub const EXISTS: usize = 0;
    /// `TYPE` — the type token, or `none`.
    pub const TYPE: usize = 1;
    /// `PTTL` — milliseconds, so the reply can be `ttlMs` without conversion.
    pub const TTL: usize = 2;
    /// `MEMORY USAGE` — bytes.
    pub const MEMORY: usize = 3;
}

/// The probe batch: `EXISTS`, `TYPE`, `PTTL`, `MEMORY USAGE` of one key.
///
/// `PTTL` rather than `TTL` because the contract field is `ttlMs`: a second
/// scaling step in the UI would be a unit bug waiting to happen, and `PTTL`
/// also keeps sub-second TTLs from being read as "expired". Value reads
/// (`GET`/`HGETALL`/`LRANGE`/…) are deliberately absent — this is an attribute
/// probe, and `scan_keys` is the command that renders previews.
///
/// Public and side-effect free because it *is* the contract surface.
pub fn build_key_probe_pipeline(key: &str) -> redis::Pipeline {
    let mut pipe = redis::Pipeline::with_capacity(KEY_PROBE_PIPELINE_LEN);
    pipe.cmd("EXISTS")
        .arg(key)
        .cmd("TYPE")
        .arg(key)
        .cmd("PTTL")
        .arg(key)
        .cmd("MEMORY")
        .arg("USAGE")
        .arg(key);
    pipe
}

/// Payload of the `key_probe` command.
///
/// `ttlMs` is already in **milliseconds** (it comes from `PTTL`): unlike the
/// `ttl` of `scan_keys` / `list_children`, which is in seconds, it must not be
/// multiplied by 1000 again downstream.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyProbe {
    /// The key is there.
    pub exists: bool,
    /// Redis `TYPE`; `None` for a missing key or an unreadable answer.
    #[serde(rename = "type")]
    pub key_type: Option<String>,
    /// `PTTL` in milliseconds: `-1` no expiry, `-2` missing, `>0` remaining.
    pub ttl_ms: i64,
    /// `MEMORY USAGE` in bytes; `None` when unsupported or unreadable.
    pub memory_bytes: Option<u64>,
}

fn slot(values: &[RValue], index: usize) -> RValue {
    values.get(index).cloned().unwrap_or(RValue::Nil)
}

/// Assemble the probe from one batch's replies.
///
/// `EXISTS` answers presence and `TYPE` answers shape, and each is the fallback
/// for the other: when one of them was rejected (a `NOPERM` on `TYPE`, a
/// folded-away `EXISTS`) the other still says whether the key is there. `None`
/// is the honest "we cannot say" — both presence answers were unusable, which
/// the caller reports as a failure rather than as an empty key.
pub fn parse_key_probe(values: &[RValue]) -> Option<KeyProbe> {
    let exists_reply = slot(values, probe_slots::EXISTS);
    let type_reply = slot(values, probe_slots::TYPE);

    let declared_exists = if is_unusable_reply(&exists_reply) {
        None
    } else {
        parse_opt_int(&exists_reply).map(|n| n > 0)
    };
    let type_is_readable = !is_unusable_reply(&type_reply) && !type_reply_says_absent(&type_reply);
    let type_says_missing = type_reply_says_absent(&type_reply);

    let exists = match (declared_exists, type_says_missing, type_is_readable) {
        (Some(exists), _, _) => exists,
        // No usable EXISTS: TYPE can still answer, positively or negatively.
        (None, true, _) => false,
        (None, false, true) => true,
        (None, false, false) => {
            tracing::debug!("redis key_probe: neither EXISTS nor TYPE answered, key unknown");
            return None;
        }
    };

    if !exists {
        // Presence is the one question the probe was asked; everything else
        // about a key that is not there would be invented.
        return Some(KeyProbe {
            exists: false,
            key_type: None,
            ttl_ms: TTL_MISSING,
            memory_bytes: None,
        });
    }

    let key_type = parse_type_token(&type_reply);
    let ttl_ms = match parse_opt_int(&slot(values, probe_slots::TTL)) {
        Some(n) => n,
        None => {
            // PTTL is unavailable below Redis 2.6; "no expiry" is the least
            // misleading value and the event is logged, as in `key_object_info`.
            tracing::warn!(
                ttl_reply = ?slot(values, probe_slots::TTL),
                "redis key_probe: PTTL unreadable, reporting no-expiry"
            );
            TTL_NO_EXPIRY
        }
    };

    Some(KeyProbe {
        exists: true,
        key_type,
        ttl_ms,
        memory_bytes: parse_opt_int(&slot(values, probe_slots::MEMORY))
            .and_then(|n| u64::try_from(n).ok()),
    })
}

/// Probe one key: type, expiry, size — never the value.
pub(crate) async fn key_probe<C>(
    conn: &mut C,
    key: &str,
    topology: Topology,
) -> Result<KeyProbe, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let pipe = build_key_probe_pipeline(key);
    let values = fetch_key_group(conn, key, &pipe, topology).await?;
    parse_key_probe(&values).ok_or_else(|| "redis_key_probe_unreadable".to_string())
}

/// Does this single key exist?
///
/// The exact-name short circuit of `count_matching` (PRD §4 I-3: a search for a
/// known key must show `1/1`, not `n+`) and of the tree's own leaf resolution.
/// One command, and on a cluster an *addressed* one: `EXISTS` is routed by redis
/// itself, but naming the slot keeps every key-targeted command of this module
/// built the same way instead of relying on a reader knowing the routing table.
pub(crate) async fn key_exists<C>(
    conn: &mut C,
    key: &str,
    topology: Topology,
) -> Result<bool, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let mut cmd = redis::cmd("EXISTS");
    cmd.arg(key);
    let raw = if matches!(topology, Topology::Cluster) {
        conn.command_at_slot(&cmd, get_slot(key.as_bytes()))
            .await
            .map_err(|e| e.to_string())?
    } else {
        conn.req_packed_command(&cmd)
            .await
            .map_err(|e| e.to_string())?
    };
    parse_opt_int(&raw)
        .map(|n| n > 0)
        .ok_or_else(|| "redis_key_exists_unreadable".to_string())
}

/// A RESP error line parsed into a `Value` — how a rejected command arrives.
#[cfg(test)]
pub(crate) fn err_reply(detail: &str) -> RValue {
    let line = format!("-ERR {detail}\r\n");
    // A malformed fixture must never pass as an error reply, so this test helper
    // (not production code) panics on an invalid literal.
    match redis::parse_redis_value(line.as_bytes()) {
        Ok(v) => v,
        Err(e) => panic!("test fixture produced an invalid error reply: {e}"),
    }
}

/// A bulk-string reply — the shape scalar answers actually arrive in.
#[cfg(test)]
pub(crate) fn bulk(s: &str) -> RValue {
    RValue::BulkString(s.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(pipe: &redis::Pipeline) -> Vec<Vec<String>> {
        pipe.cmd_iter()
            .map(|cmd| {
                cmd.args_iter()
                    .map(|arg| match arg {
                        redis::Arg::Simple(bytes) => String::from_utf8_lossy(bytes).into_owned(),
                        redis::Arg::Cursor => "@cursor".into(),
                    })
                    .collect()
            })
            .collect()
    }

    fn full_probe_replies() -> Vec<RValue> {
        vec![
            RValue::Int(1),      // EXISTS
            bulk("hash"),        // TYPE
            RValue::Int(60_000), // PTTL
            RValue::Int(1_040),  // MEMORY USAGE
        ]
    }

    #[test]
    fn probe_batch_is_four_commands_on_one_key_and_reads_no_value() {
        let batch = names(&build_key_probe_pipeline("app:cache:{session}:1"));
        assert_eq!(batch.len(), KEY_PROBE_PIPELINE_LEN);
        let heads: Vec<&str> = batch.iter().map(|c| c[0].as_str()).collect();
        assert_eq!(heads, vec!["EXISTS", "TYPE", "PTTL", "MEMORY"]);
        assert_eq!(batch[probe_slots::MEMORY][1], "USAGE");
        for args in &batch {
            assert_eq!(
                args.last().map(String::as_str),
                Some("app:cache:{session}:1"),
                "every command must name the probed key, or the batch leaves its slot"
            );
        }
        // The whole point of the probe: no value read among the command heads
        // or their subcommands.
        for args in &batch {
            for token in args.iter().skip(1) {
                assert!(
                    !matches!(
                        token.to_ascii_uppercase().as_str(),
                        "GET" | "HGETALL" | "LRANGE" | "ZRANGE" | "SMEMBERS" | "XRANGE" | "SET"
                    ),
                    "key_probe must never fetch a value, got {args:?}"
                );
            }
        }
    }

    #[test]
    fn probe_reads_every_field_when_the_server_supports_them() {
        assert_eq!(
            parse_key_probe(&full_probe_replies()),
            Some(KeyProbe {
                exists: true,
                key_type: Some("hash".to_string()),
                ttl_ms: 60_000,
                memory_bytes: Some(1_040),
            })
        );
    }

    #[test]
    fn probe_reports_a_missing_key_as_an_answer_not_an_error() {
        let values = vec![
            RValue::Int(0),
            bulk("none"),
            RValue::Int(-2),
            err_reply("no such key"),
        ];
        assert_eq!(
            parse_key_probe(&values),
            Some(KeyProbe {
                exists: false,
                key_type: None,
                ttl_ms: TTL_MISSING,
                memory_bytes: None,
            })
        );
    }

    #[test]
    fn probe_degrades_memory_and_ttl_independently() {
        let mut values = full_probe_replies();
        values[probe_slots::MEMORY] =
            err_reply("unknown subcommand or wrong number of args for 'USAGE'");
        let probe = parse_key_probe(&values).expect("EXISTS/TYPE still answer");
        assert!(probe.exists);
        assert_eq!(probe.key_type.as_deref(), Some("hash"));
        assert_eq!(probe.memory_bytes, None);
        assert_eq!(probe.ttl_ms, 60_000);

        let mut values = full_probe_replies();
        values[probe_slots::TTL] = err_reply("unknown command 'PTTL'");
        let probe = parse_key_probe(&values).expect("presence still answered");
        assert_eq!(
            probe.ttl_ms, TTL_NO_EXPIRY,
            "unreadable expiry is not 'gone'"
        );
        assert_eq!(probe.memory_bytes, Some(1_040));
    }

    #[test]
    fn exists_and_type_carry_the_other_when_one_is_rejected() {
        // `TYPE` denied: EXISTS still decides presence.
        let values = vec![
            RValue::Int(1),
            err_reply("NOPERM this user has no permissions to run the 'type' command"),
            RValue::Int(-1),
            RValue::Int(88),
        ];
        let probe = parse_key_probe(&values).expect("EXISTS answers");
        assert!(probe.exists);
        assert_eq!(probe.key_type, None, "an unreadable type stays None");
        assert_eq!(probe.memory_bytes, Some(88));

        // `EXISTS` folded away: a real type token answers presence positively.
        let values = vec![
            RValue::Nil,
            bulk("string"),
            RValue::Int(500),
            RValue::Int(12),
        ];
        let probe = parse_key_probe(&values).expect("TYPE answers");
        assert!(probe.exists);
        assert_eq!(probe.key_type.as_deref(), Some("string"));
        assert_eq!(probe.ttl_ms, 500);
    }

    #[test]
    fn probe_refuses_to_guess_when_neither_presence_answer_arrived() {
        let values = vec![
            err_reply("connection reset"),
            err_reply("connection reset"),
            RValue::Int(-1),
            RValue::Int(10),
        ];
        assert_eq!(parse_key_probe(&values), None);
        assert_eq!(parse_key_probe(&[]), None);
    }

    #[test]
    fn probe_serializes_the_frozen_camel_case_shape() {
        let json = serde_json::to_value(KeyProbe {
            exists: true,
            key_type: Some("zset".to_string()),
            ttl_ms: 1_500,
            memory_bytes: Some(64),
        })
        .expect("KeyProbe is serializable");
        assert_eq!(
            json,
            serde_json::json!({
                "exists": true,
                "type": "zset",
                "ttlMs": 1_500,
                "memoryBytes": 64
            })
        );
        // A missing key keeps the field present so the UI never has to branch
        // on key existence in JSON.
        let json = serde_json::to_value(KeyProbe {
            exists: false,
            key_type: None,
            ttl_ms: TTL_MISSING,
            memory_bytes: None,
        })
        .expect("KeyProbe is serializable");
        assert_eq!(
            json,
            serde_json::json!({ "exists": false, "type": null, "ttlMs": -2, "memoryBytes": null })
        );
    }
}
