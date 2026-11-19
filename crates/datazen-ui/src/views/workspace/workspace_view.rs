//! Port of `packages/frontend/src/windows/workspace/WorkspaceView.tsx` (T200).
//!
//! The workspace mode's capstone layout: the wapp navigator rail on the
//! left, the drag seam beside it, and the right column — the independent
//! tab strip over the default card grid (while no tab is open) or over one
//! preserved [`WappPageShell`] per open tab, stacked exactly as React's
//! `absolute inset-0` children of the `relative` container stack. The
//! component also owns the `wapps:open-page` deep-link listener, the
//! fire-and-forget initial wapp-list load, and the `wapps:changed` diff
//! that closes the tabs of wapps another window disabled or uninstalled.
//!
//! The component owns no React state beyond the resizable sidebar (a
//! persisted layout size here), so the port is a pure builder composing
//! the four already-ported siblings plus the three effect bodies as
//! host-called associated functions.
//!
//! ## What maps where
//!
//! | React | Rune |
//! |---|---|
//! | `useWorkspacePages()` (the derived enabled-wapp × page list) | the `pages` parameter — the T145 [`WorkspacePageEntry`]; `workspacePages.ts`'s own port (T224) feeds it |
//! | `useWorkspaceTabsStore((s) => s.tabs)` / `((s) => s.activeKey)` | the `tabs` + `active_key` parameters (the T067 [`WorkspaceTabVm`]; the store's own port, T143/T172, feeds them) |
//! | `useWappStore((s) => s.wapps)` / `((s) => s.loaded)` | the `wapps` / `wapps_loaded` arguments of the three effect functions below (the host's store projection — the fields `AppState` already carries) |
//! | `useResizable({ initialSize: 200, minSize: 150, maxSize: 480, storageKey: 'workspace-sidebar-width' })` | the host's splitter seam: the [`WORKSPACE_SIDEBAR_WIDTH_*`](crate::constants) constants + `splitter::splitter_size` / `splitter::splitter_handle`; the view takes the measured size and the built handle through [`WorkspaceView::sidebar_width`] / [`WorkspaceView::resize_handle`] |
//! | the mount effect `if (!loaded) void fetch()` | [`WorkspaceView::initial_wapp_load`] — the host calls it once per mount (the `[]` deps are its discipline) |
//! | the `[wapps, wappsLoaded]` diff-close effect | [`WorkspaceView::resync_wapps`] — the host calls it on every wapp-list change; the visited-set semantics and the not-before-load gate are the ported body |
//! | the `wapps:open-page` listener (the EventChannel port) | [`WorkspaceView::open_page_deep_link`] — the subscription itself is the host's edge (Rune has no cross-window event bus; the `ai_write_proposal_host` precedent), and the event name (`'wapps:open-page'`, mirroring `src-tauri/src/wapps/protocol.rs`) belongs to `workspacePages.ts`'s own port (T224) |
//! | `onOpenExtensions` prop | [`WorkspaceView::on_open_extensions`] — one [`OpenExtensionsSlot`] fanned out to the navigator's and the cards' empty states, exactly React's two `<X onOpenExtensions={onOpenExtensions}>` sites |
//! | `openWappPage` (the cards' `onOpen`, the navigator's item press, the deep link's resolve) | [`WorkspaceView::on_open_page`] — one [`OpenWappPageSlot`] fanned to all three sites, as React's three call sites share the one module function; the resolve-then-open body is T224's port |
//! | the tab bar's `activate(tab.key)` / `close(tab.key)` | [`WorkspaceView::on_activate_tab`] / [`WorkspaceView::on_close_tab`] fanned into the strip |
//! | `<WappPageShell key={tab.key} tab={tab} active={…} />` | the `shell_states` parameter — one [`WappPageShellState`] per open tab keyed by tab key (the T067 hosting contract) — plus [`WorkspaceView::shell_lens`] |
//! | `WappIcon` (navigator, tab bar, cards, shell headers) | the T017 [`WappIconImages`] wire, fanned out to every sibling |
//! | the resolved iframe's surface | the [`WappPageSurface`] wire, forwarded to every shell |
//!
//! ## Honest seams (each belongs to its own later task)
//!
//! - The workspace-tabs store's action bodies — `open`'s idempotent
//!   refresh, `close`/`closeByWapp`'s neighbor rules, `activate`'s
//!   membership gate — are T143/T172's port; here they are the four slots
//!   above, and the diff-close/deep-link bodies call them exactly where
//!   React's effects do.
//! - The deep-link payload's `params` (`Record<string, string>`) is stored
//!   with the tab by the bridge consumer in F6 and is deliberately not
//!   carried here. A raw event that is not an object with string
//!   `wappId`/`pageId` fields maps to `None`s and fails the truthiness
//!   gates, exactly as React's optional chaining does.
//! - A `shell_states` miss (a host that has not mounted the tab's state
//!   yet) renders that tab's shell from the fresh-mount state — idle,
//!   loading, no watchdog — and leaves its recovery buttons unwired (the
//!   shell's own not-yet-wired case), never a panic.
//! - The lens idiom the future host writes is
//!   `move |s, key| s.shells.entry(key.to_owned()).or_default()` — the
//!   state springs into existence on the first press that needs it, as
//!   React's lazy mount does; a host that cannot resolve a key must leave
//!   the lens `None` instead of fabricating one.
//!
//! ## Keys
//!
//! React's `data-testid` values stay verbatim; the unkeyed React divs take
//! the area's Rune-side identity keys:
//!
//! | part | key |
//! |---|---|
//! | root (React's unkeyed `flex h-full min-h-0 flex-1` div) | `workspace-view` |
//! | resize handle (React `workspace-sidebar-resize`) | `workspace-sidebar-resize` — the view re-keys whatever handle the host hands it, so the splitter's own `splitter:<key>` identity yields to the React testid |
//! | right column (React's unkeyed `flex min-h-0 min-w-0 flex-1 flex-col` div) | `workspace-right` |
//! | shells stack (React's unkeyed `relative min-h-0 flex-1` div) | `workspace-shells` |
//! | per-tab stacking slot (Rune's `absolute inset-0` device) | `workspace-shell-slot-<tab.key>` (the shell root inside keeps `wapp-page-shell-<tab.key>`) |
//!
//! Style deltas (documented, per the area's rulings): pixel classes are
//! points at the 1px=1pt convention (T067); `-ml-0.5` (React's 4px handle
//! straddling the rail's 1px border) has no negative-margin counterpart —
//! the rail draws its border inside its box and the 1pt handle sits flush
//! after it, so the drag affordance lands on the seam without the overlap;
//! the fallback handle keeps React's transparent base while a host
//! splitter handle keeps the splitter's own drag fill (its established
//! skin — a visible affordance React's base lacks); `hover:bg-accent/30`
//! is the standard `El::hover` overlay at accent 0.3 alpha;
//! `transition-colors` has no counterpart; the shells' `absolute inset-0`
//! stack is one `el().absolute(0.0, 0.0).w_full().h_full()` wrapper per
//! tab (the kit scrim's own idiom) in tab order — later tabs paint above,
//! exactly React's DOM order, and the inactive shells' T067
//! `opacity(0)` + `inert()` treatment keeps the active tab's input.

#![allow(dead_code)] // reusable port; wired by the workspace host tracks

use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use rune::{col, el, row, Cursor, El};

use crate::constants::{
    WORKSPACE_SIDEBAR_WIDTH_DEFAULT, WORKSPACE_SIDEBAR_WIDTH_MAX,
    WORKSPACE_SIDEBAR_WIDTH_MIN,
};
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::wapp_icon::WappIconImages;
use crate::views::wapp_page_shell::{
    WappPageShell, WappPageShellState, WappPageSurface, WorkspaceTabVm,
};
use crate::views::workspace_default_cards::WorkspaceDefaultCards;
use crate::views::workspace_navigator::{
    OpenExtensionsSlot, OpenWappPageSlot, WorkspaceNavigator, WorkspacePageEntry,
};
use crate::views::workspace_tab_bar::{ActivateTabSlot, CloseTabSlot, WorkspaceTabBar};
use crate::views::WappSummaryVm;

/// The wapp store's `fetch` — the initial list load's host slot.
pub(crate) type FetchWappsSlot<S> = Rc<dyn Fn(&mut S)>;

/// The workspace-tabs store's `closeByWapp(wappId)` — the external-refresh
/// diff's host slot.
pub(crate) type CloseByWappSlot<S> = Rc<dyn Fn(&mut S, &str)>;

/// The host's per-tab lens: from its state and a tab key to that tab's
/// [`WappPageShellState`] (the T067 `WappShellLens` is unkeyed; this view
/// keys it because it hosts one shell per open tab). See the module docs
/// for the `entry().or_default()` idiom the host writes.
pub(crate) type WorkspaceShellLens<S> =
    Rc<dyn for<'host> Fn(&'host mut S, &str) -> &'host mut WappPageShellState>;

/// React's `OpenPageEventPayload` (`windows/workspace/workspacePages.ts`):
/// the raw `wapps:open-page` deep-link event — no Tauri envelope, the
/// EventChannel rail delivers the bare object. `params` rides the F6
/// bridge consumer and is deliberately not carried.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OpenPageEventPayload {
    pub(crate) wapp_id: Option<String>,
    pub(crate) page_id: Option<String>,
}

/// The `WorkspaceView` React props plus the Rune seams: the derived page
/// list, the store projections, the sidebar's measured size and built
/// handle, the four host press slots, the per-tab shell lens and the T017
/// picture / page-surface wires.
pub(crate) struct WorkspaceView<'a, S: 'static> {
    pages: &'a [WorkspacePageEntry],
    tabs: &'a [WorkspaceTabVm],
    shell_states: &'a BTreeMap<String, WappPageShellState>,
    active_key: Option<&'a str>,
    sidebar_width: f32,
    resize_handle: Option<El>,
    on_open_extensions: Option<OpenExtensionsSlot<S>>,
    on_open_page: Option<OpenWappPageSlot<S>>,
    on_activate_tab: Option<ActivateTabSlot<S>>,
    on_close_tab: Option<CloseTabSlot<S>>,
    shell_lens: Option<WorkspaceShellLens<S>>,
    images: Option<WappIconImages>,
    surface: Option<WappPageSurface>,
}

impl<'a, S: 'static> WorkspaceView<'a, S> {
    /// React's four store reads (`pages`, `tabs`, `activeKey` — plus the
    /// wapp projections the effect functions take) — the actions are the
    /// slots below.
    pub(crate) fn new(
        pages: &'a [WorkspacePageEntry],
        tabs: &'a [WorkspaceTabVm],
        shell_states: &'a BTreeMap<String, WappPageShellState>,
        active_key: Option<&'a str>,
    ) -> Self {
        Self {
            pages,
            tabs,
            shell_states,
            active_key,
            // React's `useResizable` first render: the hook's
            // `initialSize: 200` before the host's persisted size arrives.
            sidebar_width: WORKSPACE_SIDEBAR_WIDTH_DEFAULT as f32,
            resize_handle: None,
            on_open_extensions: None,
            on_open_page: None,
            on_activate_tab: None,
            on_close_tab: None,
            shell_lens: None,
            images: None,
            surface: None,
        }
    }

    /// The `useResizable` size (the navigator's `width` prop), in points —
    /// the host feeds `splitter::splitter_size(state,
    /// WORKSPACE_SIDEBAR_WIDTH_KEY, WORKSPACE_SIDEBAR_WIDTH_DEFAULT)`. The
    /// hook's initializer clamps a persisted value into `[minSize,
    /// maxSize]`, so the view clamps whatever arrives.
    pub(crate) fn sidebar_width(mut self, width: f32) -> Self {
        self.sidebar_width =
            width.clamp(WORKSPACE_SIDEBAR_WIDTH_MIN as f32, WORKSPACE_SIDEBAR_WIDTH_MAX as f32);
        self
    }

    /// The built drag handle — the host feeds `splitter::splitter_handle`
    /// (the `useResizable` `handleRef` div's drag wiring). The view layers
    /// the React presentation over whatever it receives: the testid key,
    /// the tooltip, the column-resize cursor and the accent hover. Without
    /// it the fallback handle renders the same presentation minus the
    /// drag.
    pub(crate) fn resize_handle(mut self, handle: El) -> Self {
        self.resize_handle = Some(handle);
        self
    }

    /// The `onOpenExtensions` prop — fanned out to the navigator's and the
    /// cards' empty-state buttons.
    pub(crate) fn on_open_extensions(mut self, slot: OpenExtensionsSlot<S>) -> Self {
        self.on_open_extensions = Some(slot);
        self
    }

    /// The `openWappPage` seam — the navigator's item presses, the cards'
    /// presses and the deep link all route through the one slot.
    pub(crate) fn on_open_page(mut self, slot: OpenWappPageSlot<S>) -> Self {
        self.on_open_page = Some(slot);
        self
    }

    /// The store's `activate(tab.key)` — fanned into the tab strip.
    pub(crate) fn on_activate_tab(mut self, slot: ActivateTabSlot<S>) -> Self {
        self.on_activate_tab = Some(slot);
        self
    }

    /// The store's `close(tab.key)` — fanned into the tab strip's close
    /// buttons.
    pub(crate) fn on_close_tab(mut self, slot: CloseTabSlot<S>) -> Self {
        self.on_close_tab = Some(slot);
        self
    }

    /// The per-tab lens the shells' reload/retry presses act through.
    pub(crate) fn shell_lens(mut self, lens: WorkspaceShellLens<S>) -> Self {
        self.shell_lens = Some(lens);
        self
    }

    /// The icons' picture wire (the T017 port's `WappIconImages`), fanned
    /// out to every sibling that draws a `WappIcon`.
    pub(crate) fn images(mut self, images: WappIconImages) -> Self {
        self.images = Some(images);
        self
    }

    /// The page-surface wire ([`WappPageSurface`]), forwarded to every
    /// shell.
    pub(crate) fn surface(mut self, surface: WappPageSurface) -> Self {
        self.surface = Some(surface);
        self
    }

    /// Build the layout at the given theme.
    pub(crate) fn build(mut self, dark: bool) -> El {
        let handle = self.resize_handle.take();
        row()
            .key("workspace-view")
            // React: `flex h-full min-h-0 flex-1`.
            .w_full()
            .h_full()
            .min_h(0.0)
            .grow(1.0)
            .child(self.navigator(dark))
            .child(self.resize_handle_el(dark, handle))
            .child(self.right_column(dark))
    }

    /// React's `<WorkspaceNavigator width={sidebarWidth}
    /// onOpenExtensions={onOpenExtensions} />` — the item presses route
    /// through the same `openWappPage` seam the cards use.
    fn navigator(&self, dark: bool) -> El {
        let mut navigator = WorkspaceNavigator::new(self.pages, self.active_key)
            .width(self.sidebar_width);
        if let Some(slot) = &self.on_open_extensions {
            navigator = navigator.on_open_extensions(Rc::clone(slot));
        }
        if let Some(slot) = &self.on_open_page {
            navigator = navigator.on_open_page(Rc::clone(slot));
        }
        if let Some(images) = &self.images {
            navigator = navigator.images(Rc::clone(images));
        }
        navigator.build(dark)
    }

    /// React's handle div: `w-1 -ml-0.5 shrink-0 cursor-col-resize
    /// bg-transparent hover:bg-accent/30 transition-colors` with
    /// `title={t('main.sidebar.resize')}`. The drag wiring (and the
    /// splitter's own drag fill) is the host handle's; the view re-skins
    /// either handle with the React identity.
    fn resize_handle_el(&self, dark: bool, handle: Option<El>) -> El {
        let k = kit(dark);
        let handle = match handle {
            Some(handle) => handle,
            // The fallback: React's transparent base, no drag.
            None => el().w(1.0).h_full().shrink(),
        };
        handle
            .key("workspace-sidebar-resize")
            .w(1.0) // w-1
            .h_full()
            .shrink() // shrink-0
            .cursor(Cursor::ResizeColumn) // cursor-col-resize
            .hover(k.alpha(k.c.bg_accent, 0.3)) // hover:bg-accent/30
            .help(t("main.sidebar.resize"))
    }

    /// React's right column: `flex min-h-0 min-w-0 flex-1 flex-col` — the
    /// tab strip over the default cards (no tab open) or the shell stack.
    fn right_column(&self, dark: bool) -> El {
        let mut column = col()
            .key("workspace-right")
            .min_h(0.0) // min-h-0
            .min_w(0.0) // min-w-0
            .grow(1.0); // flex-1
        if let Some(bar) = self.tab_bar(dark) {
            column = column.child(bar);
        }
        column.child(if self.tabs.is_empty() {
            self.default_cards(dark)
        } else {
            self.shells(dark)
        })
    }

    /// React's `<WorkspaceTabBar />` — no props in React (the strip reads
    /// the store itself); here the store's two actions and the picture
    /// wire fan in.
    fn tab_bar(&self, dark: bool) -> Option<El> {
        let mut bar = WorkspaceTabBar::new(self.tabs, self.active_key);
        if let Some(slot) = &self.on_activate_tab {
            bar = bar.on_activate(Rc::clone(slot));
        }
        if let Some(slot) = &self.on_close_tab {
            bar = bar.on_close(Rc::clone(slot));
        }
        if let Some(images) = &self.images {
            bar = bar.images(Rc::clone(images));
        }
        bar.build(dark)
    }

    /// React's `<WorkspaceDefaultCards pages={pages}
    /// onOpen={(page) => openWappPage(page.wappId, page.pageId)}
    /// onOpenExtensions={onOpenExtensions} />`.
    fn default_cards(&self, dark: bool) -> El {
        let mut cards = WorkspaceDefaultCards::new(self.pages);
        if let Some(slot) = &self.on_open_page {
            let slot = Rc::clone(slot);
            cards = cards.on_open(Rc::new(
                move |state: &mut S, page: &WorkspacePageEntry| {
                    slot(state, &page.wapp_id, &page.page_id);
                },
            ));
        }
        if let Some(slot) = &self.on_open_extensions {
            cards = cards.on_open_extensions(Rc::clone(slot));
        }
        if let Some(images) = &self.images {
            cards = cards.images(Rc::clone(images));
        }
        cards.build(dark)
    }

    /// React's shell stack: `<div className="relative min-h-0 flex-1">`
    /// with one shell per tab — see [`Self::shell`] for the stacking
    /// device.
    fn shells(&self, dark: bool) -> El {
        col()
            .key("workspace-shells")
            .grow(1.0)
            .min_h(0.0)
            .children(self.tabs.iter().map(|tab| self.shell(tab, dark)))
    }

    /// React's `<WappPageShell key={tab.key} tab={tab} active={tab.key ===
    /// activeKey} />`: the shell fills one `absolute inset-0` slot of the
    /// stack (the kit scrim's own out-of-flow idiom — `absolute(0, 0)` +
    /// `w_full` + `h_full`), so every open tab's shell overlaps in tab
    /// order and the inactive ones' T067 `opacity(0)` + `inert()` keeps
    /// the active tab's input.
    fn shell(&self, tab: &WorkspaceTabVm, dark: bool) -> El {
        let active = self.active_key == Some(tab.key.as_str());
        // The fresh-mount state for a host map miss (the module docs'
        // honest seam); the host's own states render otherwise.
        let fallback = WappPageShellState::default();
        let state = self.shell_states.get(&tab.key).unwrap_or(&fallback);
        let mut shell = WappPageShell::new(tab, state, active);
        if let Some(lens) = &self.shell_lens {
            let lens = Rc::clone(lens);
            let key = tab.key.clone();
            shell = shell.shell(Rc::new(move |state: &mut S| lens(state, &key)));
        }
        if let Some(images) = &self.images {
            shell = shell.images(Rc::clone(images));
        }
        if let Some(surface) = &self.surface {
            shell = shell.surface(Rc::clone(surface));
        }
        el()
            .key(format!("workspace-shell-slot-{}", tab.key))
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .child(shell.build(dark))
    }

    // ---- the three effect bodies (host-called) --------------------------------

    /// React's mount effect: the fire-and-forget initial wapp-list load —
    /// `if (!loaded) void fetch()`. The host runs this once per mount
    /// (React's `[]` deps are its calling discipline); the refreshed loads
    /// afterwards ride the store's own `wapps:changed` seam.
    pub(crate) fn initial_wapp_load(
        wapps_loaded: bool,
        fetch: &FetchWappsSlot<S>,
        host: &mut S,
    ) {
        if wapps_loaded {
            return;
        }
        fetch(host);
    }

    /// React's `[wapps, wappsLoaded]` effect: a `wapps:changed` refresh
    /// triggered outside this window (another window disabling or
    /// uninstalling a wapp) must also close that wapp's workspace tabs —
    /// the management page only covers its own actions. The diff only runs
    /// once the store has loaded, so the initial (possibly empty) wapp
    /// list can never close pre-existing tabs; each open tab's wapp id is
    /// visited once, in tab order, and a missing or disabled wapp closes
    /// its tabs through the store's `closeByWapp`.
    pub(crate) fn resync_wapps(
        wapps: &[WappSummaryVm],
        wapps_loaded: bool,
        open_tabs: &[WorkspaceTabVm],
        close_by_wapp: &CloseByWappSlot<S>,
        host: &mut S,
    ) {
        if !wapps_loaded {
            return;
        }
        let mut visited: HashSet<&str> = HashSet::new();
        for tab in open_tabs {
            let id = tab.wapp_id.as_str();
            if !visited.insert(id) {
                continue;
            }
            // React: `if (!wapp || !wapp.enabled) closeByWapp(id)`.
            let gone = !wapps.iter().any(|wapp| wapp.id == id && wapp.enabled);
            if gone {
                close_by_wapp(host, id);
            }
        }
    }

    /// React's `wapps:open-page` listener body: the raw deep-link payload
    /// (no Tauri envelope) opens the target wapp's page when the store can
    /// resolve it — the wapp exists and is enabled, and it contributes that
    /// page. Everything else (a missing or empty `wappId`/`pageId`, an
    /// unknown or disabled wapp, an unknown page) is ignored. Returns
    /// whether the link passed the gates and was forwarded to the
    /// `openWappPage` seam; the tab the resolve opens (and its `params`)
    /// is the workspace-tabs/T224 seam.
    pub(crate) fn open_page_deep_link(
        payload: &OpenPageEventPayload,
        wapps: &[WappSummaryVm],
        open_page: &OpenWappPageSlot<S>,
        host: &mut S,
    ) -> bool {
        // React: `const targetId = payload?.wappId; if (!targetId ||
        // !payload?.pageId) return;` — truthiness: empty strings fail too.
        let (target_id, page_id) = match (
            payload.wapp_id.as_deref(),
            payload.page_id.as_deref(),
        ) {
            (Some(target_id), Some(page_id)) => (target_id, page_id),
            _ => return false,
        };
        if target_id.is_empty() || page_id.is_empty() {
            return false;
        }
        // React: `const wapp = useWappStore.getState().byId(targetId);
        // if (!wapp || !wapp.enabled) return;`.
        let Some(wapp) = wapps.iter().find(|wapp| wapp.id == target_id) else {
            return false;
        };
        if !wapp.enabled {
            return false;
        }
        // React: `if (!wapp.pages.some((p) => p.id === payload.pageId)) return;`.
        if !wapp.pages.iter().any(|page| page.id == page_id) {
            return false;
        }
        open_page(host, target_id, page_id);
        true
    }
}
