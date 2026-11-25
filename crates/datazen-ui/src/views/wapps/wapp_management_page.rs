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
//! | `useWappStore` (`wapps`/`loaded`/`error`) | `AppState::wapps` / `AppState::wapps_loaded` / `AppState::wapps_error` — the store's fields, filled by the seam transition `wapps_loaded` (`wapps_state.rs`) |
//! | `useEffect(() => { if (!loaded) fetch(); }, [loaded])` | `AppState::select`'s Wapps arm runs the same `!loaded` fetch guard when the rail enters the page — React's mount effect, at the moment the page mounts |
//! | `search`/`filter` restored from the `workspacePanelStateStore` snapshot on mount, saved on unmount | the two live in `AppState::wapp_search`/`AppState::wapp_filter` and simply persist — the section switch that unmounts React's page and remounts it cannot drop Rune state, so the snapshot round-trip is a no-op of the same observable behaviour |
//! | `filtered`/`counts`/`allGroups` `useMemo`s | the functions of them in `wapp_summary.rs` |
//! | `useConfirmDialog()`'s pending `{options, resolve}` | `AppState::wapp_remove_request` (+ `wapp_remove_epoch`: each ask is a fresh mount) and `answer_remove_wapp` — the running-execution close host's pattern |
//! | `actionError` (the page's catch slot) | `AppState::wapp_action_error` |
//! | the install press (`extensions.page.install`) | `open_install_wapp_dialog` — the T022 dialog's production caller this port makes real |
//! | the grid `repeat(auto-fill,minmax(260px,1fr))` | a wrapping row of 260pt cards with the same column count (the `1fr` stretch is the one simplification) |
//!
//! ## Honest seams (each belongs to its own later task)
//!
//! - **The wapp host use cases** — `list_wapps`, `set_wapp_enabled`,
//!   `remove_wapp` (and the `read_wapp_file` the card's icon would need) —
//!   have no `DesktopAdapter` methods. The store's fetch therefore runs
//!   React's failure branch honestly (the seam sentence names the missing use
//!   case, in the same form the T022 install dialog's answers take), and the
//!   toggle/uninstall actions run their catches: the optimistic flip, then
//!   the store's error slot, then the page's action error. The success
//!   transitions (`wapps_loaded`, `wapp_removed`) are the surface the wiring
//!   will call; tests pin them.
//! - `openWappPage` — the workspace tab the Open press creates
//!   (`workspaceTabKey(wappId, pageId)` keyed, with the page's title/icon/
//!   version) and the extension page shell it hosts belong to the workspace
//!   tabs port; this build's workspace carries table/query tabs only. The
//!   resolve gate (enabled, has pages, the first page) is real, and the host
//!   switch the caller makes of it — `onOpenInWorkspace()` →
//!   `select(Section::Workspace)` — is the half this port performs.
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
    let counts = wapp_counts(&state.wapps);

    // React: `min-h-0 flex-1 overflow-y-auto` — the page fills its slot in
    // the shell's page column and scrolls its own content.
    let page = col()
        .key("extension-management-page")
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
        .child(page_header(state, k, dark));

    content = content.child(search_row(state, k, dark, counts).mt(16.0));

    // React: `{(actionError || storeError) && <div …>` — the page's catch
    // slot first, the store's own error when no action ran.
    if let Some(error) = state
        .wapp_action_error
        .clone()
        .or_else(|| state.wapps_error.clone())
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

    if filtered.is_empty() {
        // React: `filtered.length === 0` — the empty state, whatever the error
        // row says above it (`mt-10`).
        content = content.child(empty_state(k).mt(40.0));
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
                    .child(wapp_grid(&items, dark)),
            );
        }
        content = content.child(group_stack);
    } else {
        // The single-kind views keep React's flat grid without headers.
        content = content.child(wapp_grid(&filtered, dark).mt(16.0));
    }

    page.child(content)
}

/// The page header (React: `flex items-start justify-between gap-4`) — the
/// title with the installed count, the subtitle, and the install press that
/// opens the T022 dialog over the window.
fn page_header(state: &AppState, k: &'static Kit, dark: bool) -> El {
    let _ = dark; // the kit build below already carries the appearance
    let mut header = row().key("extension-page-header").w_full().items_start().gap(16.0);
    header = header.child(
        col()
            .key("extension-page-heading")
            .grow(1.0)
            .min_w(0.0)
            .gap(4.0)
            .child(
                row()
                    .key("extension-page-title-row")
                    .w_full()
                    .gap(6.0)
                    .child(
                        text(t("extensions.page.title"))
                            .key("extension-page-title")
                            // React: `text-base font-semibold text-fg`.
                            .font(TextStyle::new(16.0, 600.0))
                            .color(k.c.text_base),
                    )
                    .child(
                        text(
                            t("extensions.page.count")
                                .replace("{count}", &state.wapps.len().to_string()),
                        )
                        .key("extension-page-count")
                        // React: `text-xs font-normal text-fg-muted`.
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_faint),
                    ),
            )
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
    header = header.child(
        // React's default Button (primary, md/h-9 = 36pt) leading with the
        // Download glyph — the kit's Contrast skin and ExtraLarge control,
        // the mapping the ported dialog footers use.
        Button::new(t("extensions.page.install"))
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::ExtraLarge)
            .icon(&ICON_DOWNLOAD)
            .on_press(|state: &mut AppState| state.open_install_wapp_dialog())
            .build(k)
            .key("extension-install-button")
            .shrink(),
    );
    header
}

/// The search row (React: `mt-4 flex flex-wrap items-center gap-2`) — the
/// search input and the three filter chips with their counts.
fn search_row(state: &AppState, k: &'static Kit, dark: bool, counts: (usize, usize, usize)) -> El {
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
            .text_style(k.ty.text_xsmall)
            .placeholder(t("extensions.page.searchPlaceholder"))
            .on_edit(|state: &mut AppState| &mut state.wapp_search)
            .build(dark),
    );
    let count_for = |filter: WappFilter| match filter {
        WappFilter::All => counts.0,
        WappFilter::Workspace => counts.1,
        WappFilter::Theme => counts.2,
    };
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
        let label = t(filter.label_key()).replace("{count}", &count_for(filter).to_string());
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

/// The card grid (React: `grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))]
/// gap-3`) — a wrapping row of 260pt cards, the same column count the
/// auto-fill picks at the same width; the line's cards stretch to one height
/// (CSS grid's default cross-axis stretch).
fn wapp_grid(items: &[&WappSummaryVm], dark: bool) -> El {
    let mut grid = row()
        .key("extension-grid")
        .w_full()
        .gap(12.0)
        .flex_wrap()
        .items_stretch();
    for wapp in items {
        grid = grid.child(wapp_card(wapp, dark));
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
