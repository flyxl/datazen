//! Workspace tabs store, ported from React
//! `packages/frontend/src/stores/workspaceTabsStore.ts` (tasklist T486).
//!
//! The zustand store is state + four transitions: `open` (idempotent per
//! key, metadata-refresh in place), `close` (neighbor rule below),
//! `activate`, and `closeByWapp` (uninstall/disable: every tab of one
//! workspace app, same neighbor rule). In immediate mode the store is the
//! state: the workspace shell host owns one [`WorkspaceTabsStore`] field.
//!
//! ## The neighbor rule (both `close` and `closeByWapp`)
//!
//! When the closed tab was the active one, its **right** neighbor (now
//! sitting at the closed tab's index) is activated; if there is none, the
//! **left** one; if nothing remains, no tab is active. A `close` of an
//! inactive tab never moves the activation.

/// Stable workspace tab key: `` `${wappId}:${pageId}` ``.
pub(crate) fn workspace_tab_key(wapp_id: &str, page_id: &str) -> String {
    format!("{wapp_id}:{page_id}")
}

/// React `WorkspaceTab`: one open workspace-app page.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
pub(crate) struct WorkspaceTab {
    /// `` `${wappId}:${pageId}` `` — the store's identity.
    pub(crate) key: String,
    pub(crate) wapp_id: String,
    pub(crate) page_id: String,
    pub(crate) title: String,
    /// Optional icon id; absent in most tabs.
    pub(crate) icon: Option<String>,
    /// The page version last read.
    pub(crate) version: String,
}

impl WorkspaceTab {
    /// Build a tab with its derived key (the caller that constructed a
    /// `{wappId, pageId, …}` literal without a key).
    #[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
    pub(crate) fn with_key(
        wapp_id: impl Into<String>,
        page_id: impl Into<String>,
        title: impl Into<String>,
        icon: Option<String>,
        version: impl Into<String>,
    ) -> Self {
        let wapp_id = wapp_id.into();
        let page_id = page_id.into();
        Self {
            key: workspace_tab_key(&wapp_id, &page_id),
            wapp_id,
            page_id,
            title: title.into(),
            icon,
            version: version.into(),
        }
    }
}

/// The store (`useWorkspaceTabsStore`'s state): the open tabs and which is
/// active.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
pub(crate) struct WorkspaceTabsStore {
    pub(crate) tabs: Vec<WorkspaceTab>,
    /// The active tab's key, or none when no tab is open.
    pub(crate) active_key: Option<String>,
}

#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
impl WorkspaceTabsStore {
    /// `open(tab)`: opens (or refocuses/updates) a tab; idempotent per
    /// `key` — a key conflict refreshes the metadata **in place** (order
    /// kept) and re-activates; a new key appends and activates.
    pub(crate) fn open(&mut self, tab: WorkspaceTab) {
        match self.tabs.iter().position(|existing| existing.key == tab.key) {
            Some(index) => {
                self.tabs[index] = tab.clone();
                self.active_key = Some(tab.key);
            }
            None => {
                self.tabs.push(tab.clone());
                self.active_key = Some(tab.key);
            }
        }
    }

    /// `close(key)`: closes a tab. When the closed tab was active, activates
    /// its right neighbor first (now at `index`), otherwise the left one;
    /// `null` when nothing remains. A key that matches no tab is a no-op.
    pub(crate) fn close(&mut self, key: &str) {
        let Some(index) = self.tabs.iter().position(|tab| tab.key == key) else {
            return;
        };
        if self.active_key.as_deref() != Some(key) {
            self.tabs.remove(index);
            return;
        }
        self.tabs.remove(index);
        self.active_key = self.neighbor_after_close(index);
    }

    /// `activate(key)`: only a key that belongs to an open tab activates.
    pub(crate) fn activate(&mut self, key: &str) {
        if self.tabs.iter().any(|tab| tab.key == key) {
            self.active_key = Some(key.to_string());
        }
    }

    /// `closeByWapp(wappId)`: closes every tab of a workspace app
    /// (uninstall/disable). The same neighbor rule — but when the active
    /// tab survives (it belongs to another wapp), only the tabs change.
    pub(crate) fn close_by_wapp(&mut self, wapp_id: &str) {
        let Some(first_removed_index) = self.tabs.iter().position(|tab| tab.wapp_id == wapp_id)
        else {
            return;
        };
        let active_survives = self
            .active_key
            .as_deref()
            .map(|active| {
                self.tabs
                    .iter()
                    .any(|tab| tab.key == active && tab.wapp_id != wapp_id)
            })
            .unwrap_or(false);
        self.tabs.retain(|tab| tab.wapp_id != wapp_id);
        if active_survives {
            return;
        }
        self.active_key = self.neighbor_after_close(first_removed_index);
    }

    /// `next[index] ?? next[index - 1] ?? null` over the post-close list.
    /// `index` is the removed tab's index; after removal that slot holds
    /// the right neighbor, and the slot before it the left one.
    fn neighbor_after_close(&self, index: usize) -> Option<String> {
        self.tabs
            .get(index)
            .or_else(|| if index > 0 { self.tabs.get(index - 1) } else { None })
            .map(|tab| tab.key.clone())
    }
}
