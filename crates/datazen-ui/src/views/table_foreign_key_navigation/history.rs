//! Pure port of `stores/fkNavigationStore.ts` — the back/forward stack of FK
//! jumps. The host owns one machine and translates its returned entries into
//! `setActivePanel` calls; no store, IPC or timer lives here.

use std::collections::{BTreeMap, BTreeSet};

use super::logic::{find_entry_id_by_nav_key, FK_NAV_MAX_DEPTH};
use super::model::{FkNavEntryVm, FkNavOriginVm, GridViewportVm};

/// React `FkNavigationState` + the entry id counter.
#[allow(dead_code)] // state machine; consumer: the table host (not wired).
#[derive(Default)]
pub(crate) struct FkNavigationMachine {
    pub(crate) back: Vec<FkNavEntryVm>,
    pub(crate) forward: Vec<FkNavEntryVm>,
    pub(crate) truncated: bool,
    viewports: BTreeMap<String, GridViewportVm>,
    counter: usize,
}

impl FkNavigationMachine {
    #[allow(dead_code)] // accessor; consumed by the breadcrumb and tests.
    pub(crate) fn viewports(&self) -> &BTreeMap<String, GridViewportVm> {
        &self.viewports
    }

    /// React `push`: assign a fresh id, clear forward, drop the oldest entry
    /// past the depth limit and flag truncation.
    #[allow(dead_code)] // state transition; remove with the host wiring.
    pub(crate) fn push(
        &mut self,
        panel_id: &str,
        nav_key: &str,
        origin: FkNavOriginVm,
    ) -> FkNavEntryVm {
        self.counter += 1;
        let entry = FkNavEntryVm {
            id: format!("fk-nav-{}", self.counter),
            panel_id: panel_id.to_owned(),
            nav_key: nav_key.to_owned(),
            origin,
        };
        self.back.push(entry.clone());
        self.forward.clear();
        if self.back.len() > FK_NAV_MAX_DEPTH {
            self.back.remove(0);
            self.truncated = true;
        }
        entry
    }

    /// React `goBack`: skip entries whose tab was closed, drop the dead ones
    /// above the surviving entry, and put the left entry on top of `forward`.
    #[allow(dead_code)] // state transition; remove with the host wiring.
    pub(crate) fn go_back(&mut self, existing: &BTreeSet<String>) -> Option<FkNavEntryVm> {
        let mut index = self.back.len().saturating_sub(1);
        while index < self.back.len() && !existing.contains(&self.back[index].panel_id) {
            if index == 0 {
                break;
            }
            index -= 1;
        }
        let found = self.back.get(index).filter(|e| existing.contains(&e.panel_id)).cloned();
        match found {
            Some(entry) => {
                self.back.truncate(index);
                self.forward.push(entry.clone());
                Some(entry)
            }
            None => {
                if !self.back.is_empty() {
                    self.back.clear();
                }
                None
            }
        }
    }

    /// React `goForward`: the inverse of `goBack`, with depth capping.
    #[allow(dead_code)] // state transition; remove with the host wiring.
    pub(crate) fn go_forward(&mut self, existing: &BTreeSet<String>) -> Option<FkNavEntryVm> {
        let mut index = self.forward.len().saturating_sub(1);
        while index < self.forward.len() && !existing.contains(&self.forward[index].panel_id) {
            if index == 0 {
                break;
            }
            index -= 1;
        }
        let found = self.forward.get(index).filter(|e| existing.contains(&e.panel_id)).cloned();
        match found {
            Some(entry) => {
                self.back.push(entry.clone());
                if self.back.len() > FK_NAV_MAX_DEPTH {
                    let excess = self.back.len() - FK_NAV_MAX_DEPTH;
                    self.back.drain(0..excess);
                }
                self.forward.truncate(index);
                Some(entry)
            }
            None => {
                if !self.forward.is_empty() {
                    self.forward.clear();
                }
                None
            }
        }
    }

    /// React `rewindTo`: keep only the prefix up to `entry_id`, clear forward.
    #[allow(dead_code)] // state transition; remove with the host wiring.
    pub(crate) fn rewind_to(&mut self, entry_id: &str) {
        if let Some(index) = self.back.iter().position(|entry| entry.id == entry_id) {
            self.back.truncate(index + 1);
            self.forward.clear();
        }
    }

    /// React `findEntryIdByNavKey` (newest first).
    #[allow(dead_code)] // state query; remove with the host wiring.
    pub(crate) fn find_entry_id_by_nav_key(&self, nav_key: &str) -> Option<String> {
        find_entry_id_by_nav_key(&self.back, nav_key)
    }

    /// React `prune`: drop entries/viewports of closed tabs.
    #[allow(dead_code)] // state transition; remove with the host wiring.
    pub(crate) fn prune(&mut self, existing: &BTreeSet<String>) {
        self.back.retain(|entry| existing.contains(&entry.panel_id));
        self.forward.retain(|entry| existing.contains(&entry.panel_id));
        self.viewports.retain(|panel_id, _| existing.contains(panel_id));
    }

    /// React `recordViewport`: only the latest snapshot of a tab is kept.
    #[allow(dead_code)] // state transition; remove with the host wiring.
    pub(crate) fn record_viewport(&mut self, panel_id: &str, viewport: GridViewportVm) {
        self.viewports.insert(panel_id.to_owned(), viewport);
    }

    /// React `peekViewport`: reading does not consume.
    #[allow(dead_code)] // state query; remove with the host wiring.
    pub(crate) fn peek_viewport(&self, panel_id: &str) -> Option<&GridViewportVm> {
        self.viewports.get(panel_id)
    }

    /// React `takeViewport`: reading consumes the snapshot.
    #[allow(dead_code)] // state transition; remove with the host wiring.
    pub(crate) fn take_viewport(&mut self, panel_id: &str) -> Option<GridViewportVm> {
        self.viewports.remove(panel_id)
    }

    /// React `reset`.
    #[allow(dead_code)] // state transition; remove with the host wiring.
    pub(crate) fn reset(&mut self) {
        self.back.clear();
        self.forward.clear();
        self.viewports.clear();
        self.truncated = false;
        self.counter = 0;
    }
}
