//! Port of `packages/frontend/src/windows/workspace/workspacePages.ts` (T224).
//!
//! The workspace mode's pages derivation: the flattened "enabled wapp ×
//! contributed page" rows the navigator rail, the default card grid and the
//! deep-link resolve all share, plus the module-level pieces around it — the
//! `wapps:open-page` event name and payload, the workspace tab key builder,
//! and the resolve-then-open `openWappPage` entry point. The React file is
//! pure derivation over the wapp store's list (Rune: the `wapps` slice
//! `AppState` already carries, filled by the `wapps_state.rs` seam), so the
//! port is free functions the host calls — no state, no effects.
//!
//! ## What maps where
//!
//! | React | Rune |
//! |---|---|
//! | `WAPPS_OPEN_PAGE_EVENT` | [`WAPPS_OPEN_PAGE_EVENT`] — same literal, mirroring `crates/datazen/src/wapps/protocol.rs` |
//! | `OpenPageEventPayload` (`wappId?`, `pageId?`, `params?`) | [`OpenPageEventPayload`] — re-homed from the T200 port's mirror (the deep-link body's parameter); `params` is ignored by the React file itself (the F6 bridge consumer stores it with the tab) and is deliberately not carried |
//! | `WorkspacePageEntry` | the T145 mirror in `workspace_navigator.rs` — reused unchanged; the shape fits the derivation exactly |
//! | `workspaceTabKey` (imported from `stores/workspaceTabsStore.ts`) | [`workspace_tab_key`] — the store's own port (T143/T172 area) should reuse this, not re-spell the form |
//! | `toEntry` (private) | [`to_entry`] (private) |
//! | `deriveWorkspacePages` | [`derive_workspace_pages`] |
//! | `useWorkspacePages` (the memoized store read) | the host's call of [`derive_workspace_pages`] over `&state.wapps` per build — the `useMemo` disappears in immediate mode |
//! | `buildWorkspaceTab` | [`build_workspace_tab`] → the T067 `WorkspaceTabVm` (`wapp_page_shell.rs`) |
//! | `openWappPage` (resolve + `useWorkspaceTabsStore.getState().open(tab)`) | [`open_wapp_page`] — the wapp-store read is the `wapps` slice parameter; the tabs store's `open` action is the [`OpenWorkspaceTabSlot`] host seam (the store port is ahead, so the host wires it) |
//!
//! ## Derivation rules (the contract the tests pin)
//!
//! - only enabled wapps contribute, and only when they contribute at least
//!   one page (`p.enabled && p.pages.length > 0`);
//! - rows keep install order — the wapp list's order, each wapp's pages in
//!   their contribution order (React's `filter` + `flatMap`);
//! - `title` is `page.title || wapp.name` — truthiness, so an *empty* page
//!   title falls back to the wapp's name while a whitespace title survives;
//! - `key` is `workspaceTabKey(wapp.id, page.id)` = `${wappId}:${pageId}`;
//!   `icon` is the page's own optional icon (never the wapp package icon);
//!   `version`/`author`/`description` are the wapp's;
//! - `openWappPage`'s resolve gates: `false` for a missing wapp, a disabled
//!   wapp, a wapp with no pages, or a `pageId` no page matches — and a
//!   *falsy* `pageId` (`undefined` or the empty string) opens the wapp's
//!   first page.

#![allow(dead_code)] // reusable port; wired by the workspace host tracks

use std::rc::Rc;

use crate::views::wapp_page_shell::WorkspaceTabVm;
use crate::views::workspace_navigator::WorkspacePageEntry;
use crate::views::{WappPageSummaryVm, WappSummaryVm};

/// Mirrors `WAPPS_OPEN_PAGE_EVENT` in `crates/datazen/src/wapps/protocol.rs`:
/// the event the host emits when a `datazen://…/open?page=…` deep link
/// resolves to a contributed page. Payload: `{ wappId, pageId, params }`.
pub(crate) const WAPPS_OPEN_PAGE_EVENT: &str = "wapps:open-page";

/// React's `OpenPageEventPayload` (`windows/workspace/workspacePages.ts`):
/// the raw `wapps:open-page` deep-link event — no Tauri envelope, the
/// EventChannel rail delivers the bare object. `params` (startup params the
/// bridge forwards to the wapp page) is ignored by the React file itself and
/// rides the F6 bridge consumer, so it is deliberately not carried.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OpenPageEventPayload {
    pub(crate) wapp_id: Option<String>,
    pub(crate) page_id: Option<String>,
}

/// React's `workspaceTabKey` (`stores/workspaceTabsStore.ts`): the stable
/// `${wappId}:${pageId}` key an open tab and its entry share. Declared here
/// because that is the file that spells it; the workspace-tabs store's own
/// port (T143/T172 area) should reuse this instead of re-spelling the form.
pub(crate) fn workspace_tab_key(wapp_id: &str, page_id: &str) -> String {
    format!("{wapp_id}:{page_id}")
}

/// React's `toEntry`: one flattened row from an enabled wapp and one of its
/// contributed pages. `title: page.title || wapp.name` — truthiness, so an
/// empty page title falls back to the wapp's name.
fn to_entry(wapp: &WappSummaryVm, page: &WappPageSummaryVm) -> WorkspacePageEntry {
    let title = if page.title.is_empty() {
        wapp.name.clone()
    } else {
        page.title.clone()
    };
    WorkspacePageEntry {
        key: workspace_tab_key(&wapp.id, &page.id),
        wapp_id: wapp.id.clone(),
        page_id: page.id.clone(),
        title,
        // React: `icon: page.icon` — the page contribution's own icon, not
        // the wapp package icon.
        icon: page.icon.clone(),
        version: wapp.version.clone(),
        author: wapp.author.clone(),
        description: wapp.description.clone(),
    }
}

/// React's `deriveWorkspacePages`: all pages contributed by enabled wapps,
/// in install order — the wapp list's order, each wapp's pages in their
/// contribution order. This is the body `useWorkspacePages` memoizes; the
/// host calls it over its wapps slice whenever it builds the workspace.
pub(crate) fn derive_workspace_pages(wapps: &[WappSummaryVm]) -> Vec<WorkspacePageEntry> {
    wapps
        .iter()
        .filter(|wapp| wapp.enabled && !wapp.pages.is_empty())
        .flat_map(|wapp| wapp.pages.iter().map(|page| to_entry(wapp, page)))
        .collect()
}

/// React's `buildWorkspaceTab`: the workspace tab a resolved page opens —
/// the entry's fields minus `author`/`description` (the tab carries
/// neither), i.e. the T067 `WorkspaceTabVm` payload.
pub(crate) fn build_workspace_tab(
    wapp: &WappSummaryVm,
    page: &WappPageSummaryVm,
) -> WorkspaceTabVm {
    let entry = to_entry(wapp, page);
    WorkspaceTabVm {
        key: entry.key,
        wapp_id: entry.wapp_id,
        page_id: entry.page_id,
        title: entry.title,
        icon: entry.icon,
        version: entry.version,
    }
}

/// The workspace-tabs store's `open(tab)` — the host seam the resolve hands
/// its built tab to (React: `useWorkspaceTabsStore.getState().open(tab)`).
pub(crate) type OpenWorkspaceTabSlot<S> = Rc<dyn Fn(&mut S, WorkspaceTabVm)>;

/// React's `openWappPage`: resolve + open a workspace app page tab
/// (activating it — the store's `open` is idempotent per key). Returns
/// `false` when the wapp is missing/disabled or has no such page — callers
/// decide whether to surface that to the user. The wapp-store read is the
/// `wapps` slice parameter; the tabs-store write is the [`OpenWorkspaceTabSlot`].
pub(crate) fn open_wapp_page<S: 'static>(
    wapps: &[WappSummaryVm],
    wapp_id: &str,
    page_id: Option<&str>,
    open: &OpenWorkspaceTabSlot<S>,
    host: &mut S,
) -> bool {
    // React: `const wapp = useWappStore.getState().byId(wappId);
    // if (!wapp?.enabled || wapp.pages.length === 0) return false;`
    let Some(wapp) = wapps.iter().find(|wapp| wapp.id == wapp_id) else {
        return false;
    };
    if !wapp.enabled || wapp.pages.is_empty() {
        return false;
    }
    // React: `const page = pageId ? wapp.pages.find((p) => p.id === pageId)
    // : wapp.pages[0];` — truthiness: a falsy pageId (undefined *or the
    // empty string*) means the wapp's first page.
    let page = match page_id.filter(|id| !id.is_empty()) {
        Some(page_id) => match wapp.pages.iter().find(|page| page.id == page_id) {
            Some(page) => page,
            // React: `if (!page) return false;`
            None => return false,
        },
        // The gate above guarantees a first page; the honest `None` arm (not
        // an unwrap) keeps that invariant local to this line.
        None => match wapp.pages.first() {
            Some(page) => page,
            None => return false,
        },
    };
    open(host, build_workspace_tab(wapp, page));
    true
}
