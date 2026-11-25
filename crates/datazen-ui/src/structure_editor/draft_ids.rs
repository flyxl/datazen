//! Port of `packages/frontend/src/lib/structureEditor/draftIds.ts`.
//!
//! React builds an id as `` `col_${Math.random().toString(36).slice(2, 8)}` ``:
//! a fixed `col_`/`idx_` prefix followed by exactly 6 base-36 characters.
//! Rune has no JS RNG, so the 6-character suffix comes from a deterministic
//! per-process `AtomicU64` counter rendered big-endian in base 36 and zero
//! padded to width 6. The character set is identical to JS `toString(36)`
//! (`0-9a-z`), the prefix and length are preserved, and every call produces a
//! distinct suffix until the counter wraps at 36^6 (2,176,782,336) allocations
//! in one process — far more than a structure-editor session can create.
//!
//! This is a documented adaptation, not a behavioural regression: the React
//! suffix is random (and can collide), while the Rune suffix is monotonic and
//! collision-free within a session. Callers only use these ids as React-style
//! draft keys, so no consumer depends on the suffix being random.

use std::sync::atomic::{AtomicU64, Ordering};

/// JS `toString(36)` alphabet, in digit order.
const ID_ALPHABET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
/// 36^6: the number of distinct 6-character base-36 suffixes.
const SUFFIX_SPACE: u64 = 36 * 36 * 36 * 36 * 36 * 36;
/// Six-character width of the React suffix (`slice(2, 8)`).
const SUFFIX_LEN: usize = 6;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// Render `value` as a big-endian, zero-padded, 6-character base-36 string.
fn suffix(value: u64) -> String {
    let mut value = value % SUFFIX_SPACE;
    let mut chars = ['0'; SUFFIX_LEN];
    for slot in chars.iter_mut().rev() {
        *slot = ID_ALPHABET[(value % 36) as usize] as char;
        value /= 36;
    }
    chars.into_iter().collect()
}

fn next_suffix() -> String {
    suffix(NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

/// Port of `newColumnId()` — `col_` + 6 base-36 characters.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub fn new_column_id() -> String {
    format!("col_{}", next_suffix())
}

/// Port of `newIndexId()` — `idx_` + 6 base-36 characters.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub fn new_index_id() -> String {
    format!("idx_{}", next_suffix())
}
