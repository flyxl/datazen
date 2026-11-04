//! The two reply shapes (`TypeDistribution`, `KeyObjectInfo`) with their constructors, and the reply parsers the probes fold into them.

#[allow(unused_imports)]
use super::key_info::key_object_info;
use super::primitives::is_sample_truncated;
use super::primitives::KEY_INFO_PIPELINE_LEN;
use super::primitives::TTL_MISSING;
use super::primitives::TTL_NO_EXPIRY;
use redis::Value as RValue;
use serde::Serialize;
use std::collections::BTreeMap;

/// Payload of the `type_distribution` command (KV context bar).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeDistribution {
    /// `TYPE` token → key count; serializes as a JSON object. Module types
    /// keep their raw server token (e.g. `ReJSON-RL`). On a cluster these come
    /// from the one pinned shard of [`CLUSTER_SCAN_ANCHOR`], not from every
    /// node.
    pub counts: BTreeMap<String, u64>,
    /// Keys whose type was actually resolved. Always equals the sum of
    /// [`Self::counts`], which is what lets the UI label the sample honestly.
    pub sampled: u64,
    /// `DBSIZE` of the selected database, read exactly once per command. On a
    /// cluster redis routes `DBSIZE` to every master and sums the answers, so
    /// this is the whole cluster's count — a different scope than
    /// [`Self::sampled`], which is why [`Self::truncated`] is rarely `false`
    /// there.
    pub dbsize: u64,
    /// `sampled < dbsize` — the distribution is a sample, not a census.
    pub truncated: bool,
}

impl TypeDistribution {
    /// Fold resolved types into the payload, maintaining the
    /// `sampled == counts.values().sum()` invariant.
    ///
    /// Used for every topology: [`is_sample_truncated`] already says the right
    /// thing on a cluster, where the sample covers one shard and `dbsize` the
    /// whole cluster. A non-truncated cluster answer therefore *is* a census —
    /// all of the cluster's keys live on the scanned shard.
    pub fn from_sample(counts: BTreeMap<String, u64>, dbsize: u64) -> Self {
        let sampled = counts.values().sum();
        Self {
            counts,
            sampled,
            dbsize,
            truncated: is_sample_truncated(sampled, dbsize),
        }
    }
}

/// Payload of the `key_object_info` command (key-attribute sidebar).
///
/// Every field except `missing` / `ttlMs` is optional on purpose: the reply is
/// a partial view, not an all-or-nothing read.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyObjectInfo {
    /// Key vanished (expired / deleted) — a success case, not an error.
    pub missing: bool,
    /// Redis `TYPE`; `None` when that one reply errored.
    #[serde(rename = "type")]
    pub key_type: Option<String>,
    /// `MEMORY USAGE` in bytes; `None` on Redis < 4.0 or when unsupported.
    pub memory_bytes: Option<u64>,
    /// `OBJECT ENCODING`; `None` for module types and on error.
    pub encoding: Option<String>,
    /// `OBJECT IDLETIME` in seconds.
    pub idle_seconds: Option<u64>,
    /// `OBJECT FREQ`; `None` unless `maxmemory-policy` is LFU.
    pub freq: Option<u64>,
    /// `PTTL` in milliseconds: -1 no expiry, -2 missing, >0 remaining.
    pub ttl_ms: i64,
}

impl KeyObjectInfo {
    /// A missing key has no attributes worth reporting; `ttl_ms` keeps the
    /// Redis `-2` sentinel so a pill renderer still has a defined value.
    pub fn missing() -> Self {
        Self {
            missing: true,
            ttl_ms: TTL_MISSING,
            ..Self::default()
        }
    }
}

/// Reply slots of [`build_key_info_pipeline`]; keep in sync with its order.
pub mod key_info_slots {
    pub const MEMORY: usize = 0;
    pub const ENCODING: usize = 1;
    pub const IDLE: usize = 2;
    pub const FREQ: usize = 3;
    pub const TTL: usize = 4;
    pub const TYPE: usize = 5;
}

/// Build the single pipeline behind `key_object_info`.
///
/// Public and side-effect free because it *is* the contract surface: which
/// commands, in what order, one batch, and no keyspace-scanning command.
pub fn build_key_info_pipeline(key: &str) -> redis::Pipeline {
    let mut pipe = redis::Pipeline::with_capacity(KEY_INFO_PIPELINE_LEN);
    pipe.cmd("MEMORY")
        .arg("USAGE")
        .arg(key)
        .cmd("OBJECT")
        .arg("ENCODING")
        .arg(key)
        .cmd("OBJECT")
        .arg("IDLETIME")
        .arg(key)
        .cmd("OBJECT")
        .arg("FREQ")
        .arg(key)
        .cmd("PTTL")
        .arg(key)
        .cmd("TYPE")
        .arg(key);
    pipe
}

/// Does this reply carry no value for the slot it fills?
///
/// `Nil` is unambiguous. A structured `ServerError` / `Push` / `Attribute`
/// carries the payload itself, containers that hold nothing (or nothing
/// usable) answer nothing either. Shapes that are simply not part of a scalar
/// answer stay `false` here — "not unusable" is *not* "trusted": the caller
/// must still recognise the value, which is what keeps an unexpected future
/// shape from being read as a type name or as a missing key
/// (see [`parse_key_info`], state 4).
pub fn is_unusable_reply(value: &RValue) -> bool {
    match value {
        RValue::Nil => true,
        RValue::Array(items) | RValue::Set(items) => {
            items.is_empty() || items.iter().any(is_unusable_reply)
        }
        RValue::Map(_) | RValue::Attribute { .. } | RValue::Push { .. } => true,
        RValue::ServerError(_) => true,
        _ => false,
    }
}

/// Parse a `TYPE` reply into a distribution token.
///
/// `None` means "no usable answer": an errored / absent reply or `none` (the
/// key disappeared between SCAN and TYPE). Dropping those keeps `sampled`
/// equal to the sum of the counts.
pub fn parse_type_token(value: &RValue) -> Option<String> {
    if is_unusable_reply(value) {
        return None;
    }
    let raw = match value {
        RValue::BulkString(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        RValue::SimpleString(s) => s.clone(),
        RValue::VerbatimString { text, .. } => text.clone(),
        RValue::Int(n) => n.to_string(),
        _ => return None,
    };
    let token = raw.trim().to_string();
    if token.is_empty() || token.eq_ignore_ascii_case("none") {
        return None;
    }
    Some(token)
}

/// Does `TYPE` positively answer "this key is not there"?
///
/// Narrower than [`parse_type_token`] returning `None`, which also covers
/// replies that simply make no sense: only an explicit `none` (or the empty
/// token some proxies send) may be reported to the user as "the key expired".
/// Anything else is unreadable, not absent — see [`parse_key_info`].
pub fn type_reply_says_absent(value: &RValue) -> bool {
    parse_opt_string(value).is_some_and(|token| {
        let token = token.trim();
        token.is_empty() || token.eq_ignore_ascii_case("none")
    })
}

/// Parse an integer-ish reply, tolerating the bulk-string form some servers use
/// for integer replies. Errors, nil and non-numeric payloads degrade to `None`.
pub fn parse_opt_int(value: &RValue) -> Option<i64> {
    if is_unusable_reply(value) {
        return None;
    }
    match value {
        RValue::Int(n) => Some(*n),
        RValue::BulkString(bytes) => String::from_utf8_lossy(bytes).trim().parse().ok(),
        RValue::SimpleString(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Parse a string-ish reply; errors and nil degrade to `None`.
pub fn parse_opt_string(value: &RValue) -> Option<String> {
    if is_unusable_reply(value) {
        return None;
    }
    match value {
        RValue::BulkString(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        RValue::SimpleString(s) => Some(s.clone()),
        RValue::VerbatimString { text, .. } => Some(text.clone()),
        _ => None,
    }
}

pub(crate) fn slot(values: &[RValue], index: usize) -> RValue {
    values.get(index).cloned().unwrap_or(RValue::Nil)
}

/// Assemble the sidebar payload from one batch reply vector.
///
/// Slot order is fixed by [`build_key_info_pipeline`]; `TYPE` decides the state:
///
/// 1. usable type token → full payload;
/// 2. explicit `none` / empty → `missing: true`, i.e. "the key expired";
/// 3. errored or absent reply → "attributes unreadable";
/// 4. any other shape → also "attributes unreadable".
///
/// States 3 and 4 are `(missing: false, type: null, ttlMs: -1)`: neither claims
/// the key is gone, which is why an unrecognised protocol shape can never be
/// silently rendered as an expired key.
pub fn parse_key_info(values: &[RValue]) -> KeyObjectInfo {
    let type_reply = slot(values, key_info_slots::TYPE);
    if is_unusable_reply(&type_reply) {
        tracing::debug!("redis key_object_info: TYPE reply unusable, key kept as present");
        return unreadable_key_state();
    }
    if type_reply_says_absent(&type_reply) {
        // TYPE == "none": the key expired or was deleted before this read.
        return KeyObjectInfo::missing();
    }
    let Some(key_type) = parse_type_token(&type_reply) else {
        tracing::warn!(
            type_reply = ?type_reply,
            "redis key_object_info: TYPE answered something that is not a type name"
        );
        return unreadable_key_state();
    };

    let ttl_ms = match parse_opt_int(&slot(values, key_info_slots::TTL)) {
        Some(n) => n,
        None => {
            // PTTL does not exist below Redis 2.6; -1 (no expiry) is the
            // least misleading value, and the event is logged.
            tracing::warn!(
                ttl_reply = ?slot(values, key_info_slots::TTL),
                "redis key_object_info: PTTL unreadable, reporting no-expiry"
            );
            TTL_NO_EXPIRY
        }
    };

    KeyObjectInfo {
        missing: false,
        key_type: Some(key_type),
        memory_bytes: parse_opt_int(&slot(values, key_info_slots::MEMORY))
            .and_then(|n| u64::try_from(n).ok()),
        encoding: parse_opt_string(&slot(values, key_info_slots::ENCODING)),
        idle_seconds: parse_opt_int(&slot(values, key_info_slots::IDLE))
            .and_then(|n| u64::try_from(n).ok()),
        freq: parse_opt_int(&slot(values, key_info_slots::FREQ))
            .and_then(|n| u64::try_from(n).ok()),
        ttl_ms,
    }
}

/// The "we cannot say anything about this key" payload: the key is not claimed
/// to be gone, and no attribute is offered as fact.
pub(crate) fn unreadable_key_state() -> KeyObjectInfo {
    KeyObjectInfo {
        missing: false,
        ttl_ms: TTL_NO_EXPIRY,
        ..KeyObjectInfo::default()
    }
}
