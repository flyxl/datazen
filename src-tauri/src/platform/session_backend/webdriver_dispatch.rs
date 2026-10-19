//! Bounded test evidence at the physical provider dispatch boundary.
//! Only dedicated fixture profiles are counted; no SQL, handles or credentials.
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
static COUNTS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
const PREFIX: &str = "monitor-webdriver-fixture-";
const MAX_PROFILES: usize = 64;
pub(super) fn dispatch(connection_id: &str) {
    if !connection_id.starts_with(PREFIX) {
        return;
    }
    let mut counts = COUNTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if counts.contains_key(connection_id) || counts.len() < MAX_PROFILES {
        let count = counts.entry(connection_id.to_owned()).or_default();
        *count = count.saturating_add(1);
    }
}
pub(crate) fn count(connection_id: &str) -> Option<u64> {
    if !connection_id.starts_with(PREFIX) {
        return None;
    }
    let counts = COUNTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    counts
        .get(connection_id)
        .copied()
        .or_else(|| (counts.len() < MAX_PROFILES).then_some(0))
}
