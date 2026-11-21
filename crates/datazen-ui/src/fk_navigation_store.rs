//! T232: the foreign-key navigation stack — port of
//! `packages/frontend/src/stores/fkNavigationStore.ts` (231 lines).
//!
//! The back / forward history of foreign-key jumps (DB-07): following a
//! key opens a new tab, and the tab bar is not a history, so this store
//! is — the jumps' stack plus the viewport snapshot each tab left
//! behind. Three rules run through it, React's own:
//!
//! - **The stack is memory only.** A restart has no history (07 §3.5,
//!   the product decision), so nothing here persists.
//! - **A jump is recorded once.** The cycle rule (A → B → A) reuses the
//!   tab that is already open, so the stack cannot grow without bound;
//!   [`FK_NAV_MAX_DEPTH`] is only the second line of defence.
//! - **A dead tab leaves the stack.** [`FkNavigationStore::prune`] (and
//!   the skip loops inside `go_back` / `go_forward`) drop entries whose
//!   tab no longer exists, so "back" never activates a closed panel.
//!
//! A store of its own rather than a slice of the panel store, exactly as
//! React splits it (that file is at its size limit, and a navigation
//! stack depending on the panel store would be a cycle).
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `useFkNavigationStore` (the hook) | the host's [`FkNavigationStore`] field |
//! | `push` / `goBack` / `goForward` / `rewindTo` | the four same-name methods |
//! | `recordViewport` / `peekViewport` / `takeViewport` | the three same-name methods |
//! | `findEntryIdByNavKey` / `prune` / `reset` | the three same-name methods |
//! | `resetFkNavigationEntryIds` (test-only) | each Rust store owns its counter, so a fresh store already starts the ids at `fk-nav-1` |
//!
//! ## Deliberate deviations from the React source
//!
//! * **The `lib/fkNavigation.ts` surface this store needs is carried
//!   here** ([`FK_NAV_MAX_DEPTH`], [`GridViewport`]) until that lib's own
//!   ledger task lands and absorbs them — the same rule
//!   `sql_editor`'s `lib_helpers` documents for `sqlPathPrefix.ts`.
//! * **The entry ids are store-scoped, not module-global.** React's
//!   module-level `entryCounter` spans every store instance in the
//!   process; the port's counter lives in the store, so a fresh store
//!   restarts at `fk-nav-1` — the observable behaviour per store is
//!   identical (ids unique per session, never persisted).
//! * **`set` clones are gone.** The immutable re-writes React's `set`
//!   does (spread + slice) are in-place Vec moves; the return contracts
//!   (the created entry, the stepped-to entry or `None`) are the same.

use std::collections::{HashMap, HashSet};

/// React `FK_NAV_MAX_DEPTH` (`lib/fkNavigation.ts:22`): the stack's
/// depth cap; the oldest entry is dropped beyond it. Carried here until
/// the lib's own task absorbs it.
pub(crate) const FK_NAV_MAX_DEPTH: usize = 32;

/// React `GridViewport` (`lib/fkNavigation.ts:25`): the snapshot taken
/// when the grid is left. Carried here until the lib's own task absorbs
/// it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GridViewport {
    pub(crate) scroll_top: f64,
    /// Anchor row inside the page; `None` means there is no anchor
    /// (React's `anchorRow: number | null`).
    pub(crate) anchor_row: Option<usize>,
    /// Anchor column name; `None` means there is no anchor.
    pub(crate) anchor_column: Option<String>,
}

/// React `FkNavEntry['origin']`: the minimum the breadcrumb and the
/// restore need.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FkNavOrigin {
    /// The tab the jump started from (the "back" target).
    pub(crate) source_panel_id: String,
    /// The table the jump started from.
    pub(crate) source_table: String,
    /// The local columns of the foreign key, in pairing order.
    pub(crate) local_columns: Vec<String>,
    /// The table the jump landed on.
    pub(crate) target_table: String,
    /// The referenced columns, in pairing order.
    pub(crate) target_columns: Vec<String>,
}

/// One recorded jump: the tab it opened and where it came from
/// (React `FkNavEntry`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FkNavEntry {
    /// Entry id; unique per session, never persisted (`fk-nav-{n}`).
    pub(crate) id: String,
    /// The tab this jump opened.
    pub(crate) panel_id: String,
    /// `fkNavKey(target, values)` — the cycle key and the de-duplication
    /// key.
    pub(crate) nav_key: String,
    /// Where the jump came from.
    pub(crate) origin: FkNavOrigin,
}

/// The input of [`FkNavigationStore::push`]: React's
/// `Omit<FkNavEntry, 'id'>` — the jump without its id (the store mints
/// it).
pub(crate) struct FkNavJump {
    /// The tab this jump opened.
    pub(crate) panel_id: String,
    /// `fkNavKey(target, values)`.
    pub(crate) nav_key: String,
    /// Where the jump came from.
    pub(crate) origin: FkNavOrigin,
}

/// The store (`useFkNavigationState`'s state + actions): the back
/// stack, the forward stack, the truncation flag, and the viewport
/// snapshots keyed by tab id. The top of the back stack is its last
/// element, and the top of the forward stack is the same.
#[derive(Debug, Default)]
pub(crate) struct FkNavigationStore {
    back: Vec<FkNavEntry>,
    forward: Vec<FkNavEntry>,
    truncated: bool,
    viewports: HashMap<String, GridViewport>,
    entry_counter: u64,
}

impl FkNavigationStore {
    /// The empty store (React's initial state; the counter still starts
    /// at zero, so the first push mints `fk-nav-1`).
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The back stack; the top is the last element.
    pub(crate) fn back(&self) -> &[FkNavEntry] {
        &self.back
    }

    /// The forward stack; produced by `go_back`, consumed by
    /// `go_forward`.
    pub(crate) fn forward(&self) -> &[FkNavEntry] {
        &self.forward
    }

    /// True when entries were dropped because the stack exceeded its
    /// depth.
    pub(crate) fn truncated(&self) -> bool {
        self.truncated
    }

    /// Record a new jump; clears the forward stack. The depth limit
    /// drops the **oldest** entry and sets `truncated` — the breadcrumb
    /// says so instead of pretending the history is complete.
    pub(crate) fn push(&mut self, jump: FkNavJump) -> FkNavEntry {
        self.entry_counter += 1;
        let created = FkNavEntry {
            id: format!("fk-nav-{}", self.entry_counter),
            panel_id: jump.panel_id,
            nav_key: jump.nav_key,
            origin: jump.origin,
        };
        self.back.push(created.clone());
        if self.back.len() > FK_NAV_MAX_DEPTH {
            self.back.remove(0);
            self.truncated = true;
        }
        self.forward.clear();
        created
    }

    /// Step back one jump. Entries whose tab no longer exists are
    /// skipped and dropped; the answer is the entry the caller has to
    /// act on, or `None` when there is nothing to go back to.
    ///
    /// **The tab to activate is `entry.origin.source_panel_id`** — the
    /// tab the jump started from. (`entry.panel_id` is the tab the jump
    /// *opened*, which is what `go_forward` activates.)
    pub(crate) fn go_back(&mut self, existing_panel_ids: &HashSet<String>) -> Option<FkNavEntry> {
        let mut index = self.back.len();
        while index > 0 && !existing_panel_ids.contains(&self.back[index - 1].panel_id) {
            index -= 1;
        }
        if index == 0 {
            // Every entry's tab is gone: the back stack empties (the
            // forward stack stays — React's `set({ back: [], forward })`).
            if !self.back.is_empty() {
                self.back.clear();
            }
            return None;
        }
        let entry = self.back[index - 1].clone();
        // Everything above the entry (dead entries included) drops.
        self.back.truncate(index - 1);
        // The entry just left goes **on top** of the forward stack: the
        // top is the last element, so "forward" continues where "back"
        // stopped (the browser's own rule).
        self.forward.push(entry.clone());
        Some(entry)
    }

    /// Step forward one jump; the inverse of [`Self::go_back`]. **The
    /// tab to activate is `entry.panel_id`** — the tab the jump opened.
    /// The back stack is capped from the front (the oldest drop) without
    /// the truncation flag — React's own `goForward` arm.
    pub(crate) fn go_forward(&mut self, existing_panel_ids: &HashSet<String>) -> Option<FkNavEntry> {
        let mut index = self.forward.len();
        while index > 0 && !existing_panel_ids.contains(&self.forward[index - 1].panel_id) {
            index -= 1;
        }
        if index == 0 {
            if !self.forward.is_empty() {
                self.back.clear();
                self.forward.clear();
            }
            return None;
        }
        let entry = self.forward[index - 1].clone();
        self.forward.truncate(index - 1);
        self.back.push(entry.clone());
        if self.back.len() > FK_NAV_MAX_DEPTH {
            self.back.drain(0..self.back.len() - FK_NAV_MAX_DEPTH);
        }
        Some(entry)
    }

    /// Cut the back stack down to the entry (inclusive) — the cycle
    /// rule: A → B → A keeps the tab of A open but forgets the detour
    /// through B; clears `forward`.
    pub(crate) fn rewind_to(&mut self, entry_id: &str) {
        let Some(index) = self.back.iter().position(|entry| entry.id == entry_id) else {
            return;
        };
        self.back.truncate(index + 1);
        self.forward.clear();
    }

    /// Record the viewport a tab left behind; only the latest one is
    /// kept (two jumps out of the same tab must not restore two
    /// different scroll positions).
    pub(crate) fn record_viewport(&mut self, panel_id: impl Into<String>, viewport: GridViewport) {
        self.viewports.insert(panel_id.into(), viewport);
    }

    /// The viewport snapshot of a tab, if one was recorded; reading it
    /// keeps it.
    pub(crate) fn peek_viewport(&self, panel_id: &str) -> Option<&GridViewport> {
        self.viewports.get(panel_id)
    }

    /// Take the viewport snapshot of a tab; taking it consumes it — a
    /// tab that is entered twice does not restore the same scroll
    /// position twice.
    pub(crate) fn take_viewport(&mut self, panel_id: &str) -> Option<GridViewport> {
        self.viewports.remove(panel_id)
    }

    /// The entry id of a jump with this key (the newest first), or
    /// `None`.
    pub(crate) fn find_entry_id_by_nav_key(&self, nav_key: &str) -> Option<&str> {
        self.back
            .iter()
            .rev()
            .find(|entry| entry.nav_key == nav_key)
            .map(|entry| entry.id.as_str())
    }

    /// Drop every entry whose tab is gone — both stacks and the
    /// viewport snapshots.
    pub(crate) fn prune(&mut self, existing_panel_ids: &HashSet<String>) {
        self.back.retain(|entry| existing_panel_ids.contains(&entry.panel_id));
        self.forward.retain(|entry| existing_panel_ids.contains(&entry.panel_id));
        self.viewports.retain(|panel_id, _| existing_panel_ids.contains(panel_id));
    }

    /// Forget everything (the session ends): both stacks, the flag, the
    /// snapshots. The entry counter survives — React's module-global
    /// counter does too (`resetFkNavigationEntryIds` is test-only).
    pub(crate) fn reset(&mut self) {
        self.back.clear();
        self.forward.clear();
        self.truncated = false;
        self.viewports.clear();
    }
}
