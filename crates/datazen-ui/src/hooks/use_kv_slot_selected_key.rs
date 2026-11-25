//! Port of `packages/frontend/src/hooks/useKvSlotSelectedKey.ts` (T353)
//! — the leaf-side read of a KV panel's selected key.
//!
//! The toolbar's AI button and the assistant drawer both need to know
//! which key the panel is focused on; they subscribe to that one
//! scalar through the host-owned relay instead of the workspace
//! passing it down (a click in the driver's key tree re-renders these
//! two leaves and nothing above them).
//!
//! ## What maps where
//!
//! - `useSyncExternalStore(state ? state.subscribe : NO_SUBSCRIBE,
//!   state ? state.getSelectedKey : NO_SELECTED_KEY)` is the host-owned
//!   relay seam: `state.subscribe` collapses into the host delivering
//!   into [`KvSlotSelectedKeyState::deliver`] when the relay fires
//!   (Rune leaves re-render off the state field they read — the relay
//!   never crosses into the engine), and `getSelectedKey` is the
//!   [`KvSlotSelectedKeyState::selected_key`] read.
//! - `NO_SUBSCRIBE` / `NO_SELECTED_KEY`: a `None` relay (a non-KV
//!   panel) is a subscription that never fires and a snapshot of "no
//!   key" — [`KvSlotSelectedKeyState::default`]'s `None`, verbatim.
//! - Pure read, nothing cached (the React file's own note): the state
//!   mirrors the relay's latest answer, nothing here writes back, and
//!   the facts built FROM the key live in `lib/kvAiContext.ts` (its
//!   own task).
//!
//! Known deltas: none behavioural.

/// The leaf's mirror of the relay's scalar (React's hook output).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KvSlotSelectedKeyState {
    /// The relay's latest answer: the selected key, or `None` while no
    /// relay exists (relational panel) / nothing is selected.
    selected_key: Option<String>,
}

impl KvSlotSelectedKeyState {
    /// `NO_SELECTED_KEY`: a leaf with no relay starts (and stays, until
    /// the host mounts one) at "no key".
    pub fn new() -> Self {
        Self::default()
    }

    /// The hook's read: the selected key of the panel behind the
    /// relay, or `None`.
    pub fn selected_key(&self) -> Option<&str> {
        self.selected_key.as_deref()
    }

    /// The relay fired (`subscribe`'s callback): the host delivers the
    /// fresh scalar. A `None` relay never fires — a delivered `None`
    /// is the "nothing selected" answer of a LIVE relay, which the
    /// read answers identically.
    pub fn deliver(&mut self, key: Option<String>) {
        self.selected_key = key;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leaf_without_a_relay_reads_no_key() {
        // NO_SUBSCRIBE + NO_SELECTED_KEY: never fires, null snapshot.
        let state = KvSlotSelectedKeyState::new();
        assert_eq!(state.selected_key(), None);
        assert_eq!(state, KvSlotSelectedKeyState::default());
    }

    #[test]
    fn the_relay_delivery_is_the_read() {
        let mut state = KvSlotSelectedKeyState::new();
        state.deliver(Some("users:1".to_owned()));
        assert_eq!(state.selected_key(), Some("users:1"));
        // A live relay's "nothing selected" answer reads as no key.
        state.deliver(None);
        assert_eq!(state.selected_key(), None);
        // The next key click replaces.
        state.deliver(Some("orders:9".to_owned()));
        assert_eq!(state.selected_key(), Some("orders:9"));
    }
}
