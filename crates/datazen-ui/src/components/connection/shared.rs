//! Port of `packages/frontend/src/components/connection/shared.tsx`.
//!
//! The small helpers the connection dialogs share: a fresh id mint, the colour
//! tag data, and the visible `Label` primitive. React's `Label` is a re-export
//! of `@datazen/ui`; Rune's equivalent lives in `ui::label::Label` and is
//! re-exported here so callers keep the same import surface.

use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic id source. React mints random `conn_xxxxxxxx` ids; immediate mode
/// needs no cryptography here (ids are local UI keys), so a counter gives stable
/// uniqueness within a process. Removal stage: when a host `newId` port lands.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// React `newId`: an id prefixed `conn_`.
#[allow(dead_code)] // shared helper; consumers are the connection dialogs.
pub fn new_id() -> String {
    let n = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("conn_{n:08x}")
}

/// React `COLOR_KEYS` — the tag value (hex data) and its exact i18n key.
/// Reuses T103's `COLOR_TAGS` so there is a single definition.
#[allow(unused_imports)] // shared data; consumed by the connection dialogs.
pub use super::connection_advanced_settings::COLOR_TAGS as COLOR_KEYS;

/// React's `Label` re-export from `@datazen/ui`.
#[allow(unused_imports)] // shared primitive; consumed by the connection dialogs.
pub use crate::ui::label::Label;

// Leftover L1 (removal stage): when a Rust `newId` service exists (the host's
// id generator) drop the local counter; when `shared.tsx` is fully ported keep
// these re-exports as the single public surface for the connection dialogs.
