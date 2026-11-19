//! Port of `packages/frontend/src/windows/workspace/WorkspaceTabBar.tsx` (T174).
//!
//! The workspace tab strip: one entry per open wapp page tab — the wapp
//! icon, the truncated title, the hover-revealed close — with the active
//! tab's accent underline. It visually mirrors the connection
//! `PanelTabBar` but is backed exclusively by the workspace-tabs store
//! (independent of connection tabs), and renders nothing while no wapp
//! page tab is open. The component is stateless in React (a pure
//! projection of the store), so it is a pure builder here: the host feeds
//! the tab list and the active key, and the store's two actions are the
//! host press slots.
//!
//! ## What maps where
//!
//! | React | Rune |
//! |---|---|
//! | `useWorkspaceTabsStore((s) => s.tabs)` | the `tabs` parameter — [`WorkspaceTabVm`], the T067 mirror of React's `WorkspaceTab` (`stores/workspaceTabsStore.ts`; the store's own port, T143/T172, is ahead and will feed this view through it) |
//! | `useWorkspaceTabsStore((s) => s.activeKey)` | the `active_key` parameter (`tab.key === activeKey` picks the tint; `None` is React's `null`) |
//! | `tabs.length === 0 → return null` | [`WorkspaceTabBar::build`] returns `None` (the `FilterBar` precedent — no `workspace-tabbar` element exists in the tree) |
//! | `activate(tab.key)` | [`WorkspaceTabBar::on_activate`] — the host slot the entry's press routes through |
//! | `close(tab.key)` | [`WorkspaceTabBar::on_close`] — the close button's host slot |
//! | `onWheel` (vertical wheel → `scrollLeft` on the strip) | `El::scroll_x` — Rune's horizontal viewport already moves on the wheel, which is the redirect React hand-rolls |
//! | `WappIcon` (`h-3.5 w-3.5`) | the T017 port at 14pt, the `images` wire forwarded |
//! | `t('common.close')` (the close button's aria-label) | `crate::i18n::t` with the same key |
//!
//! Style deltas (documented, per the area's rulings): pixel classes are
//! points at the 1px=1pt convention (T067); `bg-surface-alt` → the kit's
//! `layer_01`, `bg-surface` → `bg_base`, `border-edge` → `border_base`,
//! `text-fg` → `text_base`, `text-fg-secondary`/`text-fg-muted` → the
//! kit's one `text_muted` token (the established area mapping); the
//! `group` + `opacity-0 group-hover:opacity-100` close reveal is Rune's
//! `hover_group` + `group_opacity(0.0, 1.0)` on the entry (React's plain
//! `group-hover:` maps to plain `hover_group` — no `:focus-visible` arm);
//! `hover:bg-surface-raised hover:text-fg` reduces to the standard
//! `El::hover` overlay at `surface_raised` (the ink swap has no
//! counterpart); `transition-colors` and the underline's
//! `transition-opacity duration-300` have no counterpart (the overlay and
//! the fill cross-fade on their own); `title={`${tab.title} ·
//! v${tab.version}`}` is the entry's help text; the active underline
//! (`absolute inset-x-0 bottom-0 h-0.5`) is an always-present 2pt
//! in-flow row under the entry's content (Rune has no absolute
//! positioning), so an entry stands 2pt taller than React's overlaid bar —
//! and its inactive state is the transparent fill, not an absent element.
//!
//! The per-entry icon's sticky fallback (T017's `WappIconState`, one per
//! site) rides the future workspace-tabs host's state exactly as the
//! shell's header icon does: this view forwards only the `images` wire,
//! and a `None`/errored answer already draws the puzzle that frame.
//!
//! ## Keys
//!
//! React's `data-testid` values stay verbatim where they are unique; the
//! per-entry testids merge with the tab key the way the `wapp_card`
//! precedent does (the single-key runtime cannot host N entries under one
//! testid):
//!
//! | part | key |
//! |---|---|
//! | root (React `workspace-tabbar`) | `workspace-tabbar` |
//! | scroll strip (React's unkeyed inner div) | `workspace-tabbar-strip` |
//! | entry (React `workspace-tab` + React list key) | `workspace-tab-<tab.key>` |
//! | activate button (React's unkeyed `<button>`) | `workspace-tab-<key>-activate` |
//! | icon / title | `-icon` (the T017 port's key), `-title` |
//! | close button (React `workspace-tab-close`) | `workspace-tab-<key>-close` |
//! | active underline (React's unkeyed span) | `workspace-tab-<key>-underline` |

#![allow(dead_code)] // reusable port; wired by the workspace host tracks

use std::rc::Rc;

use rune::{col, icon, row, text, El, TextStyle};

use crate::i18n::t;
use crate::icons::ICON_X;
use crate::ui::kit::kit;
use crate::views::wapp_icon::{WappIcon, WappIconImages};
use crate::views::wapp_page_shell::WorkspaceTabVm;

/// The store's `activate(key)` — the host slot the entry's press routes
/// through.
pub(crate) type ActivateTabSlot<S> = Rc<dyn Fn(&mut S, &str)>;

/// The store's `close(key)` — the close button's host slot.
pub(crate) type CloseTabSlot<S> = Rc<dyn Fn(&mut S, &str)>;

/// The `WorkspaceTabBar` React props' store projections plus the Rune
/// seams: the tab list, the active key, the two host press slots and the
/// T017 picture wire.
pub(crate) struct WorkspaceTabBar<'a, S: 'static> {
    tabs: &'a [WorkspaceTabVm],
    active_key: Option<&'a str>,
    on_activate: Option<ActivateTabSlot<S>>,
    on_close: Option<CloseTabSlot<S>>,
    images: Option<WappIconImages>,
}

impl<'a, S: 'static> WorkspaceTabBar<'a, S> {
    /// React's three store reads (`tabs`, `activeKey`) — the actions are
    /// the two slots below.
    pub(crate) fn new(tabs: &'a [WorkspaceTabVm], active_key: Option<&'a str>) -> Self {
        Self {
            tabs,
            active_key,
            on_activate: None,
            on_close: None,
            images: None,
        }
    }

    /// The store's `activate(tab.key)` — the entry press's host slot;
    /// without it the entries render but their presses no-op (the
    /// not-yet-wired host's case).
    pub(crate) fn on_activate(mut self, slot: ActivateTabSlot<S>) -> Self {
        self.on_activate = Some(slot);
        self
    }

    /// The store's `close(tab.key)` — the close button's host slot.
    pub(crate) fn on_close(mut self, slot: CloseTabSlot<S>) -> Self {
        self.on_close = Some(slot);
        self
    }

    /// The entries' icon picture wire (the T017 port's `WappIconImages`).
    pub(crate) fn images(mut self, images: WappIconImages) -> Self {
        self.images = Some(images);
        self
    }

    /// Build the strip at the given theme — `None` while no tab is open
    /// (React's `return null`).
    pub(crate) fn build(self, dark: bool) -> Option<El> {
        if self.tabs.is_empty() {
            return None;
        }
        let k = kit(dark);
        Some(
            row()
                .key("workspace-tabbar")
                .w_full()
                .shrink()
                .items_center()
                // React: `border-b border-edge` — the bottom edge only.
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                // React: `bg-surface-alt` → `layer_01` (the T141 ruling).
                .fill(k.c.layer_01)
                .child(
                    row()
                        .key("workspace-tabbar-strip")
                        // React: `flex min-w-0 flex-1 overflow-x-auto` — the
                        // horizontal viewport; the wheel already moves it,
                        // which is the redirect React's `onWheel` hand-rolls.
                        .grow(1.0)
                        .min_w(0.0)
                        .scroll_x()
                        .children(self.tabs.iter().map(|tab| self.entry(&k, dark, tab))),
                ),
        )
    }

    /// React's per-tab entry: `group relative flex items-center gap-1.5
    /// border-r border-edge px-3 py-2 text-xs transition-colors` —
    /// `bg-surface text-fg` when active, else `text-fg-secondary
    /// hover:bg-surface-raised hover:text-fg` — around the activate
    /// button, the close button and the active underline.
    fn entry(&self, k: &'static rune_kit::Kit, dark: bool, tab: &WorkspaceTabVm) -> El {
        let active = self.active_key == Some(tab.key.as_str());
        let entry_key = format!("workspace-tab-{}", tab.key);

        // React's activate button: `flex items-center gap-1.5` around the
        // icon and `max-w-[160px] truncate` title.
        let mut wapp_icon = WappIcon::new(Some(&tab.wapp_id), tab.icon.as_deref())
            .key(format!("{entry_key}-icon"))
            .size_pt(14.0); // h-3.5 w-3.5
        if let Some(images) = &self.images {
            wapp_icon = wapp_icon.images(Rc::clone(images));
        }
        let mut activate = row()
            .key(format!("{entry_key}-activate"))
            .items_center()
            .gap(6.0) // gap-1.5
            // React: `<button type="button">`.
            .role("button")
            .focusable(true)
            .child(wapp_icon.build(dark))
            .child(
                text(tab.title.clone())
                    .key(format!("{entry_key}-title"))
                    // React: `max-w-[160px] truncate` at the entry's
                    // inherited `text-xs` ink — `text-fg` when active,
                    // `text-fg-secondary` otherwise (the one `text_muted`
                    // token, the area's ruling).
                    .font(TextStyle::new(12.0, 400.0))
                    .color(if active { k.c.text_base } else { k.c.text_muted })
                    .max_w(160.0)
                    .truncate(),
            );
        if let Some(slot) = &self.on_activate {
            let slot = Rc::clone(slot);
            let key = tab.key.clone();
            activate = activate.on_press(move |state: &mut S| slot(state, &key));
        }

        // React's close button: `rounded p-0.5 text-fg-muted opacity-0
        // hover:bg-surface-raised hover:text-fg group-hover:opacity-100`
        // around the `X` (`h-3 w-3`), `aria-label={`${t('common.close')}
        // ${tab.title}`}`.
        let mut close = row()
            .key(format!("{entry_key}-close"))
            .shrink()
            .padding(2.0) // p-0.5
            .radius(4.0) // rounded
            // React: `opacity-0 group-hover:opacity-100` — the group reveal.
            .group_opacity(0.0, 1.0)
            // React: `hover:bg-surface-raised` — the standard hover overlay;
            // `hover:text-fg` has no counterpart.
            .hover(k.c.surface_raised)
            .label(format!("{} {}", t("common.close"), tab.title))
            // React: `<button type="button">`.
            .role("button")
            .focusable(true)
            .child(
                icon(&ICON_X, 12.0) // h-3 w-3
                    .key(format!("{entry_key}-close-x"))
                    .color(k.c.text_muted),
            );
        if let Some(slot) = &self.on_close {
            let slot = Rc::clone(slot);
            let key = tab.key.clone();
            close = close.on_press(move |state: &mut S| slot(state, &key));
        }

        // React's content row: the entry's `flex items-center gap-1.5
        // px-3 py-2` half, with the underline drawn under it in flow.
        let content = row()
            .key(format!("{entry_key}-content"))
            .items_center()
            .gap(6.0) // gap-1.5
            .px(12.0) // px-3
            .py(8.0) // py-2
            .child(activate)
            .child(close);

        // React's active underline: `absolute inset-x-0 bottom-0 h-0.5
        // bg-accent transition-opacity duration-300` — always present,
        // accent when active and transparent otherwise (a Rune entry is 2pt
        // taller for carrying it in flow; there is no absolute positioning).
        let underline = row()
            .key(format!("{entry_key}-underline"))
            .w_full()
            .h(2.0) // h-0.5
            .shrink()
            .fill(if active { k.c.bg_accent } else { rune::Color::TRANSPARENT });

        let mut entry = col()
            .key(entry_key.clone())
            .shrink()
            // React: `border-r border-edge` — the right edge only.
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            // React: the entry's `title={`${tab.title} · v${tab.version}`}`.
            .help(format!("{} · v{}", tab.title, tab.version))
            // React: `group` — the close button's hover reveal tracks the
            // whole entry.
            .hover_group()
            .child(content)
            .child(underline);
        if active {
            // React: `bg-surface text-fg`.
            entry = entry.fill(k.c.bg_base);
        } else {
            // React: `hover:bg-surface-raised` — the standard hover overlay
            // (`transition-colors` and `hover:text-fg` have no counterpart).
            entry = entry.hover(k.c.surface_raised);
        }
        entry
    }
}
