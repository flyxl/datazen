//! Reply parsers and the value bridges they share — the only place a raw `Value` is turned into one of those shapes.

use super::types::ConsumerInfo;
use super::types::StreamEntry;
use super::types::StreamGroupInfo;
use super::types::XpendingEntry;
use super::types::XpendingResult;
use std::collections::HashMap;

pub(crate) fn value_to_string(v: &redis::Value) -> String {
    match v {
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).into(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::VerbatimString { text, .. } => text.clone(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::Okay => "OK".into(),
        redis::Value::Nil => String::new(),
        other => format!("{other:?}"),
    }
}

pub(crate) fn value_to_u64(v: &redis::Value) -> u64 {
    match v {
        redis::Value::Int(i) => *i as u64,
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).parse().unwrap_or(0),
        _ => 0,
    }
}

pub(crate) fn parse_stream_entry(v: &redis::Value) -> Option<StreamEntry> {
    let parts = match v {
        redis::Value::Array(items) if !items.is_empty() => items,
        _ => return None,
    };
    let id = value_to_string(&parts[0]);
    let mut fields = HashMap::new();
    if parts.len() >= 2 {
        if let redis::Value::Array(pairs) = &parts[1] {
            for chunk in pairs.chunks(2) {
                if chunk.len() == 2 {
                    fields.insert(value_to_string(&chunk[0]), value_to_string(&chunk[1]));
                }
            }
        }
    }
    Some(StreamEntry { id, fields })
}

pub(crate) fn parse_xinfo_groups(raw: &redis::Value) -> Result<Vec<StreamGroupInfo>, String> {
    let groups = match raw {
        redis::Value::Array(items) => items,
        _ => return Ok(vec![]),
    };

    let mut out = Vec::new();
    for group in groups {
        let mut name = String::new();
        let mut consumers = 0u64;
        let mut pending = 0u64;
        let mut last_delivered_id = String::new();

        match group {
            redis::Value::Array(fields) => {
                for chunk in fields.chunks(2) {
                    if chunk.len() != 2 {
                        continue;
                    }
                    let key = value_to_string(&chunk[0]).to_ascii_lowercase();
                    let val = &chunk[1];
                    match key.as_str() {
                        "name" => name = value_to_string(val),
                        "consumers" => consumers = value_to_u64(val),
                        "pending" => pending = value_to_u64(val),
                        "last-delivered-id" => last_delivered_id = value_to_string(val),
                        _ => {}
                    }
                }
            }
            redis::Value::Map(pairs) => {
                for (k, v) in pairs {
                    let key = value_to_string(k).to_ascii_lowercase();
                    match key.as_str() {
                        "name" => name = value_to_string(v),
                        "consumers" => consumers = value_to_u64(v),
                        "pending" => pending = value_to_u64(v),
                        "last-delivered-id" => last_delivered_id = value_to_string(v),
                        _ => {}
                    }
                }
            }
            _ => continue,
        }

        if !name.is_empty() {
            out.push(StreamGroupInfo {
                name,
                consumers,
                pending,
                last_delivered_id,
            });
        }
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub(crate) fn parse_xpending_entries(raw: &redis::Value) -> Result<XpendingResult, String> {
    let items = match raw {
        redis::Value::Array(items) => items,
        _ => {
            return Ok(XpendingResult {
                total: 0,
                entries: vec![],
            })
        }
    };

    if items.is_empty() {
        return Ok(XpendingResult {
            total: 0,
            entries: vec![],
        });
    }

    if items.len() >= 4 && matches!(&items[0], redis::Value::Int(_)) {
        let total = value_to_u64(&items[0]);
        return Ok(XpendingResult {
            total,
            entries: vec![],
        });
    }

    let mut entries = Vec::new();
    for item in items {
        let parts = match item {
            redis::Value::Array(parts) if parts.len() >= 4 => parts,
            _ => continue,
        };
        entries.push(XpendingEntry {
            id: value_to_string(&parts[0]),
            consumer: value_to_string(&parts[1]),
            idle_ms: value_to_u64(&parts[2]),
            delivery_count: value_to_u64(&parts[3]),
        });
    }

    Ok(XpendingResult {
        total: entries.len() as u64,
        entries,
    })
}

pub(crate) fn parse_xinfo_consumers(raw: &redis::Value) -> Result<Vec<ConsumerInfo>, String> {
    let items = match raw {
        redis::Value::Array(items) => items,
        _ => return Ok(vec![]),
    };

    let mut out = Vec::new();
    for item in items {
        let mut name = String::new();
        let mut pending = 0u64;
        let mut idle_ms = 0u64;
        let mut delivery_count = 0u64;

        match item {
            redis::Value::Array(fields) => {
                for chunk in fields.chunks(2) {
                    if chunk.len() != 2 {
                        continue;
                    }
                    let key = value_to_string(&chunk[0]).to_ascii_lowercase();
                    let val = &chunk[1];
                    match key.as_str() {
                        "name" => name = value_to_string(val),
                        "pending" => pending = value_to_u64(val),
                        "idle" => idle_ms = value_to_u64(val),
                        "delivery-count" => delivery_count = value_to_u64(val),
                        _ => {}
                    }
                }
            }
            redis::Value::Map(pairs) => {
                for (k, v) in pairs {
                    let key = value_to_string(k).to_ascii_lowercase();
                    match key.as_str() {
                        "name" => name = value_to_string(v),
                        "pending" => pending = value_to_u64(v),
                        "idle" => idle_ms = value_to_u64(v),
                        "delivery-count" => delivery_count = value_to_u64(v),
                        _ => {}
                    }
                }
            }
            _ => continue,
        }

        if !name.is_empty() {
            out.push(ConsumerInfo {
                name,
                pending,
                idle_ms,
                delivery_count,
            });
        }
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Parse a Redis stream ID string, returning the numeric sequence part.
/// For IDs like "1000-0", returns (milliseconds, sequence_number).
pub(crate) fn parse_stream_id(id: &str) -> Option<(u64, u64)> {
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() != 2 {
        return None;
    }
    let ms = parts[0].parse::<u64>().ok()?;
    let seq = parts[1].parse::<u64>().ok()?;
    Some((ms, seq))
}
