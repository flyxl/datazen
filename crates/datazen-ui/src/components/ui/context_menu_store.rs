//! `stores/contextMenuStore.ts` (T157): the web context-menu store —
//! the zustand singleton the `WebContextMenuHost` renders from and the
//! driver bridge writes to.
//!
//! React's store is a module-level `create()` singleton with two
//! actions (`show`/`hide`); `showWebContextMenu` is the plain entry the
//! bridge hands to `@datazen/driver-sdk`'s `bindContextMenuBridge`, so
//! host and driver code share one `showNativeContextMenu` implementation.
//! The Rust port keeps that shape: a `thread_local!` singleton over the
//! already-ported [`WebContextMenuState`] (T096 carries the whole
//! vertical — the item contract, `normalizeNativeMenuItems`, the
//! position math and the host — and its `show`/`hide` methods are this
//! store's actions, see the module docs there).
//!
//! The bridge itself: React injects `{ show, hide }` into the driver-sdk
//! package so drivers can call `showNativeContextMenu` without importing
//! host code. In the Rust tree the driver-sdk contracts are crate
//! modules shared by both sides, so there is nothing to inject — the
//! entry points below *are* the bridge, and a driver that links this
//! crate calls them directly.
//!
//! The store is **open-by-normalized-content only**: an item list that
//! normalizes to nothing (empty, or separators-only) never opens the
//! menu; the position arrives with the items and lands on the host
//! state verbatim.

use std::cell::RefCell;

use super::web_context_menu::{NativeMenuItemDef, WebContextMenuState};

thread_local! {
    /// The store singleton (`useContextMenuStore`).
    static CONTEXT_MENU_STORE: RefCell<WebContextMenuState> =
        RefCell::new(WebContextMenuState::default());
}

/// `showWebContextMenu` (`contextMenuStore.ts:31`): the bridge's show
/// entry — normalize the items and open at the position, or stay closed
/// when nothing survives normalization.
pub(crate) fn show_web_context_menu(items: Vec<NativeMenuItemDef>, pos: (f32, f32)) {
    CONTEXT_MENU_STORE.with_borrow_mut(|store| store.show(items, pos));
}

/// `useContextMenuStore.getState().hide()` — the bridge's hide entry.
pub(crate) fn hide_web_context_menu() {
    CONTEXT_MENU_STORE.with_borrow_mut(|store| store.hide());
}

/// Read the store (`useContextMenuStore.getState()`): open flag, cursor
/// position, normalized items, the submenu and the measured sizes the
/// host reads.
pub(crate) fn with_context_menu_state<R>(f: impl FnOnce(&WebContextMenuState) -> R) -> R {
    CONTEXT_MENU_STORE.with_borrow(|store| f(&store))
}
