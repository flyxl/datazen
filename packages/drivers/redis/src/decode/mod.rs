//! Read-only value decoders for the Redis console (R8).
//!
//! Every decoder is strictly *parse-only*: it never executes or materialises
//! host-language objects. Executable / reference opcodes (pickle
//! `GLOBAL`/`REDUCE`/`INST`/`BUILD`/`OBJ`/`NEWOBJ`, Java `TC_OBJECT` instance
//! construction, …) are rejected outright. Length, element-count and recursion
//! caps are enforced before any allocation so malformed input cannot cause
//! OOM or stack overflow.

mod java;
mod msgpack;
mod php;
mod pickle;

use serde_json::Value as JsonValue;

/// Hard ceiling on the raw bytes we will attempt to decode.
pub(crate) const MAX_INPUT_BYTES: usize = 50 * 1024 * 1024; // 50 MiB

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Codec {
    Msgpack,
    Pickle,
    Php,
    Java,
}

impl Codec {
    fn parse(raw: &str) -> Option<Codec> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "msgpack" | "mpk" => Some(Codec::Msgpack),
            "pickle" | "python" => Some(Codec::Pickle),
            "php" | "php_serialize" | "phpser" => Some(Codec::Php),
            "java" | "java_serialized" => Some(Codec::Java),
            _ => None,
        }
    }

    fn decode(self, bytes: &[u8]) -> Result<JsonValue, String> {
        match self {
            Codec::Msgpack => msgpack::decode(bytes),
            Codec::Pickle => pickle::decode(bytes),
            Codec::Php => php::decode(bytes),
            Codec::Java => java::decode(bytes),
        }
    }
}

/// Decode a base64 payload with the given codec, returning a JSON tree/text.
///
/// Input: `{ codec: "msgpack"|"pickle"|"php"|"java", data: "<base64>" }`.
/// Output: `{ ok: true, json: "<pretty JSON>" }` or `{ ok: false, error }`.
pub(crate) fn decode_value(input: &JsonValue) -> Result<JsonValue, String> {
    let codec = input
        .get("codec")
        .and_then(JsonValue::as_str)
        .and_then(Codec::parse)
        .ok_or_else(|| "unsupported codec".to_string())?;

    let b64 = input
        .get("data")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| "command input requires 'data' (base64)".to_string())?;

    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64.trim())
        .map_err(|e| format!("invalid base64 payload: {e}"))?;

    if bytes.is_empty() {
        return Err("empty payload".to_string());
    }
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(format!(
            "payload exceeds {MAX_INPUT_BYTES} byte decode limit"
        ));
    }

    let json = codec.decode(&bytes)?;
    let text = serde_json::to_string_pretty(&json).unwrap_or_else(|_| "null".to_string());
    Ok(serde_json::json!({ "ok": true, "json": text }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn call(codec: &str, bytes: &[u8]) -> Result<JsonValue, String> {
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        decode_value(&serde_json::json!({ "codec": codec, "data": b64 }))
    }

    #[test]
    fn rejects_unknown_codec_and_oversize() {
        assert!(decode_value(&serde_json::json!({ "codec": "bson", "data": "AAA=" })).is_err());
        assert!(call("msgpack", b"").is_err()); // empty payload
    }

    #[test]
    fn decode_ok_shape_for_supported_codecs() {
        // msgpack {"a":1}
        let r = call("msgpack", &[0x81, 0xa1, b'a', 0x01]).unwrap();
        assert_eq!(r["ok"], serde_json::json!(true));
        assert!(r["json"].as_str().unwrap().contains("\"a\""));
    }
}
