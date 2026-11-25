//! Port of `packages/frontend/src/windows/workspace/WorkspaceNavigator.tsx` (T145).
//!
//! The workspace mode's left rail: one item per page contributed by an
//! enabled wapp — the wapp icon, the page title, the wapp description as a
//! second line — with the active tab's item tinted `bg-accent/20
//! text-accent`, and, when no enabled wapp contributes any page, the empty
//! state's guidance plus the jump-to-extensions button. The component is
//! stateless in React (a pure projection of the wapp and workspace-tabs
//! stores), so it is a pure builder here: the host feeds the derived page
//! list, the active tab key and the two press slots.
//!
//! ## What maps where
//!
//! | React | Rune |
//! |---|---|
//! | `useWorkspacePages()` (the derived enabled-wapp × page list) | the `pages` parameter — [`WorkspacePageEntry`], this port's mirror of React's `WorkspacePageEntry` (`windows/workspace/workspacePages.ts`; that file's own port, T224, is the [`derive_workspace_pages`](crate::views::workspace_pages::derive_workspace_pages) derivation that feeds this view) |
//! | `useWorkspaceTabsStore((s) => s.activeKey)` | the `active_key` parameter (`page.key === activeKey` picks the tint) |
//! | `onOpenExtensions` prop | [`WorkspaceNavigator::on_open_extensions`] — the empty-state button's host slot; `None` renders no button, exactly as React's `{onOpenExtensions ? … : null}` |
//! | the item press `openWappPage(page.wappId, page.pageId)` | [`WorkspaceNavigator::on_open_page`] — the per-item host slot with the wapp and page ids; the resolve-then-open store flow is the workspace-tabs seam (the T143/T172/T224 area) |
//! | `width` prop (the resizable sidebar) | [`WorkspaceNavigator::width`]; `None` keeps the fixed `w-[180px]` ([`DEFAULT_WIDTH_PT`]) |
//! | `WappIcon` (`mt-0.5 h-4 w-4`) | the T017 port at 16pt with a 2pt top lead, the `images` wire forwarded |
//! | `t('nav.workspacePages')` / `t('workspace.emptyHint')` / `t('workspace.openExtensions')` | `crate::i18n::t` with the same keys |
//!
//! Style deltas (documented, per the area's rulings): pixel classes are
//! points at the area's 1px=1pt convention (T067); `bg-surface-alt` → the
//! kit's `layer_01`, `border-edge` → `border_base`, `bg-accent/20` →
//! `bg_accent` at 0.2 alpha, `hover:bg-surface-raised` → the standard
//! `El::hover` overlay; `text-fg-secondary` / `text-fg-muted` → the kit's
//! one `text_muted` token (the established T015/T029/T141 mapping; the T072
//! page's group headers use `text_faint` for the same stack — a sibling
//! divergence, not followed here); `transition-colors` and the inactive
//! item's `hover:text-fg` ink swap have no counterpart (the ink stays
//! secondary); `scrollbar-hide` needs no counterpart (a Rune scroll viewport
//! draws no bar by default); `text-left`/`text-center` have no text-align
//! counterpart (the empty column centers its children as blocks — wrapped
//! hint lines align start within the text block, a cosmetic delta for the
//! single-line zh-CN string).
//!
//! The per-item icon's sticky fallback (T017's `WappIconState`, one per
//! site) rides the future workspace host's state exactly as the shell's
//! header icon does: this view forwards only the `images` wire, and a
//! `None`/errored answer already draws the puzzle that frame. React's
//! unmount test (the rail leaves with its host) has no Rune counterpart to
//! pin: the state the host drops *is* the instance.
//!
//! ## Keys
//!
//! React's `data-testid` values stay verbatim where they are unique; the
//! per-item testid + `data-page-key` pair merges the way the `wapp_card`
//! precedent does (the single-key runtime cannot host N items under one
//! testid):
//!
//! | part | key |
//! |---|---|
//! | root aside (React `workspace-navigator`) | `workspace-navigator` |
//! | header label | `workspace-navigator-label` |
//! | empty state / hint | `workspace-navigator-empty`, `workspace-navigator-empty-hint` |
//! | empty-state button (React `workspace-open-extensions`) | `workspace-open-extensions` |
//! | nav list (React `<nav>`) | `workspace-navigator-nav` |
//! | item (React `workspace-nav-item` + `data-page-key={page.key}`) | `workspace-nav-item-<page.key>` |
//! | item icon / title / description | `-icon` (the T017 port's key), `-title`, `-desc` |

#![allow(dead_code)] // reusable port; wired by the workspace host tracks

use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec, TextCase, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::wapp_icon::{WappIcon, WappIconImages};

/// React's fixed rail width when the host passes no `width` prop
/// (`w-[180px]`).
pub const DEFAULT_WIDTH_PT: f32 = 180.0;

/// React's `WorkspacePageEntry` (`windows/workspace/workspacePages.ts`): the
/// flattened enabled-wapp × contributed-page row the navigator lists. The
/// derivation (`deriveWorkspacePages`: enabled wapps with pages, in install
/// order, `title: page.title || wapp.name`) is that file's own port — T224,
/// [`crate::views::workspace_pages::derive_workspace_pages`]; the host feeds
/// the rows through this mirror. `version` and
/// `author` ride the entry for the future tab payload — this rail renders
/// neither.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspacePageEntry {
    /// `${wappId}:${pageId}` — the workspace tab key (React's
    /// `workspaceTabKey`; the T067 `WorkspaceTabVm::key` form).
    pub key: String,
    pub wapp_id: String,
    pub page_id: String,
    /// `page.title || wapp.name` — the entry's display title.
    pub title: String,
    /// The page contribution's optional icon path (the item's `WappIcon`).
    pub icon: Option<String>,
    /// The wapp's version (the future tab's `v{version}` chip).
    pub version: String,
    /// The wapp's author (carried, not rendered here).
    pub author: Option<String>,
    /// The wapp's description — the item's second line and tooltip body.
    pub description: Option<String>,
}

/// The empty state's jump-to-extensions slot — React's `onOpenExtensions`
/// prop; without it the button does not render.
pub type OpenExtensionsSlot<S> = Rc<dyn Fn(&mut S)>;

/// The per-item open slot — React's item press
/// `openWappPage(page.wappId, page.pageId)`, with the wapp and page ids.
pub type OpenWappPageSlot<S> = Rc<dyn Fn(&mut S, &str, &str)>;

/// The `WorkspaceNavigator` React props plus the Rune seams: the two store
/// projections (`pages`, `active_key`), the `width` prop, the two host
/// press slots and the T017 picture wire.
pub struct WorkspaceNavigator<'a, S: 'static> {
    pages: &'a [WorkspacePageEntry],
    active_key: Option<&'a str>,
    width: Option<f32>,
    on_open_extensions: Option<OpenExtensionsSlot<S>>,
    on_open_page: Option<OpenWappPageSlot<S>>,
    images: Option<WappIconImages>,
}

impl<'a, S: 'static> WorkspaceNavigator<'a, S> {
    /// React `WorkspaceNavigatorProps` (`onOpenExtensions?`, `width?`) over
    /// the two store projections: the derived page list and the active tab
    /// key (`null` when no wapp tab is open).
    pub fn new(pages: &'a [WorkspacePageEntry], active_key: Option<&'a str>) -> Self {
        Self {
            pages,
            active_key,
            width: None,
            on_open_extensions: None,
            on_open_page: None,
            images: None,
        }
    }

    /// The `width` prop (pixels) for the resizable sidebar; without it the
    /// rail keeps its fixed `w-[180px]`.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// The `onOpenExtensions` prop — the empty-state button's host slot.
    pub fn on_open_extensions(mut self, slot: OpenExtensionsSlot<S>) -> Self {
        self.on_open_extensions = Some(slot);
        self
    }

    /// The per-item host slot the press routes through (React's
    /// `openWappPage`); without it the items render but their presses
    /// no-op — the not-yet-wired host's case.
    pub fn on_open_page(mut self, slot: OpenWappPageSlot<S>) -> Self {
        self.on_open_page = Some(slot);
        self
    }

    /// The items' icon picture wire (the T017 port's `WappIconImages`).
    pub fn images(mut self, images: WappIconImages) -> Self {
        self.images = Some(images);
        self
    }

    /// Build the rail at the given theme.
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        col()
            .key("workspace-navigator")
            .h_full()
            .shrink()
            // React: `border-r border-edge` (drawn inside, no layout room —
            // the T067 border-b treatment on the right side).
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            // React: `bg-surface-alt` → `layer_01` (the T141 ruling).
            .fill(k.c.layer_01)
            // React: `style={width ? { width } : undefined}` over
            // `!width && 'w-[180px]'` — a falsy width (0, like undefined)
            // keeps the fixed rail.
            .w(self.width.filter(|w| *w != 0.0).unwrap_or(DEFAULT_WIDTH_PT))
            .child(self.header_label(&k))
            .child(if self.pages.is_empty() {
                self.empty_state(&k)
            } else {
                self.nav_list(&k, dark)
            })
    }

    /// React's header div: `px-3 pt-3 pb-1 text-[11px] font-semibold
    /// tracking-wide text-fg-muted uppercase` around
    /// `t('nav.workspacePages')`.
    fn header_label(&self, k: &'static rune_kit::Kit) -> El {
        text(t("nav.workspacePages"))
            .key("workspace-navigator-label")
            // The T072 page's stack for the same classes (`text-[11px]
            // font-semibold uppercase tracking-wide text-fg-muted`); the
            // ink follows the general fg-muted → text_muted ruling instead.
            .font(TextStyle::new(11.0, 600.0).tracking(0.3))
            .color(k.c.text_muted)
            .text_case(TextCase::Uppercase)
            .px(12.0) // px-3
            .pt(12.0) // pt-3
            .pb(4.0) // pb-1
    }

    /// React's empty branch: `flex flex-1 flex-col items-center
    /// justify-center gap-3 px-3 pb-6 text-center` — the guidance over the
    /// optional jump-to-extensions button.
    fn empty_state(&self, k: &'static rune_kit::Kit) -> El {
        let mut empty = col()
            .key("workspace-navigator-empty")
            .grow(1.0)
            .min_h(0.0)
            .center()
            .gap(12.0) // gap-3
            .px(12.0) // px-3
            .pb(24.0) // pb-6
            .child(
                text(t("workspace.emptyHint"))
                    .key("workspace-navigator-empty-hint")
                    // React: `text-xs leading-relaxed text-fg-muted` (1.625
                    // × 12 = 19.5pt line height).
                    .font(TextStyle::new(12.0, 400.0).lh(19.5))
                    .color(k.c.text_muted),
            );
        if let Some(slot) = &self.on_open_extensions {
            let slot = Rc::clone(slot);
            // React: `<Button size="sm" variant="secondary"
            // data-testid="workspace-open-extensions"
            // onClick={onOpenExtensions}>` — secondary (transparent + edge
            // border + fg text) is the kit's Outline skin (the T022/T067
            // mapping) and sm (h-7 = 28pt) is the kit's Large height.
            let button = Button::new(t("workspace.openExtensions"))
                .variant(ButtonVariant::Outline)
                .control_size(ControlSize::Large)
                .on_press(move |state: &mut S| slot(state))
                .build(k)
                .key("workspace-open-extensions")
                .shrink();
            empty = empty.child(button);
        }
        empty
    }

    /// React's `<nav className="scrollbar-hide min-h-0 flex-1
    /// overflow-y-auto px-2 pb-2">` — the scrolling item column (the
    /// landmark's implicit `navigation` role). `scrollbar-hide` needs no
    /// counterpart: a Rune viewport draws no bar by default.
    fn nav_list(&self, k: &'static rune_kit::Kit, dark: bool) -> El {
        col()
            .key("workspace-navigator-nav")
            .role("navigation")
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .px(8.0) // px-2
            .pb(8.0) // pb-2
            .children(self.pages.iter().map(|page| self.item(k, dark, page)))
    }

    /// React's per-page item: `mb-0.5 flex w-full items-start gap-2
    /// rounded-md px-2 py-1.5 text-left transition-colors` around the
    /// wapp icon and the title/description column — `bg-accent/20
    /// text-accent` when the page's key is the active tab, else
    /// `text-fg-secondary hover:bg-surface-raised hover:text-fg`.
    fn item(&self, k: &'static rune_kit::Kit, dark: bool, page: &WorkspacePageEntry) -> El {
        let active = self.active_key == Some(page.key.as_str());
        let item_key = format!("workspace-nav-item-{}", page.key);
        let title_color = if active { k.c.text_accent } else { k.c.text_muted };

        let mut icon = WappIcon::new(Some(&page.wapp_id), page.icon.as_deref())
            .key(format!("{item_key}-icon"))
            .size_pt(16.0); // h-4 w-4
        if let Some(images) = &self.images {
            icon = icon.images(Rc::clone(images));
        }
        // React: `mt-0.5` — the icon's 2pt top lead under `items-start`.
        let icon = icon.build(dark).mt(2.0);

        // React: `<span class="min-w-0">` — the shrinkable text column
        // (`block truncate` lines clip at the row's remaining width).
        let mut lines = col()
            .min_w(0.0)
            .grow(1.0)
            .child(
                text(page.title.clone())
                    .key(format!("{item_key}-title"))
                    // React: `block truncate text-xs font-medium`.
                    .font(TextStyle::new(12.0, 500.0))
                    .color(title_color)
                    .truncate(),
            );
        // React: `{page.description ? <span class="block truncate
        // text-[10px] text-fg-muted">{page.description}</span> : null}` — a
        // truthiness check, so an empty string renders nothing either.
        if let Some(description) = page.description.as_deref().filter(|d| !d.is_empty()) {
            lines = lines.child(
                text(description.to_owned())
                    .key(format!("{item_key}-desc"))
                    .font(TextStyle::new(10.0, 400.0))
                    .color(k.c.text_muted)
                    .truncate(),
            );
        }

        let mut item = row()
            .key(item_key)
            .w_full()
            .items_start()
            .gap(8.0) // gap-2
            .radius(k.radius.md) // rounded-md
            .px(8.0) // px-2
            .py(6.0) // py-1.5
            .mb(2.0) // mb-0.5
            // React: `title={page.description ?? page.title}` — the hover
            // tooltip (nullish: an empty description stays empty).
            .help(page.description.clone().unwrap_or_else(|| page.title.clone()))
            // React: `<button type="button">`.
            .role("button")
            .focusable(true)
            .child(icon)
            .child(lines);
        if active {
            // React: `bg-accent/20`.
            item = item.fill(k.alpha(k.c.bg_accent, 0.2));
        } else {
            // React: `hover:bg-surface-raised` — the standard hover overlay;
            // `transition-colors` and `hover:text-fg` have no counterpart.
            item = item.hover(k.c.surface_raised);
        }
        if let Some(slot) = &self.on_open_page {
            let slot = Rc::clone(slot);
            let wapp_id = page.wapp_id.clone();
            let page_id = page.page_id.clone();
            item = item.on_press(move |state: &mut S| slot(state, &wapp_id, &page_id));
        }
        item
    }
}
