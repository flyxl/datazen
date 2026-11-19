//! Tab-closed notification, keyed by a panel's id, ported from React
//! `packages/frontend/src/stores/panelCloseNotifier.ts` (T293).
//!
//! ## Why this exists
//!
//! A tab's state inside a driver has to outlive the *view* (the host
//! renders only the active tab, so switching tabs unmounts it) but must
//! not outlive the *tab*. Only the host can tell those apart, so it has
//! to say so — and the only reliable signal is the tab disappearing,
//! not the view unmounting.
//!
//! ## Why the ids arrive as a diff
//!
//! A tab can disappear through nine different actions (`removePanel`,
//! `removeAllForConnection`, `removePanelsForRelation`,
//! `removePanelsForDatabase`, `closeOtherPanels`, `closeAllPanels`,
//! `closePanelsToTheRight`, `closePanelsToTheLeft`, ...). Firing from
//! each would be a ninth place to remember, and the first action added
//! later would silently leak. React solves it with one subscriber that
//! diffs the id set after every `set(...)` on the panel store. The Rust
//! host has no store-subscription seam, so the port takes the diff
//! directly: the caller hands the previous and next panel-id lists to
//! [`PanelCloseNotifier::panels_changed`] after any mutation — one
//! place, same guarantee, including paths that do not exist yet.
//!
//! ## Why ids are a safe signal
//!
//! `nextPanelId` is a monotonic counter and the store is memory-only,
//! so an id is never reused within a session. "Present before, absent
//! after" therefore means exactly one thing: that tab was closed.
//!
//! ## Rust shape
//!
//! * The unsubscribe closure becomes [`PanelCloseSubscription`] — a
//!   plain value the caller hands to [`PanelCloseNotifier::unsubscribe`].
//! * React's `Set<PanelCloseHandler>` dedupes by function identity; the
//!   port's `Vec` dedupes by `Rc` pointer identity ([`Rc::ptr_eq`]).
//! * React wraps each handler in `try/catch` so one driver's cleanup
//!   failing never breaks closing a tab; the port keeps that with
//!   [`std::panic::catch_unwind`] (a panicking handler is caught, the
//!   entry is still dropped first, the remaining handlers still run).
//! * Registering again for the same panel is allowed and keeps both
//!   handlers; registering for an already-closed panel is allowed too —
//!   the handler is simply never fired, which is the right outcome for
//!   a view with nothing left to clean up.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

/// One close handler (`PanelCloseHandler`).
pub(crate) type PanelCloseHandler = Rc<dyn Fn()>;

/// The unsubscribe a registration returns: hand it back to
/// [`PanelCloseNotifier::unsubscribe`].
#[derive(Clone)]
pub(crate) struct PanelCloseSubscription {
    panel_id: String,
    handler: PanelCloseHandler,
}

/// The notifier (`createPanelCloseNotifier`'s return).
#[derive(Default)]
pub(crate) struct PanelCloseNotifier {
    handlers: HashMap<String, Vec<PanelCloseHandler>>,
}

impl PanelCloseNotifier {
    /// `createPanelCloseNotifier` — in Rust the store seam is the
    /// caller's `panels_changed` call, so the constructor is plain.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `onPanelClosed`: register a close handler for a tab; keep the
    /// returned [`PanelCloseSubscription`] to unsubscribe. Registering
    /// the same handler again for the same panel is a no-op (React's
    /// `Set.add`), and a different handler is kept alongside.
    pub(crate) fn on_panel_closed(
        &mut self,
        panel_id: &str,
        handler: PanelCloseHandler,
    ) -> PanelCloseSubscription {
        let set = self.handlers.entry(panel_id.to_owned()).or_default();
        // React's Set dedupes by function identity; Rc identity is the
        // port's counterpart.
        if !set.iter().any(|existing| Rc::ptr_eq(existing, &handler)) {
            set.push(handler.clone());
        }
        PanelCloseSubscription {
            panel_id: panel_id.to_owned(),
            handler,
        }
    }

    /// The unsubscribe closure: drop the handler; an empty entry goes
    /// with it.
    pub(crate) fn unsubscribe(&mut self, subscription: &PanelCloseSubscription) {
        if let Some(set) = self.handlers.get_mut(&subscription.panel_id) {
            set.retain(|existing| !Rc::ptr_eq(existing, &subscription.handler));
            if set.is_empty() {
                self.handlers.remove(&subscription.panel_id);
            }
        }
    }

    /// The store subscriber's diff, run by the host after any panel-set
    /// mutation: a panel present before and absent after was closed, and
    /// its handlers fire. Returns the panel ids that fired (for tests
    /// and logging).
    pub(crate) fn panels_changed(&mut self, prev_ids: &[&str], next_ids: &[&str]) -> Vec<String> {
        let after: std::collections::HashSet<&str> = next_ids.iter().copied().collect();
        let mut fired = Vec::new();
        for panel_id in prev_ids {
            if after.contains(panel_id) {
                continue;
            }
            let Some(set) = self.handlers.remove(*panel_id) else {
                continue;
            };
            fired.push((*panel_id).to_owned());
            // The entry is already dropped; a panicking handler leaves no
            // registration behind and does not stop the rest.
            for handler in &set {
                let _ = catch_unwind(AssertUnwindSafe(|| handler()));
            }
        }
        fired
    }
}
