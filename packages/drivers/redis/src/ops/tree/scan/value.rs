//! Per-key value batches: the logical-length / preview commands, the preview renderers, and `fetch_key_values`.

use super::batch::fetch_page_groups;
use super::batch::PageKey;
use super::meta::reply_at;
use super::transport::VALUE_FIELDS_MAX;
use crate::connect::Topology;
#[allow(unused_imports)]
use crate::ops::workbench::{
    cluster_scan_anchor_slot, parse_opt_int, parse_type_token, type_reply_says_absent,
    SlotRoutedConnection, TTL_MISSING,
};
#[allow(unused_imports)]
use crate::value::{parse_scan_result, preview_value_to_string, truncate_preview, value_to_string};
use redis::aio::ConnectionLike;
use redis::Value as RValue;

/// Preview truncation, identical to the pre-pipeline implementation.
const PREVIEW_MAX: usize = 120;

/// What the value batch renders for a stream, where no read is issued at all.
const STREAM_PREVIEW: &str = "(stream)";

/// Value-derived attributes of one key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ValueFields {
    /// Logical length (`STRLEN` / `HLEN` / …) in elements or bytes; `0` when the
    /// type has no length command or the reply was unusable.
    pub logical_len: u64,
    /// Truncated preview text; `""` for module / unknown types.
    pub preview: String,
}

/// `O(1)` length command for a type, or `None` when there is none.
pub fn length_command_for(key_type: &str) -> Option<&'static str> {
    match key_type {
        "string" => Some("STRLEN"),
        "list" => Some("LLEN"),
        "set" => Some("SCARD"),
        "zset" => Some("ZCARD"),
        "hash" => Some("HLEN"),
        "stream" => Some("XLEN"),
        _ => None,
    }
}

/// Does this type get a preview read at all?
///
/// Mirrors the pre-pipeline `preview_on` exactly, including the deliberate
/// omissions: `stream` needs no read (a fixed label), and any other type —
/// modules such as `ReJSON-RL` included — is reported without a preview rather
/// than read blind.
pub fn has_preview_command(key_type: &str) -> bool {
    matches!(key_type, "string" | "list" | "hash" | "set" | "zset")
}

/// Append this key's preview command to `pipe`.
fn push_preview_cmd(pipe: &mut redis::Pipeline, key_type: &str, key: &str) {
    match key_type {
        "string" => {
            pipe.cmd("GET").arg(key);
        }
        "list" => {
            pipe.cmd("LRANGE").arg(key).arg(0).arg(2);
        }
        // `HSCAN COUNT 3`, never `HGETALL`: a large hash must not be loaded just
        // to show two fields.
        "hash" => {
            pipe.cmd("HSCAN").arg(key).arg(0).arg("COUNT").arg(3);
        }
        "set" => {
            pipe.cmd("SRANDMEMBER").arg(key).arg(3);
        }
        "zset" => {
            pipe.cmd("ZRANGE").arg(key).arg(0).arg(2).arg("WITHSCORES");
        }
        _ => {}
    }
}

/// Build the second batch: per key `[length] + [preview]`, only for the types
/// that answer those questions. `types` is index-aligned with `keys`.
pub fn build_value_pipeline(keys: &[PageKey]) -> (redis::Pipeline, Vec<usize>) {
    let mut pipe = redis::Pipeline::with_capacity(keys.len() * VALUE_FIELDS_MAX);
    let mut sizes = Vec::with_capacity(keys.len());
    for item in keys {
        let mut commands = 0usize;
        if let Some(cmd) = length_command_for(&item.key_type) {
            pipe.cmd(cmd).arg(&item.key);
            commands += 1;
        }
        if has_preview_command(&item.key_type) {
            push_preview_cmd(&mut pipe, &item.key_type, &item.key);
            commands += 1;
        }
        sizes.push(commands);
    }
    (pipe, sizes)
}

/// Array reply → list of strings, `[]` for anything that is not an array.
fn reply_array(value: &RValue) -> Vec<String> {
    match value {
        RValue::Array(items) => items.iter().map(value_to_string).collect(),
        RValue::Nil => Vec::new(),
        other => vec![value_to_string(other)],
    }
}

/// Extract field/value pairs from an `HSCAN` reply for the preview.
///
/// Moved verbatim from `redis_driver_on`, where it served the per-key preview
/// loop; `HSCAN` answers `[cursor, [field, value, …]]`.
pub fn extract_hscan_preview(raw: &RValue) -> Vec<String> {
    if let RValue::Array(items) = raw {
        if items.len() >= 2 {
            if let RValue::Array(members) = &items[1] {
                let mut result = Vec::new();
                for chunk in members.chunks(2) {
                    if chunk.len() == 2 {
                        let field = value_to_string(&chunk[0]);
                        let value = value_to_string(&chunk[1]);
                        result.push(format!("{field}: {value}"));
                    }
                }
                return result;
            }
        }
    }
    Vec::new()
}

/// Render one key's preview from its type and the replies the batch collected.
pub fn render_preview(key_type: &str, preview_reply: Option<&RValue>) -> String {
    let raw = match key_type {
        "string" => preview_value_to_string(preview_reply.unwrap_or(&RValue::Nil), "string"),
        "list" | "set" => {
            let vals = preview_reply.map(reply_array).unwrap_or_default();
            format!("{vals:?}")
        }
        "zset" => {
            let vals = preview_reply.map(reply_array).unwrap_or_default();
            format!("{vals:?}")
        }
        "hash" => {
            let vals = preview_reply.map(extract_hscan_preview).unwrap_or_default();
            format!("{vals:?}")
        }
        "stream" => STREAM_PREVIEW.to_string(),
        _ => return String::new(),
    };
    truncate_preview(&raw, PREVIEW_MAX)
}

/// Assemble one key's length/preview replies. Slot order is fixed by
/// [`build_value_pipeline`], and both presence checks are re-derived from the
/// type, so the group is read the same way it was written.
pub fn parse_value_group(values: &[RValue], key_type: &str) -> ValueFields {
    let has_len = length_command_for(key_type).is_some();
    let has_preview = has_preview_command(key_type);
    let logical_len = if has_len {
        parse_opt_int(&reply_at(values, 0))
            .and_then(|n| u64::try_from(n).ok())
            .unwrap_or(0)
    } else {
        0
    };
    let preview_index = usize::from(has_len);
    let preview_reply = if has_preview {
        match values.get(preview_index) {
            Some(RValue::Nil) | None => None,
            Some(value) => Some(value),
        }
    } else {
        None
    };
    ValueFields {
        logical_len,
        preview: render_preview(key_type, preview_reply),
    }
}

/// Fetch logical length + preview for a page whose types are already known.
pub(crate) async fn fetch_key_values<C>(
    conn: &mut C,
    items: &[PageKey],
    topology: Topology,
) -> Result<Vec<ValueFields>, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let groups = fetch_page_groups(conn, items, topology, build_value_pipeline).await?;
    Ok(groups
        .iter()
        .zip(items.iter())
        .map(|(group, item)| parse_value_group(group, &item.key_type))
        .collect())
}

// ---------------------------------------------------------------------------
// Command-level pages
// ---------------------------------------------------------------------------
