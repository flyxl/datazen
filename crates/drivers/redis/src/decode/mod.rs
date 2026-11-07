//! Read-only value decoders for the Redis console (R8, codec set extended by W3-C).
//!
//! Every decoder is strictly *parse-only*: it never executes or materialises
//! host-language objects. Executable / reference opcodes (pickle
//! `GLOBAL`/`REDUCE`/`INST`/`BUILD`/`OBJ`/`NEWOBJ`, Java `TC_OBJECT` instance
//! construction, …) are rejected outright. Length, element-count and recursion
//! caps are enforced before any allocation so malformed input cannot cause
//! OOM or stack overflow.
//!
//! The accepted codec set is the **same set the value viewer offers** (see
//! `ui/value-editors/valueView/codecs.ts`): `none`, `base64`, the three
//! DEFLATE framings (`gzip` / `zlib` / raw `deflate`) and the four structured
//! formats. Headless callers (Workflow steps, the MCP server) select a codec by
//! the same name the GUI button carries, so a choice can no longer be silently
//! unsupported on the backend side.
//!
//! Failures are **in-band and decidable**: a rejected payload answers
//! `{ ok: false, reason, suggestedCodec, retryable }` instead of throwing a
//! prose error, so the UI can render the PRD's "retry as DEFLATE" affordance and
//! keep showing the raw bytes. `reason` is a stable kebab-case code; no caller
//! may branch on the human-readable `message`.

mod compress;
mod java;
mod msgpack;
mod php;
mod pickle;

#[cfg(test)]
mod tests;

use base64::Engine;
use serde_json::Value as JsonValue;
/// Hard ceiling on the raw bytes we will attempt to decode.
pub(crate) const MAX_INPUT_BYTES: usize = 50 * 1024 * 1024; // 50 MiB

/// Hard ceiling on decompressed output (zip-bomb guard), same budget as input.
pub(crate) const MAX_DECOMPRESSED_BYTES: usize = MAX_INPUT_BYTES;

/// One base64 payload, decoded with one codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Codec {
    /// Identity: report the stored bytes as-is (UTF-8 view + base64).
    None,
    /// The stored payload is itself base64 *text*; decode it to bytes.
    Base64,
    /// RFC 1952 gzip container.
    Gzip,
    /// RFC 1950 zlib stream.
    Zlib,
    /// RFC 1951 raw DEFLATE (no container bytes).
    Deflate,
    Msgpack,
    Pickle,
    Php,
    Java,
    /// Known to the product matrix, deliberately not implemented (P2/P3):
    /// protobuf needs a schema, which this driver never has.
    Protobuf,
}

/// What a codec produced: a structured tree, or plain bytes to re-inspect.
enum Decoded {
    Json(JsonValue),
    Bytes(Vec<u8>),
}

/// Why `decode_value` refused to produce a view. Stable codes only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The `codec` string is not part of the registry at all.
    UnknownCodec,
    /// Registered but intentionally unimplemented (`protobuf`).
    CodecNotImplemented,
    /// Input carried no `data` field.
    MissingData,
    /// The `data` field itself is not valid base64.
    InvalidBase64,
    /// Zero-byte payload.
    EmptyPayload,
    /// Payload above [`MAX_INPUT_BYTES`].
    PayloadTooLarge,
    /// The codec parsed and rejected these bytes.
    DecodeFailed,
    /// Well-formed compressed stream overrunning the output cap.
    DecompressedTooLarge,
}

impl Reason {
    /// Wire code asserted by driver UI tests and Workflow steps.
    fn code(self) -> &'static str {
        match self {
            Reason::UnknownCodec => "unknown-codec",
            Reason::CodecNotImplemented => "codec-not-implemented",
            Reason::MissingData => "missing-data",
            Reason::InvalidBase64 => "invalid-base64",
            Reason::EmptyPayload => "empty-payload",
            Reason::PayloadTooLarge => "payload-too-large",
            Reason::DecodeFailed => "decode-failed",
            Reason::DecompressedTooLarge => "decompressed-too-large",
        }
    }
}

/// A codec rejection plus its developer-facing detail (never a matching key).
struct Failure {
    reason: Reason,
    message: Option<String>,
}

impl Failure {
    fn with(reason: Reason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: Some(message.into()),
        }
    }
}

impl From<compress::Framing> for Codec {
    fn from(framing: compress::Framing) -> Self {
        match framing {
            compress::Framing::Gzip => Codec::Gzip,
            compress::Framing::Zlib => Codec::Zlib,
            compress::Framing::Deflate => Codec::Deflate,
        }
    }
}

impl Codec {
    /// Canonical name, identical to the UI option id and to the `codec` field
    /// echoed back in every response.
    fn name(self) -> &'static str {
        match self {
            Codec::None => "none",
            Codec::Base64 => "base64",
            Codec::Gzip => "gzip",
            Codec::Zlib => "zlib",
            Codec::Deflate => "deflate",
            Codec::Msgpack => "msgpack",
            Codec::Pickle => "pickle",
            Codec::Php => "php",
            Codec::Java => "java",
            Codec::Protobuf => "protobuf",
        }
    }

    fn parse(raw: &str) -> Option<Codec> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "none" | "raw" | "identity" => Some(Codec::None),
            "base64" | "b64" => Some(Codec::Base64),
            "gzip" | "gunzip" => Some(Codec::Gzip),
            // `deflate` alone means *raw* DEFLATE, matching the UI option and
            // `CompressionFormat('deflate-raw')`; the zlib-wrapped stream is
            // only reachable under its own names.
            "zlib" | "zlib-stream" | "deflate-zlib" => Some(Codec::Zlib),
            "deflate" | "raw-deflate" | "deflate-raw" => Some(Codec::Deflate),
            "msgpack" | "mpk" => Some(Codec::Msgpack),
            "pickle" | "python" => Some(Codec::Pickle),
            "php" | "php_serialize" | "phpser" => Some(Codec::Php),
            "java" | "java_serialized" => Some(Codec::Java),
            "protobuf" | "proto3" | "pb" => Some(Codec::Protobuf),
            _ => None,
        }
    }

    /// Codecs whose result is a byte payload rather than a JSON tree.
    fn is_byte_codec(self) -> bool {
        matches!(
            self,
            Codec::None | Codec::Base64 | Codec::Gzip | Codec::Zlib | Codec::Deflate
        )
    }

    fn decode(self, bytes: &[u8]) -> Result<Decoded, Failure> {
        match self {
            Codec::None => Ok(Decoded::Bytes(bytes.to_vec())),
            Codec::Base64 => {
                // The stored value is base64 *text*, so tolerate the line breaks
                // and wrapping whitespace viewers love to insert.
                let stripped: Vec<u8> = bytes
                    .iter()
                    .copied()
                    .filter(|b| !b.is_ascii_whitespace())
                    .collect();
                base64::engine::general_purpose::STANDARD
                    .decode(&stripped)
                    .map(Decoded::Bytes)
                    .map_err(|e| {
                        Failure::with(Reason::DecodeFailed, format!("payload is not base64: {e}"))
                    })
            }
            Codec::Gzip => decompress(compress::Framing::Gzip, bytes),
            Codec::Zlib => decompress(compress::Framing::Zlib, bytes),
            Codec::Deflate => decompress(compress::Framing::Deflate, bytes),
            Codec::Msgpack => msgpack::decode(bytes)
                .map(Decoded::Json)
                .map_err(|e| Failure::with(Reason::DecodeFailed, e)),
            Codec::Pickle => pickle::decode(bytes)
                .map(Decoded::Json)
                .map_err(|e| Failure::with(Reason::DecodeFailed, e)),
            Codec::Php => php::decode(bytes)
                .map(Decoded::Json)
                .map_err(|e| Failure::with(Reason::DecodeFailed, e)),
            Codec::Java => java::decode(bytes)
                .map(Decoded::Json)
                .map_err(|e| Failure::with(Reason::DecodeFailed, e)),
            Codec::Protobuf => Err(Failure::with(
                Reason::CodecNotImplemented,
                "protobuf needs a .proto schema; decode as base64 and use an external tool",
            )),
        }
    }
}

/// Which stable code a container rejection maps to. Kept separate from
/// [`decompress`] so the zip-bomb branch is pinned without inflating 50 MiB.
fn framing_failure_reason(err: &compress::DecompressFailure) -> Reason {
    match err {
        compress::DecompressFailure::TooLarge(_) => Reason::DecompressedTooLarge,
        compress::DecompressFailure::Failed(_) => Reason::DecodeFailed,
    }
}

fn decompress(framing: compress::Framing, bytes: &[u8]) -> Result<Decoded, Failure> {
    framing
        .decode(bytes, MAX_DECOMPRESSED_BYTES)
        .map(Decoded::Bytes)
        .map_err(|err| {
            let reason = framing_failure_reason(&err);
            Failure::with(
                reason,
                format!("{} container: {}", framing.name(), err.message()),
            )
        })
}

/// Positive format detection from a magic header, for structured payloads only.
///
/// Deliberately narrow: a wrong guess costs the user a pointless retry button,
/// so anything ambiguous (msgpack fixmap `0x80` vs a pickle protocol header)
/// returns [`None`] instead.
fn sniff_structured(bytes: &[u8]) -> Option<Codec> {
    if bytes.len() < 2 {
        return None;
    }
    // Java serialization stream magic 0xAC ED.
    if bytes[0] == 0xac && bytes[1] == 0xed {
        return Some(Codec::Java);
    }
    // pickle protocol >= 2: `\x80<ver>` then a FRAME opcode.
    if bytes[0] == 0x80 && (0x02..=0x06).contains(&bytes[1]) && bytes.get(2) == Some(&0x95) {
        return Some(Codec::Pickle);
    }
    // PHP serialize() always opens with a type tag: `a:`, `s:`, `O:`, … or `N;`.
    if bytes.starts_with(b"N;") || (bytes[0].is_ascii_alphabetic() && bytes[1] == b':') {
        return Some(Codec::Php);
    }
    None
}

/// The "try this instead" hint carried by every in-band failure.
///
/// Order matters: a real container header beats a signature guess, which beats
/// an actual inflate probe of the headerless framing. A failed container
/// selection never suggests a *structured* format — that would send the user
/// from "GZip" to "PHP" with no evidence.
fn suggest_codec_for(codec: Codec, bytes: &[u8]) -> Option<Codec> {
    if let Some(detected) = compress::sniff_framing(bytes).map(Codec::from) {
        // Same container as the rejected attempt: the stream itself is broken,
        // so a retry with another framing would only produce the same error.
        return (detected != codec).then_some(detected);
    }
    if !codec.is_byte_codec() {
        if let Some(other) = sniff_structured(bytes) {
            if other != codec {
                return Some(other);
            }
        }
    }
    // Raw DEFLATE carries no header, so a successful inflate is the only proof,
    // and it is only worth probing when the user did not already pick it.
    let worth_probing = matches!(
        codec,
        Codec::Gzip | Codec::Zlib | Codec::Msgpack | Codec::Pickle | Codec::Php | Codec::Java
    );
    if worth_probing
        && compress::Framing::Deflate
            .decode(bytes, MAX_DECOMPRESSED_BYTES)
            .is_ok()
    {
        return Some(Codec::Deflate);
    }
    None
}

/// Guards the two size extremes before any decoder is handed the bytes.
fn payload_rejection(len: usize) -> Option<(Reason, Option<String>)> {
    if len == 0 {
        return Some((Reason::EmptyPayload, None));
    }
    if len > MAX_INPUT_BYTES {
        return Some((
            Reason::PayloadTooLarge,
            Some(format!(
                "payload exceeds {MAX_INPUT_BYTES} byte decode limit"
            )),
        ));
    }
    None
}

fn failure_envelope(
    codec: &str,
    reason: Reason,
    message: Option<String>,
    suggested: Option<Codec>,
) -> JsonValue {
    serde_json::json!({
        "ok": false,
        "codec": codec,
        "reason": reason.code(),
        "suggestedCodec": suggested.map(Codec::name),
        "retryable": suggested.is_some(),
        "message": message.unwrap_or_default(),
    })
}

/// Decode a base64 payload with the given codec.
///
/// Input: `{ codec: <name from [`Codec::parse`], data: "<base64>" }`.
/// Output (structured codec):
/// `{ ok: true, codec, kind: "json", inBytes, bytes, json, data: null, text: null }`.
/// Output (byte codec, i.e. `none`/`base64`/the three framings):
/// `{ ok: true, codec, kind: "bytes", inBytes, bytes, data: "<base64>", text, json }`
/// where `text` is present only when the result is valid UTF-8 and `json` only
/// when it parses as JSON — so a `gzip` step can be followed by a second
/// `decode_value` call on the returned `data` for a chained `gzip`+`msgpack`.
///
/// Any rejection answers `{ ok: false, codec, reason, suggestedCodec,
/// retryable, message }` and never throws.
pub(crate) fn decode_value(input: &JsonValue) -> JsonValue {
    let raw_codec = input
        .get("codec")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let Some(codec) = Codec::parse(&raw_codec) else {
        return failure_envelope(
            &raw_codec,
            Reason::UnknownCodec,
            Some(format!("'{raw_codec}' is not a codec of this driver")),
            None,
        );
    };

    let Some(b64) = input.get("data").and_then(JsonValue::as_str) else {
        return failure_envelope(
            codec.name(),
            Reason::MissingData,
            Some("command input requires 'data' (base64)".to_string()),
            None,
        );
    };

    let bytes = match base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
        Ok(decoded) => decoded,
        Err(err) => {
            return failure_envelope(
                codec.name(),
                Reason::InvalidBase64,
                Some(err.to_string()),
                None,
            )
        }
    };

    if let Some((reason, message)) = payload_rejection(bytes.len()) {
        return failure_envelope(codec.name(), reason, message, None);
    }

    let in_bytes = bytes.len();
    match codec.decode(&bytes) {
        Ok(Decoded::Json(tree)) => json_envelope(codec, in_bytes, &tree),
        Ok(Decoded::Bytes(out)) => bytes_envelope(codec, in_bytes, &out),
        Err(failure) => {
            let suggested = match failure.reason {
                // PRD §6: protobuf degrades to "base64 + an external tool".
                Reason::CodecNotImplemented => Some(Codec::Base64),
                _ => suggest_codec_for(codec, &bytes),
            };
            failure_envelope(codec.name(), failure.reason, failure.message, suggested)
        }
    }
}

fn json_envelope(codec: Codec, in_bytes: usize, tree: &JsonValue) -> JsonValue {
    let text = serde_json::to_string_pretty(tree).unwrap_or_else(|_| "null".to_string());
    serde_json::json!({
        "ok": true,
        "codec": codec.name(),
        "kind": "json",
        "inBytes": in_bytes,
        "bytes": in_bytes,
        "json": text,
        "data": JsonValue::Null,
        "text": JsonValue::Null,
    })
}

fn bytes_envelope(codec: Codec, in_bytes: usize, out: &[u8]) -> JsonValue {
    let utf8 = std::str::from_utf8(out).ok().map(str::to_string);
    let json = utf8
        .as_ref()
        .and_then(|text| serde_json::from_str::<JsonValue>(text).ok())
        .and_then(|tree| serde_json::to_string_pretty(&tree).ok());
    serde_json::json!({
        "ok": true,
        "codec": codec.name(),
        "kind": "bytes",
        "inBytes": in_bytes,
        "bytes": out.len(),
        "data": base64::engine::general_purpose::STANDARD.encode(out),
        "text": utf8,
        "json": json,
    })
}
