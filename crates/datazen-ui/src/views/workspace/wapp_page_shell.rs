//! Port of `packages/frontend/src/windows/workspace/WappPageShell.tsx` (T067).
//!
//! The host-side shell around a sandboxed wapp page: the tab header (the
//! wapp icon, the title, the `v{version}` chip), the lazy entry resolve (an
//! inline `entry` on the summary payload first, then the full manifest,
//! memoized per wapp id while the version holds), the 10s load watchdog with
//! its reload/retry recovery, and the loading/missing/ready body branches.
//! Lifecycle in React: lazy-mount on first activation → CSS-hidden (instance
//! preserved) while inactive → unmounted together with its tab (shell key =
//! tab key). Rune keeps one [`WappPageShellState`] per open tab in the host
//! state, which preserves the instance by construction.
//!
//! ## What maps where
//!
//! | React | Rune |
//! |---|---|
//! | module-scope `entryCache` + `clearWappEntryCache` | [`WappEntryCache`], one per host and shared by every shell (keyed by wapp id; [`WappEntryCacheHit`] is React's `EntryCacheHit`); `clear` is the same test seam |
//! | `resolveEntry` (inline summary `entry`, else `getWappManifest`) | [`WappPageShellState::begin_resolve`] — the `[everActivated, phase, tab.wappId, tab.version]` effect's gate — and [`WappPageShellState::resolve_answer`], the host's manifest-fetch landing; the inline branch resolves synchronously ([`ResolveStart`]) |
//! | `useState` ×5 + `resolvingRef` | [`WappPageShellState`]: `ever_activated`, `phase`, `loaded`, `failed`, `reload_nonce`, `resolving` |
//! | `useState(active)` (the `everActivated` initializer) | [`WappPageShellState::new`]'s `active` parameter |
//! | `useEffect(() => { if (active) setEverActivated(true) }, [active])` | [`WappPageShellState::sync_activation`] (never retracts — React's effect only sets true) |
//! | the load-watchdog effect (`[phase, failed, loaded, reloadNonce]`, `setTimeout(10s)`) | the `watchdog_until` deadline: every dep-changing transition re-runs `resync_watchdog` (arm/disarm + `tx.wake_after`), and the host's poll fires it with [`WappPageShellState::tick`] — the `saved_until` deadline pattern |
//! | `handleReload` | [`WappPageShellState::reload`], wired to both buttons' presses through the host's lens ([`WappPageShell::shell`]) |
//! | the iframe's `onLoad` / `onError` | [`WappPageShellState::on_load`] / [`WappPageShellState::on_error`] — the host's page surface calls them |
//! | the `<iframe>` (`sandbox="allow-scripts"`, `src`, `title`) | the [`WappPageSurface`] wire: the host's `El` for the resolved src mounts inside the keyed iframe box; `None` (no production host yet) is the transparent empty box (`bg-transparent border-0`) |
//! | `buildSrc` → `buildWappAssetSrc` (the desktop default) | [`build_page_src`], byte-identical: `datazen://{id}/{entry}?v={encodeURIComponent(version)}` with one leading `./` stripped (the shared `wapp_icon` helper); the web form and the assembly-time resolver registry are `lib/wappAssetOrigin.ts`'s own port (ahead) |
//! | `WappIcon` (`h-3.5 w-3.5`) | the T017 port at 14pt, its `broken` projection from the shell's own [`WappIconState`] ([`WappPageShellState::sync_icon`] runs its `[icon]` effect) |
//! | `active` prop (`hidden` class + `aria-hidden`) | `opacity(0)` + `inert()` + `a11y_hidden()` on the root: Rune has no `display:none`, so the inactive shell paints nothing, passes input through and leaves the a11y tree while its element identity (and the state behind it) persists — the "instance preserved" React's test pins |
//! | `t('workspace.shell.loadFailed')` / `t('workspace.shell.reload')` | `crate::i18n::t` with the same keys |
//!
//! ## Honest seams (each belongs to its own later task)
//!
//! - The manifest fetch (`wappCommands.getWappManifest`) and the wapp store's
//!   `byId` are host concerns with no adapter method yet: `begin_resolve`'s
//!   `FetchManifest` answer and `resolve_answer`'s payload are the wiring
//!   surface (the `wapps_state.rs` seam style — nothing here fakes a
//!   manifest). The inline `entry` is React's duck-typed probe (`'entry' in
//!   summary`, a field beyond the `WappSummary` TS type the server payload
//!   carries); the host passes it as [`WappInlineEntry`].
//! - The F6 RPC bridge (`attachBridge`: one attach per ready iframe with the
//!   manifest permissions and the settings locale, the
//!   `datazen:theme-pack-changed` listener, the documentElement `class`
//!   MutationObserver) is `lib/wappBridge.ts`'s own port (ahead): with no
//!   iframe there is nothing to attach to, and the permissions/locale ride
//!   that wiring.
//! - `absolute inset-0` (the workspace's stacking context) becomes "fills the
//!   slot the host mounts it in" (`w_full` + `grow`): the overlay stacking of
//!   several shells is the workspace-tabs host's concern (the T143/T172
//!   area).
//! - A live shell does not re-resolve when its tab's `version` prop changes:
//!   React's effect re-runs but the `phase.kind !== 'idle'` gate holds, so
//!   the src keeps the version it was built with; only a fresh mount reads
//!   the new version through the cache/resolve. Mirrored, not fixed.
//! - `sandbox="allow-scripts"` has no Rune counterpart; the sandbox policy
//!   rides the future webview surface.
//!
//! ## Keys
//!
//! React's `data-testid="wapp-page-shell"` + `data-shell-key={tab.key}` merge
//! into one namespaced root key (the `wapp_card` precedent: the single-key
//! runtime cannot host two shells under one testid, and the workspace stacks
//! one shell per open wapp tab). The sub-testids namespace off the root the
//! same way.
//!
//! | part | key |
//! |---|---|
//! | root (testid + `data-shell-key`) | `wapp-page-shell-<tab.key>` |
//! | header / icon / title / version chip | `-header`, `-icon` (the T017 port's key), `-title`, `-version` |
//! | header failure note / reload button | `-header-failed`, `-reload` (React `wapp-shell-reload`) |
//! | body / loading / spinner | `-body`, `-loading` (React `wapp-shell-loading`), `-loading-spinner` |
//! | iframe (React `wapp-iframe`, React key `${tab.key}#${nonce}`) | `-iframe`, with `#<nonce>` suffixed from the first reload on (the remount identity) |
//! | missing block / failure text / retry button | `-missing`, `-missing-failed`, `-retry` (React `wapp-shell-retry`) |

#![allow(dead_code)] // reusable port; wired by the workspace wapp tracks

use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use rune::{col, el, row, text, El, TextStyle, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::ICON_ROTATE_CW;
use crate::ui::badge::{Badge, BadgeTone};
use crate::ui::kit::kit;
use crate::ui::palette;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};
use crate::views::wapp_icon::{strip_leading_dot_slash, WappIcon, WappIconImages, WappIconState};

/// React `LOAD_TIMEOUT_MS` (10 000 ms), in the runtime's seconds.
pub const LOAD_TIMEOUT_S: f64 = 10.0;

/// React's `WorkspaceTab` (`stores/workspaceTabsStore.ts`): the tab payload a
/// shell hosts — the stable `key` (`workspaceTabKey(wappId, pageId)`), the
/// wapp identity, the page's title/icon/version. The workspace-tabs store's
/// own port (T143/T172 area) is ahead, so the future host feeds this view
/// model; `page_id` only rides the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceTabVm {
    /// `${wappId}:${pageId}` — the shell's identity (React unmounts the shell
    /// with its tab, keyed by this).
    pub key: String,
    pub wapp_id: String,
    pub page_id: String,
    pub title: String,
    /// The page contribution's optional icon path (the header `WappIcon`).
    pub icon: Option<String>,
    pub version: String,
}

/// React's `EntryCacheHit`: one memoized manifest entry, reused while the
/// wapp's version is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WappEntryCacheHit {
    /// The version the entry was resolved under (the summary's version for
    /// the inline branch, the manifest's for the fetch branch).
    pub version: String,
    pub entry: String,
}

/// React's module-scope `entryCache` — resolved page entries keyed by wapp id.
/// One instance lives in the host state and is shared by every shell (the
/// module singleton's scope); [`WappEntryCache::clear`] is the
/// `clearWappEntryCache` test seam.
#[derive(Clone, Debug, Default)]
pub struct WappEntryCache {
    hits: BTreeMap<String, WappEntryCacheHit>,
}

impl WappEntryCache {
    /// React's `clearWappEntryCache()`: the test seam that resets the
    /// memoized entries.
    pub fn clear(&mut self) {
        self.hits.clear();
    }

    /// React's `entryCache.get(wappId)`.
    pub fn hit(&self, wapp_id: &str) -> Option<&WappEntryCacheHit> {
        self.hits.get(wapp_id)
    }

    /// React's `entryCache.set(wappId, { version, entry })`.
    pub fn set(&mut self, wapp_id: &str, version: &str, entry: &str) {
        self.hits.insert(
            wapp_id.to_owned(),
            WappEntryCacheHit {
                version: version.to_owned(),
                entry: entry.to_owned(),
            },
        );
    }

    /// React's `cachedSrc(tab)`: a hit under the tab's version builds the
    /// src; a miss (or a version change) is `null`.
    pub fn cached_src(&self, tab: &WorkspaceTabVm) -> Option<String> {
        let hit = self.hit(&tab.wapp_id)?;
        if hit.version != tab.version {
            return None;
        }
        Some(build_page_src(&tab.wapp_id, &hit.entry, &tab.version))
    }
}

/// React's `EntryPhase`: idle → resolving → ready (with the src) / missing.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum EntryPhase {
    #[default]
    Idle,
    Resolving,
    Ready { src: String },
    Missing,
}

/// React `DESKTOP_WAPP_ASSET_RESOLVER.buildSrc` — the desktop wapp asset
/// protocol's page-entry URL: `datazen://{id}/{entry}?v={version}`, one
/// leading `./` stripped from the entry and the version
/// `encodeURIComponent`-escaped. Byte-identical to the form React's test pins
/// (`datazen://acme.bill-audit/index.html?v=1.0.0`); the web form and the
/// resolver registry are `lib/wappAssetOrigin.ts`'s own port (ahead), and
/// this seam re-points there when it lands.
pub fn build_page_src(target_id: &str, entry: &str, version: &str) -> String {
    format!(
        "datazen://{}/{}?v={}",
        target_id,
        strip_leading_dot_slash(entry),
        encode_uri_component(version)
    )
}

/// JavaScript's `encodeURIComponent` (React's `encodeVersion`): every byte
/// outside `A-Z a-z 0-9 - _ . ! ~ * ' ( )` percent-encodes; UTF-8 bytes pass
/// through verbatim (multi-byte sequences encode per byte, as JS does).
fn encode_uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The duck-typed `entry` the wapp summary payload may carry beyond its TS
/// type (React's `'entry' in summary` probe): the inline page entry with the
/// version it was declared under — `resolveEntry`'s preference, no manifest
/// round-trip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WappInlineEntry {
    pub version: String,
    pub entry: String,
}

/// What `begin_resolve` did — the honest answer the host acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveStart {
    /// The effect's gate held (not yet activated, or the phase is past idle,
    /// or a resolve is already in flight): nothing started.
    NotStarted,
    /// The summary's inline entry answered: the phase is `Ready` already.
    ResolvedFromSummary,
    /// The phase is `Resolving` and the host must run the manifest fetch,
    /// answering with `resolve_answer`.
    FetchManifest,
}

/// React's `WappPageShell` local state — the five `useState`s, the
/// `resolvingRef` latch, the load-watchdog deadline and the header icon
/// site's own [`WappIconState`] — kept in the host state, one per open tab.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WappPageShellState {
    /// `useState(active)` + the `[active]` effect: latched on first
    /// activation, never retracted.
    ever_activated: bool,
    /// The entry resolve's phase.
    phase: EntryPhase,
    /// The iframe's `onLoad` — a frame that signalled load cannot time out.
    loaded: bool,
    /// The watchdog's fire or the iframe's `onError`.
    failed: bool,
    /// `reloadNonce`: bumped per reload, the iframe's remount identity.
    reload_nonce: u32,
    /// `resolvingRef`: one resolve at a time.
    resolving: bool,
    /// The load watchdog's deadline (`now + LOAD_TIMEOUT_S` while armed).
    watchdog_until: Option<f64>,
    /// The header `WappIcon`'s own state (the T017 port's contract).
    icon: WappIconState,
}

impl WappPageShellState {
    /// The component's mount: the `useState` initializer — `everActivated`
    /// starts at the mount-time `active`, and a cache hit under the tab's
    /// version starts the phase `Ready` (anything else `Idle`) — plus the
    /// load-watchdog effect's mount run (armed for a ready mount).
    pub fn new(
        tab: &WorkspaceTabVm,
        active: bool,
        cache: &WappEntryCache,
        tx: &mut Tx,
    ) -> Self {
        let mut state = Self {
            ever_activated: active,
            phase: cache
                .cached_src(tab)
                .map(|src| EntryPhase::Ready { src })
                .unwrap_or_default(),
            loaded: false,
            failed: false,
            reload_nonce: 0,
            resolving: false,
            watchdog_until: None,
            icon: WappIconState::default(),
        };
        state.resync_watchdog(tx);
        state
    }

    /// The `useEffect(() => { if (active) setEverActivated(true) }, [active])`
    /// resync: the host runs it with the tab's `active` prop each time it
    /// changes (the latch never retracts).
    pub fn sync_activation(&mut self, active: bool) {
        if active {
            self.ever_activated = true;
        }
    }

    /// The header `WappIcon`'s `[icon]` effect — the T017 port's contract:
    /// the host runs it with the tab's `icon` value whenever it changes.
    pub fn sync_icon(&mut self, icon: Option<&str>) {
        self.icon.sync_icon(icon);
    }

    /// The entry-resolve effect's run (`[everActivated, phase, tab.wappId,
    /// tab.version]`): gated exactly as React gates it, it moves the phase to
    /// `Resolving` and either resolves from the summary's inline entry
    /// (synchronously — React's microtask) or asks the host for the manifest.
    pub fn begin_resolve(
        &mut self,
        tab: &WorkspaceTabVm,
        inline: Option<&WappInlineEntry>,
        cache: &mut WappEntryCache,
        tx: &mut Tx,
    ) -> ResolveStart {
        // React: `if (!everActivated || phase.kind !== 'idle' ||
        // resolvingRef.current) return;`
        if !self.ever_activated || self.phase != EntryPhase::Idle || self.resolving {
            return ResolveStart::NotStarted;
        }
        self.resolving = true;
        self.phase = EntryPhase::Resolving;
        // resolveEntry's inline branch: a non-empty string `entry` on the
        // summary payload answers from the store, cached under the summary's
        // version, without a manifest round-trip.
        if let Some(WappInlineEntry { version, entry }) =
            inline.filter(|inline| !inline.entry.is_empty())
        {
            cache.set(&tab.wapp_id, version, entry);
            self.phase = EntryPhase::Ready {
                src: build_page_src(&tab.wapp_id, entry, &tab.version),
            };
            self.resolving = false;
            self.resync_watchdog(tx);
            return ResolveStart::ResolvedFromSummary;
        }
        // The manifest branch is the host's fetch (`getWappManifest`); the
        // answer lands in `resolve_answer`.
        self.resync_watchdog(tx);
        ResolveStart::FetchManifest
    }

    /// The manifest fetch's landing (the promise's `.then`/`.catch`): a
    /// manifest with a non-empty entry caches it (under the *manifest's*
    /// version) and builds the src from the *tab's* version; a failed fetch
    /// or an entry-less manifest (`!manifest.entry` is falsy for `""` too)
    /// lands in the missing state — React's catch swallows the message, so
    /// the phase carries none.
    pub fn resolve_answer(
        &mut self,
        tab: &WorkspaceTabVm,
        manifest: Option<(&str, &str)>,
        cache: &mut WappEntryCache,
        tx: &mut Tx,
    ) {
        // The promise chain answers once per live resolve (`resolvingRef`'s
        // `finally` clears the latch; a second answer has no effect).
        if !self.resolving {
            return;
        }
        self.resolving = false;
        match manifest.filter(|(_, entry)| !entry.is_empty()) {
            Some((version, entry)) => {
                cache.set(&tab.wapp_id, version, entry);
                self.phase = EntryPhase::Ready {
                    src: build_page_src(&tab.wapp_id, entry, &tab.version),
                };
            }
            None => self.phase = EntryPhase::Missing,
        }
        self.resync_watchdog(tx);
    }

    /// The iframe's `onLoad`: a loaded frame disarms the watchdog (the
    /// effect's cleanup on the `loaded` dep).
    pub fn on_load(&mut self) {
        self.loaded = true;
        self.watchdog_until = None;
    }

    /// The iframe's `onError`: the failure state, the watchdog disarmed.
    pub fn on_error(&mut self) {
        self.failed = true;
        self.watchdog_until = None;
    }

    /// React's `handleReload`: clear the failure, clear the load, bump the
    /// remount nonce, and reopen a missing shell (the phase back to idle so
    /// the resolve runs again). A ready shell keeps its src — only the frame
    /// remounts.
    pub fn reload(&mut self, tx: &mut Tx) {
        self.failed = false;
        self.loaded = false;
        self.reload_nonce = self.reload_nonce.wrapping_add(1);
        if self.phase == EntryPhase::Missing {
            self.phase = EntryPhase::Idle;
        }
        self.resync_watchdog(tx);
    }

    /// The host's poll (the watchdog timer's fire): a fresh frame that
    /// outlived the budget flips into the failure state. Returns whether it
    /// fired.
    pub fn tick(&mut self, now: f64) -> bool {
        if self.watchdog_until.is_some_and(|deadline| now >= deadline) {
            self.watchdog_until = None;
            self.failed = true;
            return true;
        }
        false
    }

    /// The load-watchdog effect's body, re-run at every dep change
    /// (`[phase, failed, loaded, reloadNonce]` — every transition here calls
    /// it): armed exactly while a fresh frame is showing (ready, not failed,
    /// not loaded), the 10s budget re-based at each change, with the wake
    /// scheduled so the host's poll can fire it.
    fn resync_watchdog(&mut self, tx: &mut Tx) {
        let now = tx.clock();
        self.watchdog_until = (matches!(self.phase, EntryPhase::Ready { .. })
            && !self.failed
            && !self.loaded)
            .then(|| now + LOAD_TIMEOUT_S);
        if self.watchdog_until.is_some() {
            tx.wake_after(Duration::from_secs_f64(LOAD_TIMEOUT_S));
        }
    }

    /// The current entry phase (the render's branch input).
    pub fn phase(&self) -> &EntryPhase {
        &self.phase
    }

    /// Whether the failure/recovery rows show.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// Whether the frame signalled load.
    pub fn loaded(&self) -> bool {
        self.loaded
    }

    /// The reload count (the iframe's remount identity).
    pub fn reload_nonce(&self) -> u32 {
        self.reload_nonce
    }

    /// Whether the tab has ever been active (the lazy-mount latch).
    pub fn ever_activated(&self) -> bool {
        self.ever_activated
    }
}

/// The host's page-surface wire: the `El` to mount inside the iframe box for
/// the resolved entry src — the future sandboxed webview layer — or `None`
/// when the host has no surface for it (the box renders empty: React's
/// iframe before its document arrives; `bg-transparent border-0`). Kit
/// markdown's `images` callback is the pattern.
pub type WappPageSurface = Rc<dyn Fn(&str) -> Option<El>>;

/// The host's lens from its state `S` to this shell's
/// [`WappPageShellState`] (the workspace host keeps one per open wapp tab,
/// keyed by tab key) — what the reload/retry presses act through.
pub type WappShellLens<S> = Rc<dyn Fn(&mut S) -> &mut WappPageShellState>;

/// The `WappPageShell` React props plus the Rune seams: the `active` prop,
/// the state projections, the two host wires and the lens the buttons use.
pub struct WappPageShell<'a, S: 'static> {
    tab: &'a WorkspaceTabVm,
    state: &'a WappPageShellState,
    active: bool,
    images: Option<WappIconImages>,
    surface: Option<WappPageSurface>,
    lens: Option<WappShellLens<S>>,
}

impl<'a, S: 'static> WappPageShell<'a, S> {
    /// React's `WappPageShellProps` (`tab`, `active`).
    pub fn new(tab: &'a WorkspaceTabVm, state: &'a WappPageShellState, active: bool) -> Self {
        Self {
            tab,
            state,
            active,
            images: None,
            surface: None,
            lens: None,
        }
    }

    /// The header icon's picture wire (the T017 port's `WappIconImages`).
    pub fn images(mut self, images: WappIconImages) -> Self {
        self.images = Some(images);
        self
    }

    /// The page-surface wire ([`WappPageSurface`]).
    pub fn surface(mut self, surface: WappPageSurface) -> Self {
        self.surface = Some(surface);
        self
    }

    /// The lens the reload/retry presses run `handleReload` through. Without
    /// it the buttons render but their presses no-op — the not-yet-wired
    /// host's case.
    pub fn shell(mut self, lens: WappShellLens<S>) -> Self {
        self.lens = Some(lens);
        self
    }

    /// Build the shell at the given theme.
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let root_key = format!("wapp-page-shell-{}", self.tab.key);
        let mut root = col()
            .key(root_key.clone())
            // React: `absolute inset-0 flex min-h-0 flex-col bg-surface` —
            // the shell fills the slot the host mounts it in (the stacking
            // is the workspace-tabs host's concern; the seam note above).
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .fill(k.c.bg_base)
            .child(self.header(k, dark, &root_key))
            .child(self.body(k, dark, &root_key));
        if !self.active {
            // React: the `hidden` class + `aria-hidden={true}` — mounted but
            // invisible, untouchable and out of the a11y tree, the element
            // identity (and the state behind it) preserved.
            root = root.opacity(0.0).inert().a11y_hidden();
        }
        root
    }

    /// React: `flex h-9 shrink-0 items-center gap-2 border-b border-edge px-3`
    /// — the icon, the title, the version chip, then (only when failed) the
    /// right-packed failure note and reload button.
    fn header(&self, k: &'static Kit, dark: bool, root_key: &str) -> El {
        let mut header = row()
            .key(format!("{root_key}-header"))
            .h(36.0) // h-9
            .shrink()
            .items_center()
            .gap(8.0) // gap-2
            .px(12.0) // px-3
            // border-b border-edge (border-box: the hairline draws inside
            // the 36pt, no layout room).
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base);

        let mut icon = WappIcon::new(Some(&self.tab.wapp_id), self.tab.icon.as_deref())
            .key(format!("{root_key}-icon"))
            .size_pt(14.0) // h-3.5 w-3.5
            .broken(self.state.icon.is_broken());
        if let Some(images) = &self.images {
            icon = icon.images(Rc::clone(images));
        }
        header = header.child(icon.build(dark));

        header = header.child(
            text(self.tab.title.clone())
                .key(format!("{root_key}-title"))
                // React: `truncate text-xs font-semibold text-fg`.
                .font(TextStyle::new(12.0, 600.0))
                .color(k.c.text_base)
                .min_w(0.0)
                .truncate(),
        );

        header = header.child(
            Badge::new(format!("{root_key}-version"), format!("v{}", self.tab.version))
                .tone(BadgeTone::Neutral)
                // React's `className` overrides: `px-1.5 py-0 text-[10px]
                // font-normal` over the badge base.
                .pad(6.0, 0.0)
                .font(TextStyle::new(10.0, 400.0).tracking(0.25))
                .build(dark),
        );

        if self.state.failed {
            header = header
                // React: the note's `ml-auto` — the free space to its left.
                .child(col().key(format!("{root_key}-spacer")).grow(1.0))
                .child(
                    text(t("workspace.shell.loadFailed"))
                        .key(format!("{root_key}-header-failed"))
                        // React: `text-[11px] text-red-400`.
                        .font(k.ty.small)
                        .color(palette::danger_text(dark)),
                )
                .child(self.action_button(k, format!("{root_key}-reload")));
        }
        header
    }

    /// React: `relative min-h-0 flex-1` — the ready iframe, the missing
    /// recovery, or the loading spinner.
    fn body(&self, k: &'static Kit, dark: bool, root_key: &str) -> El {
        let branch = match self.state.phase() {
            EntryPhase::Ready { src } => self.iframe(src, root_key),
            EntryPhase::Missing => self.missing(k, dark, root_key),
            EntryPhase::Idle | EntryPhase::Resolving => self.loading(dark, root_key),
        };
        col()
            .key(format!("{root_key}-body"))
            .grow(1.0)
            .min_h(0.0)
            .child(branch)
    }

    /// The sandboxed page: React's `<iframe data-testid="wapp-iframe"
    /// title={tab.title} sandbox="allow-scripts" src={phase.src}
    /// key={reloadNonce > 0 ? `${tab.key}#${reloadNonce}` : tab.key}
    /// className="h-full w-full border-0 bg-transparent">` — the keyed box
    /// (the reload nonce suffix is the remount identity), the tab's title as
    /// its accessible name, and the host's surface for the resolved src
    /// inside (`None`: the empty transparent box).
    fn iframe(&self, src: &str, root_key: &str) -> El {
        let nonce = self.state.reload_nonce();
        let key = if nonce > 0 {
            format!("{root_key}-iframe#{nonce}")
        } else {
            format!("{root_key}-iframe")
        };
        let mut iframe = el()
            .key(key)
            .w_full()
            .h_full()
            .label(self.tab.title.clone());
        if let Some(surface) = self.surface.as_ref().and_then(|surface| surface(src)) {
            iframe = iframe.child(surface);
        }
        iframe
    }

    /// React's missing branch: `flex h-full flex-col items-center
    /// justify-center gap-3` — the failure text and the retry button.
    fn missing(&self, k: &'static Kit, dark: bool, root_key: &str) -> El {
        col()
            .key(format!("{root_key}-missing"))
            .w_full()
            .h_full()
            .center()
            .gap(12.0) // gap-3
            .child(
                text(t("workspace.shell.loadFailed"))
                    .key(format!("{root_key}-missing-failed"))
                    // React: `text-sm text-red-400`.
                    .font(k.ty.body)
                    .color(palette::danger_text(dark)),
            )
            .child(self.action_button(k, format!("{root_key}-retry")))
    }

    /// React's loading branch: `flex h-full items-center justify-center
    /// text-fg-muted` around `<Spinner size="xl" />` — the idle and
    /// resolving phases share it. React's `xl` (h-5 w-5 = 20) is the base's
    /// `Lg` tier (the base collapsed React's six tiers to 12/16/20); the
    /// `current` tone inherits the wrapper's `text-fg-muted`, so `Muted`.
    fn loading(&self, dark: bool, root_key: &str) -> El {
        el()
            .key(format!("{root_key}-loading"))
            .w_full()
            .h_full()
            .center()
            .child(
                Spinner::new(format!("{root_key}-loading-spinner"))
                    .size(SpinnerSize::Lg)
                    .tone(SpinnerTone::Muted)
                    .build(dark),
            )
    }

    /// React's two `Button size="sm" variant="secondary"` recovery actions —
    /// the same `handleReload` press at both sites, leading with the
    /// `RotateCw` glyph (React `h-3 w-3`; the kit sizes the glyph itself).
    fn action_button(&self, k: &'static Kit, key: String) -> El {
        let mut button = Button::new(t("workspace.shell.reload"))
            // secondary (transparent + edge border + fg text) is the kit
            // Outline skin — the T022/wapp_card mapping.
            .variant(ButtonVariant::Outline)
            // sm (h-7 = 28pt) is the kit's Large control height.
            .control_size(ControlSize::Large)
            .icon(&ICON_ROTATE_CW);
        if let Some(lens) = &self.lens {
            let lens = Rc::clone(lens);
            button = button.on_press_tx(move |state: &mut S, tx: &mut Tx| lens(state).reload(tx));
        }
        button.build(k).key(key).shrink()
    }
}
