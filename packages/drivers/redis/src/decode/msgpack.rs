//! MessagePack decoder via `rmpv` (parse-only).

use serde_json::{Map, Value as JsonValue};

const MAX_DEPTH: u32 = 128;

/// Decode a full MessagePack document into a JSON tree.
pub(crate) fn decode(bytes: &[u8]) -> Result<JsonValue, String> {
    let mut cursor = std::io::Cursor::new(bytes);
    let value =
        rmpv::decode::read_value(&mut cursor).map_err(|e| format!("msgpack parse error: {e}"))?;
    // Reject a trailing second document (mostly malformed input / padding).
    let consumed = cursor.position() as usize;
    let rest = &bytes[consumed..];
    if !rest.iter().all(|b| *b == 0) {
        // allow trailing NUL padding only; anything else is a parse ambiguity
        if !rest.is_empty() {
            return Err(format!(
                "unexpected trailing bytes after msgpack value ({rest_len} byte(s))",
                rest_len = rest.len()
            ));
        }
    }
    to_json(&value, 0)
}

fn to_json(v: &rmpv::Value, depth: u32) -> Result<JsonValue, String> {
    if depth > MAX_DEPTH {
        return Err("msgpack nesting exceeds depth limit".to_string());
    }
    use rmpv::Value as Mp;
    Ok(match v {
        Mp::Nil => JsonValue::Null,
        Mp::Boolean(b) => JsonValue::Bool(*b),
        Mp::Integer(i) => integer_to_json(i),
        Mp::F32(f) => float_to_json(*f as f64),
        Mp::F64(f) => float_to_json(*f),
        Mp::String(s) => JsonValue::String(
            s.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| String::from_utf8_lossy(s.as_bytes()).into_owned()),
        ),
        Mp::Binary(b) => {
            use base64::Engine;
            JsonValue::String(base64::engine::general_purpose::STANDARD.encode(b.as_slice()))
        }
        Mp::Ext(ty, data) => {
            use base64::Engine;
            let mut m = Map::new();
            m.insert("$ext".into(), JsonValue::from(*ty as i64));
            m.insert(
                "data".into(),
                JsonValue::String(
                    base64::engine::general_purpose::STANDARD.encode(data.as_slice()),
                ),
            );
            JsonValue::Object(m)
        }
        Mp::Array(items) => {
            if items.len() > element_cap(MAX_DEPTH - depth) {
                return Err("msgpack array exceeds size limit".to_string());
            }
            let mut out = Vec::with_capacity(items.len().min(1024));
            for item in items {
                out.push(to_json(item, depth + 1)?);
            }
            JsonValue::Array(out)
        }
        Mp::Map(entries) => {
            if entries.len() > element_cap(MAX_DEPTH - depth) {
                return Err("msgpack map exceeds size limit".to_string());
            }
            let mut obj = Map::with_capacity(entries.len().min(1024));
            for (k, val) in entries {
                let key = key_to_string(k)?;
                obj.insert(key, to_json(val, depth + 1)?);
            }
            JsonValue::Object(obj)
        }
    })
}

/// Geometric size budget that shrinks with depth so total nodes stay bounded.
fn element_cap(remaining_depth: u32) -> usize {
    (1usize << remaining_depth.min(16)).min(100_000)
}

fn float_to_json(f: f64) -> JsonValue {
    if f.is_finite() {
        serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .unwrap_or_else(|| JsonValue::String(format!("{f}")))
    } else if f.is_nan() {
        JsonValue::String("NaN".into())
    } else if f.is_sign_negative() {
        JsonValue::String("-Infinity".into())
    } else {
        JsonValue::String("Infinity".into())
    }
}

fn integer_to_json(i: &rmpv::Integer) -> JsonValue {
    if let Some(n) = i.as_i64() {
        return JsonValue::Number(n.into());
    }
    if let Some(n) = i.as_u64() {
        return JsonValue::Number(n.into());
    }
    // i128 / u128 beyond JSON's safe integer range → string, keeps precision.
    JsonValue::String(i.to_string())
}

fn key_to_string(k: &rmpv::Value) -> Result<String, String> {
    use rmpv::Value as Mp;
    Ok(match k {
        Mp::String(s) => s
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| String::from_utf8_lossy(s.as_bytes()).into_owned()),
        Mp::Integer(i) => i.to_string(),
        Mp::Boolean(b) => b.to_string(),
        Mp::Nil => "null".to_string(),
        other => serde_json::to_string(&to_json(other, MAX_DEPTH)?)
            .map_err(|e| format!("msgpack key encode error: {e}"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_scalar_and_container() {
        // {"a": 1, "b": [true, nil, 2.5]}
        // {"a":1,"b":[true,nil,2.5]}
        let bytes: &[u8] = &[
            0x82, 0xa1, b'a', 0x01, 0xa1, b'b', 0x93, 0xc3, 0xc0, 0xcb, 0x40, 0x04, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];
        let v = decode(bytes).unwrap();
        assert_eq!(v["a"], JsonValue::from(1));
        assert_eq!(v["b"][0], JsonValue::Bool(true));
        assert!(v["b"][1].is_null());
        assert_eq!(v["b"][2], JsonValue::from(2.5));
    }

    #[test]
    fn rejects_trailing_garbage() {
        let bytes: &[u8] = &[0x01, 0x02]; // int 1 then a stray byte
        assert!(decode(bytes).is_err());
    }
}
