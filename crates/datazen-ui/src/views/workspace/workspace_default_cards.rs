//! Port of `packages/frontend/src/windows/workspace/WorkspaceDefaultCards.tsx` (T111).
//!
//! The workspace mode's default right-hand view when no tab is open: the
//! card grid over the pages enabled wapps contribute (one card per page —
//! the wapp icon, the title, the optional description, the `v{version} ·
//! {author}` byline), or, when no enabled wapp contributes any page, the
//! centered empty-state guidance with its optional jump-to-extensions
//! shortcut. The component is stateless in React (a pure projection of the
//! derived page list and two press props), so it is a pure builder here:
//! the host feeds the derived list and the two press slots.
//!
//! ## What maps where
//!
//! | React | Rune |
//! |---|---|
//! | `pages` prop (`useWorkspacePages()`'s derived list) | the `pages` parameter — [`WorkspacePageEntry`], the T145 mirror of React's `WorkspacePageEntry` (the derivation's own port, T224, feeds this view through it) |
//! | `onOpen: (page) => void` | [`WorkspaceDefaultCards::on_open`] — the per-card host slot receiving the whole entry, exactly React's call shape (`openWappPage(page.wappId, page.pageId)` is the caller's body, the workspace-tabs seam) |
//! | `onOpenExtensions?` prop | [`WorkspaceDefaultCards::on_open_extensions`] — the T145 [`OpenExtensionsSlot`]; `None` renders no button, exactly as React's `{onOpenExtensions ? … : null}` |
//! | `WappIcon` (`h-5 w-5` in the `h-10 w-10` well) | the T017 port at 20pt inside the 40pt rounded well, the `images` wire forwarded |
//! | `t('workspace.emptyTitle')` / `emptyHint` / `openExtensions` / `defaultTitle` / `defaultHint` | `crate::i18n::t` with the same keys |
//!
//! Style deltas (documented, per the area's rulings): pixel classes are
//! points at the area's 1px=1pt convention (T067); `bg-surface-alt` → the
//! kit's `layer_01`, `bg-surface-raised` → `layer_02`, `border-edge` →
//! `border_base`, `text-fg` → `text_base`, `text-fg-muted` → the kit's one
//! `text_muted` token; the grid's `repeat(auto-fill,minmax(240px,1fr))`
//! lays the cards at the same 240pt basis in a wrapped row (the `wapp_card`
//! precedent for `minmax(260px,1fr)`; the `1fr` stretch of the remainder is
//! that precedent's one simplification); the card's hover stack
//! (`hover:-translate-y-px hover:border-accent hover:shadow-lg`) reduces to
//! the standard `El::hover` overlay at `surface_raised` (`hover:bg-surface-
//! raised`, the connection cards' precedent — Rune has no transform or
//! shadow hover); `text-left`/`text-center` have no text-align counterpart
//! (blocks align start; the centered empty column centers its children as
//! blocks, so a wrapped hint line's text sits start-of-block — a cosmetic
//! delta for the single-line zh-CN strings); `transition-all` has no
//! counterpart (the hover overlay cross-fades on its own).
//!
//! The per-card icon's sticky fallback (T017's `WappIconState`, one per
//! site) rides the future workspace host's state exactly as the shell's
//! header icon does: this view forwards only the `images` wire, and a
//! `None`/errored answer already draws the puzzle that frame.
//!
//! ## Keys
//!
//! React's `data-testid` values stay verbatim where they are unique; the
//! per-card testid merges with the React list key the way the `wapp_card`
//! precedent does (the single-key runtime cannot host N cards under one
//! testid):
//!
//! | part | key |
//! |---|---|
//! | cards root (React `workspace-default-cards`) | `workspace-default-cards` |
//! | scroll body (React's unkeyed `px-6 py-6` div) | `workspace-default-cards-body` |
//! | heading / hint (React's unkeyed `h2`/`p`) | `workspace-default-cards-title`, `workspace-default-cards-hint` |
//! | card grid (React's unkeyed grid div) | `workspace-default-cards-grid` |
//! | card (React `workspace-default-card` + list key) | `workspace-default-card-<page.key>` |
//! | icon well / icon | `-icon-slot`, `-icon` (the T017 port's key) |
//! | title / description | `-title`, `-desc` |
//! | byline row (React's unkeyed `v{version}` span) | `-meta` |

#![allow(dead_code)] // reusable port; wired by the workspace host tracks

use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::wapp_icon::{WappIcon, WappIconImages};
use crate::views::workspace_navigator::{OpenExtensionsSlot, WorkspacePageEntry};

/// React's card-grid floor: `repeat(auto-fill,minmax(240px,1fr))` — the
/// wrapped row lays the cards out at the same 240pt basis (the `wapp_card`
/// precedent for the 260pt grid).
pub const CARD_MIN_PT: f32 = 240.0;

/// The per-card open slot — React's `onOpen(page)` prop, entry and all.
pub type OpenPageSlot<S> = Rc<dyn Fn(&mut S, &WorkspacePageEntry)>;

/// The `WorkspaceDefaultCards` React props plus the Rune seams: the derived
/// page list, the two host press slots and the T017 picture wire.
pub struct WorkspaceDefaultCards<'a, S: 'static> {
    pages: &'a [WorkspacePageEntry],
    on_open: Option<OpenPageSlot<S>>,
    on_open_extensions: Option<OpenExtensionsSlot<S>>,
    images: Option<WappIconImages>,
}

impl<'a, S: 'static> WorkspaceDefaultCards<'a, S> {
    /// React `WorkspaceDefaultCardsProps` (`pages`, `onOpen`,
    /// `onOpenExtensions?`).
    pub fn new(pages: &'a [WorkspacePageEntry]) -> Self {
        Self {
            pages,
            on_open: None,
            on_open_extensions: None,
            images: None,
        }
    }

    /// The `onOpen` prop — the per-card host slot (the entry React hands
    /// the caller); without it the cards render but their presses no-op —
    /// the not-yet-wired host's case.
    pub fn on_open(mut self, slot: OpenPageSlot<S>) -> Self {
        self.on_open = Some(slot);
        self
    }

    /// The `onOpenExtensions` prop — the empty-state button's host slot.
    pub fn on_open_extensions(mut self, slot: OpenExtensionsSlot<S>) -> Self {
        self.on_open_extensions = Some(slot);
        self
    }

    /// The cards' icon picture wire (the T017 port's `WappIconImages`).
    pub fn images(mut self, images: WappIconImages) -> Self {
        self.images = Some(images);
        self
    }

    /// Build the view at the given theme. React's two branches: the empty
    /// state when no page contributes, the scrolling card grid otherwise.
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        if self.pages.is_empty() {
            self.empty_state(&k)
        } else {
            self.cards_root(&k, dark)
        }
    }

    /// React's cards root: `min-h-0 flex-1 overflow-y-auto` — the scrolling
    /// column (a Rune viewport draws no bar by default, so `scrollbar-hide`
    /// needs no counterpart).
    fn cards_root(&self, k: &'static rune_kit::Kit, dark: bool) -> El {
        col()
            .key("workspace-default-cards")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(self.cards_body(k, dark))
    }

    /// React's inner div: `px-6 py-6` around the heading, the hint and the
    /// card grid.
    fn cards_body(&self, k: &'static rune_kit::Kit, dark: bool) -> El {
        col()
            .key("workspace-default-cards-body")
            .px(24.0) // px-6
            .py(24.0) // py-6
            .child(
                text(t("workspace.defaultTitle"))
                    .key("workspace-default-cards-title")
                    // React: `text-base font-semibold text-fg` (16/600).
                    .font(TextStyle::new(16.0, 600.0))
                    .color(k.c.text_base),
            )
            .child(
                text(t("workspace.defaultHint"))
                    .key("workspace-default-cards-hint")
                    // React: `mt-1 mb-5 text-xs text-fg-muted`.
                    .font(TextStyle::new(12.0, 400.0))
                    .color(k.c.text_muted)
                    .mt(4.0) // mt-1
                    .mb(20.0), // mb-5
            )
            .child(
                row()
                    .key("workspace-default-cards-grid")
                    .w_full()
                    // React: `grid grid-cols-[repeat(auto-fill,minmax(240px,
                    // 1fr))] gap-3.5` — the wrapped row at the 240pt basis
                    // (the `wapp_card` precedent).
                    .flex_wrap()
                    .gap(14.0) // gap-3.5
                    .children(self.pages.iter().map(|page| self.card(k, dark, page))),
            )
    }

    /// React's per-page card: `flex flex-col gap-2.5 rounded-xl border
    /// border-edge bg-surface-alt p-4 text-left transition-all
    /// hover:-translate-y-px hover:border-accent hover:bg-surface-raised
    /// hover:shadow-lg` — a `<button type="button">` opening the page.
    fn card(&self, k: &'static rune_kit::Kit, dark: bool, page: &WorkspacePageEntry) -> El {
        let card_key = format!("workspace-default-card-{}", page.key);

        // React: `<span class="flex h-10 w-10 items-center justify-center
        // rounded-lg bg-surface-raised">` around `WappIcon h-5 w-5`.
        let mut icon = WappIcon::new(Some(&page.wapp_id), page.icon.as_deref())
            .key(format!("{card_key}-icon"))
            .size_pt(20.0); // h-5 w-5
        if let Some(images) = &self.images {
            icon = icon.images(Rc::clone(images));
        }
        let icon_well = row()
            .key(format!("{card_key}-icon-slot"))
            .size(40.0, 40.0) // h-10 w-10
            .shrink()
            .center()
            .radius(k.radius.md) // rounded-lg
            .fill(k.c.layer_02) // bg-surface-raised
            .child(icon.build(dark));

        // React: `<span class="min-w-0">` — the shrinkable text column.
        let mut lines = col()
            .key(format!("{card_key}-lines"))
            .w_full()
            .min_w(0.0)
            .child(
                text(page.title.clone())
                    .key(format!("{card_key}-title"))
                    // React: `block truncate text-sm font-semibold text-fg`.
                    .font(TextStyle::new(14.0, 600.0))
                    .color(k.c.text_base)
                    .truncate(),
            );
        // React: `{page.description ? <span class="mt-1 block text-xs
        // leading-relaxed text-fg-muted">…</span> : null}` — a truthiness
        // check, so an empty string renders nothing either.
        if let Some(description) = page.description.as_deref().filter(|d| !d.is_empty()) {
            lines = lines.child(
                text(description.to_owned())
                    .key(format!("{card_key}-desc"))
                    // React: `mt-1 block text-xs leading-relaxed
                    // text-fg-muted` (1.625 × 12 = 19.5pt line height).
                    .font(TextStyle::new(12.0, 400.0).lh(19.5))
                    .color(k.c.text_muted)
                    .mt(4.0) // mt-1
                    .wrap(),
            );
        }

        // React: `mt-auto border-t border-edge pt-2 text-[11px] text-fg-muted`
        // — `v{version}` plus ` · {author}` only for a truthy author.
        let byline = match page.author.as_deref() {
            Some(author) if !author.is_empty() => format!(" · {author}"),
            _ => String::new(),
        };
        let meta = row()
            .key(format!("{card_key}-meta"))
            .w_full()
            .pt(8.0) // pt-2
            // React: `border-t border-edge` — the top edge only.
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .child(
                text(format!("v{}{}", page.version, byline))
                    .key(format!("{card_key}-meta-text"))
                    // React: `text-[11px] text-fg-muted`.
                    .font(TextStyle::new(11.0, 400.0))
                    .color(k.c.text_muted),
            );

        // React: the `mt-auto` pin — a filler column keeps the byline at the
        // card's bottom edge however tall the description grew.
        let mut card = col()
            .key(card_key.clone())
            .w(CARD_MIN_PT)
            .gap(10.0) // gap-2.5
            .radius(k.radius.lg) // rounded-xl
            .border(1.0, k.c.border_base) // border border-edge
            .fill(k.c.layer_01) // bg-surface-alt
            .padding(16.0) // p-4
            // React: `hover:-translate-y-px hover:border-accent
            // hover:bg-surface-raised hover:shadow-lg` — the standard hover
            // overlay at `surface_raised` (the connection cards'
            // precedent); the translate and the shadow have no counterpart.
            .hover(k.c.surface_raised)
            .child(icon_well)
            .child(lines)
            .child(col().key(format!("{card_key}-spacer")).grow(1.0).min_h(0.0))
            .child(meta)
            // React: `<button type="button">`.
            .role("button")
            .focusable(true);
        if let Some(slot) = &self.on_open {
            let slot = Rc::clone(slot);
            let page = Rc::new(page.clone());
            card = card.on_press(move |state: &mut S| slot(state, &page));
        }
        card
    }

    /// React's empty branch: `flex flex-1 flex-col items-center
    /// justify-center gap-3 p-8 text-center` — the title, the hint and the
    /// optional jump-to-extensions button.
    fn empty_state(&self, k: &'static rune_kit::Kit) -> El {
        let mut empty = col()
            .key("workspace-empty")
            .grow(1.0)
            .min_h(0.0)
            .center()
            .gap(12.0) // gap-3
            .padding(32.0) // p-8
            .child(
                text(t("workspace.emptyTitle"))
                    .key("workspace-empty-title")
                    // React: `text-sm font-semibold text-fg` (14/600).
                    .font(TextStyle::new(14.0, 600.0))
                    .color(k.c.text_base),
            )
            .child(
                text(t("workspace.emptyHint"))
                    .key("workspace-empty-hint")
                    // React: `max-w-xs text-xs leading-relaxed
                    // text-fg-muted` (max-w-xs = 20rem = 320pt).
                    .font(TextStyle::new(12.0, 400.0).lh(19.5))
                    .color(k.c.text_muted)
                    .max_w(320.0)
                    .wrap(),
            );
        if let Some(slot) = &self.on_open_extensions {
            let slot = Rc::clone(slot);
            // React: `<Button size="sm" variant="secondary"
            // data-testid="workspace-open-extensions"
            // onClick={onOpenExtensions}>` — secondary (transparent + edge
            // border + fg text) is the kit's Outline skin (the T022/T067
            // mapping) and sm (h-7 = 28pt) is the kit's Large height.
            empty = empty.child(
                Button::new(t("workspace.openExtensions"))
                    .variant(ButtonVariant::Outline)
                    .control_size(ControlSize::Large)
                    .on_press(move |state: &mut S| slot(state))
                    .build(k)
                    .key("workspace-open-extensions")
                    .shrink(),
            );
        }
        empty
    }
}
