//! Port of `packages/frontend/src/windows/wapps/WappManagementPage.tsx`
//! (T072 in this segment's live CLI numbering; the committed parent
//! tasklist's older renumbering says T073), the wapps area's page:
//! the installed list with its search box, the all/workspace/theme filter
//! chips, the uninstall confirmation and the install dialog's open press.
//! React hosts the page inside the connection window as the `extension`
//! workspace mode; the Rune shell mounts it from `Section::Wapps`
//! (`views/workspace.rs`'s `page_body`), which the rail's Extensions entry
//! selects.
//!
//! ## React → Rune mapping
//!
//! | React | Rune |
//! |---|---|
//! | `useWappStore` (`wapps`/`loaded`/`error`) | The adapter's read-only snapshot, plus the store's last successful list and error |
//! | Wapp page mount | The rail requests the native profile's registry once on entry, or refreshes the last snapshot on re-entry |
//! | `search`/`filter` restored from the `workspacePanelStateStore` snapshot on mount, saved on unmount | the two live in `AppState::wapp_search`/`AppState::wapp_filter` and simply persist — the section switch that unmounts React's page and remounts it cannot drop Rune state, so the snapshot round-trip is a no-op of the same observable behaviour |
//! | `filtered`/`counts`/`allGroups` `useMemo`s | the functions of them in `wapp_summary.rs` |
//! | `useConfirmDialog()`'s pending `{options, resolve}` | `AppState::wapp_remove_request` (+ `wapp_remove_epoch`: each ask is a fresh mount) and `answer_remove_wapp` — the running-execution close host's pattern |
//! | `actionError` (the page's catch slot) | `AppState::wapp_action_error` |
//! | the install press (`extensions.page.install`) | `open_install_wapp_dialog` — the T022 dialog's production caller this port makes real |
//! | the grid `repeat(auto-fill,minmax(260px,1fr))` | equal-width tracks computed from each mounted grid’s measured width |
//!
//! ## Honest seams (each belongs to its own later task)
//!
//! - The native adapter can list validated local metadata only. Opening,
//!   enabling, removing, loading icons, and activating themes are unavailable.
//!   The install button still opens the existing picker dialog; choosing a
//!   package does not inspect or install it in this build.
//! - `PERMISSION_LABELS` (`permissionLabels.ts`, T484) — see the card's own
//!   seams.
//! - `ensureWappsChangedListener` (`wapps:changed` cross-window refresh,
//!   `wappStore.ts:62-94`) — Rune is single-window with no event bus
//!   (`lib/crossWindowBus.ts` is T437); no second window can broadcast the
//!   wapps change, so the listener has nothing to install. The install
//!   success transition (`install_wapp_succeeded`) carries the same refresh
//!   for the one writer this window has.
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | page root / content column | `extension-management-page`, `extension-page-content` (React's testid on the root) |
//! | title / count / subtitle | `extension-page-title`, `extension-page-count`, `extension-page-subtitle` |
//! | install button | `extension-install-button` (React's testid) |
//! | search row / input | `extension-page-search`, `extension-search-input` (React's testid) |
//! | filter chips | `extension-filter-all` / `extension-filter-workspace` / `extension-filter-theme` (React's testids) |
//! | error row | `extension-page-error` (React's testid) |
//! | empty state | `extension-page-empty` (React's testid) |
//! | flat grid / group / group header | `extension-grid`, `extension-group-workspace` / `extension-group-theme` (React's testids), `-header` |
//! | remove-confirm panel / message / warning glyph | `wapp-remove-dialog*` (from the `ui/dialog` base), `wapp-remove-dialog-message`, `-warning` |
//! | remove-confirm cancel / confirm | `extension-remove-cancel`, `extension-remove-confirm` (React's shared `confirm-dialog-*` testids, renamed: the running-execution close host already owns those keys and the single-key runtime cannot host both) |

use crate::i18n::t;
use crate::icons::{ICON_DOWNLOAD, ICON_PACKAGE_OPEN};
use crate::state_def::AppState;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::palette;
use crate::views::wapps::wapp_card::wapp_card;
use crate::views::wapps::wapp_summary::{
    all_wapp_groups, filtered_wapps, wapp_counts, WappFilter, WappSummaryVm,
};
use datazen_ui_api::WappListError;
use rune::tok;
use rune::{col, icon, row, text, Color, El, ScrollSpec, TextCase, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

/// React's `max-w-5xl` content column (64rem in points at scale 1).
const CONTENT_MAX_WIDTH: f32 = 1024.0;

/// The uninstall ask the confirm dialog renders — the `useConfirmDialog`
/// options object at ask time (`WappManagementPage.tsx:188-203`: the
/// uninstall title, the `{name}`-resolved message, the warning kind). The
/// title and kind are constants the dialog builds from; the message names the
/// wapp, so it is resolved once here, exactly as React's closure does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WappRemoveRequest {
    /// The wapp the uninstall targets (React: the closure's `wapp.id`).
    pub wapp_id: String,
    /// The confirm message, `{name}` resolved at ask time.
    pub message: String,
}

/// The wapps management page — React's `WappManagementPage` body, branch by
/// branch: the header (title, installed count, subtitle, the install press),
/// the search row (input + three chips), the error row, then the empty state,
/// the "all" view's two groups or the flat grid. The dialogs float from the
/// root modal layer (`views/modals.rs`).
pub fn wapp_management_page(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let _ = theme; // the kit tokens carry the page's paint; tok::Theme stays in the shell's type
    let k = kit(dark);
    let filtered = filtered_wapps(state);
    let read_only = state.desktop.wapp_listing_is_read_only();
    let show_counts = !read_only || state.wapps_snapshot_known;
    let counts = show_counts.then(|| wapp_counts(&state.wapps));

    // React: `min-h-0 flex-1 overflow-y-auto` — the page fills its slot in
    // the shell's page column and scrolls its own content.
    let page = col()
        .key("extension-management-page")
        .fill(k.c.page)
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .scroll(ScrollSpec { stick_bottom: false });

    // React: `mx-auto max-w-5xl px-6 py-6` — the centered column the blocks
    // stack inside (`mt-4`/`mt-3`/`mt-4` between them, spelled as margins so
    // the conditional error row leaves no gap behind it).
    let mut content = col()
        .key("extension-page-content")
        .w_full()
        .max_w(CONTENT_MAX_WIDTH)
        .self_center()
        .px(24.0)
        .py(24.0)
        .child(page_header(state, k, dark, show_counts));

    content = content.child(search_row(state, k, dark, counts).mt(16.0));

    // React: `{(actionError || storeError) && <div …>` — the page's catch
    // slot first, the store's own error when no action ran.
    if let Some(error) = state
        .wapp_action_error
        .clone()
        .or_else(|| state.wapps_error.clone())
        .or_else(|| state.wapps_list_problem.map(|problem| t(match problem {
            WappListError::Unavailable => "extensions.page.nativeListUnavailable",
            WappListError::Unreadable => "extensions.page.nativeListUnreadable",
        }).to_owned()))
    {
        content = content.child(
            text(error)
                .key("extension-page-error")
                // React: `select-text mt-3 text-sm text-red-400` — one
                // selectable paragraph in the danger ink (BODY is 14pt/400,
                // React's text-sm).
                .font(k.ty.body)
                .color(palette::danger_text(dark))
                .selectable(true)
                .w_full()
                .wrap()
                .mt(12.0),
        );
    }

    if state.wapps_pending.is_some() {
        content = content.child(
            text(t("common.loading")).key("extension-page-loading").color(k.c.text_muted).mt(12.0),
        );
    }
    if read_only && !state.wapps.is_empty() {
        content = content.child(
            text(t("extensions.page.nativeReadOnly"))
                .key("extension-page-read-only")
                .font(k.ty.body)
                .color(k.c.text_muted)
                .w_full()
                .wrap()
                .mt(12.0),
        );
    }

    if filtered.is_empty() {
        // A failed or unknown stock snapshot does not establish an empty
        // registry. Leave any verified rows intact above/below the warning.
        if !read_only || (state.wapps_snapshot_known && state.wapps_list_problem.is_none()) {
            content = content.child(empty_state(k).mt(40.0));
        }
    } else if let Some(groups) = all_wapp_groups(&filtered, state.wapp_filter) {
        // The "all" view: the two sections, each with its header and grid
        // (React: `mt-4 flex flex-col gap-6` over the non-empty groups — the
        // gap counts rendered sections only, and a skipped group leaves none
        // behind it).
        let mut group_stack = col().key("extension-groups").w_full().mt(16.0).gap(24.0);
        for (group_key, items) in groups {
            if items.is_empty() {
                // React: `items.length === 0 ? null` — an empty group renders
                // nothing at all.
                continue;
            }
            let label_key = match group_key {
                "workspace" => "extensions.page.groupWorkspace",
                _ => "extensions.page.groupTheme",
            };
            group_stack = group_stack.child(
                col()
                    .key(format!("extension-group-{group_key}"))
                    .w_full()
                    .child(
                        text(t(label_key))
                            .key(format!("extension-group-{group_key}-header"))
                            // React: `mb-2 text-[11px] font-semibold uppercase
                            // tracking-wide text-fg-muted`.
                            .font(TextStyle::new(11.0, 600.0).tracking(0.3))
                            .color(k.c.text_faint)
                            .text_case(TextCase::Uppercase)
                            .w_full()
                            .mb(8.0),
                    )
                    .child(wapp_grid(&format!("extension-grid-{group_key}"), &items, dark, read_only)),
            );
        }
        content = content.child(group_stack);
    } else {
        // The single-kind views keep React's flat grid without headers.
        content = content.child(wapp_grid("extension-grid", &filtered, dark, read_only).mt(16.0));
    }

    page.child(content)
}

/// The page header (React: `flex items-start justify-between gap-4`) — the
/// title with the installed count, the subtitle, and the install press that
/// opens the T022 dialog over the window.
fn page_header(state: &AppState, k: &'static Kit, dark: bool, show_counts: bool) -> El {
    let _ = dark; // the kit build below already carries the appearance
    let mut header = row().key("extension-page-header").w_full().items_start().gap(16.0);
    let mut title = row()
        .key("extension-page-title-row")
        .w_full()
        .items_center()
        .gap(8.0)
        .child(
            text(t("extensions.page.title"))
                .key("extension-page-title").font(TextStyle::new(16.0, 600.0))
                .color(k.c.text_base),
        );
    if show_counts {
        title = title.child(
            text(t("extensions.page.count").replace("{count}", &state.wapps.len().to_string()))
                .key("extension-page-count").font(k.ty.text_xsmall).color(k.c.text_faint),
        );
    }
    header = header.child(
        col()
            .key("extension-page-heading").grow(1.0).min_w(0.0).child(title)
            .child(
                text(t("extensions.page.subtitle"))
                    .key("extension-page-subtitle")
                    // React: `mt-1 text-xs text-fg-muted`.
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint)
                    .w_full()
                    .wrap()
                    .mt(4.0),
            ),
    );
    // WappManagementPage uses packages/ui Button with its default primary
    // md variant: 36px, 18px inline padding, 13px semibold and 9px radius.
    let shadow = Color::BLACK.alpha(26.0 / 255.0);
    header = header.child(
        row()
            .key("extension-install-button")
            .h(36.0)
            .px(18.0)
            .gap(8.0)
            .center()
            .radius(9.0)
            .fill(k.c.bg_accent)
            .hover(k.c.submit_hover)
            .focus_ring(2.0, 0.0, k.c.bg_accent.alpha(0.60))
            // Exact Tailwind shadow-sm geometry; both layers use #0000001a.
            .shadow(1.0, 3.0, shadow)
            .box_shadow(rune::Shadow {
                y: 1.0,
                blur: 2.0,
                spread: -1.0,
                color: shadow,
                ..rune::Shadow::default()
            })
            .role("button")
            .focusable(true)
            .label(t("extensions.page.install"))
            .child(
                icon(&ICON_DOWNLOAD, 16.0)
                    .key("extension-install-button-icon")
                    .color(k.c.submit_icon)
                    .a11y_hidden(),
            )
            .child(
                text(t("extensions.page.install"))
                    .key("extension-install-button-label")
                    .font(TextStyle::new(13.0, 600.0).lh(19.5))
                    .color(k.c.submit_fg)
                    .a11y_hidden(),
            )
            .on_press(|state: &mut AppState| state.open_install_wapp_dialog()),
    );
    header
}

/// The search row (React: `mt-4 flex flex-wrap items-center gap-2`) — the
/// search input and the three filter chips with their counts.
fn search_row(state: &AppState, k: &'static Kit, dark: bool, counts: Option<(usize, usize, usize)>) -> El {
    let _ = dark; // the TextField build carries the appearance
    let mut search = row()
        .key("extension-page-search")
        .w_full()
        .items_center()
        .gap(8.0)
        .flex_wrap();
    search = search.child(
        TextField::new("extension-search-input", &state.wapp_search)
            // React: `h-8 max-w-xs text-xs` on the shared Input base.
            .height(32.0)
            .width(320.0)
            .shrink()
            .pad_x(12.0)
            .radius(k.radius.md)
            .fill(k.c.bg_base)
            .border(1.0, k.c.border_base)
            .text_style(k.ty.text_xsmall)
            .placeholder(t("extensions.page.searchPlaceholder"))
            .on_edit(|state: &mut AppState| &mut state.wapp_search)
            .build(dark),
    );
    let count_for = |filter: WappFilter| counts.map(|counts| match filter {
        WappFilter::All => counts.0,
        WappFilter::Workspace => counts.1,
        WappFilter::Theme => counts.2,
    });
    for filter in WappFilter::ALL {
        let selected = state.wapp_filter == filter;
        // React: selected `border-accent/60 bg-accent/15 text-accent`; else
        // `border-edge text-fg-secondary`.
        let (fill, border, ink) = if selected {
            (
                k.alpha(k.c.bg_accent, 0.15),
                k.alpha(k.c.bg_accent, 0.6),
                k.c.bg_accent,
            )
        } else {
            (Color::TRANSPARENT, k.c.border_base, k.c.text_muted)
        };
        let count = count_for(filter).map(|count| count.to_string()).unwrap_or_else(|| "—".into());
        let label = t(filter.label_key()).replace("{count}", &count);
        search = search.child(
            row()
                .key(filter.key())
                // React: `rounded-full border px-3 py-1 text-xs
                // transition-colors`.
                .h(26.0)
                .px(12.0)
                .center_y()
                .radius(13.0)
                .fill(fill)
                .border(1.0, border)
                .role("button")
                .focusable(true)
                .label(label.clone())
                .on_press(move |state: &mut AppState| state.wapp_filter = filter)
                .child(text(label).font(k.ty.text_xsmall).color(ink)),
        );
    }
    search
}

/// The empty state (React: `mt-10 flex flex-col items-center gap-2
/// rounded-lg border border-dashed border-edge px-4 py-12 text-center`).
fn empty_state(k: &'static Kit) -> El {
    col()
        .key("extension-page-empty")
        .w_full()
        .items_center()
        .gap(8.0)
        .px(16.0)
        .py(48.0)
        .radius(k.radius.lg)
        .border(1.0, k.c.border_base)
        .border_dash(4.0, 4.0)
        .child(
            // React: `PackageOpen className="h-6 w-6 text-fg-muted"`.
            icon(&ICON_PACKAGE_OPEN, 24.0)
                .key("extension-page-empty-icon")
                .color(k.c.text_faint),
        )
        .child(
            text(t("extensions.page.emptyTitle"))
                .key("extension-page-empty-title")
                // React: `text-sm text-fg-secondary`.
                .font(k.ty.body)
                .color(k.c.text_muted),
        )
        .child(
            text(t("extensions.page.emptyHint"))
                .key("extension-page-empty-hint")
                // React: `max-w-sm text-xs text-fg-muted`.
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint)
                .max_w(384.0)
                .wrap()
                .align(rune::Align::Center),
        )
}

/// Match CSS auto-fill, not auto-fit: a short last row (even a single card)
/// keeps the same track width as a full row. Explicit rows avoid flex-wrap
/// moving the last card when the fractional track widths round differently.
fn wapp_grid(key: &str, items: &[&WappSummaryVm], dark: bool, read_only: bool) -> El {
    use super::grid_tracks::{tracks, GAP};
    let measured = rune::memo::layout_with(key, |rect| rect[2]);
    let (columns, width) = tracks(measured);
    let mut grid = col().key(key.to_owned()).w_full().gap(GAP);
    for (index, chunk) in items.chunks(columns).enumerate() {
        let mut line = row()
            .key(format!("{key}-row-{index}"))
            .w_full()
            .items_stretch()
            .gap(GAP);
        for wapp in chunk {
            line = line.child(wapp_card(wapp, dark, read_only, width));
        }
        grid = grid.child(line);
    }
    grid
}

/// The uninstall confirmation — React's `useConfirmDialog` rendering the
/// shared `ConfirmDialog` (the running-execution close host's inline twin): the
/// warning kind, the message with the wapp's name, and the two answers —
/// confirm (danger) and cancel (ghost). The header close and Escape are the
/// cancel answer, as React's `onClose={onCancel}` is. Mounted by the root
/// modal layer while a request is pending.
pub fn wapp_remove_confirm_dialog(state: &AppState, dark: bool) -> Dialog {
    // The ask is always present while the dialog is mounted (the modal layer
    // checks); a missing ask keeps an empty message out of the tree anyway.
    let message = state
        .wapp_remove_request
        .as_ref()
        .map(|request| request.message.clone())
        .unwrap_or_default();
    let k = kit(dark);
    Dialog::new("wapp-remove-dialog", t("extensions.page.uninstallTitle"))
        .open(true)
        .open_epoch(state.wapp_remove_epoch)
        // React's `max-w-lg` on the packages/ui Dialog.
        .width(512.0)
        .fit()
        // React caps the body at 70vh; 448 keeps that cap inside the 640pt
        // minimum window (the import dialog's documented figure).
        .body_max_height(448.0)
        .child(
            row()
                .key("wapp-remove-dialog-message-row")
                .w_full()
                .items_start()
                .gap(12.0)
                .child(
                    // React: `kind === 'warning'` — the amber glyph leads the
                    // message (the kit's warning tokens, the close host's
                    // precedent).
                    icon(&crate::icons::ICON_ALERT_TRIANGLE, 20.0)
                        .key("wapp-remove-dialog-warning")
                        // React `mt-0.5`: the glyph centers on the first line.
                        .mt(2.0)
                        .color(k.c.warning.fg),
                )
                .child(
                    text(message)
                        .key("wapp-remove-dialog-message")
                        // React: `text-sm text-fg-secondary`.
                        .font(k.ty.body)
                        .color(k.c.text_muted)
                        .wrap()
                        .grow(1.0)
                        .min_w(0.0)
                        .selectable(true),
                ),
        )
        .footer(
            Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Large)
                .on_press(|state: &mut AppState| state.answer_remove_wapp(false))
                .build(k)
                .key("extension-remove-cancel"),
        )
        .footer(
            // React's warning kind: the confirm is the danger variant.
            Button::new(t("common.confirm"))
                .variant(ButtonVariant::Danger)
                .control_size(ControlSize::Large)
                .on_press(|state: &mut AppState| state.answer_remove_wapp(true))
                .build(k)
                .key("extension-remove-confirm"),
        )
        // React's `onClose={onCancel}`: the header close and Escape are the
        // same answer as the cancel button.
        .on_close(|state: &mut AppState| state.answer_remove_wapp(false))
}
