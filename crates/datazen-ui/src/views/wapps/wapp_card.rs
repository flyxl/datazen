//! The wapps management page's card — React's `renderCard` and its
//! `WappCardIcon` helper (`windows/wapps/WappManagementPage.tsx:51-102,209-316`),
//! split out of the page module (the file is a `split` task: one card per
//! installed wapp, rendered by both the flat grid and the "all" view's groups).
//!
//! ## React → Rune mapping
//!
//! | React | Rune |
//! |---|---|
//! | `WappCardIcon`'s `<span>` letter slot (`h-9 w-9 rounded-md bg-surface-raised text-accent`) | [`card_icon_slot`]: the 36pt rounded well with the accent initial |
//! | the `<img>` branch (`readWappFile` → blob URL) | the honest seam below — the letter always renders |
//! | the identity block (name, `v{version}` chip, `by {author}`/id) | the same three lines, truncate on the two React truncates |
//! | the badge row (api mismatch, theme, one per permission) | [`Badge`](crate::ui::badge::Badge) with React's tones |
//! | the footer row (switch, Open, spacer, uninstall) | kit `Switch` (web track), an Outline sm button, an `IconButton` |
//!
//! ## Honest seams (each belongs to its own later task)
//!
//! - `readWappFile` — the package-level icon read has no adapter method, so
//!   the `<img>` branch never materialises: a wapp that declares an icon (and
//!   is enabled — React's same `!iconPath || !wapp.enabled` gate) still shows
//!   the letter avatar, which is exactly the fallback React renders when the
//!   read fails. The icon path rides [`WappSummaryVm::icon`] for the wiring.
//! - `PERMISSION_LABELS` (`windows/wapps/permissionLabels.ts` is T484): the
//!   permission badges render without their tooltip texts until that port
//!   lands; the mismatch badge's own hint tooltip is real (its key is in the
//!   zh-CN table).
//! - disabling a wapp closes its workspace tabs in React (`closeByWapp`) —
//!   the workspace tabs this build carries are table/query tabs; no wapp tab
//!   exists to close (the workspace-tabs port is ahead).
//!
//! ## Keys
//!
//! React's card testid (`extension-card`) and its `data-wapp-id` become one
//! namespaced key per card — the single-key runtime has no scoped query, so a
//! shared `extension-card` key could not tell two cards apart (the same
//! namespacing the history entries' port chose for React's shared
//! `global-history-*` testids). The card's per-wapp actions follow the card's
//! namespace; the rest of the page keeps React's fixed testids where they are
//! unique.
//!
//! | part | key |
//! |---|---|
//! | card / icon slot / initial | `extension-card-<wappId>` (the `data-wapp-id` is the suffix), `extension-card-<id>-icon`, `-icon-initial` |
//! | name / version chip / byline | `extension-card-<id>-name`, `-version`, `-byline` |
//! | description / badge row | `extension-card-<id>-description`, `-badges` |
//! | mismatch badge / theme badge / permission badge | `extension-card-<id>-badge-api-mismatch`, `-badge-theme`, `-badge-permission-<perm>` |
//! | theme hint / footer row | `extension-card-<id>-theme-hint`, `-footer` |
//! | toggle / open / uninstall | `extension-card-<id>-toggle`, `-open`, `-uninstall` (React's `extension-toggle`/`extension-open`/`extension-uninstall` testids, namespaced) |

use crate::i18n::t;
use crate::icons::ICON_TRASH;
use crate::state_def::AppState;
use crate::ui::badge::{Badge, BadgeTone};
use crate::ui::kit::kit;
use crate::ui::toggle::Switch;
use crate::views::wapps::wapp_summary::{has_pages, has_themes, WAPP_API_VERSION, WappSummaryVm};
use rune::{col, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit};

/// One installed wapp's card. `dark` carries the theme the hosting page built
/// with; the card is a pure function of the wapp row (React's `renderCard`
/// closes over nothing but the wapp).
pub(crate) fn wapp_card(wapp: &WappSummaryVm, dark: bool) -> El {
    let k = kit(dark);
    let card_key = format!("extension-card-{}", wapp.id);
    let api_mismatch = wapp.api_version != WAPP_API_VERSION;
    let dimmed = api_mismatch || !wapp.enabled;

    let mut card = col()
        .key(card_key.clone())
        // React's grid floor: `repeat(auto-fill,minmax(260px,1fr))` — the
        // wrapped row lays the cards out at the same 260pt basis (the `1fr`
        // stretch of the remainder is the one simplification).
        .w(260.0)
        .px(16.0)
        .py(16.0)
        .gap(10.0)
        .radius(k.radius.lg)
        .border(1.0, k.c.border_base)
        .fill(k.c.layer_01)
        // React: `transition-colors hover:border-accent/50` — the hover
        // feedback rides the standard hover overlay (the connection cards'
        // precedent) instead of a border tint.
        .hover(k.c.hover);

    // The identity row: the icon slot beside the name block
    // (React: `flex items-center gap-2.5`).
    card = card.child(
        row()
            .key(format!("{card_key}-identity"))
            .w_full()
            .gap(10.0)
            .items_center()
            .child(card_icon_slot(wapp, &k, &card_key))
            .child(
                col()
                    .key(format!("{card_key}-head"))
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(2.0)
                    .child(
                        row()
                            .key(format!("{card_key}-name-row"))
                            .w_full()
                            .gap(6.0)
                            .items_center()
                            .child(
                                text(wapp.name.clone())
                                    .key(format!("{card_key}-name"))
                                    // React: `truncate text-sm font-semibold`
                                    // (14/600 → the kit's 14/500 medium).
                                    .font(k.ty.body_medium)
                                    .color(k.c.text_base)
                                    .grow(1.0)
                                    .min_w(0.0)
                                    .truncate(),
                            )
                            .child(version_chip(wapp, &k, &card_key)),
                    )
                    .child(card_byline(wapp, &k, &card_key)),
            ),
    );

    if let Some(description) = wapp
        .description
        .clone()
        .filter(|description| !description.is_empty())
    {
        card = card.child(
            text(description)
                .key(format!("{card_key}-description"))
                // React: `line-clamp-2 text-xs leading-relaxed
                // text-fg-secondary`.
                .font(k.ty.text_xsmall.lh(19.5))
                .color(k.c.text_muted)
                .w_full()
                .wrap()
                .max_lines(2),
        );
    }

    // The badge row (React: `flex flex-wrap gap-1`).
    let mut badges = row()
        .key(format!("{card_key}-badges"))
        .w_full()
        .gap(4.0)
        .flex_wrap();
    if api_mismatch {
        let hint = t("extensions.page.apiMismatchHint")
            .replace("{wapp}", &wapp.api_version.to_string())
            .replace("{host}", &WAPP_API_VERSION.to_string());
        badges = badges.child(
            Badge::new(
                format!("{card_key}-badge-api-mismatch"),
                t("extensions.page.apiMismatch"),
            )
            .tone(BadgeTone::Warning)
            .build(dark)
            // React: `title={t('extensions.page.apiMismatchHint', …)}`.
            .help(hint),
        );
    }
    if !has_pages(wapp) && has_themes(wapp) {
        badges = badges.child(
            Badge::new(
                format!("{card_key}-badge-theme"),
                t("extensions.page.themeBadge"),
            )
            .tone(BadgeTone::Accent)
            .build(dark),
        );
    }
    for perm in wapp.permissions.clone() {
        // The tooltip text is T484's port (`PERMISSION_LABELS`); the badge
        // itself renders the permission string, as React's does.
        badges = badges.child(
            Badge::new(format!("{card_key}-badge-permission-{perm}"), perm.clone()).build(dark),
        );
    }
    card = card.child(badges);

    if !has_pages(wapp) && has_themes(wapp) {
        card = card.child(
            text(t("extensions.page.themeHint"))
                .key(format!("{card_key}-theme-hint"))
                // React: `text-[11px] text-fg-muted`.
                .font(k.ty.small)
                .color(k.c.text_faint)
                .w_full()
                .wrap(),
        );
    }

    // React: the footer's `mt-auto` — a filler column pins it to the card's
    // bottom edge, so a stretched line's shorter cards keep their actions
    // aligned (the wrap row stretches its children).
    card = card.child(col().key(format!("{card_key}-spacer")).grow(1.0).min_h(0.0));

    card = card.child(card_footer(wapp, &k, &card_key, api_mismatch));

    // React: `dimmed && 'opacity-60'` — the whole card composites at 60%.
    // The element's own opacity is the one the a11y tree's opacity channel
    // reads (a `layer_opacity` paints the same dim but never reports), and
    // only a fully transparent element stops taking presses — the dimmed
    // card's uninstall stays live, as React's un-disabled button does.
    if dimmed {
        card = card.opacity(0.6);
    }
    card
}

/// React's `WappCardIcon` (`WappManagementPage.tsx:51-102`): the 36pt rounded
/// well with the accent letter, or the package icon image when the wapp
/// declares one and is enabled. The image branch is the `readWappFile` host
/// seam (no adapter method), so the letter — React's fallback for a missing
/// icon, a failed read or a disabled wapp — is what renders.
fn card_icon_slot(wapp: &WappSummaryVm, k: &Kit, card_key: &str) -> El {
    // React: `wapp.name.slice(0, 1).toUpperCase()` — the first character,
    // uppercased (a multi-character uppercase stays whole, as JS's does).
    let initial = wapp
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_default();
    row()
        .key(format!("{card_key}-icon"))
        // React: `flex h-9 w-9 shrink-0 items-center justify-center
        // overflow-hidden rounded-md bg-surface-raised text-base
        // font-semibold text-accent`.
        .size(36.0, 36.0)
        .shrink()
        .center()
        .radius(k.radius.md)
        .fill(k.c.layer_02)
        .child(
            text(initial)
                .key(format!("{card_key}-icon-initial"))
                .font(TextStyle::new(16.0, 600.0))
                .color(k.c.bg_accent),
        )
}

/// The `v{version}` chip beside the name (React: `shrink-0 rounded
/// bg-surface-raised px-1 py-px text-[10px] text-fg-muted`).
fn version_chip(wapp: &WappSummaryVm, k: &Kit, card_key: &str) -> El {
    row()
        .key(format!("{card_key}-version"))
        .shrink()
        .center_y()
        .px(4.0)
        .py(1.0)
        .radius(4.0)
        .fill(k.c.layer_02)
        .child(
            text(format!("v{}", wapp.version))
                .key(format!("{card_key}-version-text"))
                .font(TextStyle::new(10.0, 400.0))
                .color(k.c.text_faint),
        )
}

/// `by {author}` when the manifest carries a non-empty author, else the id
/// (React: `wapp.author ? \`by ${wapp.author}\` : wapp.id`).
fn card_byline(wapp: &WappSummaryVm, k: &Kit, card_key: &str) -> El {
    let byline = match wapp.author.as_deref() {
        Some(author) if !author.is_empty() => format!("by {author}"),
        _ => wapp.id.clone(),
    };
    text(byline)
        .key(format!("{card_key}-byline"))
        // React: `truncate text-[11px] text-fg-muted`.
        .font(k.ty.small)
        .color(k.c.text_faint)
        .w_full()
        .min_w(0.0)
        .truncate()
}

/// The card's action row (React: `mt-auto flex items-center gap-2 border-t
/// border-edge pt-2.5`) — the enable switch, the Open action, the spacer and
/// the uninstall.
fn card_footer(wapp: &WappSummaryVm, k: &Kit, card_key: &str, api_mismatch: bool) -> El {
    let mut footer = row()
        .key(format!("{card_key}-footer"))
        .w_full()
        .items_center()
        .gap(8.0)
        .pt(10.0)
        // React: `border-t border-edge` — the top edge only.
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base);

    // React's hand-drawn switch: `role="switch"` with `aria-checked`, the
    // toggle key as `aria-label`, disabled (with the opacity) on an API
    // mismatch, and the title swapping to the mismatch label. The kit's web
    // track (24 × 16) is the closest of its three switch skins to React's
    // hand-drawn 32 × 18.
    let wapp_id = wapp.id.clone();
    footer = footer.child(
        Switch::new(format!("{card_key}-toggle"), wapp.enabled)
            .web()
            .label(t("extensions.page.toggle"))
            .disabled(api_mismatch)
            .on_toggle(move |state: &mut AppState| state.toggle_wapp(&wapp_id))
            .build(k)
            .help(if api_mismatch {
                t("extensions.page.apiMismatch")
            } else {
                t("extensions.page.toggle")
            }),
    );

    // The Open action — only a wapp with pages and no mismatch carries one
    // (React: `hasPages(wapp) && !apiMismatch`).
    if has_pages(wapp) && !api_mismatch {
        let wapp_id = wapp.id.clone();
        footer = footer.child(
            // React: `Button size="sm" variant="secondary"` (h-7 = 28pt); the
            // T022 mapping of `secondary` (transparent + edge border + fg
            // text) is the kit Outline skin.
            Button::new(t("extensions.page.open"))
                .variant(ButtonVariant::Outline)
                .control_size(ControlSize::Large)
                .disabled(!wapp.enabled)
                .on_press(move |state: &mut AppState| {
                    state.open_wapp_page(&wapp_id);
                })
                .build(k)
                .key(format!("{card_key}-open")),
        );
    }

    // React: `<span className="flex-1" />` between the Open action and the
    // uninstall.
    footer = footer.child(col().key(format!("{card_key}-footer-spacer")).grow(1.0));

    let wapp_id = wapp.id.clone();
    footer = footer.child(
        // React: `Button size="sm" variant="ghost"` wrapping a `Trash2`
        // (h-3.5) — the kit's ghost IconButton at the sm height.
        IconButton::new(&ICON_TRASH)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Large)
            .label(t("extensions.page.uninstall"))
            .help(t("extensions.page.uninstall"))
            .on_press(move |state: &mut AppState| state.ask_remove_wapp(&wapp_id))
            .build(k)
            .key(format!("{card_key}-uninstall")),
    );
    footer
}
