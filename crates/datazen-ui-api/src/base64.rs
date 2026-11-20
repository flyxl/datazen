//! Base64 conversion for the wire's byte payloads — ported from React
//! `packages/frontend/src/lib/base64.ts` (task T310, seg2 lib layer).
//!
//! The native file dialogs exchange binary as base64 because the value has to
//! cross the IPC boundary as JSON text; the grid's own byte payloads travel as
//! untagged number arrays. React keeps exactly two helpers here so the padding
//! rules cannot drift between callers. This module is the Rust twin of those
//! two helpers:
//!
//! - [`bytes_to_base64`] is `bytesToBase64`: standard padded RFC 4648 base64,
//!   the byte-for-byte equivalent of `btoa(String.fromCharCode(...bytes))`.
//! - [`base64_to_bytes`] is `base64ToBytes`: the byte-for-byte equivalent of
//!   `atob(data)` followed by a `Uint8Array` copy.
//!
//! ## No new dependency
//!
//! `atob`/`btoa` follow the WHATWG *forgiving-base64* algorithm, whose exact
//! leniency (ASCII-whitespace stripping, trailing-`=` handling, no trailing-bit
//! validation) is not the default of any single `base64` crate configuration.
//! Hand-writing the ~90 lines here pins those rules without adding a
//! dependency to this crate, and lets every edge case be tested in-file.
//!
//! ## Divergence from the JavaScript original
//!
//! - JS `atob` throws `InvalidCharacterError`; Rust returns
//!   [`Base64DecodeError`] instead — no panic on a production path.
//! - JS strings are UTF-16; this module operates on bytes (the wire payload).
//!   A non-ASCII character in the *base64 text* is rejected, exactly as `atob`
//!   rejects any code unit outside the base64 alphabet. UTF-8 bytes carried
//!   *inside* a valid payload round-trip untouched (see the tests).

use std::error::Error;
use std::fmt;

/// Standard RFC 4648 alphabet, the one `btoa`/`atob` use (not base64url).
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Why a base64 string could not be decoded, mirroring `atob`'s
/// `InvalidCharacterError` split into its two causes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base64DecodeError {
    /// The length after whitespace/padding cleanup is `1 mod 4` — `atob`'s
    /// "string should be (a multiple of 4)-length or 2/3 trailing bits" case.
    InvalidLength,
    /// A byte outside the base64 alphabet (and not a legal trailing `=`),
    /// including any non-ASCII character in the input.
    InvalidCharacter,
}

impl fmt::Display for Base64DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => f.write_str("invalid base64 length"),
            Self::InvalidCharacter => f.write_str("invalid base64 character"),
        }
    }
}

impl Error for Base64DecodeError {}

/// `btoa(String.fromCharCode(...bytes))` in one pass: standard, padded base64.
///
/// Empty input maps to the empty string; padding (`=`) appears only when the
/// byte count is not a multiple of three.
pub fn bytes_to_base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let (b0, b1, b2) = (bytes[i], bytes[i + 1], bytes[i + 2]);
        out.push(ALPHABET[(b0 >> 2) as usize] as char);
        out.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(ALPHABET[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        out.push(ALPHABET[(b2 & 0x3F) as usize] as char);
        i += 3;
    }
    match bytes.len() - i {
        1 => {
            let b0 = bytes[i];
            out.push(ALPHABET[(b0 >> 2) as usize] as char);
            out.push(ALPHABET[((b0 & 0x03) << 4) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let (b0, b1) = (bytes[i], bytes[i + 1]);
            out.push(ALPHABET[(b0 >> 2) as usize] as char);
            out.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            out.push(ALPHABET[((b1 & 0x0F) << 2) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

/// The five code points the WHATWG algorithm calls "ASCII whitespace".
fn is_ascii_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}' | '\u{000A}' | '\u{000C}' | '\u{000D}' | '\u{0020}'
    )
}

/// One base64 character's sextet, or `None` for anything outside the alphabet.
fn sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// `atob(data)` + the `Uint8Array` copy, as a `Result`: the forgiving-base64
/// decode algorithm from the WHATWG spec, minus the throw.
///
/// In particular it strips ASCII whitespace, removes trailing `=` only when
/// the cleaned length is a multiple of four, rejects a `1 mod 4` length, and
/// does **not** validate the unused trailing bits of the last quantum — the
/// leniency that distinguishes `atob` from a strict decoder.
pub fn base64_to_bytes(data: &str) -> Result<Vec<u8>, Base64DecodeError> {
    // 1. Drop ASCII whitespace; any remaining non-ASCII code point is invalid.
    let mut cleaned = Vec::with_capacity(data.len());
    for ch in data.chars() {
        if is_ascii_whitespace(ch) {
            continue;
        }
        if !ch.is_ascii() {
            return Err(Base64DecodeError::InvalidCharacter);
        }
        cleaned.push(ch as u8);
    }

    // 2. Trailing padding is only understood on a multiple-of-four length;
    //    a lone `=` on a 2/3-length tail stays put and is then an invalid char.
    if cleaned.len() % 4 == 0 {
        while cleaned.last() == Some(&b'=') {
            cleaned.pop();
        }
    }

    // 3. A `1 mod 4` length can never encode anything.
    if cleaned.len() % 4 == 1 {
        return Err(Base64DecodeError::InvalidLength);
    }

    // 4. Decode quanta of four sextets; the final quantum may hold 2 or 3.
    let mut out = Vec::with_capacity(cleaned.len() / 4 * 3);
    let mut i = 0;
    while i < cleaned.len() {
        let take = (cleaned.len() - i).min(4);
        if take < 2 {
            return Err(Base64DecodeError::InvalidLength);
        }
        let mut s = [0u8; 4];
        for (slot, byte) in s[..take].iter_mut().zip(&cleaned[i..i + take]) {
            *slot = sextet(*byte).ok_or(Base64DecodeError::InvalidCharacter)?;
        }
        out.push((s[0] << 2) | (s[1] >> 4));
        if take >= 3 {
            out.push(((s[1] & 0x0F) << 4) | (s[2] >> 2));
        }
        if take == 4 {
            out.push(((s[2] & 0x03) << 6) | s[3]);
        }
        i += 4;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_rfc_4648_test_vectors() {
        let cases: [(&[u8], &str); 7] = [
            (b"", ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ];
        for (bytes, expected) in cases {
            assert_eq!(bytes_to_base64(bytes), expected, "input {bytes:?}");
        }
    }

    #[test]
    fn encodes_binary_bytes_not_text() {
        // Every byte value, including NUL and 0xFF, must survive exactly as
        // `String.fromCharCode(...bytes)` + `btoa` would carry it.
        let all: Vec<u8> = (0..=255u8).collect();
        let encoded = bytes_to_base64(&all);
        assert_eq!(base64_to_bytes(&encoded).unwrap(), all);
        assert_eq!(bytes_to_base64(&[0, 0, 0]), "AAAA");
        assert_eq!(bytes_to_base64(&[255, 255, 255]), "////");
    }

    #[test]
    fn decodes_the_rfc_4648_test_vectors() {
        let cases: [(&str, &[u8]); 7] = [
            ("", b""),
            ("Zg==", b"f"),
            ("Zm8=", b"fo"),
            ("Zm9v", b"foo"),
            ("Zm9vYg==", b"foob"),
            ("Zm9vYmE=", b"fooba"),
            ("Zm9vYmFy", b"foobar"),
        ];
        for (encoded, expected) in cases {
            assert_eq!(
                base64_to_bytes(encoded).unwrap(),
                expected,
                "input {encoded:?}"
            );
        }
    }

    #[test]
    fn round_trips_every_length() {
        for len in 0..40usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let encoded = bytes_to_base64(&bytes);
            assert_eq!(base64_to_bytes(&encoded).unwrap(), bytes, "len {len}");
        }
    }

    #[test]
    fn round_trips_utf8_payload_bytes() {
        // The wire carries bytes, not text: a UTF-8 string encoded to bytes
        // must come back byte-identical and still decode as the same text.
        let text = "你好, DataZen 🚀";
        let bytes = text.as_bytes();
        let decoded = base64_to_bytes(&bytes_to_base64(bytes)).unwrap();
        assert_eq!(decoded, bytes);
        assert_eq!(std::str::from_utf8(&decoded).unwrap(), text);
    }

    #[test]
    fn decodes_without_padding() {
        // `atob` accepts the 2/3-char final quantum with no `=` at all.
        assert_eq!(base64_to_bytes("Zg").unwrap(), b"f");
        assert_eq!(base64_to_bytes("Zm8").unwrap(), b"fo");
    }

    #[test]
    fn strips_ascii_whitespace_like_atob() {
        assert_eq!(base64_to_bytes(" Zm9 v\n").unwrap(), b"foo");
        assert_eq!(base64_to_bytes("Zm9v\r\t ").unwrap(), b"foo");
    }

    #[test]
    fn forgives_unused_trailing_bits() {
        // 'Y','R' -> 0b011000 010001 -> first byte 0b01100001 = 'a'; the
        // leftover low bits are ignored, exactly as forgiving decode does.
        assert_eq!(base64_to_bytes("YR==").unwrap(), b"a");
        assert_eq!(base64_to_bytes("YQ==").unwrap(), b"a");
    }

    #[test]
    fn rejects_bad_lengths() {
        assert_eq!(base64_to_bytes("A"), Err(Base64DecodeError::InvalidLength));
        assert_eq!(
            base64_to_bytes("Zm9v="),
            Err(Base64DecodeError::InvalidLength)
        );
        assert_eq!(
            base64_to_bytes("ABCDE"),
            Err(Base64DecodeError::InvalidLength)
        );
    }

    #[test]
    fn rejects_invalid_and_non_ascii_characters() {
        assert_eq!(
            base64_to_bytes("Zm9!").unwrap_err(),
            Base64DecodeError::InvalidCharacter
        );
        assert_eq!(
            base64_to_bytes("Z=9v").unwrap_err(),
            Base64DecodeError::InvalidCharacter
        );
        // A non-ASCII code point in the base64 text is never a valid sextet.
        assert_eq!(
            base64_to_bytes("Zm9vé").unwrap_err(),
            Base64DecodeError::InvalidCharacter
        );
        // A pad on a non-multiple-of-four tail is a stray `=`, not padding.
        assert_eq!(
            base64_to_bytes("Zg=").unwrap_err(),
            Base64DecodeError::InvalidCharacter
        );
    }

    #[test]
    fn error_display_is_informational_without_payload() {
        assert_eq!(
            Base64DecodeError::InvalidLength.to_string(),
            "invalid base64 length"
        );
        assert_eq!(
            Base64DecodeError::InvalidCharacter.to_string(),
            "invalid base64 character"
        );
    }
}
