//! ULID — lexicographically sortable 128-bit identifiers.
//!
//! Favorites are stored as one `.sql` file per entry, and the file name is the
//! entry's identity. The name must therefore be short, stable, and — because a
//! sync
//! folder is watched by other tools and by `git` — free of characters that
//! would need escaping on any platform. A ULID gives all of that: 26
//! Crockford base32 characters, `0-9A-HJKMNP-TV-Z` only, with the 48-bit
//! millisecond timestamp in the high bits so plain byte order equals creation
//! order.
//!
//! Implemented in-tree rather than pulled from a crate: the format is fully
//! specified, the whole encoder is ~40 lines, and it keeps the host free of a
//! new dependency for one identifier format.

use std::fmt;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::RngCore;

/// Crockford base32: digits plus uppercase letters, minus `I`, `L`, `O`, `U`.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Characters of the canonical 26-character representation.
pub const ULID_LEN: usize = 26;

/// A ULID: 48-bit timestamp + 80-bit randomness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ulid {
    timestamp_ms: u64,
    randomness: [u8; 10],
}

/// Reasons a string is not a canonical ULID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UlidError {
    #[error("ULID must be {ULID_LEN} characters, got {0}")]
    BadLength(usize),
    #[error("ULID contains a character outside the Crockford base32 alphabet")]
    BadCharacter,
    #[error("ULID timestamp overflows 48 bits")]
    TimestampOverflow,
}

impl Ulid {
    /// Build from raw parts. `randomness` is exactly 10 bytes (80 bits).
    pub fn from_parts(timestamp_ms: u64, randomness: [u8; 10]) -> Result<Self, UlidError> {
        if timestamp_ms >> 48 != 0 {
            return Err(UlidError::TimestampOverflow);
        }
        Ok(Self {
            timestamp_ms,
            randomness,
        })
    }

    /// Millisecond timestamp carried by the high 48 bits.
    pub fn timestamp_ms(&self) -> u64 {
        self.timestamp_ms
    }

    /// Randomness carried by the low 80 bits.
    pub fn randomness(&self) -> [u8; 10] {
        self.randomness
    }

    /// Parse a canonical 26-character ULID.
    pub fn parse(s: &str) -> Result<Self, UlidError> {
        let bytes = s.as_bytes();
        if bytes.len() != ULID_LEN {
            return Err(UlidError::BadLength(bytes.len()));
        }

        let mut timestamp = 0u64;
        let mut randomness = 0u128;
        for (i, &b) in bytes.iter().enumerate() {
            let v = decode_symbol(b).ok_or(UlidError::BadCharacter)? as u128;
            if i < 10 {
                timestamp = (timestamp << 5) | (v as u64 & 0x1F);
            } else {
                randomness = (randomness << 5) | v;
            }
        }

        // The first character only carries 3 significant bits; reject any
        // value that would need a 49th bit rather than silently truncating.
        if decode_symbol(bytes[0]).ok_or(UlidError::BadCharacter)? > 7 {
            return Err(UlidError::TimestampOverflow);
        }

        // `randomness` holds exactly 80 bits, so the high 48 bits of the u128
        // are zero and the low 10 bytes are the value verbatim.
        let bytes = randomness.to_be_bytes();
        let mut out = [0u8; 10];
        out.copy_from_slice(&bytes[6..]);
        Ok(Self {
            timestamp_ms: timestamp,
            randomness: out,
        })
    }

    /// True when `s` is a canonical ULID.
    ///
    /// **Not** the path-safety gate — that is `FavoritesStore::resolve_file`'s
    /// `is_safe_stem`, which deliberately accepts a wider set than a ULID so a
    /// favorite the user renamed by hand stays editable. This predicate is
    /// stricter (26 canonical Crockford characters, correct length) and exists
    /// for the tests that check the encoding and for a future strict-import
    /// path; the tests use it, so it is test-only.
    #[cfg(test)]
    pub fn is_valid(s: &str) -> bool {
        Self::parse(s).is_ok()
    }
}

impl fmt::Display for Ulid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::with_capacity(ULID_LEN);
        let ts = u128::from(self.timestamp_ms);
        for i in (0..10).rev() {
            out.push(ALPHABET[((ts >> (5 * i)) & 0x1F) as usize] as char);
        }
        let rnd = randomness_u128(&self.randomness);
        for i in (0..16).rev() {
            out.push(ALPHABET[((rnd >> (5 * i)) & 0x1F) as usize] as char);
        }
        f.write_str(&out)
    }
}

fn decode_symbol(b: u8) -> Option<u8> {
    // Strictly uppercase: favorites file names must be identical on
    // case-insensitive file systems (macOS, Windows) or the same ULID could
    // exist twice.
    ALPHABET.iter().position(|&c| c == b).map(|i| i as u8)
}

/// Last issued ULID, so ids minted inside one millisecond still increase.
///
/// Without this, two favorites saved in the same millisecond would sort by
/// their random bits instead of by creation — the storage layout relies on
/// directory listing order being creation order.
static LAST: Mutex<Option<Ulid>> = Mutex::new(None);

/// Largest value representable in the ULID's 48-bit millisecond field.
const MAX_TIMESTAMP_MS: u64 = (1u64 << 48) - 1;

/// Mint a new ULID from the system clock, monotonic within a millisecond.
pub fn new_ulid() -> Ulid {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
        .min(MAX_TIMESTAMP_MS);

    // A poisoned lock must not take the app down: losing monotonicity inside a
    // single millisecond degrades ordering, it cannot corrupt data.
    let Ok(mut guard) = LAST.lock() else {
        return random_ulid(now_ms);
    };

    let issued = match *guard {
        // Same millisecond: bump the 80-bit randomness as one integer, which
        // is exactly what makes the next id strictly greater.
        Some(prev) if prev.timestamp_ms() == now_ms => {
            let next = randomness_u128(&prev.randomness()).saturating_add(1);
            if next >> 80 != 0 {
                // 80 bits exhausted inside one millisecond (2^80 writes per
                // millisecond is not physically reachable). Step to the next
                // millisecond and start over so ordering still increases.
                match Ulid::from_parts((now_ms + 1).min(MAX_TIMESTAMP_MS), [0; 10]) {
                    Ok(u) => u,
                    Err(_) => return random_ulid(now_ms),
                }
            } else {
                match Ulid::from_parts(now_ms, tail_bytes(next)) {
                    Ok(u) => u,
                    Err(_) => return random_ulid(now_ms),
                }
            }
        }
        _ => random_ulid(now_ms),
    };

    *guard = Some(issued);
    issued
}

/// Low 80 bits of `v` as the ULID randomness field.
fn tail_bytes(v: u128) -> [u8; 10] {
    let bytes = v.to_be_bytes();
    let mut out = [0u8; 10];
    out.copy_from_slice(&bytes[6..]);
    out
}

/// The randomness field as an integer, so it can be incremented and compared
/// as one value rather than as 10 independent bytes.
fn randomness_u128(randomness: &[u8; 10]) -> u128 {
    let mut bytes = [0u8; 16];
    bytes[6..].copy_from_slice(randomness);
    u128::from_be_bytes(bytes)
}

fn random_ulid(millis: u64) -> Ulid {
    let mut randomness = [0u8; 10];
    rand::thread_rng().fill_bytes(&mut randomness);
    // `millis` is already clamped by the caller, so the timestamp always fits.
    // The fallback keeps this total even if that ever stops being true.
    Ulid::from_parts(millis.min(MAX_TIMESTAMP_MS), randomness).unwrap_or(Ulid {
        timestamp_ms: 0,
        randomness,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_its_string_form() {
        let u = new_ulid();
        let s = u.to_string();
        assert_eq!(s.len(), ULID_LEN);
        assert_eq!(Ulid::parse(&s).expect("parses"), u);
    }

    #[test]
    fn encodes_the_spec_example() {
        // Canonical example from the ULID spec (ulid.io/data.json): the first
        // 10 characters are the Crockford base32 timestamp, and they decode to
        // 2016-07-30T23:54:10Z — the era the spec was written in.
        let u = Ulid::parse("01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("spec example");
        assert_eq!(u.timestamp_ms(), 1_469_922_850_259);
        assert_eq!(u.to_string(), "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        let when = chrono::DateTime::from_timestamp_millis(1_469_922_850_259).expect("valid");
        assert_eq!(when.to_rfc3339(), "2016-07-30T23:54:10.259+00:00");
    }

    #[test]
    fn decodes_the_first_ten_characters_as_the_timestamp() {
        // Pinned independently of the encoder above: the first 10 characters
        // are the timestamp, the last 16 are the randomness field.
        let u = Ulid::parse("01ARZ3NDEKZZZZZZZZZZZZZZZZ").expect("valid");
        assert_eq!(u.timestamp_ms(), 1_469_922_850_259);
        assert_eq!(u.randomness(), [0xFF; 10]);
        assert_eq!(u.to_string(), "01ARZ3NDEKZZZZZZZZZZZZZZZZ");
    }

    #[test]
    fn sorts_by_creation_time() {
        let a = Ulid::from_parts(1_000, [0; 10]).expect("valid");
        let b = Ulid::from_parts(2_000, [0; 10]).expect("valid");
        assert!(a.to_string() < b.to_string());
    }

    #[test]
    fn successive_ids_strictly_increase_within_one_millisecond() {
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis() as u64;
        let ids: Vec<String> = (0..64).map(|_| new_ulid().to_string()).collect();
        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis() as u64;
        for pair in ids.windows(2) {
            assert!(pair[0] < pair[1], "{} !< {}", pair[0], pair[1]);
        }
        let first = Ulid::parse(&ids[0]).expect("parses").timestamp_ms();
        let last = Ulid::parse(&ids[ids.len() - 1])
            .expect("parses")
            .timestamp_ms();
        assert!((before..=after.max(before)).contains(&first) || first <= last);
    }

    #[test]
    fn rejects_everything_that_is_not_a_canonical_ulid() {
        assert!(!Ulid::is_valid(""));
        assert!(!Ulid::is_valid("01ARZ3NDEKTSV4RRFFQ69G5FA")); // 25 chars
        assert!(!Ulid::is_valid("01ARZ3NDEKTSV4RRFFQ69G5FAVV")); // 27 chars
        assert!(!Ulid::is_valid("01arz3ndektsv4rrffq69g5fav")); // lowercase
        assert!(!Ulid::is_valid("01ARZ3NDEKTSV4RRFFQ69G5FAU")); // 'U' excluded
        assert!(!Ulid::is_valid("01ARZ3NDEKTSV4RRFFQ69G5FAI")); // 'I' excluded
        assert!(!Ulid::is_valid("01ARZ3NDEKTSV4RRFFQ69G5FAO")); // 'O' excluded
        assert!(!Ulid::is_valid("01ARZ3NDEKTSV4RRFFQ69G5FA-")); // path separator
        assert!(!Ulid::is_valid("01ARZ3NDEKTSV4RRFFQ69G5FA/")); // path separator
        assert!(!Ulid::is_valid("../../etc/passwd"));
        assert!(!Ulid::is_valid("01ARZ3NDEKTSV4RRFFQ69G5FA\u{0}")); // NUL
        assert!(!Ulid::is_valid("8ARZ3NDEKTSV4RRFFQ69G5FAV")); // 49th bit
        assert!(Ulid::is_valid("01ARZ3NDEKTSV4RRFFQ69G5FAV"));
    }
}
